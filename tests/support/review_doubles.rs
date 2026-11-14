//! Test doubles for the stage-2 review regression tests: a fully manual clock, a
//! handoff and a lending binding with a scriptable flood, a heap-backed frame and
//! the helpers that assemble whole frames. Adapted from the reviewer's proofs.
#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code,
    missing_docs,
    missing_debug_implementations
)]

use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use core::time::Duration;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use up_rust_core::communication::LocalUriProvider;
use up_rust_core::frame::header::{
    header_fields_len, parse_field_block, write_field_block_uninit, HeaderLimits, TxHeaderParts,
};
use up_rust_core::memory::StartupMemory;
use up_rust_core::platform::{Monotonic, Platform, PlatformError, PlatformInit, WaitResult};
use up_rust_core::transport::binding::{
    BindingRefusal, BindingRoute, BindingSetup, Capabilities, Flow, HandoffRuntime, InboundFrame,
    InboundRoute, LendingRuntime, OutgoingFrame, PayloadBytes, PeerRef, PreparedRoute, ReplyToken,
    RouteDeclaration, RouteKind, RuntimePort, 
};
use up_rust_core::transport::qos::{QosSupport, SendReport};
use up_rust_core::transport::{
    Envelope, Frame, InitializedTx, LoanError, LoanedPayload, PayloadLoanProvenance,
    PreparedTxStorage, SendError, TxFrameLayout,
};
use up_rust_core::uattributes::{PayloadEncoding, UMessageType, UPriority};
use up_rust_core::ucode::UCode;
use up_rust_core::uri::UUri;
use up_rust_core::uuid::UUID;
use up_rust_core::wire::{
    EncodedSize, PayloadWriter, Ready, RuntimeHeap, StartupSlot, Wire, WireError, WireIdentity,
    WireInfo, WirePayloadIdentity,
};

pub const ENCODING: PayloadEncoding = PayloadEncoding::private_use(0xF001);
pub const WIRE_ID: WireIdentity = WireIdentity::new("review.u32", 0);
pub const FAMILY_ID: WireIdentity = WireIdentity::new("review.u32.family", 0);
pub const ENVELOPES: &[Envelope] = &[Envelope::NativeWholeFrame];

/// A fully manual clock.
pub struct Clock {
    pub mono: AtomicU64,
    pub wall: AtomicU64,
}

