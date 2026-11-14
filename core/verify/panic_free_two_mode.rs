//! Link-time no-panic and no-heap proof for the two-mode runtime paths.
//!
//! A `no_std` cdylib over a loopback handoff binding and a one-slot lending
//! binding. Every runtime path of the six roles is reachable. If any panic path
//! survives link-time optimization, the link fails on the trap symbol. Nothing
//! here runs; the point is what the linker can still reach.

#![no_std]
#![allow(clippy::all, unused_variables, dead_code)]

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU32, Ordering};
use core::time::Duration;

use up_rust_core::communication::two_mode::{
    NotificationListener, Notifier, Publisher, RpcClient, RpcServer, Subscriber,
};
use up_rust_core::communication::LocalUriProvider;
use up_rust_core::frame::header::WrittenHeader;
use up_rust_core::memory::StartupRegion;
use up_rust_core::payload::agreement::{NativeAgreement, NativeAgreementEntry};
use up_rust_core::platform::NATIVE;
use up_rust_core::transport::binding::{
    BindingRefusal, BindingRoute, BindingSetup, Capabilities, Flow, HandoffRuntime, InboundFrame,
    InboundRoute, LendingRuntime, OutgoingFrame, PayloadBytes, PeerRef, PreparedRoute, ReplyToken,
    RouteDeclaration, RouteKind, RuntimePort, SendRefusal,
};
use up_rust_core::transport::carrier::{
    CarriedFrame, CarriedWire, CarrierRoute, EgressRoute, IngressRoute, ParkedFrame,
};
use up_rust_core::transport::envelope::Envelope;
use up_rust_core::transport::error::CallError;
use up_rust_core::transport::mode::{
    enter_runtime_all, MessageFields, TransportRuntime, TransportSetup,
};
use up_rust_core::transport::qos::{Overflow, Qos, QosSupport, SendReport};
use up_rust_core::transport::{
    Frame, InitializedTx, LoanError, LoanedPayload, PayloadLoanProvenance, PreparedTxStorage,
    SendError, TxFrameLayout,
};
use up_rust_core::uattributes::{PayloadEncoding, UMessageType};
use up_rust_core::ucode::UCode;
use up_rust_core::uri::UUri;
use up_rust_core::wire::{
    InPlace, NativeWire, PayloadWriter, Ready, RuntimeHeap, StartupSlot, Wire, WireError,
    WireIdentity, WireInfo, WirePayloadIdentity,
};

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    extern "C" {
        fn __up_rust_core_panic_path_survived() -> !;
    }
    // SAFETY: never called when the link succeeds; a surviving path fails the link.
    unsafe { __up_rust_core_panic_path_survived() }
}

struct Provider;

impl LocalUriProvider for Provider {
    fn get_resource_uri(&self, resource_id: u16) -> UUri {
        UUri::try_from_wire_parts("door", 0x0001_0001, 1, resource_id)
            .unwrap_or_else(|_| UUri::any())
    }

    fn get_source_uri(&self) -> UUri {
        self.get_resource_uri(0)
    }
}

#[derive(Clone, Copy, Default)]
struct DoorWire;

const DOOR: PayloadEncoding = PayloadEncoding::private_use(0xF001);

impl Wire<u32> for DoorWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "door",
            WireIdentity::new("test.door", 0x8A01),
            WireIdentity::new("test.door-family", 0x8A02),
            WirePayloadIdentity::serialized(DOOR),
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
        .max_encoded_len(4)
        .exact()
    }

    fn encode(&self, value: &u32, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        out.put(&value.to_be_bytes())
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, u32>) -> Ready<'s, u32> {
        slot.write(0)
    }

    fn decode(
        &self,
        encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut u32,
    ) -> Result<(), WireError> {
        if encoding != DOOR {
            return Err(WireError::UnknownEncoding(encoding));
        }
        let [a, b, c, d] = *bytes else {
            return Err(WireError::InvalidLength {
                expected: 4,
                actual: bytes.len(),
            });
        };
        *into = u32::from_be_bytes([a, b, c, d]);
        Ok(())
    }
}

