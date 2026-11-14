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

//! An in-memory lending binding, for contract tests. Not for production.
//!
//! A [`LendingBus`] connects participants. Publishing loans a slot from a fixed
//! per-route pool, the payload is initialized in place, and `publish` queues one
//! reference per matching receiver; the slot returns to the pool when every
//! receiver is done with it. The middleware's queue is the queue: overflow is
//! applied here, as an iceoryx2-style binding would, and each discard is reported
//! to its route with `RuntimePort::dropped`. One mutex guards the bus. It
//! is locked once when created, so a first lock cannot land on a runtime path,
//! except in the deliberate positive-control option `lazy_mutex`.

use std::collections::VecDeque;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use up_rust_core::frame::header::{parse_header, HeaderLimits, WrittenHeader};
use up_rust_core::transport::binding::{
    BindingRefusal, BindingRoute, BindingSetup, Capabilities, InboundRoute, LendingRuntime,
    PeerRef, PreparedRoute, RouteDeclaration, RouteKind, RuntimePort,
};
use up_rust_core::transport::envelope::Envelope;
use up_rust_core::transport::error::RouteDirection;
use up_rust_core::transport::qos::{Overflow, Qos, QosSupport, SendReport};
use up_rust_core::transport::waiver::RuntimePermit;
use up_rust_core::transport::{
    Frame, InitializedTx, LoanError, LoanedPayload, PayloadLoanProvenance, PreparedTxStorage,
    SendError, TxFrameLayout,
};
use up_rust_core::uattributes::UMessageType;
use up_rust_core::uri::UUri;
use up_rust_core::wire::{
    RuntimeHeap, FORMAT_VERSION, ID_REF_COMPACT, ID_REF_LITERAL, NATIVE_PREFIX_MAGIC,
    UFRAME_FIELDS_METADATA_LAYOUT_ID,
};

const ENVELOPES: &[Envelope] = &[Envelope::UFrameFields];

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct TxSlot {
    buffer: Box<[MaybeUninit<u8>]>,
    payload_offset: usize,
    metadata_len: usize,
    payload_len: Option<usize>,
    refs: u32,
    /// Bytes published, after any truncation.
    published_payload: Option<usize>,
    /// Actual message addressing; shared response routes can address many clients.
    routing: Option<Routing>,
    /// Identity and order at publication, preserved across simulated restarts.
    publication: Option<Publication>,
}

struct TxRouteState {
    participant: u64,
    binding: BindingRoute,
    slots: Vec<TxSlot>,
    free: Vec<usize>,
    /// Newest last. Reference-counted slots kept for late joiners.
    history: VecDeque<usize>,
    history_capacity: usize,
    payload_capacity: usize,
    metadata_capacity: usize,
    alignment: usize,
}

#[derive(Clone, Copy)]
struct Publication {
    peer: PeerRef,
    sequence: u64,
}

struct QueuedRef {
    route: usize,
    slot: usize,
    peer: PeerRef,
}

struct RxRouteState {
    participant: u64,
    binding: BindingRoute,
    inbound: InboundRoute,
    kind: Option<UMessageType>,
    source_filter: UUri,
    sink_filter: Option<UUri>,
    qos: Qos,
    queue: VecDeque<QueuedRef>,
    /// Inactive until `join_late`: a late joiner does not exist yet.
    active: bool,
    /// Late-join replay, if any, applied once on activation.
    wants_history: bool,
    push: bool,
}

#[derive(Default)]
struct BusState {
    tx: Vec<TxRouteState>,
    rx: Vec<RxRouteState>,
    /// Globally ordered publications; exhaustion refuses before sharing a slot.
    publication_sequence: u64,
}

struct Shared {
    state: Mutex<BusState>,
    participants: Mutex<Vec<&'static LendParticipant>>,
    /// Counts `Frame` drops, for the binding contract.
    frame_drops: AtomicU64,
}

struct LendParticipant {
    peer_id: u64,
    incarnation: AtomicU64,
    alive: AtomicBool,
    port: OnceLock<RuntimePort>,
}

