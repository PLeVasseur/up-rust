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

//! An in-memory handoff binding, for contract tests. Not for production.
//!
//! An [`InMemoryBus`] connects any number of participants, each a
//! [`TransportSetup`](crate::two_mode::TransportSetup) built from [`InMemoryBus::participant`]. Frames are copied
//! once, at handoff, into the queue of each matching receive route, and delivered
//! on `try_recv`, or for an RPC response by `TransportRuntime::pump` too. A pull
//! binding's queue is the route's queue, so each one holds at most its route's
//! depth and applies its overflow policy: `DropOldest` discards the oldest queued
//! frame, `DropNewest` the arriving one, and each discard is reported to the SDK
//! (`RuntimePort::dropped`). A receive route deeper than the bus's `frames` is
//! refused at setup, since its queue could not hold its depth; a send route's
//! depth sizes no queue here, so any is taken. `Busy` means the medium itself
//! is full: [`InMemoryBus::with_medium`] bounds the frames in flight. The bus gives
//! a readiness signal and can kill or restart a peer. Every mutex here is locked
//! once when created, so a first lock on a pthread target cannot land on a runtime
//! path.

use up_rust_core::transport::{HandoffFailure, SendReport};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use up_rust_core::transport::binding::{
    BindingRefusal, BindingRoute, BindingSetup, Capabilities, Flow, HandoffRuntime, InboundFrame,
    InboundRoute, OutgoingFrame, PayloadBytes, PeerRef, PreparedRoute, ReplyToken,
    RouteDeclaration, RouteKind, RuntimePort, SendRefusal,
};
use up_rust_core::transport::envelope::Envelope;
use up_rust_core::transport::error::RouteDirection;
use up_rust_core::transport::qos::{Overflow, QosSupport};
use up_rust_core::transport::waiver::RuntimePermit;
use up_rust_core::uattributes::UMessageType;
use up_rust_core::ucode::UCode;
use up_rust_core::uri::UUri;
use up_rust_core::wire::RuntimeHeap;

const ENVELOPES: &[Envelope] = &[Envelope::UFrameFields, Envelope::NativeWholeFrame];

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct QueuedFrame {
    buffer: usize,
    metadata_len: usize,
    payload_len: Option<usize>,
    peer: PeerRef,
    reply: ReplyToken,
}

struct QueueState {
    buffers: Vec<Box<[u8]>>,
    free: Vec<usize>,
    queued: VecDeque<QueuedFrame>,
}

/// One receive route of one participant.
struct RxEntry {
    participant: u64,
    binding: BindingRoute,
    inbound: InboundRoute,
    kind: Option<UMessageType>,
    source_filter: UUri,
    sink_filter: Option<UUri>,
    /// A whole-frame envelope is delivered as one buffer, with no separate metadata.
    whole_frame: bool,
    /// The route's depth: this queue holds at most this many frames.
    depth: usize,
    /// The route's overflow policy, applied here when the queue is at depth.
    overflow: Overflow,
    queue: Mutex<QueueState>,
}

impl RxEntry {
    /// Whether a send to this route would be accepted now: within its depth with
    /// a buffer free, or at depth under a policy that makes room by dropping.
    fn accepts(&self, queue: &QueueState) -> bool {
        if queue.queued.len() >= self.depth {
            !matches!(self.overflow, Overflow::Wait { .. })
        } else {
            !queue.free.is_empty()
        }
    }
}

struct TxDecl {
    route: BindingRoute,
    kind: Option<UMessageType>,
    source: UUri,
    sink: Option<UUri>,
}

struct Participant {
    peer_id: u64,
    incarnation: AtomicU64,
    alive: AtomicBool,
    port: OnceLock<RuntimePort>,
    tx_routes: Mutex<Vec<TxDecl>>,
}

struct BusShared {
    frames: usize,
    frame_len: usize,
    /// Frames the medium holds at once, each queued copy counted; `None` for no
    /// limit beyond the routes' own queues.
    medium: Option<usize>,
    in_flight: AtomicUsize,
    rx: Mutex<Vec<&'static RxEntry>>,
    participants: Mutex<Vec<&'static Participant>>,
}