/// A per-kind inbound route and the port, set during `into_running`.
struct Link {
    port: UnsafeCell<MaybeUninit<RuntimePort>>,
    inbound: UnsafeCell<[Option<InboundRoute>; 4]>,
}

// SAFETY: written during initialization only, read after; this never runs.
unsafe impl Sync for Link {}

impl Link {
    const fn new() -> Self {
        Self {
            port: UnsafeCell::new(MaybeUninit::uninit()),
            inbound: UnsafeCell::new([None; 4]),
        }
    }

    fn store(&self, kind: UMessageType, inbound: Option<InboundRoute>) {
        if let (Some(cell), Some(inbound)) = (
            unsafe { (*self.inbound.get()).get_mut(lane(kind)) },
            inbound,
        ) {
            *cell = Some(inbound);
        }
    }

    fn route(&self, kind: UMessageType) -> Option<InboundRoute> {
        unsafe { (*self.inbound.get()).get(lane(kind)).copied().flatten() }
    }

    fn port(&self) -> RuntimePort {
        unsafe { (*self.port.get()).assume_init() }
    }
}

fn lane(kind: UMessageType) -> usize {
    match kind {
        UMessageType::Publish => 0,
        UMessageType::Notification => 1,
        UMessageType::Request => 2,
        UMessageType::Response => 3,
    }
}

static HANDOFF: Link = Link::new();

fn capabilities(native: bool) -> Capabilities {
    Capabilities::new(
        Envelope::UFrameFields,
        &[Envelope::UFrameFields, Envelope::NativeWholeFrame],
        4096,
    )
    .with_per_message_encoding(true)
    .with_truncate_payload(true)
    .with_declared_clients(false)
    .with_chunked_receive(true)
    .with_native_loans(native)
}

fn support() -> QosSupport {
    QosSupport::new()
        .with_reliable(true)
        .with_drop_oldest(true)
        .with_drop_newest(true)
        .with_wait(false)
        .with_max_depth(16)
        .with_max_history(0)
        .with_priority_levels(0)
}

struct LoopbackSetup;
struct LoopbackRunning;

impl BindingSetup for LoopbackSetup {
    type Running = LoopbackRunning;

    fn name(&self) -> &'static str {
        "loopback"
    }

    fn capabilities(&self) -> Capabilities {
        capabilities(false)
    }

    fn qos_support(&self, _kind: RouteKind) -> QosSupport {
        support()
    }

    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        let kind = route.kind.unwrap_or(UMessageType::Publish);
        HANDOFF.store(kind, route.inbound);
        Ok(PreparedRoute::new(
            BindingRoute(lane(kind) as u32),
            RuntimeHeap::None,
        ))
    }

    fn into_running(self, port: RuntimePort) -> Result<LoopbackRunning, BindingRefusal> {
        unsafe { (*HANDOFF.port.get()).write(port) };
        Ok(LoopbackRunning)
    }
}