/// Capabilities of a lending participant.
#[derive(Clone, Copy, Debug)]
pub struct LendingOptions {
    /// Report that the middleware allocates, so entering runtime needs a waiver.
    pub middleware_permit: Option<&'static RuntimePermit>,
    /// Number of loan slots per send route.
    pub slots: usize,
    /// Honor `Overflow::DropOldest`.
    pub drop_oldest: bool,
    /// Honor `Overflow::DropNewest`.
    pub drop_newest: bool,
    /// Honor `Overflow::Wait`.
    pub wait: bool,
    /// Largest depth.
    pub max_depth: u16,
    /// Largest history.
    pub max_history: u16,
    /// Priority levels acted on.
    pub priority_levels: u8,
    /// Declare `declared_clients`.
    pub declared_clients: bool,
    /// Declare that it can carry short payloads.
    pub truncate_payload: bool,
    /// Deliver through the port's `deliver_frame` instead of pull `receive`.
    pub push: bool,
    /// Positive control: lock a never-primed mutex on the first send.
    pub lazy_mutex: bool,
    /// Receive routes stay inactive until `LendingBus::join_late`.
    pub late_joiner: bool,
}

impl Default for LendingOptions {
    fn default() -> Self {
        Self {
            middleware_permit: None,
            slots: 16,
            drop_oldest: true,
            drop_newest: true,
            wait: true,
            max_depth: 1024,
            max_history: 64,
            priority_levels: 0,
            declared_clients: false,
            truncate_payload: true,
            push: false,
            lazy_mutex: false,
            late_joiner: false,
        }
    }
}

/// An in-memory lending medium.
#[derive(Clone, Copy, Debug)]
pub struct LendingBus {
    shared: &'static Shared,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LendingShared").finish_non_exhaustive()
    }
}

impl LendingBus {
    /// Creates an empty bus.
    #[must_use]
    pub fn new() -> Self {
        let shared: &'static Shared = Box::leak(Box::new(Shared {
            state: Mutex::new(BusState::default()),
            participants: Mutex::new(Vec::new()),
            frame_drops: AtomicU64::new(0),
        }));
        drop(lock(&shared.state));
        drop(lock(&shared.participants));
        Self { shared }
    }

    /// Creates a participant identified to its peers as `peer_id`.
    #[must_use]
    pub fn participant(&self, peer_id: u64) -> LendingParticipant {
        let participant: &'static LendParticipant = Box::leak(Box::new(LendParticipant {
            peer_id,
            incarnation: AtomicU64::new(1),
            alive: AtomicBool::new(true),
            port: OnceLock::new(),
        }));
        lock(&self.shared.participants).push(participant);
        LendingParticipant {
            shared: self.shared,
            participant,
            options: LendingOptions::default(),
            next_route: 0,
        }
    }

    fn find(&self, peer_id: u64) -> Option<&'static LendParticipant> {
        lock(&self.shared.participants)
            .iter()
            .copied()
            .find(|participant| participant.peer_id == peer_id)
    }

    /// Marks a participant dead: its frames go nowhere and it is not delivered to.
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

    /// Activates a late-joining receive route, replaying at most its requested
    /// history (which cannot exceed its depth). The newest matching publications
    /// across all transmitters are replayed in publication order. History
    /// selection precedes live admission, so even a Wait route never needs to
    /// drop or block to replay history. Repeated activation is a no-op.
    pub fn join_late(&self, peer_id: u64, binding: BindingRoute) {
        let mut state = lock(&self.shared.state);
        let BusState { tx, rx, .. } = &mut *state;
        let Some(route) = rx
            .iter_mut()
            .find(|route| route.participant == peer_id && route.binding == binding)
        else {
            return;
        };
        if route.active {
            return;
        }
        route.active = true;
        if !route.wants_history {
            return;
        }
        route.wants_history = false;
        // Inactive receivers have no live queue. Select no more than the setup
        // history budget, regardless of how many transmitters retained history.
        let limit = usize::from(route.qos.history.min(route.qos.depth));
        let mut before = None;
        for _ in 0..limit {
            let mut newest: Option<(usize, usize, Publication)> = None;
            for (index, tx_route) in tx.iter().enumerate() {
                if tx_route.participant == peer_id {
                    continue;
                }
                for &slot in &tx_route.history {
                    let Some(entry) = tx_route.slots.get(slot) else {
                        continue;
                    };
                    let Some(publication) = entry.publication else {
                        continue;
                    };
                    if before.is_some_and(|end| publication.sequence >= end)
                        || newest
                            .is_some_and(|(_, _, current)| publication.sequence <= current.sequence)
                        || !entry
                            .routing
                            .as_ref()
                            .is_some_and(|actual| frame_matches(route, actual))
                    {
                        continue;
                    }
                    newest = Some((index, slot, publication));
                }
            }
            let Some((index, slot, publication)) = newest else {
                break;
            };
            let Some(entry) = tx.get_mut(index).and_then(|tx| tx.slots.get_mut(slot)) else {
                break;
            };
            entry.refs += 1;
            route.queue.push_front(QueuedRef {
                route: index,
                slot,
                peer: publication.peer,
            });
            before = Some(publication.sequence);
        }
    }

    /// Takes the oldest frame queued for `peer_id`, as raw bytes, without
    /// delivering it: a real frame, for tests that replay one by another path.
    #[must_use]
    pub fn take_raw(&self, peer_id: u64) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
        let mut state = lock(&self.shared.state);
        let BusState { tx, rx, .. } = &mut *state;
        let queued = rx
            .iter_mut()
            .filter(|route| route.participant == peer_id)
            .find_map(|route| route.queue.pop_front())?;
        let route = tx.get_mut(queued.route)?;
        let slot = route.slots.get_mut(queued.slot)?;
        let base = slot.buffer.as_ptr().cast::<u8>();
        // SAFETY: a queued slot was fully initialized before it was queued, and
        // it is not written while referenced. The ranges were fixed at setup.
        let metadata = unsafe { std::slice::from_raw_parts(base, slot.metadata_len) }.to_vec();
        let payload = slot.published_payload.map(|len| {
            // SAFETY: as above; the payload region holds `len` initialized bytes.
            unsafe { std::slice::from_raw_parts(base.add(slot.payload_offset), len) }.to_vec()
        });
        slot.refs = slot.refs.saturating_sub(1);
        if slot.refs == 0 {
            route.free.push(queued.slot);
        }
        Some((metadata, payload))
    }

    /// The number of frames released by drop since creation.
    #[must_use]
    pub fn frame_drops(&self) -> u64 {
        self.shared.frame_drops.load(Ordering::SeqCst)
    }
}

