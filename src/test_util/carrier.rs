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

//! Test doubles for carrier routes. Not for production.
//!
//! [`ScriptedHandoff`] is a handoff binding whose receive side plays back a fixed
//! script of frames, chunked if asked, through the pull form, and whose send side
//! keeps the last frame in fixed storage, so a window that counts heap events
//! sees none. Frames in a script are leaked before a window opens.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

#[cfg(up_rust_test_hooks)]
use up_rust_core::frame::header::TxHeaderParts;
use up_rust_core::transport::binding::{
    BindingRefusal, BindingRoute, BindingSetup, Capabilities, Flow, HandoffRuntime, InboundFrame,
    InboundRoute, OutgoingFrame, PayloadBytes, PeerRef, PreparedRoute, ReplyToken,
    RouteDeclaration, RouteKind, RuntimePort, SendRefusal,
};
use up_rust_core::transport::envelope::Envelope;
use up_rust_core::transport::error::RouteDirection;
use up_rust_core::transport::qos::QosSupport;
use up_rust_core::uattributes::UMessageType;
use up_rust_core::uri::UUri;
#[cfg(up_rust_test_hooks)]
use up_rust_core::uuid::UUID;
use up_rust_core::wire::RuntimeHeap;
#[cfg(up_rust_test_hooks)]
use up_rust_core::wire::{Wire, WireInfo};

#[cfg(up_rust_test_hooks)]
use super::wires::{DoorState, DoorWire, DOOR_ENCODING};
#[cfg(up_rust_test_hooks)]
use super::EnvelopeHarness;
#[cfg(up_rust_test_hooks)]
use crate::prelude::UuidExt as _;

const ENVELOPES: &[Envelope] = &[Envelope::UFrameFields, Envelope::NativeWholeFrame];
const CAPTURE_METADATA: usize = 1024;
const CAPTURE_PAYLOAD: usize = 64 * 1024;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One frame of a script. Static, so a window sees no heap event.
#[derive(Clone, Copy, Debug)]
pub struct ScriptedFrame {
    /// Encoded metadata, or empty for a whole-frame envelope.
    pub metadata: &'static [u8],
    /// Payload chunks, or the whole frame in one chunk.
    pub chunks: &'static [&'static [u8]],
    /// `false` for a frame with no payload at all.
    pub present: bool,
    /// Who sent it.
    pub peer: PeerRef,
    /// Correlation to carry through.
    pub reply: ReplyToken,
}

impl ScriptedFrame {
    /// A contiguous frame with `metadata` and one payload chunk.
    #[must_use]
    pub fn contiguous(metadata: Vec<u8>, payload: Vec<u8>) -> Self {
        let metadata: &'static [u8] = Box::leak(metadata.into_boxed_slice());
        let payload: &'static [u8] = Box::leak(payload.into_boxed_slice());
        let chunks: &'static [&'static [u8]] = Box::leak(vec![payload].into_boxed_slice());
        Self {
            metadata,
            chunks,
            present: true,
            peer: PeerRef::default(),
            reply: ReplyToken::default(),
        }
    }

    /// The same frame, with its payload split into chunks at `at`.
    #[must_use]
    pub fn chunked(metadata: Vec<u8>, payload: Vec<u8>, at: usize) -> Self {
        let metadata: &'static [u8] = Box::leak(metadata.into_boxed_slice());
        let payload: &'static [u8] = Box::leak(payload.into_boxed_slice());
        let (first, rest) = payload.split_at(at.min(payload.len()));
        let chunks: &'static [&'static [u8]] = Box::leak(vec![first, rest].into_boxed_slice());
        Self {
            metadata,
            chunks,
            present: true,
            peer: PeerRef::default(),
            reply: ReplyToken::default(),
        }
    }

    /// A frame with a whole-frame envelope in its one chunk.
    #[must_use]
    pub fn whole(frame: Vec<u8>) -> Self {
        Self::contiguous(Vec::new(), frame)
    }
}

/// The last frame a [`ScriptedHandoff`] was asked to send.
#[derive(Debug)]
struct CaptureCell {
    metadata: [u8; CAPTURE_METADATA],
    metadata_len: usize,
    metadata_ptr: usize,
    payload: [u8; CAPTURE_PAYLOAD],
    payload_len: usize,
    payload_present: bool,
    first_chunk_ptr: usize,
    chunks: usize,
    kind: Option<UMessageType>,
    reply: ReplyToken,
}