impl HandoffRuntime for LoopbackRunning {
    fn receive_bounded(
        &self, route: up_rust_core::transport::BindingRoute, budget: &up_rust_core::transport::ReceiveBudget,
        sink: &mut dyn FnMut(up_rust_core::transport::InboundFrame<'_>) -> up_rust_core::transport::Flow,
    ) -> usize {
        // Explicitly selected sink-visible adapter; no hidden filtering loop.
        if !budget.try_take() { return 0; }
        self.receive(route, &mut |frame| {
            let flow = sink(frame);
            if matches!(flow, up_rust_core::transport::Flow::Stop) || !budget.try_take() {
                up_rust_core::transport::Flow::Stop
            } else { up_rust_core::transport::Flow::Continue }
        })
    }

    fn send(&self, route: BindingRoute, frame: OutgoingFrame<'_>) -> Result<(), SendRefusal> {
        let Some(inbound) = HANDOFF.route(frame.kind) else {
            return Err(SendRefusal::Busy);
        };
        let delivered = HANDOFF.port().deliver(
            inbound,
            InboundFrame {
                metadata: frame.metadata,
                payload: frame.payload,
                peer: PeerRef {
                    peer: 1,
                    incarnation: 1,
                },
                reply: frame.reply,
            },
        );
        delivered.map_err(|_| SendRefusal::Failed {
            code: UCode::Internal,
            detail: "refused",
        })
    }

    fn has_capacity(&self, _route: BindingRoute) -> bool {
        KICK.load(Ordering::Relaxed) != 0
    }

    fn receive(
        &self,
        _route: BindingRoute,
        sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow,
    ) -> usize {
        // Reachable, so the carrier's borrowed path is linked. Nothing runs here.
        if KICK.load(Ordering::Relaxed) == 0 {
            return 0;
        }
        let flow = sink(InboundFrame {
            metadata: &[1, 2, 3, 4],
            payload: PayloadBytes::one(&[5, 6]),
            peer: PeerRef {
                peer: 1,
                incarnation: 1,
            },
            reply: ReplyToken(1),
        });
        usize::from(matches!(flow, Flow::Continue))
    }
}

/// Keeps the carrier's pull paths reachable. Never set when this links.
static KICK: AtomicU32 = AtomicU32::new(0);

/// One loan of one static buffer.
struct LendBuffer {
    bytes: UnsafeCell<[MaybeUninit<u8>; 4096]>,
    route_kind: AtomicU32,
}

unsafe impl Sync for LendBuffer {}

static LEND: LendBuffer = LendBuffer {
    bytes: UnsafeCell::new([MaybeUninit::uninit(); 4096]),
    route_kind: AtomicU32::new(0),
};

static LENDING: Link = Link::new();

struct LendSetup;
struct LendRunning;

struct LendTx {
    metadata: *mut MaybeUninit<u8>,
    metadata_len: usize,
    payload: *mut MaybeUninit<u8>,
    payload_len: Option<usize>,
}

unsafe impl Send for LendTx {}

unsafe impl PreparedTxStorage for LendTx {
    fn metadata_uninit_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        unsafe { core::slice::from_raw_parts_mut(self.metadata, self.metadata_len) }
    }

    fn payload_uninit_mut(&mut self) -> Option<&mut [MaybeUninit<u8>]> {
        let len = self.payload_len?;
        Some(unsafe { core::slice::from_raw_parts_mut(self.payload, len) })
    }

    fn record_header(&mut self, _header: WrittenHeader) -> Result<(), SendError> {
        Ok(())
    }

    fn truncate_payload(&mut self, used: usize) -> Result<(), SendError> {
        match self.payload_len {
            Some(len) if used <= len => {
                self.payload_len = Some(used);
                Ok(())
            }
            _ => Err(SendError::PayloadStorage),
        }
    }
}

struct LendFrame {
    metadata: *const u8,
    metadata_len: usize,
    payload: *const u8,
    payload_len: Option<usize>,
}

unsafe impl Send for LendFrame {}

impl Frame for LendFrame {
    fn encoded_metadata(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.metadata, self.metadata_len) }
    }

    fn payload(&self) -> Option<LoanedPayload<'_>> {
        self.payload_len.map(|len| {
            LoanedPayload::new(
                unsafe { core::slice::from_raw_parts(self.payload, len) },
                PayloadLoanProvenance::OpaqueTransportLoan,
            )
        })
    }

    fn peer(&self) -> PeerRef {
        PeerRef {
            peer: 2,
            incarnation: 1,
        }
    }
}

impl BindingSetup for LendSetup {
    type Running = LendRunning;

    fn name(&self) -> &'static str {
        "one-slot"
    }

    fn capabilities(&self) -> Capabilities {
        capabilities(true)
    }

    fn qos_support(&self, _kind: RouteKind) -> QosSupport {
        support()
    }

    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        let kind = route.kind.unwrap_or(UMessageType::Publish);
        LENDING.store(kind, route.inbound);
        LEND.route_kind.store(lane(kind) as u32, Ordering::Relaxed);
        Ok(PreparedRoute::new(
            BindingRoute(lane(kind) as u32),
            RuntimeHeap::None,
        ))
    }

    fn into_running(self, port: RuntimePort) -> Result<LendRunning, BindingRefusal> {
        unsafe { (*LENDING.port.get()).write(port) };
        Ok(LendRunning)
    }
}