impl BusShared {
    /// Takes the medium's place for one queued frame; false when it is full.
    fn enter_medium(&self) -> bool {
        let Some(medium) = self.medium else {
            return true;
        };
        let mut now = self.in_flight.load(Ordering::SeqCst);
        loop {
            if now >= medium {
                return false;
            }
            match self.in_flight.compare_exchange_weak(
                now,
                now + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return true,
                Err(actual) => now = actual,
            }
        }
    }

    /// Counts a frame placed in the medium past its limit (`inject`).
    fn force_enter_medium(&self) {
        if self.medium.is_some() {
            self.in_flight.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Gives back a frame's place in the medium.
    fn leave_medium(&self) {
        if self.medium.is_some() {
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Whether one more frame would find room in the medium now.
    fn medium_has_room(&self) -> bool {
        self.medium
            .is_none_or(|medium| self.in_flight.load(Ordering::SeqCst) < medium)
    }
}

/// An in-memory medium connecting several participants.
#[derive(Clone, Copy, Debug)]
pub struct InMemoryBus {
    shared: &'static BusShared,
}

impl std::fmt::Debug for BusShared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BusShared").finish_non_exhaustive()
    }
}

impl InMemoryBus {
    /// Creates a bus whose receive queues hold `frames` frames of `frame_len` bytes,
    /// so it takes receive routes up to `frames` deep and refuses deeper ones at
    /// setup. A send route may have any depth: it has no queue here.
    #[must_use]
    pub fn new(frames: usize, frame_len: usize) -> Self {
        Self::build(frames, frame_len, None)
    }

    /// As [`Self::new`], over a medium that holds at most `medium` frames in flight,
    /// each queued copy counted. A send beyond that is refused `Busy`, and
    /// `has_capacity` answers false, until a receiver takes a frame: the transport's
    /// own limit, apart from any route's depth.
    #[must_use]
    pub fn with_medium(frames: usize, frame_len: usize, medium: usize) -> Self {
        Self::build(frames, frame_len, Some(medium))
    }

    fn build(frames: usize, frame_len: usize, medium: Option<usize>) -> Self {
        let shared: &'static BusShared = Box::leak(Box::new(BusShared {
            frames,
            frame_len,
            medium,
            in_flight: AtomicUsize::new(0),
            rx: Mutex::new(Vec::new()),
            participants: Mutex::new(Vec::new()),
        }));
        // On pthread targets a mutex allocates on first lock, so each one is
        // forced here, during initialization, as it is created.
        drop(lock(&shared.rx));
        drop(lock(&shared.participants));
        Self { shared }
    }

    /// Creates a participant identified to its peers as `peer_id`.
    #[must_use]
    pub fn participant(&self, peer_id: u64) -> InMemoryParticipant {
        let participant: &'static Participant = Box::leak(Box::new(Participant {
            peer_id,
            incarnation: AtomicU64::new(1),
            alive: AtomicBool::new(true),
            port: OnceLock::new(),
            tx_routes: Mutex::new(Vec::with_capacity(64)),
        }));
        drop(lock(&participant.tx_routes));
        lock(&self.shared.participants).push(participant);
        InMemoryParticipant {
            shared: self.shared,
            participant,
            options: InMemoryOptions::default(),
            next_route: 0,
        }
    }

    fn find(&self, peer_id: u64) -> Option<&'static Participant> {
        lock(&self.shared.participants)
            .iter()
            .copied()
            .find(|participant| participant.peer_id == peer_id)
    }

    /// Stops a participant's traffic, as if the process had died.
    pub fn kill(&self, peer_id: u64) {
        if let Some(participant) = self.find(peer_id) {
            participant.alive.store(false, Ordering::SeqCst);
        }
    }

    /// Restarts a participant under a new incarnation, and returns it.
    pub fn restart(&self, peer_id: u64) -> u64 {
        match self.find(peer_id) {
            Some(participant) => {
                participant.alive.store(true, Ordering::SeqCst);
                participant.incarnation.fetch_add(1, Ordering::SeqCst) + 1
            }
            None => 0,
        }
    }

    /// Stamps a participant's later frames with this incarnation, even an older
    /// one, to model a frame from a previous incarnation still in transit.
    pub fn set_incarnation(&self, peer_id: u64, incarnation: u64) {
        if let Some(participant) = self.find(peer_id) {
            participant.incarnation.store(incarnation, Ordering::SeqCst);
        }
    }

    /// Places raw bytes on every receive route of one participant, for malformed-input tests.
    pub fn inject(&self, peer_id: u64, metadata: &[u8], payload: Option<&[u8]>) {
        for entry in lock(&self.shared.rx).iter() {
            if entry.participant != peer_id {
                continue;
            }
            let mut queue = lock(&entry.queue);
            let Some(index) = queue.free.pop() else {
                continue;
            };
            let total = metadata.len() + payload.map_or(0, <[u8]>::len);
            if let Some(buffer) = queue.buffers.get_mut(index) {
                if let Some(out) = buffer.get_mut(..total) {
                    let (head, tail) = out.split_at_mut(metadata.len());
                    head.copy_from_slice(metadata);
                    if let Some(bytes) = payload {
                        tail.copy_from_slice(bytes);
                    }
                }
            }
            queue.queued.push_back(QueuedFrame {
                buffer: index,
                metadata_len: metadata.len(),
                payload_len: payload.map(<[u8]>::len),
                peer: PeerRef::default(),
                reply: ReplyToken::default(),
            });
            self.shared.force_enter_medium();
        }
    }

    /// Takes the oldest frame queued for `peer_id`, as raw bytes, without
    /// delivering it: a real frame, for tests that replay one by another path.
    #[must_use]
    pub fn take_raw(&self, peer_id: u64) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
        for entry in lock(&self.shared.rx).iter() {
            if entry.participant != peer_id {
                continue;
            }
            let mut queue = lock(&entry.queue);
            let Some(frame) = queue.queued.pop_front() else {
                continue;
            };
            let bytes = queue.buffers.get(frame.buffer)?;
            let metadata = bytes.get(..frame.metadata_len)?.to_vec();
            let payload = match frame.payload_len {
                Some(len) => Some(
                    bytes
                        .get(frame.metadata_len..frame.metadata_len.saturating_add(len))?
                        .to_vec(),
                ),
                None => None,
            };
            queue.free.push(frame.buffer);
            self.shared.leave_medium();
            return Some((metadata, payload));
        }
        None
    }

    /// Reports a peer's restart to every receive route of `to_peer` (tests).
    pub fn report_peer_restarted(&self, to_peer: u64, restarted: PeerRef) {
        let Some(participant) = self.find(to_peer) else {
            return;
        };
        let Some(port) = participant.port.get() else {
            return;
        };
        for entry in lock(&self.shared.rx).iter() {
            if entry.participant == to_peer {
                port.peer_restarted(entry.inbound, restarted);
            }
        }
    }

    /// Reports a peer gone to every receive route of `to_peer` (tests).
    pub fn report_peer_lost(&self, to_peer: u64, lost: PeerRef) {
        let Some(participant) = self.find(to_peer) else {
            return;
        };
        let Some(port) = participant.port.get() else {
            return;
        };
        for entry in lock(&self.shared.rx).iter() {
            if entry.participant == to_peer {
                port.peer_lost(entry.inbound, lost);
            }
        }
    }

    /// Delivers one frame straight through a receive route's port, as a binding's
    /// own receive thread would. For tests of the push path.
    pub fn deliver_direct(
        &self,
        peer_id: u64,
        binding: BindingRoute,
        frame: InboundFrame<'_>,
    ) -> Result<(), up_rust_core::transport::error::ReceiveError> {
        let Some(participant) = self.find(peer_id) else {
            return Err(up_rust_core::transport::error::ReceiveError::UnknownRoute);
        };
        let Some(port) = participant.port.get() else {
            return Err(up_rust_core::transport::error::ReceiveError::UnknownRoute);
        };
        let inbound = lock(&self.shared.rx)
            .iter()
            .find(|entry| entry.participant == peer_id && entry.binding == binding)
            .map(|entry| entry.inbound);
        match inbound {
            Some(inbound) => port.deliver(inbound, frame),
            None => Err(up_rust_core::transport::error::ReceiveError::UnknownRoute),
        }
    }
}