impl CaptureCell {
    const fn new() -> Self {
        Self {
            metadata: [0; CAPTURE_METADATA],
            metadata_len: 0,
            metadata_ptr: 0,
            payload: [0; CAPTURE_PAYLOAD],
            payload_len: 0,
            payload_present: false,
            first_chunk_ptr: 0,
            chunks: 0,
            kind: None,
            reply: ReplyToken(0),
        }
    }
}

/// One route as declared to a [`ScriptedHandoff`]: direction, kind, source, sink, slots.
pub type DeclaredRoute = (
    RouteDirection,
    Option<UMessageType>,
    UUri,
    Option<UUri>,
    usize,
);

struct ScriptShared {
    frames: Mutex<(Vec<ScriptedFrame>, usize)>,
    capture: Mutex<CaptureCell>,
    sends: AtomicUsize,
    busy_sends: AtomicU32,
    has_capacity: AtomicBool,
    refuse_carrier_kinds: AtomicBool,
    routes: Mutex<Vec<DeclaredRoute>>,
    port: OnceLock<RuntimePort>,
    inbound: OnceLock<InboundRoute>,
}

impl std::fmt::Debug for ScriptShared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptShared").finish_non_exhaustive()
    }
}

/// A handle onto a [`ScriptedHandoff`], for scripting it and reading what it sent.
#[derive(Clone, Copy, Debug)]
pub struct ScriptedControl {
    shared: &'static ScriptShared,
}

impl ScriptedControl {
    /// Queues `frames` to play back on the receive side, in order.
    pub fn script(&self, frames: impl IntoIterator<Item = ScriptedFrame>) {
        let mut guard = lock(&self.shared.frames);
        let (script, _) = &mut *guard;
        script.extend(frames);
    }

    /// Makes `has_capacity` answer `value` from now on.
    pub fn set_capacity(&self, value: bool) {
        self.shared.has_capacity.store(value, Ordering::SeqCst);
    }

    /// Refuses the next `count` sends with `SendRefusal::Busy`.
    pub fn refuse_sends(&self, count: u32) {
        self.shared.busy_sends.store(count, Ordering::SeqCst);
    }

    /// Makes `prepare_route` refuse a route with no message kind, as a binding
    /// that names one service per kind does.
    pub fn refuse_carrier_kinds(&self) {
        self.shared
            .refuse_carrier_kinds
            .store(true, Ordering::SeqCst);
    }

    /// How many frames this binding has been asked to send.
    #[must_use]
    pub fn sends(&self) -> usize {
        self.shared.sends.load(Ordering::SeqCst)
    }

    /// The last frame's metadata, as this binding received it.
    #[must_use]
    pub fn last_metadata(&self) -> Vec<u8> {
        let capture = lock(&self.shared.capture);
        capture
            .metadata
            .get(..capture.metadata_len)
            .unwrap_or_default()
            .to_vec()
    }

    /// The last frame's payload, as this binding received it.
    #[must_use]
    pub fn last_payload(&self) -> Option<Vec<u8>> {
        let capture = lock(&self.shared.capture);
        capture.payload_present.then(|| {
            capture
                .payload
                .get(..capture.payload_len)
                .unwrap_or_default()
                .to_vec()
        })
    }

    /// Where the last frame's metadata and first payload chunk were, as pointers.
    #[must_use]
    pub fn last_pointers(&self) -> (usize, usize) {
        let capture = lock(&self.shared.capture);
        (capture.metadata_ptr, capture.first_chunk_ptr)
    }

    /// How many payload chunks the last frame arrived in.
    #[must_use]
    pub fn last_chunks(&self) -> usize {
        lock(&self.shared.capture).chunks
    }

    /// The message kind of the last frame.
    #[must_use]
    pub fn last_kind(&self) -> Option<UMessageType> {
        lock(&self.shared.capture).kind
    }

    /// The reply token of the last frame.
    #[must_use]
    pub fn last_reply(&self) -> ReplyToken {
        lock(&self.shared.capture).reply
    }

    /// Every route declared to this binding, in order.
    #[must_use]
    pub fn routes(&self) -> Vec<DeclaredRoute> {
        lock(&self.shared.routes).clone()
    }