impl LendingRuntime for LendRunning {
    // Explicit single-attempt adapter, justified by this retained implementation.
    fn receive_bounded(&self, route: up_rust_core::transport::BindingRoute, budget: &up_rust_core::transport::ReceiveBudget) -> Option<Self::Rx> {
        if !budget.try_take() { return None; }
        self.receive(route)
    }

    type Tx<'a> = LendTx;
    type Rx = LendFrame;

    fn loan(&self, route: BindingRoute, layout: TxFrameLayout) -> Result<LendTx, LoanError> {
        if layout.metadata_len > 1024 || layout.payload_len.unwrap_or(0) > 1024 {
            return Err(LoanError::SlotTooSmall);
        }
        let base = LEND.bytes.get().cast::<MaybeUninit<u8>>();
        Ok(LendTx {
            metadata: base,
            metadata_len: layout.metadata_len,
            payload: unsafe { base.add(2048) },
            payload_len: layout.payload_len,
        })
    }

    fn publish<'a>(
        &'a self,
        route: BindingRoute,
        frame: InitializedTx<LendTx>,
    ) -> Result<SendReport, SendError> {
        let tx = frame.into_storage();
        let kind = match route.0 {
            0 => UMessageType::Publish,
            1 => UMessageType::Notification,
            2 => UMessageType::Request,
            _ => UMessageType::Response,
        };
        let inbound = LENDING.route(kind).ok_or(SendError::Delivery)?;
        LENDING
            .port()
            .deliver_frame(
                inbound,
                LendFrame {
                    metadata: tx.metadata.cast::<u8>().cast_const(),
                    metadata_len: tx.metadata_len,
                    payload: tx.payload.cast::<u8>().cast_const(),
                    payload_len: tx.payload_len,
                },
            )
            .map_err(|_| SendError::Delivery)?;
        Ok({
            let mut report = SendReport::default();
            report.delivered = 1;
            report.dropped_full = 0;
            report
        })
    }

    fn receive(&self, _route: BindingRoute) -> Option<LendFrame> {
        // Reachable, so the carrier's pull path is linked. Nothing runs here.
        (KICK.load(Ordering::Relaxed) != 0).then(|| {
            let base = LEND.bytes.get().cast::<MaybeUninit<u8>>();
            LendFrame {
                metadata: base.cast::<u8>().cast_const(),
                metadata_len: 16,
                payload: unsafe { base.add(2048) }.cast::<u8>().cast_const(),
                payload_len: Some(4),
            }
        })
    }

    fn has_capacity(&self, _route: BindingRoute) -> bool {
        KICK.load(Ordering::Relaxed) != 0
    }
}

/// Collects the one runtime `enter_runtime_all` returns, without a heap.
struct One(Option<TransportRuntime>);

impl FromIterator<TransportRuntime> for One {
    fn from_iter<I: IntoIterator<Item = TransportRuntime>>(iter: I) -> Self {
        One(iter.into_iter().next())
    }
}

/// Every carrier runtime path: receive, send, park, a parked frame's resend.
fn carry(
    ingress_rt: TransportRuntime,
    ingress: IngressRoute,
    egress_rt: TransportRuntime,
    egress: EgressRoute,
    value: u32,
) -> u64 {
    let mut parked: Option<ParkedFrame> = None;
    let mut sent = 0_u64;
    let _ = ingress_rt.receive(ingress, &mut |frame: CarriedFrame<'_>| {
        if value == 77 {
            match frame.park() {
                Ok(kept) => parked = Some(kept),
                Err(_lost) => sent = sent.wrapping_add(1),
            }
            return Flow::Stop;
        }
        if !egress_rt.has_capacity(egress) {
            return Flow::Stop;
        }
        match egress_rt.send(egress, &frame) {
            Ok(_) => {
                sent = sent.wrapping_add(1);
                Flow::Continue
            }
            Err(_) => {
                match frame.park() {
                    Ok(kept) => parked = Some(kept),
                    Err(_lost) => sent = sent.wrapping_add(2),
                }
                Flow::Stop
            }
        }
    });
    if let Some(kept) = &parked {
        let frame = kept.frame();
        sent = sent.wrapping_add(u64::from(frame.metadata().len() as u32));
        sent = sent.wrapping_add(u64::from(frame.kind() as u32));
        if frame.sink().is_some() || frame.source().resource_id() == 0 {
            sent = sent.wrapping_add(3);
        }
        if egress_rt.send(egress, &frame).is_ok() {
            sent = sent.wrapping_add(1);
        }
    }
    drop(parked);
    sent
}