/// Capabilities of a participant, set before it enters runtime.
#[derive(Clone, Copy, Debug)]
pub struct InMemoryOptions {
    /// Report that the middleware allocates, so entering runtime needs a waiver.
    pub middleware_permit: Option<&'static RuntimePermit>,
    /// Allow more than one payload encoding per route.
    pub per_message_encoding: bool,
    /// The envelope for routes that do not choose one.
    pub envelope: Envelope,
    /// Honor `Overflow::DropOldest`.
    pub drop_oldest: bool,
    /// Honor `Overflow::DropNewest`.
    pub drop_newest: bool,
    /// Report `declared_clients` capability.
    pub declared_clients: bool,
    /// Make a binding's own frames fail with this status, to test refusals.
    pub fail_sends: Option<(UCode, &'static str)>,
}

impl Default for InMemoryOptions {
    fn default() -> Self {
        Self {
            middleware_permit: None,
            per_message_encoding: true,
            envelope: Envelope::UFrameFields,
            drop_oldest: true,
            drop_newest: true,
            declared_clients: false,
            fail_sends: None,
        }
    }
}

/// A participant during initialization. Passed to `TransportSetup::handoff`.
pub struct InMemoryParticipant {
    shared: &'static BusShared,
    participant: &'static Participant,
    options: InMemoryOptions,
    next_route: u32,
}

impl std::fmt::Debug for InMemoryParticipant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryParticipant")
            .field("peer_id", &self.participant.peer_id)
            .finish_non_exhaustive()
    }
}

