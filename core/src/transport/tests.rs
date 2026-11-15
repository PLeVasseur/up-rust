/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

//! Doubles for white-box tests of state the public API cannot show. `core` only,
//! because this crate is `no_std` even under `cfg(test)`.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use core::mem::MaybeUninit;
use crate::platform::atomic::AtomicU32;

use crate::communication::LocalUriProvider;
use crate::frame::header::{header_fields_len, write_field_block_uninit, TxHeaderParts};
use crate::memory::StartupMemory;
use crate::platform::{Monotonic, Platform, PlatformError, PlatformInit, WaitResult};
use crate::transport::binding::{
    BindingRefusal, BindingRoute, BindingSetup, Capabilities, HandoffRuntime, LendingRuntime,
    OutgoingFrame, PeerRef, PreparedRoute, RouteDeclaration, RouteKind, RuntimePort,
};
use crate::transport::qos::{QosSupport, SendReport};
use crate::transport::{
    Envelope, Frame, InitializedTx, LoanError, LoanedPayload, PayloadLoanProvenance,
    PreparedTxStorage, SendError, TxFrameLayout,
};
use crate::uattributes::{PayloadEncoding, UMessageType};
use crate::uri::UUri;
use crate::uuid::UUID;
use crate::wire::{
    EncodedSize, PayloadWriter, Ready, RuntimeHeap, StartupSlot, Wire, WireError, WireIdentity,
    WireInfo, WirePayloadIdentity,
};

pub(crate) const ENCODING: PayloadEncoding = PayloadEncoding::private_use(0xF001);
pub(crate) const WIRE_ID: WireIdentity = WireIdentity::new("test.u32", 0);
pub(crate) const FAMILY_ID: WireIdentity = WireIdentity::new("test.u32.family", 0);
pub(crate) const WALL_MS: u64 = 2_000_000_000_000;
const ENVELOPES: &[Envelope] = &[Envelope::NativeWholeFrame];

/// A fixed clock.
pub(crate) struct TestClock;

/// The one instance: `TransportSetup` wants a `&'static dyn PlatformInit`.
pub(crate) static CLOCK: TestClock = TestClock;

impl Platform for TestClock {
    fn now(&self) -> Monotonic {
        Monotonic(1_000_000_000)
    }
    fn unix_time_ms(&self) -> Option<u64> {
        Some(WALL_MS)
    }
    fn wait(&self, _: &AtomicU32, _: u32, _: Option<Monotonic>) -> WaitResult {
        WaitResult::TimedOut
    }
    fn wake_one(&self, _: &AtomicU32) {}
    fn wake_all(&self, _: &AtomicU32) {}
}

impl PlatformInit for TestClock {
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

/// A fixed uEntity identity.
pub(crate) struct TestUri(pub(crate) u16);

impl LocalUriProvider for TestUri {
    fn get_resource_uri(&self, resource_id: u16) -> UUri {
        UUri::try_from_wire_parts("local", (1 << 16) | u32::from(self.0), 1, resource_id)
            .expect("uri")
    }
    fn get_source_uri(&self) -> UUri {
        self.get_resource_uri(0)
    }
}

/// One `NativeWholeFrame` envelope around a publish frame, in startup memory.
pub(crate) fn whole_frame(memory: &StartupMemory, topic: &UUri, payload: &[u8]) -> &'static [u8] {
    let id =
        UUID::try_from_wire_pair((WALL_MS << 16) | 0x7000, 0x8000_0000_0000_0001).expect("uuid");
    let parts = TxHeaderParts {
        kind: UMessageType::Publish,
        priority: None,
        id: &id,
        reqid: None,
        ttl: None,
        comm_status: None,
        permission_level: None,
        source: topic,
        sink: None,
        token: None,
        traceparent: None,
        payload_encoding: Some(ENCODING),
        native_type_token: None,
    };
    let fields_len = header_fields_len(&parts).expect("fields length");
    let mut fields = [MaybeUninit::<u8>::uninit(); 256];
    write_field_block_uninit(&parts, &mut fields[..fields_len]).expect("fields");
    let total = 20 + fields_len + payload.len();
    let bytes = memory.bytes(total).expect("frame bytes");
    for slot in bytes.iter_mut() {
        slot.write(0);
    }
    let mut at = 0;
    let mut put = |piece: &[u8], at: &mut usize| {
        for byte in piece {
            bytes[*at].write(*byte);
            *at += 1;
        }
    };
    put(b"UPFE", &mut at);
    put(&[1, 1, 0, 0], &mut at);
    put(&u32::try_from(fields_len).unwrap().to_le_bytes(), &mut at);
    put(&(payload.len() as u64).to_le_bytes(), &mut at);
    // SAFETY: `write_field_block_uninit` initialized `fields_len` bytes of `fields`.
    let fields: &[u8] =
        unsafe { core::slice::from_raw_parts(fields.as_ptr().cast::<u8>(), fields_len) };
    put(fields, &mut at);
    put(payload, &mut at);
    assert_eq!(at, total);
    // SAFETY: every byte was just initialized above; the memory is `'static` and
    // nothing else writes it.
    unsafe { &*(core::ptr::from_ref(&*bytes) as *const [u8]) }
}

/// A trivial `u32` wire, whose encode can fail on demand.
#[derive(Clone, Copy)]
pub(crate) struct TestWire {
    pub(crate) fail_encode: bool,
}