impl Clock {
    pub fn leak(mono: u64, wall: u64) -> &'static Self {
        Box::leak(Box::new(Self {
            mono: AtomicU64::new(mono),
            wall: AtomicU64::new(wall),
        }))
    }
    pub fn advance_ms(&self, ms: u64) {
        self.mono.fetch_add(ms * 1_000_000, Ordering::SeqCst);
        self.wall.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Platform for Clock {
    fn now(&self) -> Monotonic {
        Monotonic(self.mono.load(Ordering::SeqCst))
    }
    /// `u64::MAX` stands for a wall clock that cannot be read.
    fn unix_time_ms(&self) -> Option<u64> {
        let wall = self.wall.load(Ordering::SeqCst);
        (wall != u64::MAX).then_some(wall)
    }
    fn wait(&self, _: &AtomicU32, _: u32, _: Option<Monotonic>) -> WaitResult {
        WaitResult::TimedOut
    }
    fn wake_one(&self, _: &AtomicU32) {}
    fn wake_all(&self, _: &AtomicU32) {}
}

impl PlatformInit for Clock {
    fn entropy(&self, out: &mut [u8]) -> Result<(), PlatformError> {
        out.fill(0x5A);
        Ok(())
    }
    unsafe fn prefault(&self, _: *mut u8, _: usize) -> Result<(), PlatformError> {
        Ok(())
    }
    fn runtime(&'static self) -> &'static dyn Platform {
        self
    }
}

/// One leaked, uninitialized MiB per setup. `Box::new_uninit_slice` writes
/// nothing; `vec![MaybeUninit::uninit(); len]` stores every element one at a time,
/// which under Miri is a million interpreted stores before a test starts.
pub fn memory() -> StartupMemory {
    let bytes: &'static mut [MaybeUninit<u8>] = Box::leak(Box::new_uninit_slice(1 << 20));
    StartupMemory::new(bytes).expect("memory")
}

/// A fixed uEntity identity.
pub struct Uri(pub u16);

impl LocalUriProvider for Uri {
    fn get_resource_uri(&self, resource_id: u16) -> UUri {
        UUri::try_from_wire_parts("local", (1 << 16) | u32::from(self.0), 1, resource_id)
            .expect("uri")
    }
    fn get_source_uri(&self) -> UUri {
        self.get_resource_uri(0)
    }
}

pub fn uri(entity: u16, resource: u16) -> UUri {
    Uri(entity).get_resource_uri(resource)
}

/// A UUIDv7 with the given millisecond timestamp.
pub fn uuid_at(millis: u64, counter: u64) -> UUID {
    UUID::try_from_wire_pair((millis << 16) | 0x7000, 0x8000_0000_0000_0000 | counter)
        .expect("uuid")
}

/// What a whole frame carries.
#[derive(Clone)]
pub struct Spec<'a> {
    pub kind: UMessageType,
    pub id: UUID,
    pub reqid: Option<UUID>,
    pub ttl: Option<Duration>,
    pub source: UUri,
    pub sink: Option<UUri>,
    pub token: Option<&'a str>,
    pub payload: Option<&'a [u8]>,
}

/// One `NativeWholeFrame` envelope around the fields and payload.
pub fn whole_frame(spec: &Spec<'_>) -> Vec<u8> {
    let priority = matches!(spec.kind, UMessageType::Request | UMessageType::Response)
        .then_some(UPriority::CS4);
    let parts = TxHeaderParts {
        kind: spec.kind,
        priority,
        id: &spec.id,
        reqid: spec.reqid.as_ref(),
        ttl: spec.ttl,
        comm_status: None,
        permission_level: None,
        source: &spec.source,
        sink: spec.sink.as_ref(),
        token: spec.token,
        traceparent: None,
        payload_encoding: spec.payload.map(|_| ENCODING),
        native_type_token: None,
    };
    let fields_len = header_fields_len(&parts).expect("fields length");
    let mut fields = vec![MaybeUninit::<u8>::uninit(); fields_len];
    write_field_block_uninit(&parts, &mut fields).expect("fields");
    let mut out = Vec::new();
    out.extend_from_slice(b"UPFE");
    out.push(1);
    out.push(u8::from(spec.payload.is_some()));
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&(fields_len as u32).to_le_bytes());
    out.extend_from_slice(&(spec.payload.map_or(0, <[u8]>::len) as u64).to_le_bytes());
    for byte in &fields {
        // SAFETY: `write_field_block_uninit` initialized every byte of `fields`.
        out.push(unsafe { byte.assume_init() });
    }
    out.extend_from_slice(spec.payload.unwrap_or_default());
    out
}

/// The header fields of a frame sent on a `NativeWholeFrame` route.
pub fn sent_request_id(sent: &[u8]) -> UUID {
    let len = u32::from_le_bytes(sent[8..12].try_into().unwrap()) as usize;
    let fields = &sent[20..20 + len];
    parse_field_block(fields, &HeaderLimits { max_token_len: 0 })
        .expect("parse")
        .id
}

/// A trivial `u32` wire with an observable decode hook.
#[derive(Clone, Copy)]
pub struct U32Wire {
    pub hook: Option<fn(&mut u32)>,
}

impl Wire<u32> for U32Wire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "u32",
            WIRE_ID,
            FAMILY_ID,
            WirePayloadIdentity::serialized(ENCODING),
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
        .max_encoded_len(4)
        .exact()
    }
    fn encoded_size(&self, _: &u32) -> EncodedSize {
        EncodedSize::Exact(4)
    }
    fn encode(&self, value: &u32, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        out.put(&value.to_le_bytes())
    }
    fn init_slot<'s>(&self, slot: StartupSlot<'s, u32>) -> Ready<'s, u32> {
        slot.write(0)
    }
    fn decode(&self, _: PayloadEncoding, bytes: &[u8], into: &mut u32) -> Result<(), WireError> {
        if let Some(hook) = self.hook {
            hook(into);
        }
        *into = u32::from_le_bytes(bytes.try_into().map_err(|_| WireError::InvalidBits)?);
        Ok(())
    }
}