impl InMemoryParticipant {
    /// Replaces this participant's capabilities.
    #[must_use]
    pub fn with_options(mut self, options: InMemoryOptions) -> Self {
        self.options = options;
        self
    }
}

impl BindingSetup for InMemoryParticipant {
    type Running = RunningParticipant;

    fn name(&self) -> &'static str {
        "in-memory-handoff"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::new(self.options.envelope, ENVELOPES, 1024 * 1024)
            .with_per_message_encoding(self.options.per_message_encoding)
            .with_truncate_payload(true)
            .with_declared_clients(self.options.declared_clients)
            .with_chunked_receive(true)
            .with_native_loans(false)
    }

    fn qos_support(&self, kind: RouteKind) -> QosSupport {
        QosSupport::new()
            .with_reliable(true)
            .with_drop_oldest(self.options.drop_oldest)
            .with_drop_newest(self.options.drop_newest)
            .with_wait(false)
            .with_max_depth(match kind.direction {
                RouteDirection::Receive => u16::try_from(self.shared.frames).unwrap_or(u16::MAX),

                RouteDirection::Send => u16::MAX,
            })
            .with_max_history(0)
            .with_priority_levels(0)
    }

    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        let binding = BindingRoute(self.next_route);
        self.next_route += 1;
        match route.direction {
            RouteDirection::Send => {
                lock(&self.participant.tx_routes).push(TxDecl {
                    route: binding,
                    kind: route.kind,
                    source: route.source.clone(),
                    sink: route.sink.cloned(),
                });
            }
            RouteDirection::Receive => {
                let frames = self.shared.frames;
                let buffers = (0..frames)
                    .map(|_| vec![0_u8; self.shared.frame_len].into_boxed_slice())
                    .collect();
                let entry: &'static RxEntry = Box::leak(Box::new(RxEntry {
                    participant: self.participant.peer_id,
                    binding,
                    inbound: route
                        .inbound
                        .ok_or(BindingRefusal("receive route has no inbound"))?,
                    kind: route.kind,
                    source_filter: route.source.clone(),
                    sink_filter: route.sink.cloned(),
                    whole_frame: !matches!(route.envelope, Envelope::UFrameFields),
                    depth: usize::from(route.qos.depth),
                    overflow: route.qos.overflow,
                    queue: Mutex::new(QueueState {
                        buffers,
                        free: (0..frames).collect(),
                        queued: VecDeque::with_capacity(frames),
                    }),
                }));
                drop(lock(&entry.queue));
                lock(&self.shared.rx).push(entry);
            }
        }
        let heap = self
            .options
            .middleware_permit
            .map_or(RuntimeHeap::None, RuntimeHeap::Middleware);
        Ok(PreparedRoute::new(binding, heap))
    }

    fn into_running(self, port: RuntimePort) -> Result<RunningParticipant, BindingRefusal> {
        let _ = self.participant.port.set(port);
        Ok(RunningParticipant {
            shared: self.shared,
            participant: self.participant,
            fail_sends: self.options.fail_sends,
        })
    }
}