    /// Delivers one frame straight through the port, as a push binding would.
    ///
    /// # Errors
    /// The port's refusal.
    pub fn deliver_borrowed(
        &self,
        metadata: &[u8],
        payload: &[u8],
    ) -> Result<(), up_rust_core::transport::error::ReceiveError> {
        let port = self
            .shared
            .port
            .get()
            .ok_or(up_rust_core::transport::error::ReceiveError::UnknownRoute)?;
        let inbound = self
            .shared
            .inbound
            .get()
            .ok_or(up_rust_core::transport::error::ReceiveError::UnknownRoute)?;
        port.deliver(
            *inbound,
            InboundFrame {
                metadata,
                payload: PayloadBytes::one(payload),
                peer: PeerRef::default(),
                reply: ReplyToken::default(),
            },
        )
    }
}

/// A handoff binding playing back a script and capturing what it sends.
#[derive(Debug)]
pub struct ScriptedHandoff {
    shared: &'static ScriptShared,
    next_route: u32,
    envelope: Envelope,
}

impl ScriptedHandoff {
    /// A binding and its control handle. `envelope` is its default.
    #[must_use]
    pub fn new(envelope: Envelope) -> (Self, ScriptedControl) {
        let shared: &'static ScriptShared = Box::leak(Box::new(ScriptShared {
            frames: Mutex::new((Vec::with_capacity(256), 0)),
            capture: Mutex::new(CaptureCell::new()),
            sends: AtomicUsize::new(0),
            busy_sends: AtomicU32::new(0),
            has_capacity: AtomicBool::new(true),
            refuse_carrier_kinds: AtomicBool::new(false),
            routes: Mutex::new(Vec::with_capacity(8)),
            port: OnceLock::new(),
            inbound: OnceLock::new(),
        }));
        // Every lazy primitive is forced here, on a pthread target too.
        drop(lock(&shared.frames));
        drop(lock(&shared.capture));
        drop(lock(&shared.routes));
        (
            Self {
                shared,
                next_route: 0,
                envelope,
            },
            ScriptedControl { shared },
        )
    }
}

impl BindingSetup for ScriptedHandoff {
    type Running = ScriptedRunning;

    fn name(&self) -> &'static str {
        "scripted-handoff"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::new(self.envelope, ENVELOPES, 128 * 1024)
            .with_per_message_encoding(true)
            .with_truncate_payload(true)
            .with_declared_clients(false)
            .with_chunked_receive(true)
            .with_native_loans(false)
    }

    fn qos_support(&self, _kind: RouteKind) -> QosSupport {
        QosSupport::new()
            .with_reliable(true)
            .with_drop_oldest(true)
            .with_drop_newest(true)
            .with_wait(false)
            .with_max_depth(1024)
            .with_max_history(0)
            .with_priority_levels(0)
    }

    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        if route.kind.is_none() && self.shared.refuse_carrier_kinds.load(Ordering::SeqCst) {
            return Err(BindingRefusal(
                "this binding names one service per kind and cannot carry every kind",
            ));
        }
        lock(&self.shared.routes).push((
            route.direction,
            route.kind,
            route.source.clone(),
            route.sink.cloned(),
            route.slots,
        ));
        if route.direction == RouteDirection::Receive {
            if let Some(inbound) = route.inbound {
                let _ = self.shared.inbound.set(inbound);
            }
        }
        let binding = BindingRoute(self.next_route);
        self.next_route += 1;
        Ok(PreparedRoute::new(binding, RuntimeHeap::None))
    }

    fn into_running(self, port: RuntimePort) -> Result<ScriptedRunning, BindingRefusal> {
        let _ = self.shared.port.set(port);
        Ok(ScriptedRunning {
            shared: self.shared,
        })
    }
}

/// A [`ScriptedHandoff`] in runtime mode.
#[derive(Debug)]
pub struct ScriptedRunning {
    shared: &'static ScriptShared,
}