impl Wire<u32> for TestWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "test-u32",
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
        if self.fail_encode {
            return Err(WireError::Unencodable("test wire refuses to encode"));
        }
        out.put(&value.to_le_bytes())
    }
    fn init_slot<'s>(&self, slot: StartupSlot<'s, u32>) -> Ready<'s, u32> {
        slot.write(0)
    }
    fn decode(&self, _: PayloadEncoding, bytes: &[u8], into: &mut u32) -> Result<(), WireError> {
        *into = u32::from_le_bytes(bytes.try_into().map_err(|_| WireError::InvalidBits)?);
        Ok(())
    }
}

fn capabilities() -> Capabilities {
    Capabilities::new(Envelope::NativeWholeFrame, ENVELOPES, 1 << 20)
        .with_per_message_encoding(true)
        .with_truncate_payload(true)
}

fn qos_support() -> QosSupport {
    QosSupport::new()
        .with_reliable(true)
        .with_drop_oldest(true)
        .with_drop_newest(true)
        .with_wait(true)
        .with_max_depth(64)
}

/// A handoff binding that accepts every send. Nothing is kept.
pub(crate) struct NullHandoff;

impl BindingSetup for NullHandoff {
    type Running = NullHandoff;
    fn name(&self) -> &'static str {
        "null-handoff"
    }
    fn capabilities(&self) -> Capabilities {
        capabilities()
    }
    fn qos_support(&self, _: RouteKind) -> QosSupport {
        qos_support()
    }
    fn prepare_route(&mut self, _: &RouteDeclaration<'_>) -> Result<PreparedRoute, BindingRefusal> {
        Ok(PreparedRoute::new(BindingRoute(0), RuntimeHeap::None))
    }
    fn into_running(self, _: RuntimePort) -> Result<NullHandoff, BindingRefusal> {
        Ok(NullHandoff)
    }
}

impl HandoffRuntime for NullHandoff {
    fn receive_bounded(
        &self, route: crate::transport::BindingRoute, budget: &crate::transport::ReceiveBudget,
        sink: &mut dyn FnMut(crate::transport::InboundFrame<'_>) -> crate::transport::Flow,
    ) -> usize {
        // Explicitly selected sink-visible adapter; no hidden filtering loop.
        if !budget.try_take() { return 0; }
        self.receive(route, &mut |frame| {
            let flow = sink(frame);
            if matches!(flow, crate::transport::Flow::Stop) || !budget.try_take() {
                crate::transport::Flow::Stop
            } else { crate::transport::Flow::Continue }
        })
    }

    fn send(&self, _: BindingRoute, _: OutgoingFrame<'_>) -> Result<crate::transport::SendReport, crate::transport::HandoffFailure> {
        Ok(crate::transport::SendReport::exact(0, 0))
    }
}

/// A frame held by reference: a `'static` buffer from startup memory.
pub(crate) struct StaticFrame {
    pub(crate) bytes: &'static [u8],
    pub(crate) peer: PeerRef,
}

impl Frame for StaticFrame {
    fn encoded_metadata(&self) -> &[u8] {
        &[]
    }
    fn payload(&self) -> Option<LoanedPayload<'_>> {
        Some(LoanedPayload::new(
            self.bytes,
            PayloadLoanProvenance::OpaqueTransportLoan,
        ))
    }
    fn peer(&self) -> PeerRef {
        self.peer
    }
}

/// A lending binding that lends nothing and pulls nothing: frames are pushed.
pub(crate) struct PushLending;

pub(crate) struct NoLoan;

// SAFETY: this type is never constructed, so these views are never used.
unsafe impl PreparedTxStorage for NoLoan {
    fn metadata_uninit_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        &mut []
    }
    fn payload_uninit_mut(&mut self) -> Option<&mut [MaybeUninit<u8>]> {
        None
    }
    fn record_header(&mut self, _: crate::frame::header::WrittenHeader) -> Result<(), SendError> {
        Ok(())
    }
}

impl BindingSetup for PushLending {
    type Running = PushLending;
    fn name(&self) -> &'static str {
        "push-lending"
    }
    fn capabilities(&self) -> Capabilities {
        capabilities()
    }
    fn qos_support(&self, _: RouteKind) -> QosSupport {
        qos_support()
    }
    fn prepare_route(&mut self, _: &RouteDeclaration<'_>) -> Result<PreparedRoute, BindingRefusal> {
        Ok(PreparedRoute::new(BindingRoute(0), RuntimeHeap::None))
    }
    fn into_running(self, _: RuntimePort) -> Result<PushLending, BindingRefusal> {
        Ok(PushLending)
    }
}

impl LendingRuntime for PushLending {
    // Explicit single-attempt adapter, justified by this retained implementation.
    fn receive_bounded(&self, route: crate::transport::BindingRoute, budget: &crate::transport::ReceiveBudget) -> Option<Self::Rx> {
        if !budget.try_take() { return None; }
        self.receive(route)
    }

    type Tx<'a> = NoLoan;
    type Rx = StaticFrame;

    fn loan(&self, _: BindingRoute, _: TxFrameLayout) -> Result<NoLoan, LoanError> {
        Err(LoanError::InvalidRoute)
    }
    fn publish(&self, _: BindingRoute, _: InitializedTx<NoLoan>) -> Result<SendReport, SendError> {
        Err(SendError::Delivery)
    }
    fn receive(&self, _: BindingRoute) -> Option<StaticFrame> {
        None
    }
}
// Unit-fixture-only access for actual worker qualification. No production hook
// or public timing-admission bypass is exported.
pub(crate) use super::{deadline_monitor::MonitorConfig, deadline_timing::ArmedDeadline,
    metadata::CoherentDeadline};