#[derive(
    Clone, Copy, Default, up_rust_macros::StablePayload, up_rust_macros::StablePayloadInit,
)]
#[repr(C)]
#[stable_payload(type_name = "test.Door", crate = "up_rust_core")]
struct NativeDoor {
    id: u32,
    open: u32,
}

fn native_agreement() -> &'static NativeAgreement {
    static ENTRY: [NativeAgreementEntry; 1] = [NativeAgreementEntry {
        type_name: "test.Door",
        revision: 1,
        digest: [0; 32],
    }];
    static AGREEMENT: NativeAgreement = NativeAgreement {
        domain: "fixture",
        version: 1,
        encoding: PayloadEncoding::private_use(0xF700),
        target_triple: "fixture",
        pointer_width: 64,
        little_endian: true,
        entries: &ENTRY,
    };
    &AGREEMENT
}

static HANDOFF_MEMORY: StartupRegion<65536> = StartupRegion::new();
static LENDING_MEMORY: StartupRegion<65536> = StartupRegion::new();

fn traffic(
    tx: TransportRuntime,
    publisher: Publisher<u32>,
    subscriber: Subscriber<u32>,
    notifier: Notifier<u32>,
    listener: NotificationListener<u32>,
    client: RpcClient<u32, u32>,
    server: RpcServer<u32, u32>,
    value: u32,
) -> Result<u64, ()> {
    let fields = MessageFields {
        token: Some("t"),
        ..MessageFields::default()
    };
    publisher.publish(&value).map_err(|_| ())?;
    publisher.publish_with(&fields, &value).map_err(|_| ())?;
    publisher
        .publish_using::<DoorWire>(&value)
        .map_err(|_| ())?;
    publisher
        .publish_encoded(DOOR, PayloadBytes::one(&[1, 2, 3, 4]))
        .map_err(|_| ())?;
    notifier.notify(&value).map_err(|_| ())?;
    let mut total = 0_u64;
    for _ in 0..4 {
        if let Ok(Some(received)) = subscriber.try_recv() {
            total = total.wrapping_add(u64::from(*received));
            let header = received.header();
            total = total.wrapping_add(header.token().map_or(0, str::len) as u64);
            total = total.wrapping_add(received.payload_bytes().map_or(0, <[u8]>::len) as u64);
        }
    }
    if let Ok(Some(received)) = listener.try_recv() {
        total = total.wrapping_add(u64::from(received.peer().incarnation));
    }
    let mut call = client.invoke(&value).map_err(|_| ())?;
    if let Ok(Some(request)) = server.try_recv() {
        let answer = *request;
        if answer == 0 {
            let _ = request.respond_failure(UCode::Internal);
        } else {
            let _ = request.respond(&answer.wrapping_add(1));
        }
    }
    match call.try_response() {
        Ok(Some(reply)) => total = total.wrapping_add(u64::from(*reply)),
        Ok(None) => {}
        Err(CallError::TimedOut | CallError::PeerLost | CallError::PeerRestarted) => {}
        Err(_) => {}
    }
    drop(call);
    let token = tx.wait_token();
    tx.wait(token, Duration::from_millis(1));
    total = total.wrapping_add(tx.pump() as u64);
    total = total.wrapping_add(tx.service() as u64);
    total = total.wrapping_add(u64::from(tx.counters(publisher.route_id()).carried));
    total = total.wrapping_add(tx.routes().len() as u64);
    total = total.wrapping_add(u64::from(subscriber.counters().refused));
    Ok(total)
}