impl Default for LendingBus {
    fn default() -> Self {
        Self::new()
    }
}

/// A participant during initialization. Passed to `TransportSetup::lending`.
pub struct LendingParticipant {
    shared: &'static Shared,
    participant: &'static LendParticipant,
    options: LendingOptions,
    next_route: u32,
}

impl std::fmt::Debug for LendingParticipant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LendingParticipant")
            .field("peer_id", &self.participant.peer_id)
            .finish_non_exhaustive()
    }
}

impl LendingParticipant {
    /// Replaces this participant's capabilities.
    #[must_use]
    pub fn with_options(mut self, options: LendingOptions) -> Self {
        self.options = options;
        self
    }
}

impl BindingSetup for LendingParticipant {
    type Running = RunningLender;

    fn name(&self) -> &'static str {
        "in-memory-lending"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::new(Envelope::UFrameFields, ENVELOPES, 128 * 1024 * 1024)
            .with_per_message_encoding(true)
            .with_truncate_payload(self.options.truncate_payload)
            .with_declared_clients(self.options.declared_clients)
            .with_chunked_receive(false)
            .with_native_loans(true)
    }

    fn qos_support(&self, _kind: RouteKind) -> QosSupport {
        QosSupport::new()
            .with_reliable(true)
            .with_drop_oldest(self.options.drop_oldest)
            .with_drop_newest(self.options.drop_newest)
            .with_wait(self.options.wait)
            .with_max_depth(self.options.max_depth)
            .with_max_history(self.options.max_history)
            .with_priority_levels(self.options.priority_levels)
    }

    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        let binding = BindingRoute(self.next_route);
        self.next_route += 1;
        let mut state = lock(&self.shared.state);
        match route.direction {
            RouteDirection::Send => {
                let slots_wanted = route.slots.max(self.options.slots);
                let alignment = route.payload_alignment.max(1);
                let metadata_capacity = route.max_metadata_len;
                let payload_capacity = route.max_payload_len;
                let size = metadata_capacity
                    .saturating_add(payload_capacity)
                    .saturating_add(alignment);
                let mut slots = Vec::with_capacity(slots_wanted);
                for _ in 0..slots_wanted {
                    let buffer: Box<[MaybeUninit<u8>]> = Box::new_uninit_slice(size);
                    let base = buffer.as_ptr() as usize;
                    let payload_start =
                        (base + metadata_capacity + alignment - 1) & !(alignment - 1);
                    slots.push(TxSlot {
                        buffer,
                        payload_offset: payload_start - base,
                        metadata_len: 0,
                        payload_len: None,
                        refs: 0,
                        published_payload: None,
                        routing: None,
                        publication: None,
                    });
                }
                let free: Vec<usize> = (0..slots_wanted).collect();
                state.tx.push(TxRouteState {
                    participant: self.participant.peer_id,
                    binding,
                    slots,
                    free,
                    history: VecDeque::with_capacity(usize::from(route.qos.history)),
                    history_capacity: usize::from(route.qos.history),
                    payload_capacity,
                    metadata_capacity,
                    alignment,
                });
            }
            RouteDirection::Receive => {
                let depth = usize::from(route.qos.depth);
                let inactive = self.options.late_joiner;
                state.rx.push(RxRouteState {
                    participant: self.participant.peer_id,
                    binding,
                    inbound: route
                        .inbound
                        .ok_or(BindingRefusal("receive route has no inbound"))?,
                    kind: route.kind,
                    source_filter: route.source.clone(),
                    sink_filter: route.sink.cloned(),
                    qos: route.qos,
                    queue: VecDeque::with_capacity(depth.saturating_add(1)),
                    active: !inactive,
                    wants_history: inactive && route.qos.history > 0,
                    push: self.options.push,
                });
            }
        }
        let heap = self
            .options
            .middleware_permit
            .map_or(RuntimeHeap::None, RuntimeHeap::Middleware);
        Ok(PreparedRoute::new(binding, heap))
    }

    fn into_running(self, port: RuntimePort) -> Result<RunningLender, BindingRefusal> {
        let _ = self.participant.port.set(port);
        let lazy = self
            .options
            .lazy_mutex
            .then(|| Box::leak(Box::new(Mutex::new(0_u32))));
        Ok(RunningLender {
            shared: self.shared,
            participant: self.participant,
            lazy: lazy.map(|mutex| &*mutex),
            lazy_allocated: AtomicBool::new(false),
        })
    }
}