/// State shared by the doubles.
pub struct Shared {
    pub port: OnceLock<RuntimePort>,
    pub inbound: Mutex<Vec<InboundRoute>>,
    pub sent: Mutex<Vec<Vec<u8>>>,
    pub queue: Mutex<VecDeque<(Vec<u8>, PeerRef)>>,
    /// While nonzero, `receive` yields `flood_frame` and decrements.
    pub flood: AtomicUsize,
    pub flood_frame: Mutex<(Vec<u8>, PeerRef)>,
    /// Heap frames that went back to this transport and were freed.
    pub frames_dropped: AtomicUsize,
}

impl Shared {
    pub fn leak() -> &'static Self {
        Box::leak(Box::new(Self {
            port: OnceLock::new(),
            inbound: Mutex::new(Vec::new()),
            sent: Mutex::new(Vec::new()),
            queue: Mutex::new(VecDeque::new()),
            flood: AtomicUsize::new(0),
            flood_frame: Mutex::new((Vec::new(), PeerRef::UNKNOWN)),
            frames_dropped: AtomicUsize::new(0),
        }))
    }
    pub fn port(&self) -> RuntimePort {
        *self.port.get().expect("running")
    }
    pub fn route(&self, index: usize) -> InboundRoute {
        self.inbound.lock().unwrap()[index]
    }
}

pub fn capabilities() -> Capabilities {
    Capabilities::new(Envelope::NativeWholeFrame, ENVELOPES, 1 << 20)
        .with_per_message_encoding(true)
        .with_truncate_payload(true)
        .with_declared_clients(false)
        .with_chunked_receive(false)
        .with_native_loans(false)
}

pub fn all_qos() -> QosSupport {
    QosSupport::new()
        .with_reliable(true)
        .with_drop_oldest(true)
        .with_drop_newest(true)
        .with_wait(true)
        .with_max_depth(64)
        .with_max_history(0)
        .with_priority_levels(0)
}

// ---------------------------------------------------------------- handoff --

pub struct HandoffDouble(pub &'static Shared);
pub struct HandoffRunning(pub &'static Shared);

impl BindingSetup for HandoffDouble {
    type Running = HandoffRunning;
    fn name(&self) -> &'static str {
        "handoff-double"
    }
    fn capabilities(&self) -> Capabilities {
        capabilities()
    }
    fn qos_support(&self, _: RouteKind) -> QosSupport {
        all_qos()
    }
    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        if let Some(inbound) = route.inbound {
            self.0.inbound.lock().unwrap().push(inbound);
        }
        Ok(PreparedRoute::new(BindingRoute(0), RuntimeHeap::None))
    }
    fn into_running(self, port: RuntimePort) -> Result<HandoffRunning, BindingRefusal> {
        let _ = self.0.port.set(port);
        Ok(HandoffRunning(self.0))
    }
}

impl HandoffRuntime for HandoffRunning {
    fn receive_bounded(
        &self, route: up_rust::two_mode::BindingRoute, budget: &up_rust::two_mode::ReceiveBudget,
        sink: &mut dyn FnMut(up_rust::two_mode::InboundFrame<'_>) -> up_rust::two_mode::Flow,
    ) -> usize {
        // Explicitly selected sink-visible adapter; no hidden filtering loop.
        if !budget.try_take() { return 0; }
        self.receive(route, &mut |frame| {
            let flow = sink(frame);
            if matches!(flow, up_rust::two_mode::Flow::Stop) || !budget.try_take() {
                up_rust::two_mode::Flow::Stop
            } else { up_rust::two_mode::Flow::Continue }
        })
    }

    fn send(&self, _: BindingRoute, frame: OutgoingFrame<'_>) -> Result<up_rust_core::transport::SendReport, up_rust_core::transport::HandoffFailure> {
        let mut all = frame.metadata.to_vec();
        for chunk in frame.payload.chunks() {
            all.extend_from_slice(chunk);
        }
        self.0.sent.lock().unwrap().push(all);
        Ok(up_rust_core::transport::SendReport::unknown())
    }

    fn receive(&self, _: BindingRoute, sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow) -> usize {
        let mut seen = 0;
        loop {
            let next = if self.0.flood.load(Ordering::SeqCst) > 0 {
                self.0.flood.fetch_sub(1, Ordering::SeqCst);
                Some(self.0.flood_frame.lock().unwrap().clone())
            } else {
                self.0.queue.lock().unwrap().pop_front()
            };
            let Some((frame, peer)) = next else {
                return seen;
            };
            seen += 1;
            let flow = sink(InboundFrame {
                metadata: &[],
                payload: PayloadBytes::one(&frame),
                peer,
                reply: ReplyToken::default(),
            });
            if matches!(flow, Flow::Stop) {
                return seen;
            }
        }
    }
}

// ---------------------------------------------------------------- lending --

/// A received frame whose bytes are on the heap, so a use after its drop is a
/// use after free that Miri reports.
pub struct HeapFrame {
    pub bytes: Vec<u8>,
    pub peer: PeerRef,
    pub shared: &'static Shared,
}

impl Drop for HeapFrame {
    fn drop(&mut self) {
        self.shared.frames_dropped.fetch_add(1, Ordering::SeqCst);
    }
}

impl Frame for HeapFrame {
    fn encoded_metadata(&self) -> &[u8] {
        &[]
    }
    fn payload(&self) -> Option<LoanedPayload<'_>> {
        Some(LoanedPayload::new(
            &self.bytes,
            PayloadLoanProvenance::OpaqueTransportLoan,
        ))
    }
    fn peer(&self) -> PeerRef {
        self.peer
    }
}