/// A participant in runtime mode.
pub struct RunningParticipant {
    shared: &'static BusShared,
    participant: &'static Participant,
    fail_sends: Option<(UCode, &'static str)>,
}

impl std::fmt::Debug for RunningParticipant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningParticipant").finish_non_exhaustive()
    }
}

fn route_matches(entry: &RxEntry, frame: &OutgoingFrame<'_>) -> bool {
    let sink_matches = match (&entry.sink_filter, frame.sink) {
        (None, None) => true,
        (Some(filter), Some(actual)) => filter.matches(actual),
        (None, Some(_)) | (Some(_), None) => false,
    };
    entry.kind.is_none_or(|kind| kind == frame.kind)
        && entry.source_filter.matches(frame.source)
        && sink_matches
}

/// Queues `frame` on one receive route, applying the route's depth and overflow
/// policy. Returns the policy that discarded a frame, if one did: the oldest
/// queued frame under `DropOldest`, or `frame` itself under `DropNewest`.
fn enqueue(
    shared: &BusShared,
    participant: &Participant,
    entry: &RxEntry,
    frame: &OutgoingFrame<'_>,
) -> Result<Option<Overflow>, HandoffFailure> {
    let mut queue = lock(&entry.queue);
    // The frame takes its place in the medium first: a full medium refuses it.
    if !shared.enter_medium() {
        return Err(HandoffFailure::before_send(SendRefusal::Busy));
    }
    let mut dropped = None;
    if queue.queued.len() >= entry.depth {
        match entry.overflow {
            Overflow::DropOldest => {
                if let Some(oldest) = queue.queued.pop_front() {
                    queue.free.push(oldest.buffer);
                    shared.leave_medium();
                    dropped = Some(Overflow::DropOldest);
                }
            }
            Overflow::DropNewest => {
                shared.leave_medium();
                return Ok(Some(Overflow::DropNewest));
            }
            // The sender is refused until the receiver drains (`ready`).
            Overflow::Wait { .. } => {
                shared.leave_medium();
                return Err(HandoffFailure::after(SendRefusal::Busy, SendReport::exact(0, u64::from(dropped.is_some()))));
            }
        }
    }
    // A route is never deeper than its buffers (`qos_support`), so one is free here
    // unless a receive holds it at this moment.
    let Some(index) = queue.free.pop() else {
        shared.leave_medium();
        return Err(HandoffFailure::after(SendRefusal::Busy, SendReport::exact(0, u64::from(dropped.is_some()))));
    };
    let total = frame.metadata.len().saturating_add(frame.payload.len());
    let Some(buffer) = queue
        .buffers
        .get_mut(index)
        .filter(|buffer| total <= buffer.len())
    else {
        queue.free.push(index);
        shared.leave_medium();
        return Err(HandoffFailure::after(SendRefusal::TooLarge, SendReport::exact(0, u64::from(dropped.is_some()))));
    };
    let (head, mut rest) = buffer.split_at_mut(frame.metadata.len());
    head.copy_from_slice(frame.metadata);
    for chunk in frame.payload.chunks() {
        let (part, remaining) = rest.split_at_mut(chunk.len());
        part.copy_from_slice(chunk);
        rest = remaining;
    }
    let peer = PeerRef {
        peer: participant.peer_id,
        incarnation: participant.incarnation.load(Ordering::SeqCst),
    };
    queue.queued.push_back(QueuedFrame {
        buffer: index,
        metadata_len: frame.metadata.len(),
        payload_len: frame.payload.is_present().then(|| frame.payload.len()),
        peer,
        reply: frame.reply,
    });
    Ok(dropped)
}