fn setup_and_run(value: u32) -> Result<u64, ()> {
    let memory = HANDOFF_MEMORY.claim().map_err(|_| ())?;
    let setup = TransportSetup::handoff_on(&memory, &NATIVE, LoopbackSetup).map_err(|_| ())?;
    let provider = Provider;
    let qos = Qos::DEFAULT
        .with_depth(4)
        .with_overflow(Overflow::DropOldest);
    let publisher = Publisher::<u32>::on(&setup, &provider, 0x8001_u16)
        .wire(DoorWire)
        .max_token_len(8)
        .qos(qos)
        .build()
        .map_err(|_| ())?;
    let subscriber = Subscriber::<u32>::on(&setup, provider.get_resource_uri(0x8001))
        .wire(DoorWire)
        .max_token_len(8)
        .qos(qos)
        .build()
        .map_err(|_| ())?;
    let notifier = Notifier::<u32>::on(&setup, &provider, 0x8002_u16, provider.get_source_uri())
        .wire(DoorWire)
        .build()
        .map_err(|_| ())?;
    let listener =
        NotificationListener::<u32>::on(&setup, provider.get_resource_uri(0x8002), &provider)
            .wire(DoorWire)
            .build()
            .map_err(|_| ())?;
    let client = RpcClient::<u32, u32>::on(&setup, &provider, provider.get_resource_uri(1))
        .wire(DoorWire)
        .max_in_flight(2)
        .build()
        .map_err(|_| ())?;
    let server = RpcServer::<u32, u32>::on(&setup, &provider, 1_u16)
        .wire(DoorWire)
        .build()
        .map_err(|_| ())?;
    let carried = [CarriedWire::of(&Wire::<u32>::describe(&DoorWire))];
    let carrier_uri = provider.get_resource_uri(0x8004);
    let carrier = CarrierRoute::new(&carrier_uri, &carried, 64);
    let ingress = setup.carrier_ingress(&carrier).map_err(|_| ())?;
    let egress = setup.carrier_egress(&carrier).map_err(|_| ())?;
    let tx = setup.enter_runtime().map_err(|_| ())?;
    let publisher = publisher.activate(&tx).map_err(|_| ())?;
    let subscriber = subscriber.activate(&tx).map_err(|_| ())?;
    let notifier = notifier.activate(&tx).map_err(|_| ())?;
    let listener = listener.activate(&tx).map_err(|_| ())?;
    let client = client.activate(&tx).map_err(|_| ())?;
    let server = server.activate(&tx).map_err(|_| ())?;
    let handoff = traffic(
        tx, publisher, subscriber, notifier, listener, client, server, value,
    )?;
    let handoff = handoff.wrapping_add(carry(tx, ingress, tx, egress, value));

    let memory = LENDING_MEMORY.claim().map_err(|_| ())?;
    let setup = TransportSetup::lending_on(&memory, &NATIVE, LendSetup).map_err(|_| ())?;
    let publisher = Publisher::<u32>::on(&setup, &provider, 0x8001_u16)
        .wire(DoorWire)
        .max_token_len(8)
        .build()
        .map_err(|_| ())?;
    let subscriber = Subscriber::<u32>::on(&setup, provider.get_resource_uri(0x8001))
        .wire(DoorWire)
        .max_token_len(8)
        .build()
        .map_err(|_| ())?;
    let notifier = Notifier::<u32>::on(&setup, &provider, 0x8002_u16, provider.get_source_uri())
        .wire(DoorWire)
        .build()
        .map_err(|_| ())?;
    let listener =
        NotificationListener::<u32>::on(&setup, provider.get_resource_uri(0x8002), &provider)
            .wire(DoorWire)
            .build()
            .map_err(|_| ())?;
    let client = RpcClient::<u32, u32>::on(&setup, &provider, provider.get_resource_uri(1))
        .wire(DoorWire)
        .max_in_flight(2)
        .build()
        .map_err(|_| ())?;
    let server = RpcServer::<u32, u32>::on(&setup, &provider, 1_u16)
        .wire(DoorWire)
        .clients(core::slice::from_ref(&UUri::any()))
        .build()
        .map_err(|_| ())?;
    let native_publisher = Publisher::<NativeDoor>::on(&setup, &provider, 0x8003_u16)
        .wire(NativeWire::new(native_agreement()))
        .build()
        .map_err(|_| ())?;
    let native_subscriber = Subscriber::<NativeDoor>::on(&setup, provider.get_resource_uri(0x8003))
        .wire(NativeWire::new(native_agreement()))
        .build()
        .map_err(|_| ())?;
    let native_client =
        RpcClient::<NativeDoor, NativeDoor>::on(&setup, &provider, provider.get_resource_uri(2))
            .wire(NativeWire::new(native_agreement()))
            .max_in_flight(1)
            .build()
            .map_err(|_| ())?;
    let native_server = RpcServer::<NativeDoor, NativeDoor>::on(&setup, &provider, 2_u16)
        .wire(NativeWire::new(native_agreement()))
        .clients(core::slice::from_ref(&UUri::any()))
        .build()
        .map_err(|_| ())?;
    let carried = [CarriedWire::of(&Wire::<u32>::describe(&DoorWire))];
    let carrier_uri = provider.get_resource_uri(0x8004);
    let carrier = CarrierRoute::new(&carrier_uri, &carried, 64);
    let ingress = setup.carrier_ingress(&carrier).map_err(|_| ())?;
    let egress = setup.carrier_egress(&carrier).map_err(|_| ())?;
    let One(tx) = enter_runtime_all::<One, _>(core::iter::once(setup)).map_err(|_| ())?;
    let tx = tx.ok_or(())?;
    let publisher = publisher.activate(&tx).map_err(|_| ())?;
    let subscriber = subscriber.activate(&tx).map_err(|_| ())?;
    let notifier = notifier.activate(&tx).map_err(|_| ())?;
    let listener = listener.activate(&tx).map_err(|_| ())?;
    let client = client.activate(&tx).map_err(|_| ())?;
    let server = server.activate(&tx).map_err(|_| ())?;
    let lending = traffic(
        tx, publisher, subscriber, notifier, listener, client, server, value,
    )?;
    let lending = lending.wrapping_add(carry(tx, ingress, tx, egress, value));
    let native_client: RpcClient<NativeDoor, NativeDoor, InPlace> =
        native_client.activate(&tx).map_err(|_| ())?;
    let native_server: RpcServer<NativeDoor, NativeDoor, InPlace> =
        native_server.activate(&tx).map_err(|_| ())?;
    let native_publisher: Publisher<NativeDoor, InPlace> =
        native_publisher.activate(&tx).map_err(|_| ())?;
    let native_subscriber: Subscriber<NativeDoor, InPlace> =
        native_subscriber.activate(&tx).map_err(|_| ())?;
    native_publisher
        .publish_in_place(|init| init.into_initializer().id(value).open(1).finish())
        .map_err(|_| ())?;
    let mut native = 0_u64;
    if let Ok(Some(lease)) = native_subscriber.try_recv() {
        if let Ok(door) = lease.payload() {
            native = u64::from(door.id);
        }
    }
    let mut call = native_client
        .invoke_in_place(|init| init.into_initializer().id(value).open(1).finish())
        .map_err(|_| ())?;
    if let Ok(Some(request)) = native_server.try_recv() {
        let asked = request.payload().map_or(0, |door| door.id);
        if asked == 0 {
            let _ = request.respond_failure(UCode::Internal);
        } else {
            let _ = request.respond_in_place(|init| {
                init.into_initializer()
                    .id(asked.wrapping_add(1))
                    .open(0)
                    .finish()
            });
        }
    }
    if let Ok(Some(reply)) = call.try_response() {
        if let Ok(door) = reply.payload() {
            native = native.wrapping_add(u64::from(door.id));
        }
    }
    drop(call);
    Ok(handoff.wrapping_add(lending).wrapping_add(native))
}

/// Runs both bindings once. Not called by anything: the linker is the proof.
#[no_mangle]
pub extern "C" fn fixture_two_mode(value: u32) -> u64 {
    setup_and_run(value).unwrap_or(0)
}

#[cfg(inject_panic)]
#[no_mangle]
pub extern "C" fn fixture_injected_panic(value: u32) -> u32 {
    assert!(value != 77);
    value
}