/// Exclusive transmit loan of one slot.
pub struct LendTx<'a> {
    shared: &'static Shared,
    route: usize,
    slot: usize,
    metadata_len: usize,
    payload_len: Option<usize>,
    metadata: *mut MaybeUninit<u8>,
    payload: *mut MaybeUninit<u8>,
    published: bool,
    _lender: std::marker::PhantomData<&'a RunningLender>,
}

// SAFETY: the pointers address a slot exclusively owned by this loan until it
// is consumed; the bus does not touch a loaned slot.
unsafe impl Send for LendTx<'_> {}

impl std::fmt::Debug for LendTx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LendTx").finish_non_exhaustive()
    }
}

// SAFETY: each view is backed by this loan's slot, which the free list gave to
// this loan exclusively, for its whole life. Metadata and payload regions are
// disjoint (`payload_offset >= metadata_capacity`), stable (boxed) and not
// resized; no method invalidates bytes already written through either view.
unsafe impl PreparedTxStorage for LendTx<'_> {
    fn metadata_uninit_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        // SAFETY: see the impl-level comment; `metadata_len` bytes are in range.
        unsafe { std::slice::from_raw_parts_mut(self.metadata, self.metadata_len) }
    }

    fn payload_uninit_mut(&mut self) -> Option<&mut [MaybeUninit<u8>]> {
        let len = self.payload_len?;
        // SAFETY: as above; the payload region is `payload_len` bytes in range.
        Some(unsafe { std::slice::from_raw_parts_mut(self.payload, len) })
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

impl Drop for LendTx<'_> {
    fn drop(&mut self) {
        if !self.published {
            let mut state = lock(&self.shared.state);
            if let Some(route) = state.tx.get_mut(self.route) {
                route.free.push(self.slot);
            }
        }
    }
}

/// A received frame. Dropping it releases its slot reference.
pub struct LendFrame {
    shared: &'static Shared,
    route: usize,
    slot: usize,
    metadata: *const u8,
    metadata_len: usize,
    payload: *const u8,
    payload_len: Option<usize>,
    peer: PeerRef,
}

// SAFETY: the bytes are retained by this frame's reference on the slot.
unsafe impl Send for LendFrame {}

impl std::fmt::Debug for LendFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LendFrame").finish_non_exhaustive()
    }
}

impl Frame for LendFrame {
    fn encoded_metadata(&self) -> &[u8] {
        // SAFETY: the slot reference held by this frame keeps these initialized
        // bytes alive and unchanged until the frame is dropped.
        unsafe { std::slice::from_raw_parts(self.metadata, self.metadata_len) }
    }