impl RunningParticipant {
    fn shared_find(&self, peer_id: u64) -> Option<&'static Participant> {
        lock(&self.shared.participants)
            .iter()
            .copied()
            .find(|participant| participant.peer_id == peer_id)
    }

    fn signal_ready(&self) {
        // Each participant's sender may be waiting on this queue draining.
        for participant in lock(&self.shared.participants).iter() {
            if let Some(port) = participant.port.get() {
                for decl in lock(&participant.tx_routes).iter() {
                    port.ready(decl.route);
                }
            }
        }
    }
}

impl HandoffRuntime for RunningParticipant {
    fn receive_bounded(
        &self, route: crate::two_mode::BindingRoute, budget: &crate::two_mode::ReceiveBudget,
        sink: &mut dyn FnMut(crate::two_mode::InboundFrame<'_>) -> crate::two_mode::Flow,
    ) -> usize {
        self.receive_limited(route, Some(budget), sink)
    }

    fn send(&self, _route: BindingRoute, frame: OutgoingFrame<'_>) -> Result<SendReport, HandoffFailure> {
        if let Some((code, detail)) = self.fail_sends {
            return Err(HandoffFailure::before_send(SendRefusal::Failed { code, detail }));
        }
        if !self.participant.alive.load(Ordering::SeqCst) {
            // A dead process's frames go nowhere; the sender does not find out.
            return Ok(SendReport::exact(0, 0));
        }
        let mut report = SendReport::default();
        for entry in lock(&self.shared.rx).iter() {
            if entry.participant != self.participant.peer_id && route_matches(entry, &frame) {
                let dropped = enqueue(self.shared, self.participant, entry, &frame)
                    .map_err(|failure| {
                        let mut prefix = report;
                        prefix.append(failure.report.unwrap_or_else(SendReport::unknown));
                        HandoffFailure::after(failure.refusal, prefix)
                    })?;
                if dropped.is_some() { report.discarded_full(); }
                if dropped != Some(Overflow::DropNewest) { report.accepted(); }
                if let Some(owner) = self.shared_find(entry.participant) {
                    if let Some(port) = owner.port.get() {
                        // The route's policy ran in this queue: the SDK counts it.
                        if let Some(overflow) = dropped {
                            port.dropped(entry.inbound, overflow);
                        }
                        // As a real binding's listener thread would: tell a parked receiver.
                        if dropped != Some(Overflow::DropNewest) {
                            port.readable(entry.inbound);
                        }
                    }
                }
            }
        }
        // Publishing with no subscriber is not an error on this medium.
        Ok(report)
    }

    fn has_capacity(&self, route: BindingRoute) -> bool {
        let tx_routes = lock(&self.participant.tx_routes);
        let Some(tx) = tx_routes.iter().find(|decl| decl.route == route) else {
            return false;
        };
        if !self.shared.medium_has_room() {
            return false;
        }
        let rx = lock(&self.shared.rx);
        rx.iter().all(|entry| {
            if entry.participant == self.participant.peer_id {
                return true;
            }
            let kind_matches = entry
                .kind
                .is_none_or(|wanted| tx.kind.is_none_or(|kind| kind == wanted));
            // A route with a wildcard address may reach any receiver.
            let source_matches =
                tx.source.check_no_wildcards().is_err() || entry.source_filter.matches(&tx.source);
            let sink_matches = match (&entry.sink_filter, &tx.sink) {
                (None, None) => true,
                (Some(filter), Some(actual)) => {
                    actual.check_no_wildcards().is_err() || filter.matches(actual)
                }
                (None, Some(_)) | (Some(_), None) => false,
            };
            !(kind_matches && source_matches && sink_matches) || entry.accepts(&lock(&entry.queue))
        })
    }

    fn receive(
        &self,
        route: BindingRoute,
        sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow,
    ) -> usize {
        self.receive_limited(route, None, sink)
    }
}

/// A queued frame's buffer while `receive` lends it to the sink. Dropped when the
/// sink returns, or as it unwinds, it puts the buffer back in its place, frees
/// its index and the frame's place in the medium, and tells waiting senders.
struct LentBuffer<'r> {
    running: &'r RunningParticipant,
    entry: &'static RxEntry,
    index: usize,
    buffer: Box<[u8]>,
}