impl HandoffRuntime for ScriptedRunning {
    fn receive_bounded(
        &self, route: crate::two_mode::BindingRoute, budget: &crate::two_mode::ReceiveBudget,
        sink: &mut dyn FnMut(crate::two_mode::InboundFrame<'_>) -> crate::two_mode::Flow,
    ) -> usize {
        // Explicitly selected sink-visible adapter; no hidden filtering loop.
        if !budget.try_take() { return 0; }
        self.receive(route, &mut |frame| {
            let flow = sink(frame);
            if matches!(flow, crate::two_mode::Flow::Stop) || !budget.try_take() {
                crate::two_mode::Flow::Stop
            } else { crate::two_mode::Flow::Continue }
        })
    }

    fn send(&self, _route: BindingRoute, frame: OutgoingFrame<'_>) -> Result<up_rust_core::transport::SendReport, up_rust_core::transport::HandoffFailure> {
        let busy = self.shared.busy_sends.load(Ordering::SeqCst);
        if busy > 0 {
            self.shared.busy_sends.store(busy - 1, Ordering::SeqCst);
            return Err(up_rust_core::transport::HandoffFailure::before_send(SendRefusal::Busy));
        }
        let mut capture = lock(&self.shared.capture);
        let capture = &mut *capture;
        capture.metadata_ptr = frame.metadata.as_ptr() as usize;
        capture.metadata_len = frame.metadata.len();
        if let Some(out) = capture.metadata.get_mut(..frame.metadata.len()) {
            out.copy_from_slice(frame.metadata);
        }
        let mut written = 0_usize;
        let mut chunks = 0_usize;
        capture.first_chunk_ptr = frame
            .payload
            .chunks()
            .next()
            .map_or(0, |chunk| chunk.as_ptr() as usize);
        for chunk in frame.payload.chunks() {
            let end = written + chunk.len();
            if let Some(out) = capture.payload.get_mut(written..end) {
                out.copy_from_slice(chunk);
            }
            written = end;
            chunks += 1;
        }
        capture.payload_len = written;
        capture.payload_present = frame.payload.is_present();
        capture.chunks = chunks;
        capture.kind = Some(frame.kind);
        capture.reply = frame.reply;
        self.shared.sends.fetch_add(1, Ordering::SeqCst);
        Ok(up_rust_core::transport::SendReport::unknown())
    }

    fn receive(
        &self,
        _route: BindingRoute,
        sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow,
    ) -> usize {
        let mut count = 0_usize;
        loop {
            let frame = {
                let mut guard = lock(&self.shared.frames);
                let (script, cursor) = &mut *guard;
                let Some(frame) = script.get(*cursor).copied() else {
                    return count;
                };
                *cursor += 1;
                frame
            };
            let (first, rest) = match frame.chunks.split_first() {
                Some((first, rest)) => (*first, rest),
                None => (&[][..], &[][..]),
            };
            let payload = if frame.present {
                PayloadBytes::Present { first, rest }
            } else {
                PayloadBytes::Absent
            };
            count += 1;
            let flow = sink(InboundFrame {
                metadata: frame.metadata,
                payload,
                peer: frame.peer,
                reply: frame.reply,
            });
            if matches!(flow, Flow::Stop) {
                return count;
            }
        }
    }

    fn has_capacity(&self, _route: BindingRoute) -> bool {
        self.shared.has_capacity.load(Ordering::SeqCst)
    }
}

/// Encodes one door frame's head and payload under `wire`, as a transport carries them.
///
/// # Panics
/// If the frame cannot be encoded; this is test setup.
#[must_use]
#[cfg(up_rust_test_hooks)]
pub fn frame_of(
    envelope: Envelope,
    wire: &WireInfo,
    source: &UUri,
    door_id: u32,
    ttl_ms: Option<u64>,
) -> (Vec<u8>, Vec<u8>) {
    let harness = EnvelopeHarness::new(envelope, wire.wire_id, wire.payload_family, wire.identity);
    let id = UUID::build();
    let parts = TxHeaderParts {
        kind: UMessageType::Publish,
        priority: None,
        id: &id,
        reqid: None,
        ttl: ttl_ms.map(std::time::Duration::from_millis),
        comm_status: None,
        permission_level: None,
        source,
        sink: None,
        token: None,
        traceparent: None,
        payload_encoding: Some(wire.identity.encoding),
        native_type_token: wire.identity.native_type_token,
    };
    let mut payload = door_id.to_be_bytes().to_vec();
    payload.push(1);
    let (head, bytes) = harness.encode(&parts, Some(&payload)).expect("frame");
    (
        bytes.get(..head).unwrap_or_default().to_vec(),
        bytes.get(head..).unwrap_or_default().to_vec(),
    )
}

/// Encodes one [`DoorWire`] frame.
///
/// # Panics
/// If the frame cannot be encoded; this is test setup.
#[must_use]
#[cfg(up_rust_test_hooks)]
pub fn door_frame(
    envelope: Envelope,
    source: &UUri,
    door_id: u32,
    ttl_ms: Option<u64>,
) -> (Vec<u8>, Vec<u8>) {
    let wire = Wire::<DoorState>::describe(&DoorWire);
    debug_assert_eq!(wire.identity.encoding, DOOR_ENCODING);
    frame_of(envelope, &wire, source, door_id, ttl_ms)
}