    fn payload(&self) -> Option<LoanedPayload<'_>> {
        self.payload_len.map(|len| {
            // SAFETY: as above.
            let bytes = unsafe { std::slice::from_raw_parts(self.payload, len) };
            LoanedPayload::new(bytes, PayloadLoanProvenance::OpaqueTransportLoan)
        })
    }

    fn peer(&self) -> PeerRef {
        self.peer
    }
}

impl Drop for LendFrame {
    fn drop(&mut self) {
        self.shared.frame_drops.fetch_add(1, Ordering::Relaxed);
        release_ref(self.shared, self.route, self.slot);
    }
}

fn release_ref(shared: &'static Shared, route: usize, slot: usize) {
    let mut state = lock(&shared.state);
    if let Some(tx) = state.tx.get_mut(route) {
        if let Some(entry) = tx.slots.get_mut(slot) {
            entry.refs = entry.refs.saturating_sub(1);
            if entry.refs == 0 {
                tx.free.push(slot);
            }
        }
    }
}

/// A participant in runtime mode.
pub struct RunningLender {
    shared: &'static Shared,
    participant: &'static LendParticipant,
    lazy: Option<&'static Mutex<u32>>,
    /// The first-use allocation of the lazy-mutex control was made.
    lazy_allocated: AtomicBool,
}

impl std::fmt::Debug for RunningLender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningLender").finish_non_exhaustive()
    }
}

impl RunningLender {
    fn route_index(state: &BusState, participant: u64, binding: BindingRoute) -> Option<usize> {
        state
            .tx
            .iter()
            .position(|route| route.participant == participant && route.binding == binding)
    }
}

#[derive(Clone)]
struct Routing {
    kind: UMessageType,
    source: UUri,
    sink: Option<UUri>,
}

fn routing_take<'a>(bytes: &mut &'a [u8], len: usize) -> Result<&'a [u8], SendError> {
    let (value, rest) = bytes
        .split_at_checked(len)
        .ok_or(SendError::PayloadStorage)?;
    *bytes = rest;
    Ok(value)
}

fn routing_identity<'a>(bytes: &mut &'a [u8]) -> Result<(u8, &'a [u8]), SendError> {
    let tag = *routing_take(bytes, 1)?
        .first()
        .ok_or(SendError::PayloadStorage)?;
    let len = match tag {
        ID_REF_COMPACT => 2,
        ID_REF_LITERAL => {
            let mut word = [0; 4];
            word.copy_from_slice(routing_take(bytes, 4)?);
            usize::try_from(u32::from_le_bytes(word)).map_err(|_| SendError::PayloadStorage)?
        }
        _ => return Err(SendError::PayloadStorage),
    };
    let identity = routing_take(bytes, len)?;
    if tag == ID_REF_LITERAL {
        std::str::from_utf8(identity).map_err(|_| SendError::PayloadStorage)?;
    }
    Ok((tag, identity))
}

impl Routing {
    fn read(metadata: &[u8]) -> Result<Self, SendError> {
        let mut rest = metadata;
        if routing_take(&mut rest, NATIVE_PREFIX_MAGIC.len())? != NATIVE_PREFIX_MAGIC {
            return Err(SendError::PayloadStorage);
        }
        let (tag, layout) = routing_identity(&mut rest)?;
        let correct_layout = if tag == ID_REF_COMPACT {
            layout == UFRAME_FIELDS_METADATA_LAYOUT_ID.compact_id().to_le_bytes()
        } else {
            layout == UFRAME_FIELDS_METADATA_LAYOUT_ID.literal_id().as_bytes()
        };
        if !correct_layout || routing_take(&mut rest, 2)? != FORMAT_VERSION.to_le_bytes() {
            return Err(SendError::PayloadStorage);
        }
        routing_identity(&mut rest)?; // selected wire
        routing_identity(&mut rest)?; // payload family
        if routing_take(&mut rest, 2)? != [0, 0] {
            return Err(SendError::PayloadStorage);
        }
        let prefix_len = metadata
            .len()
            .checked_sub(rest.len())
            .ok_or(SendError::PayloadStorage)?;
        let prefix = metadata
            .get(..prefix_len)
            .ok_or(SendError::PayloadStorage)?;
        // Address routing imposes no extra token limit; typed receive admission
        // applies its own limit. Parsing stays bounded by the prepared metadata.
        let header = parse_header(
            prefix,
            metadata,
            &HeaderLimits {
                max_token_len: metadata.len(),
            },
        )
        .map_err(|_| SendError::PayloadStorage)?;
        Ok(Self {
            kind: header.kind,
            source: header.source,
            sink: header.sink,
        })
    }
}