impl Drop for LentBuffer<'_> {
    fn drop(&mut self) {
        let buffer = std::mem::take(&mut self.buffer);
        let mut queue = lock(&self.entry.queue);
        if let Some(slot) = queue.buffers.get_mut(self.index) {
            *slot = buffer;
        }
        queue.free.push(self.index);
        drop(queue);
        self.running.shared.leave_medium();
        self.running.signal_ready();
    }
}

// Shared implementation preserves direct legacy calls; SDK calls always pass
// Some(budget), charged BEFORE each pop, including simulated-death discards.
impl RunningParticipant {
    fn receive_limited(
        &self,
        route: BindingRoute,
        budget: Option<&crate::two_mode::ReceiveBudget>,
        sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow,
    ) -> usize {
        let Some(entry) = lock(&self.shared.rx)
            .iter()
            .copied()
            .find(|entry| entry.participant == self.participant.peer_id && entry.binding == route)
        else {
            return 0;
        };
        let mut count = 0;
        let mut stop = false;
        while !stop && budget.is_none_or(|budget| budget.try_take()) {
            let (frame, buffer) = {
                let mut queue = lock(&entry.queue);
                let Some(frame) = queue.queued.pop_front() else {
                    break;
                };
                let buffer = match queue.buffers.get_mut(frame.buffer) {
                    Some(buffer) => std::mem::take(buffer),
                    None => break,
                };
                (frame, buffer)
            };
            // Gives the buffer back however this pass ends: the sink runs the SDK's
            // delivery, and a wire's `decode` there may unwind (R3-2).
            let lent = LentBuffer {
                running: self,
                entry,
                index: frame.buffer,
                buffer,
            };
            if !self.participant.alive.load(Ordering::SeqCst) {
                // A dead process does not read its queue.
            } else {
                let buffer = &lent.buffer;
                let (metadata, payload) = if entry.whole_frame {
                    let total = frame
                        .metadata_len
                        .saturating_add(frame.payload_len.unwrap_or(0));
                    (
                        &[][..],
                        PayloadBytes::one(buffer.get(..total).unwrap_or_default()),
                    )
                } else {
                    let payload = frame.payload_len.map_or(PayloadBytes::Absent, |len| {
                        PayloadBytes::one(
                            buffer
                                .get(frame.metadata_len..frame.metadata_len.saturating_add(len))
                                .unwrap_or_default(),
                        )
                    });
                    (
                        buffer.get(..frame.metadata_len).unwrap_or_default(),
                        payload,
                    )
                };
                let flow = sink(InboundFrame {
                    metadata,
                    payload,
                    peer: frame.peer,
                    reply: frame.reply,
                });
                count += 1;
                stop = matches!(flow, Flow::Stop);
            }
            drop(lent);
        }
        count
    }
}