pub struct NoTx;

// SAFETY: this loan type is never constructed.
unsafe impl PreparedTxStorage for NoTx {
    fn metadata_uninit_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        &mut []
    }
    fn payload_uninit_mut(&mut self) -> Option<&mut [MaybeUninit<u8>]> {
        None
    }
    fn record_header(
        &mut self,
        _: up_rust_core::frame::header::WrittenHeader,
    ) -> Result<(), SendError> {
        Ok(())
    }
}

pub struct LendingDouble(pub &'static Shared);
pub struct LendingRunning(pub &'static Shared);

impl BindingSetup for LendingDouble {
    type Running = LendingRunning;
    fn name(&self) -> &'static str {
        "lending-double"
    }
    fn capabilities(&self) -> Capabilities {
        capabilities()
    }
    fn qos_support(&self, _: RouteKind) -> QosSupport {
        all_qos()
    }
    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        if let Some(inbound) = route.inbound {
            self.0.inbound.lock().unwrap().push(inbound);
        }
        Ok(PreparedRoute::new(BindingRoute(0), RuntimeHeap::None))
    }
    fn into_running(self, port: RuntimePort) -> Result<LendingRunning, BindingRefusal> {
        let _ = self.0.port.set(port);
        Ok(LendingRunning(self.0))
    }
}

impl LendingRuntime for LendingRunning {
    // Explicit single-attempt adapter, justified by this retained implementation.
    fn receive_bounded(&self, route: up_rust::two_mode::BindingRoute, budget: &up_rust::two_mode::ReceiveBudget) -> Option<Self::Rx> {
        if !budget.try_take() { return None; }
        self.receive(route)
    }

    type Tx<'a> = NoTx;
    type Rx = HeapFrame;

    fn loan(&self, _: BindingRoute, _: TxFrameLayout) -> Result<NoTx, LoanError> {
        Err(LoanError::InvalidRoute)
    }
    fn publish<'a>(
        &'a self,
        _: BindingRoute,
        _: InitializedTx<NoTx>,
    ) -> Result<SendReport, SendError> {
        Err(SendError::Delivery)
    }
    fn receive(&self, _: BindingRoute) -> Option<HeapFrame> {
        if self.0.flood.load(Ordering::SeqCst) > 0 {
            self.0.flood.fetch_sub(1, Ordering::SeqCst);
            let (bytes, peer) = self.0.flood_frame.lock().unwrap().clone();
            return Some(HeapFrame {
                bytes,
                peer,
                shared: self.0,
            });
        }
        self.0
            .queue
            .lock()
            .unwrap()
            .pop_front()
            .map(|(bytes, peer)| HeapFrame {
                bytes,
                peer,
                shared: self.0,
            })
    }
}

pub fn publish_frame(
    topic: &UUri,
    id: UUID,
    ttl: Option<Duration>,
    token: Option<&str>,
    payload: &[u8],
) -> Vec<u8> {
    whole_frame(&Spec {
        kind: UMessageType::Publish,
        id,
        reqid: None,
        ttl,
        source: topic.clone(),
        sink: None,
        token,
        payload: Some(payload),
    })
}

pub fn peer(id: u64, incarnation: u64) -> PeerRef {
    PeerRef {
        peer: id,
        incarnation,
    }
}

pub fn ok_code() -> UCode {
    UCode::Ok
}