fn frame_matches(rx: &RxRouteState, actual: &Routing) -> bool {
    let sink_matches = match (&rx.sink_filter, &actual.sink) {
        (None, None) => true,
        (Some(filter), Some(actual)) => filter.matches(actual),
        (None, Some(_)) | (Some(_), None) => false,
    };
    rx.kind.is_none_or(|wanted| wanted == actual.kind)
        && rx.source_filter.matches(&actual.source)
        && sink_matches
}

impl LendingRuntime for RunningLender {
    // Explicit single-attempt adapter, justified by this retained implementation.
    fn receive_bounded(&self, route: crate::two_mode::BindingRoute, budget: &crate::two_mode::ReceiveBudget) -> Option<Self::Rx> {
        if !budget.try_take() { return None; }
        self.receive(route)
    }

    type Tx<'a> = LendTx<'a>;
    type Rx = LendFrame;

    fn loan(&self, route: BindingRoute, layout: TxFrameLayout) -> Result<LendTx<'_>, LoanError> {
        if let Some(lazy) = self.lazy {
            // Positive control: this mutex was never locked during setup, and on a
            // pthread target its first lock allocates. A futex mutex allocates
            // nothing, so there the double makes the one first-use allocation
            // itself, and the control can see a first-traffic allocation everywhere.
            drop(lock(lazy));
            if !cfg!(target_vendor = "apple") && !self.lazy_allocated.swap(true, Ordering::AcqRel) {
                Box::leak(Box::new(0_u64));
            }
        }
        let mut state = lock(&self.shared.state);
        let index = Self::route_index(&state, self.participant.peer_id, route)
            .ok_or(LoanError::InvalidRoute)?;
        let tx = state.tx.get_mut(index).ok_or(LoanError::InvalidRoute)?;
        if layout.metadata_len > tx.metadata_capacity
            || layout.payload_len.unwrap_or(0) > tx.payload_capacity
            || layout.payload_alignment > tx.alignment
        {
            return Err(LoanError::SlotTooSmall);
        }
        let slot_index = tx.free.pop().ok_or(LoanError::PoolExhausted)?;
        let slot = tx
            .slots
            .get_mut(slot_index)
            .ok_or(LoanError::InvalidRoute)?;
        slot.metadata_len = layout.metadata_len;
        slot.payload_len = layout.payload_len;
        slot.published_payload = None;
        slot.routing = None;
        slot.publication = None;
        slot.refs = 0;
        let base = slot.buffer.as_mut_ptr();
        // SAFETY: `payload_offset` was computed within the buffer at setup.
        let payload = unsafe { base.add(slot.payload_offset) };
        Ok(LendTx {
            shared: self.shared,
            route: index,
            slot: slot_index,
            metadata_len: layout.metadata_len,
            payload_len: layout.payload_len,
            metadata: base,
            payload,
            published: false,
            _lender: std::marker::PhantomData,
        })
    }

    fn publish<'a>(
        &'a self,
        _route: BindingRoute,
        frame: InitializedTx<LendTx<'a>>,
    ) -> Result<SendReport, SendError> {
        let mut tx = frame.into_storage();
        // SAFETY: InitializedTx proves every metadata byte is initialized. This
        // exclusive loan owns the stable slot until publication; parse before
        // publishing or sharing it, and borrow no payload bytes.
        let metadata =
            unsafe { std::slice::from_raw_parts(tx.metadata.cast::<u8>(), tx.metadata_len) };
        let routing = Routing::read(metadata)?;
        let sender_alive = self.participant.alive.load(Ordering::SeqCst);
        let mut report = SendReport::default();
        let peer = PeerRef {
            peer: self.participant.peer_id,
            incarnation: self.participant.incarnation.load(Ordering::SeqCst),
        };
        // Push delivery and readiness signals happen outside the lock. Fixed
        // capacity: no heap here.
        const PUSH_CAPACITY: usize = 8;
        let mut deliveries: [Option<(InboundRoute, u64, Option<QueuedRef>)>; PUSH_CAPACITY] =
            [const { None }; PUSH_CAPACITY];
        let mut delivery_count = 0_usize;
        // Discards made by this bus's queues, reported to their routes outside the
        // lock, so the route counts them as it counts its own (R44).
        let mut discards: [Option<(InboundRoute, u64, Overflow)>; PUSH_CAPACITY] =
            [const { None }; PUSH_CAPACITY];
        let mut discard_count = 0_usize;
        let mut wait_deadline: Option<Instant> = None;
        loop {
            let mut state = lock(&self.shared.state);
            let BusState {
                tx: tx_routes,
                rx,
                publication_sequence,
            } = &mut *state;
            let mut retry_wait = false;
            if sender_alive {
                // Wait routes are checked first, so nothing is delivered until
                // every waiting receiver has room.
                for rx_route in rx.iter() {
                    if rx_route.participant == self.participant.peer_id
                        || !rx_route.active
                        || !frame_matches(rx_route, &routing)
                    {
                        continue;
                    }
                    if let Overflow::Wait { timeout } = rx_route.qos.overflow {
                        if rx_route.queue.len() >= usize::from(rx_route.qos.depth) {
                            let deadline =
                                *wait_deadline.get_or_insert_with(|| Instant::now() + timeout);
                            if Instant::now() >= deadline {
                                drop(state);
                                return Err(SendError::TimedOut);
                            }
                            retry_wait = true;
                        }
                    }
                }
            }
            if retry_wait {
                drop(state);
                std::thread::sleep(Duration::from_micros(200));
                continue;
            }
            let sequence = publication_sequence
                .checked_add(1)
                .ok_or(SendError::PayloadStorage)?;
            let slot = tx_routes
                .get_mut(tx.route)
                .and_then(|route| route.slots.get_mut(tx.slot))
                .ok_or(SendError::PayloadStorage)?;
            slot.published_payload = tx.payload_len;
            slot.routing = Some(routing.clone());
            slot.publication = Some(Publication { peer, sequence });
            *publication_sequence = sequence;
            // All fallible work is complete. A refusal above returns the still
            // exclusive loan through LendTx::drop, after releasing the bus lock.
            tx.published = true;
            if sender_alive {
                for rx_route in rx.iter_mut() {
                    if rx_route.participant == self.participant.peer_id
                        || !rx_route.active
                        || !frame_matches(rx_route, &routing)
                    {
                        continue;
                    }
                    let depth = usize::from(rx_route.qos.depth);
                    if rx_route.queue.len() >= depth {
                        match rx_route.qos.overflow {
                            Overflow::DropOldest => {
                                if let Some(oldest) = rx_route.queue.pop_front() {
                                    // A receiver can queue several transmitters.
                                    // Release through the queued frame's owner,
                                    // never through this publication's route.
                                    release_queued_ref(tx_routes, oldest);
                                    if let Some(slot) = discards.get_mut(discard_count) {
                                        *slot = Some((
                                            rx_route.inbound,
                                            rx_route.participant,
                                            Overflow::DropOldest,
                                        ));
                                        discard_count = discard_count.saturating_add(1);
                                    }
                                }
                                report.discarded_full();
                            }
                            Overflow::DropNewest => {
                                report.discarded_full();
                                if let Some(slot) = discards.get_mut(discard_count) {
                                    *slot = Some((
                                        rx_route.inbound,
                                        rx_route.participant,
                                        Overflow::DropNewest,
                                    ));
                                    discard_count = discard_count.saturating_add(1);
                                }
                                continue;
                            }
                            Overflow::Wait { .. } => continue,
                        }
                    }
                    if let Some(slot) = tx_routes
                        .get_mut(tx.route)
                        .and_then(|route| route.slots.get_mut(tx.slot))
                    {
                        slot.refs += 1;
                    }
                    rx_route.queue.push_back(QueuedRef {
                        route: tx.route,
                        slot: tx.slot,
                        peer,
                    });
                    report.accepted();
                    if let Some(slot) = deliveries.get_mut(delivery_count) {
                        // A push route hands over the frame; a pull route is told it is there.
                        let queued = if rx_route.push {
                            rx_route.queue.pop_back()
                        } else {
                            None
                        };
                        *slot = Some((rx_route.inbound, rx_route.participant, queued));
                        delivery_count = delivery_count.saturating_add(1);
                    }
                }
            }
            let route = tx_routes
                .get_mut(tx.route)
                .ok_or(SendError::PayloadStorage)?;
            // History: keep the newest `history_capacity` published slots.
            let cap = route.history_capacity;
            if cap > 0 {
                if route.history.len() == cap {
                    if let Some(oldest) = route.history.pop_front() {
                        if let Some(old) = route.slots.get_mut(oldest) {
                            old.refs = old.refs.saturating_sub(1);
                            if old.refs == 0 {
                                route.free.push(oldest);
                            }
                        }
                    }
                }
                if let Some(slot) = route.slots.get_mut(tx.slot) {
                    slot.refs += 1;
                }
                route.history.push_back(tx.slot);
            }
            let refs = route.slots.get(tx.slot).map_or(0, |slot| slot.refs);
            if refs == 0 {
                // Nobody took it: return the slot.
                route.free.push(tx.slot);
            }
            break;
        }
        // Discards first, so a receiver woken below already sees them counted.
        for discard in discards.iter_mut() {
            let Some((inbound, participant, overflow)) = discard.take() else {
                continue;
            };
            let Some(target) = lock(&self.shared.participants)
                .iter()
                .copied()
                .find(|entry| entry.peer_id == participant)
            else {
                continue;
            };
            if let Some(port) = target.port.get() {
                port.dropped(inbound, overflow);
            }
        }
        for delivery in deliveries.iter_mut() {
            let Some((inbound, participant, queued)) = delivery.take() else {
                continue;
            };
            let Some(target) = lock(&self.shared.participants)
                .iter()
                .copied()
                .find(|entry| entry.peer_id == participant)
            else {
                continue;
            };
            if let Some(port) = target.port.get() {
                match queued {
                    Some(queued) => {
                        let frame = self.make_frame(queued);
                        let _ = port.deliver_frame(inbound, frame);
                    }
                    None => port.readable(inbound),
                }
            }
        }
        Ok(report)
    }

    fn has_capacity(&self, route: BindingRoute) -> bool {
        let state = lock(&self.shared.state);
        let Some(index) = Self::route_index(&state, self.participant.peer_id, route) else {
            return false;
        };
        state.tx.get(index).is_some_and(|tx| !tx.free.is_empty())
    }

    fn receive(&self, route: BindingRoute) -> Option<LendFrame> {
        if !self.participant.alive.load(Ordering::SeqCst) {
            return None;
        }
        let mut state = lock(&self.shared.state);
        let rx = state.rx.iter_mut().find(|entry| {
            entry.participant == self.participant.peer_id && entry.binding == route
        })?;
        if !rx.active {
            return None;
        }
        let queued = rx.queue.pop_front()?;
        drop(state);
        Some(self.make_frame(queued))
    }
}

impl RunningLender {
    fn make_frame(&self, queued: QueuedRef) -> LendFrame {
        let state = lock(&self.shared.state);
        let Some(tx) = state.tx.get(queued.route) else {
            drop(state);
            return empty_frame(self.shared, queued);
        };
        let Some(slot) = tx.slots.get(queued.slot) else {
            drop(state);
            return empty_frame(self.shared, queued);
        };
        // Another receiver can already hold a byte borrow of this slot.
        // Only shared access to the buffer is permitted when making a reader.
        let base = slot.buffer.as_ptr().cast::<u8>();
        // SAFETY: `payload_offset` was computed within this buffer at setup.
        let payload = unsafe { base.add(slot.payload_offset) };
        LendFrame {
            shared: self.shared,
            route: queued.route,
            slot: queued.slot,
            metadata: base,
            metadata_len: slot.metadata_len,
            payload,
            payload_len: slot.published_payload,
            peer: queued.peer,
        }
    }
}

fn empty_frame(shared: &'static Shared, queued: QueuedRef) -> LendFrame {
    LendFrame {
        shared,
        route: queued.route,
        slot: queued.slot,
        metadata: std::ptr::NonNull::dangling().as_ptr(),
        metadata_len: 0,
        payload: std::ptr::NonNull::dangling().as_ptr(),
        payload_len: None,
        peer: queued.peer,
    }
}

/// Release exactly the queue reference's originating route and slot while the
/// bus is locked. The history and other receivers retain their own references.
fn release_queued_ref(tx: &mut [TxRouteState], queued: QueuedRef) {
    if let Some(route) = tx.get_mut(queued.route) {
        if let Some(entry) = route.slots.get_mut(queued.slot) {
            if entry.refs > 0 {
                entry.refs -= 1;
                if entry.refs == 0 {
                    route.free.push(queued.slot);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "lending_routing_tests.rs"]
mod routing_tests;
