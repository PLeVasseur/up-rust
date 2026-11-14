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

//! Receive dispatch: startup slots, fixed-size decode and retained frames.
//!
//! One [`TypedInbound`] per receive route. Slots are `depth + max_held + 1`.
//! A free list holds free slots and a ready queue holds queued ones, both private
//! to the crate; a slot held by an application lease is in neither, so it can
//! never be evicted.
//! Frames are decoded into permanently reserved `T` slots, or, for a native route,
//! the retained frame itself is kept and borrowed through a checked typed view.

use core::cell::UnsafeCell;
use core::fmt;
use core::marker::PhantomData;
use core::mem::{self, MaybeUninit};
use core::ops::Deref;
use crate::platform::atomic::{AtomicU32, AtomicU64, Ordering};
use core::time::Duration;

use super::binding::{
    copy_frame, BindingRoute, InboundFrame, PayloadBytes, PeerRef, ReplyToken, StaticHeader,
};
use super::envelope::{AdmittedWire, Envelope, ParsedFrame};
use super::error::{ReceiveError, RouteName, StartupError};
use super::frame::RetainedFrame;
use super::mode::TransportShared;
use super::qos::{Overflow, Qos};
use super::report::{add, Counters, RouteId, RouteReport};
use super::slots::{ByteSlots, FreeIndices, ReadyQueue, ReadyOffer};
use super::wake::RouteWake;
use crate::frame::header::{HeaderLimits, RxHeader};
use crate::payload::loan::borrow_checked;
use crate::payload::stable::StablePayload;
use crate::platform::Monotonic;
use crate::uattributes::UMessageType;
use crate::uri::UUri;
use crate::wire::{Encoded, InPlace, RuntimeHeap, Wire, WireError, WireMode, WirePayloadIdentity};

/// One entry per remote participant. Incarnations only move forward.
///
/// `state` decides who may write the other two words, so an entry never changes
/// hands while a frame of its peer is being classified. A frame classifies as a
/// *user* of the entry's current tenure. `forget` only marks the entry retiring;
/// whoever sees it retiring with no user left frees it, and only then can another
/// peer claim it. A claim takes the zero peer word by compare-exchange; before it
/// publishes the entry as owned, `PeerTable::admit` checks that no other entry
/// holds the same peer, so a peer never ends with two entries (R2-1). For the
/// length of that check a peer can hold two words, and a third peer's frame can
/// then be refused once (R3-1, documented on `PeerTable::admit`). The
/// generation in `state` advances with every free, so a stale reading never
/// matches a later tenure. Every step is a bounded compare-exchange; nothing waits.
struct PeerEntry {
    state: AtomicU64,
    peer: AtomicU64,
    incarnation: AtomicU64,
}

/// Frames of the owning peer being classified right now.
const PEER_USERS: u64 = u32::MAX as u64;
/// The words belong to the peer in `peer`.
const PEER_OWNED: u64 = 1 << 32;
/// Reported lost: no new user enters, and the last one out frees the entry.
const PEER_RETIRING: u64 = 1 << 33;
/// Where the generation starts in `state`.
const PEER_GENERATION_SHIFT: u32 = 34;

/// How the second pass of `admit` fared on one entry.
enum PeerClaim {
    /// This frame holds the free entry's peer word for its peer. The entry is not
    /// owned yet: the caller publishes it, or gives it back.
    Won,
    /// The same peer owns this entry, or is claiming it right now.
    SamePeer,
    /// The entry serves another peer, or is between tenures.
    Taken,
}

impl PeerEntry {
    const fn new() -> Self {
        Self {
            state: AtomicU64::new(0),
            peer: AtomicU64::new(0),
            incarnation: AtomicU64::new(0),
        }
    }

    /// Enters as a user if `peer` owns this entry and it is not retiring.
    fn enter(&self, peer: u64) -> bool {
        let mut state = self.state.load(Ordering::Acquire);
        loop {
            if state & (PEER_OWNED | PEER_RETIRING) != PEER_OWNED
                || state & PEER_USERS == PEER_USERS
            {
                return false;
            }
            // Written before `PEER_OWNED` was published, and fixed for the tenure:
            // if the entry was freed since, `state` has moved and the exchange fails.
            if self.peer.load(Ordering::Relaxed) != peer {
                return false;
            }
            match self.state.compare_exchange_weak(
                state,
                state.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => state = actual,
            }
        }
    }

    /// Leaves; the last user out of a retiring entry frees it.
    fn leave(&self) {
        let previous = self.state.fetch_sub(1, Ordering::AcqRel);
        if previous & PEER_RETIRING != 0 && previous & PEER_USERS == 1 {
            self.free(previous);
        }
    }

    /// Classifies a frame of the owning peer, as a user of this entry.
    fn admit_known(&self, peer: PeerRef) -> Option<Result<PeerAdmission, ReceiveError>> {
        if !self.enter(peer.peer) {
            return None;
        }
        let admission = PeerTable::classify(self, peer);
        self.leave();
        Some(admission)
    }

    /// Takes this entry's peer word for `peer` if the entry is free. On `Won` the
    /// entry is not owned yet: see [`PeerTable::admit`].
    fn claim(&self, peer: PeerRef) -> PeerClaim {
        let state = self.state.load(Ordering::Acquire);
        if state & (PEER_OWNED | PEER_RETIRING | PEER_USERS) != 0 {
            // Owned by this same peer since `admit_known` looked: classify there.
            if state & (PEER_OWNED | PEER_RETIRING) == PEER_OWNED
                && self.peer.load(Ordering::Relaxed) == peer.peer
            {
                return PeerClaim::SamePeer;
            }
            return PeerClaim::Taken;
        }
        // Sequentially consistent, as is the read in `holds`: of two claimers of
        // one peer on two entries, each takes its word before it reads the
        // other's, so at least one of them sees the other and gives its entry back.
        match self
            .peer
            .compare_exchange(0, peer.peer, Ordering::SeqCst, Ordering::Acquire)
        {
            Ok(_) => PeerClaim::Won,
            Err(actual) if actual == peer.peer => PeerClaim::SamePeer,
            Err(_) => PeerClaim::Taken,
        }
    }

    /// Publishes a won claim: the entry is owned by `peer` from here on.
    fn publish(&self, peer: PeerRef) {
        // A zero peer word means a free entry (`free` clears it last), and only
        // the thread holding the peer word moves a free entry on: this one.
        let free = self.state.load(Ordering::Acquire);
        self.incarnation.store(peer.incarnation, Ordering::Relaxed);
        self.state.store(free | PEER_OWNED, Ordering::Release);
    }

    /// Gives a won claim back unpublished. Only an owned entry admits users, so
    /// nothing entered this one: it is free again, as it was.
    fn give_back(&self) {
        self.peer.store(0, Ordering::Release);
    }

    /// Whether this entry holds `peer`'s word outside a retirement: owned, or
    /// claimed and not yet published. An entry being freed looks the same for a
    /// moment; mistaking it costs a frame its entry, never a second one.
    fn holds(&self, peer: u64) -> bool {
        // A read-modify-write that adds nothing, so it reads the latest word, and
        // the argument in `claim` does not rest on which older store a load may
        // read. A sequentially consistent load would do in the language's model,
        // but the zero a claim replaces is a release store, and Miri lets such a
        // load read that zero after the claim, which the model forbids. It
        // writes back the word it read.
        self.peer.fetch_add(0, Ordering::SeqCst) == peer
            && self.state.load(Ordering::SeqCst) & PEER_RETIRING == 0
    }

    /// Starts retiring this entry if `peer` owns it. Frees it now when no frame of
    /// that peer is being classified; otherwise the last one out does.
    fn retire(&self, peer: u64) {
        let mut state = self.state.load(Ordering::Acquire);
        loop {
            if state & (PEER_OWNED | PEER_RETIRING) != PEER_OWNED {
                return;
            }
            if self.peer.load(Ordering::Relaxed) != peer {
                return;
            }
            match self.state.compare_exchange_weak(
                state,
                state | PEER_RETIRING,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    if state & PEER_USERS == 0 {
                        self.free(state);
                    }
                    return;
                }
                Err(actual) => state = actual,
            }
        }
    }

    /// Frees the entry for its next tenure. Exactly one party calls this per
    /// tenure: the one that saw the entry retiring with no user left.
    fn free(&self, state: u64) {
        let generation = (state >> PEER_GENERATION_SHIFT).wrapping_add(1);
        self.incarnation.store(0, Ordering::Relaxed);
        self.state
            .store(generation << PEER_GENERATION_SHIFT, Ordering::Release);
        // Last, so that a zero peer word always means a free entry.
        self.peer.store(0, Ordering::Release);
    }
}

/// A fixed table of the peers seen on one route.
#[doc(hidden)]
pub struct PeerTable {
    entries: &'static [PeerEntry],
}

impl fmt::Debug for PeerTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PeerTable").finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum PeerAdmission {
    /// The binding gave no identity.
    Unchecked,
    /// The peer is known at this incarnation, or is new.
    Current,
    /// The peer came back with a higher incarnation.
    Restarted,
}

impl PeerTable {
    /// A table for `max_peers` peers, placed in `memory`.
    ///
    /// # Errors
    /// Refuses when memory is exhausted.
    pub fn new_in(
        memory: &crate::memory::StartupMemory,
        max_peers: usize,
    ) -> Result<Self, crate::memory::StartupError> {
        let count = max_peers.max(1);
        let mut entries = memory.fixed_vec::<PeerEntry>(count)?;
        for _ in 0..count {
            entries.push(PeerEntry::new())?;
        }
        Ok(Self {
            entries: entries.into_static_slice(),
        })
    }

    /// Admits `peer`, classifying the frame as current, restarted or stale.
    ///
    /// A peer never keeps two entries, but it can hold two for a moment. When two
    /// of its first frames race and an entry comes free during the race (another
    /// peer reported lost, or a claim given back), each frame can claim an entry.
    /// The peer then holds two words until the later claimer's check sees the
    /// other and gives its entry back, which is the length of one scan of the
    /// table. In that moment a frame of a third peer can find no entry free. It is
    /// refused `PeerTableFull` and counted in the route's `refused`, and that
    /// peer's next frame, once the claim is given back, is admitted. On a table
    /// sized exactly to its peers, this one transient refusal is what a peer that
    /// reconnects at once can cost another (R3-1).
    ///
    /// # Errors
    /// `StalePeer` for an older incarnation, `PeerTableFull` when no entry is free.
    pub fn admit(&self, peer: PeerRef) -> Result<PeerAdmission, ReceiveError> {
        if peer.peer == 0 {
            return Ok(PeerAdmission::Unchecked);
        }
        // Look for the peer first: `forget` leaves holes, and a known peer must
        // not take a hole ahead of its own entry.
        for entry in self.entries {
            if let Some(admission) = entry.admit_known(peer) {
                return admission;
            }
        }
        for entry in self.entries {
            // Owned by this peer since the first pass looked: no second entry.
            if let Some(admission) = entry.admit_known(peer) {
                return admission;
            }
            match entry.claim(peer) {
                PeerClaim::Won => {
                    // This scan is not atomic: an entry it passed as taken may have
                    // been freed and claimed for the same peer since, by a frame
                    // that came after it (R2-1). Of two such claims at least one
                    // sees the other here; it gives its entry back and classifies
                    // against the other, so the peer never keeps two entries.
                    let other = self
                        .entries
                        .iter()
                        .any(|other| !core::ptr::eq(other, entry) && other.holds(peer.peer));
                    if other {
                        entry.give_back();
                        return self
                            .entries
                            .iter()
                            .find_map(|other| other.admit_known(peer))
                            .unwrap_or(Ok(PeerAdmission::Current));
                    }
                    entry.publish(peer);
                    return Ok(PeerAdmission::Current);
                }
                // A first sighting racing another first sighting of the same peer.
                PeerClaim::SamePeer => {
                    return entry
                        .admit_known(peer)
                        .unwrap_or(Ok(PeerAdmission::Current));
                }
                PeerClaim::Taken => {}
            }
        }
        Err(ReceiveError::PeerTableFull)
    }

    /// Only a user of `entry`'s current tenure calls this, so the entry cannot be
    /// freed or change hands while this raises its incarnation.
    fn classify(entry: &PeerEntry, peer: PeerRef) -> Result<PeerAdmission, ReceiveError> {
        let previous = entry
            .incarnation
            .fetch_max(peer.incarnation, Ordering::AcqRel);
        match previous.cmp(&peer.incarnation) {
            // Nothing recorded: the peer was first seen at incarnation 0. Its
            // first numbered incarnation is a first sighting, not a restart.
            core::cmp::Ordering::Less if previous == 0 => Ok(PeerAdmission::Current),
            core::cmp::Ordering::Less => Ok(PeerAdmission::Restarted),
            core::cmp::Ordering::Equal => Ok(PeerAdmission::Current),
            core::cmp::Ordering::Greater => Err(ReceiveError::StalePeer),
        }
    }

    /// Frees the entry of a peer the transport reported gone, so its place can
    /// serve another peer. Bounded by the table.
    ///
    /// Safe to race a frame of that peer being admitted: the frame either is
    /// classified against the old entry, which is freed only after it, or finds
    /// the entry gone and is admitted as a first sighting. The next peer to take
    /// the entry starts from its own incarnation.
    pub fn forget(&self, peer: PeerRef) {
        if peer.peer == 0 {
            return;
        }
        for entry in self.entries {
            entry.retire(peer.peer);
        }
    }

    /// Records a peer event reported by the transport; true if it was a restart.
    pub fn admit_event(&self, peer: PeerRef) -> bool {
        matches!(self.admit(peer), Ok(PeerAdmission::Restarted))
    }

    /// The highest incarnation recorded so far, for calls being reserved.
    #[must_use]
    pub fn latest(&self) -> u64 {
        self.entries
            .iter()
            .map(|entry| entry.incarnation.load(Ordering::Acquire))
            .max()
            .unwrap_or(0)
    }

    /// Test hook: makes free entry 0 owned by `peer` with no incarnation recorded,
    /// as for a peer first seen at incarnation 0.
    #[doc(hidden)]
    #[cfg(up_rust_test_hooks)]
    pub fn __claim_first_entry(&self, peer: u64) {
        let first = PeerRef {
            peer,
            incarnation: 0,
        };
        if let Some(entry) = self.entries.first() {
            if let PeerClaim::Won = entry.claim(first) {
                entry.publish(first);
            }
        }
    }
}

// The monitor lives in separately allocated, exclusively armed startup storage.
use super::deadline_timing::DeadlineSlot;

/// A non-zero time-to-live in whole milliseconds, rounded up. A truncated
/// sub-millisecond TTL would reach the zero `UUID::is_expired_at` reads as
/// "never expires", and fail open (M1).
pub(crate) fn ttl_millis(ttl: Duration) -> u32 {
    u32::try_from(ttl.as_nanos().div_ceil(1_000_000)).unwrap_or(u32::MAX)
}

/// Addressing and admission shared by every kind of inbound route.
pub(crate) struct RxCommon {
    pub(crate) name: &'static RouteName,
    pub(crate) id: RouteId,
    pub(crate) binding_route: BindingRoute,
    pub(crate) kind: Option<UMessageType>,
    pub(crate) source_filter: UUri,
    pub(crate) sink_filter: Option<UUri>,
    /// The deployment's clients, on a server's request route. Empty means every
    /// source is admitted. Checked before a frame can claim a peer entry (H1).
    pub(crate) clients: &'static [UUri],
    pub(crate) envelope: Envelope,
    pub(crate) admitted: &'static [AdmittedWire],
    pub(crate) limits: HeaderLimits,
    pub(crate) peers: PeerTable,
    pub(crate) counters: Counters,
    pub(crate) qos: Qos,
    pub(crate) heap: RuntimeHeap,
    pub(crate) deviations: &'static [&'static super::Deviation],
    pub(crate) shared: &'static TransportShared,
    pub(crate) monitor: DeadlineSlot,
    pub(crate) wake: RouteWake,
}

impl RxCommon {
    /// Builds the part shared by subscribers and RPC receive routes.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        name: &'static RouteName,
        id: RouteId,
        binding_route: BindingRoute,
        kind: Option<UMessageType>,
        source_filter: UUri,
        sink_filter: Option<UUri>,
        clients: &'static [UUri],
        envelope: Envelope,
        admitted: &'static [AdmittedWire],
        limits: HeaderLimits,
        peers: PeerTable,
        qos: Qos,
        heap: RuntimeHeap,
        deviations: &'static [&'static super::Deviation],
        shared: &'static TransportShared,
    ) -> Self {
        Self {
            name,
            id,
            binding_route,
            kind,
            source_filter,
            sink_filter,
            clients,
            envelope,
            admitted,
            limits,
            peers,
            counters: Counters::default(),
            qos,
            heap,
            deviations,
            shared,
            monitor: DeadlineSlot::new(),
            wake: RouteWake::new(),
        }
    }

    /// Filters a parsed frame by kind and address, then admits its peer.
    pub(crate) fn admit_addressing(
        &self,
        parsed: &ParsedFrame<'_>,
        peer: PeerRef,
    ) -> Result<PeerAdmission, ReceiveError> {
        let header = &parsed.header;
        let sink_matches = match (&self.sink_filter, &header.sink) {
            (None, None) => true,
            (Some(filter), Some(actual)) => filter.matches(actual),
            (None, Some(_)) | (Some(_), None) => false,
        };
        if self.kind.is_some_and(|kind| kind != header.kind)
            || !self.source_filter.matches(&header.source)
            || !sink_matches
        {
            add(&self.counters.refused);
            return Err(ReceiveError::WrongRoute);
        }
        // Before the peer table: a stranger must cost no entry (H1).
        if !self.clients.is_empty()
            && !self
                .clients
                .iter()
                .any(|known| known.matches(&header.source))
        {
            add(&self.counters.unknown_client);
            return Err(ReceiveError::UnknownClient);
        }
        self.peers.admit(peer).inspect_err(|refusal| {
            if matches!(refusal, ReceiveError::StalePeer) {
                add(&self.counters.stale_peer);
            } else {
                add(&self.counters.refused);
            }
        })
    }

    /// Selects the admitted wire that matches the frame's payload identity.
    pub(crate) fn wire_index(&self, parsed: &ParsedFrame<'_>) -> Result<usize, ReceiveError> {
        let header = &parsed.header;
        let unknown = |encoding: u32| {
            add(&self.counters.refused);
            ReceiveError::UnknownEncoding(encoding)
        };
        let matches = |wire: &AdmittedWire| {
            Some(wire.encoding) == header.payload_encoding
                && wire.native_type_token == header.native_type_token
        };
        match parsed.matched {
            Some(index) => match self.admitted.get(index) {
                Some(wire) if matches(wire) => Ok(index),
                _ => Err(unknown(header.payload_encoding.map_or(0, |e| e.id()))),
            },
            None => self
                .admitted
                .iter()
                .position(matches)
                .ok_or_else(|| unknown(header.payload_encoding.map_or(0, |e| e.id()))),
        }
    }

    /// Wakes every waiter on this route.
    pub(crate) fn wake_waiters(&self) {
        self.wake.wake();
        self.shared.notify();
    }

    /// R56/D7 queue arrival, before SDK parse/TTL/admission or overflow refusal.
    /// A pull binding calls RuntimePort::arrived at its real queue boundary;
    /// taking a frame and a readability hint are not arrival evidence.
    // [impl->req~ssr-qos-deadline~3]
    pub(crate) fn queue_arrived(&self, sampled: Monotonic) {
        self.monitor.arrival(sampled.0, &self.counters.deadline_missed);
        // Unconditional after the wrapper: successful rearm may return false.
        // No second counter add. A delayed arrival winner remains a J obligation.
        self.shared.notify_deadline_service();
        self.shared.notify();
    }

    pub(crate) fn check_deadline_at(&self, sampled: Monotonic) -> bool {
        self.monitor.check(sampled.0, &self.counters.deadline_missed)
    }

    pub(crate) fn check_deadline(&self) -> bool {
        self.check_deadline_at(self.shared.platform.now())
    }

    /// Whether the frame's time-to-live has passed. Refuses and counts when the
    /// clock cannot be read (fail closed: an unknown expiry is not processed).
    pub(crate) fn check_ttl(&self, header: &RxHeader<'_>) -> Result<(), ReceiveError> {
        let Some(ttl) = header.ttl else {
            return Ok(());
        };
        if ttl.is_zero() {
            return Ok(());
        }
        let Some(now) = self.shared.platform.unix_time_ms() else {
            add(&self.counters.clock_unavailable);
            return Err(ReceiveError::ClockUnavailable);
        };
        if header.id.is_expired_at(ttl_millis(ttl), u128::from(now)) {
            add(&self.counters.expired);
            return Err(ReceiveError::Expired);
        }
        Ok(())
    }

    /// Fixed report of this route.
    pub(crate) fn report(&self, slots: usize, slot_len: usize) -> RouteReport {
        RouteReport {
            route: self.name,
            id: self.id,
            send_mode: None,
            slots,
            slot_len,
            envelope: self.envelope,
            qos: self.qos,
            heap: self.heap,
            native_payload: self.admitted.iter().any(|wire| wire.native_type_token.is_some()),
            deviations: self.deviations,
            lifetime_policy: self.shared.lifetime_policy(),
            deadline_timing: self.monitor.get().map_or_else(
                || super::deadline_timing::DeadlineTimingReport::pending(self.qos.deadline.duration()),
                |armed| armed.report),
        }
    }
}

/// Non-owning raw extent. It grants no access permission or lifetime extension.
/// Moving an inline owner can invalidate it; persistent views require final placement.
///
/// A retained frame's metadata and payload can live in separate allocations, so
/// no single base pointer can describe both.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ByteRange {
    pub(crate) start: *const u8,
    pub(crate) len: usize,
}

impl ByteRange {
    /// # Safety-usable invariant
    /// Records exactly bytes' pointer/length with original provenance. It remains
    /// dereferenceable only while that allocation/owner stays alive, unmoved and
    /// immutably borrowed; this raw value does not preserve that borrow itself.
    pub(crate) fn of(bytes: &[u8]) -> Self {
        Self {
            start: bytes.as_ptr(),
            len: bytes.len(),
        }
    }
    /// # Safety
    /// Caller must prove start is nonnull/aligned even for len0, and len bytes
    /// form one live allocation of initialized u8, <=isize::MAX without wrap.
    /// For all of 'a, the owner must stay at its captured address and keep that
    /// range immutable/live; no move, release, destruction, mutable access or
    /// reuse may invalidate it. The result borrows only those exact bytes.
    pub(crate) unsafe fn as_slice<'a>(self) -> &'a [u8] {
        // SAFETY: the explicit caller contract supplies each from_raw_parts
        // obligation (including zero length) for exactly the chosen lifetime'a.
        unsafe { core::slice::from_raw_parts(self.start, self.len) }
    }

}

/// A frame's byte views, destined for its slot's metadata.
struct SlotViews {
    text: (Option<ByteRange>, Option<ByteRange>),
    payload: Option<ByteRange>,
}

/// What a slot remembers about the frame it holds. Copied here at delivery, so
/// `&self` accessors never touch the owner.
struct SlotMeta {
    header: StaticHeader,
    // Safety invariant: ranges belong to this RX-owner's final frame/fixed
    // region, with valid initialized immutable extents through every lease borrow.
    token: Option<ByteRange>,
    // Safety invariant: same RX-owner range contract as token.
    traceparent: Option<ByteRange>,
    // Safety invariant: same RX-owner range contract as token, or absent.
    payload: Option<ByteRange>,
    peer: PeerRef,
    reply: ReplyToken,
    // Safety invariant: while this meta is live, true pairs with exactly one
    // initialized RetainedFrame in the same RxSlot; false pairs with no live F.
    retained: bool,
}

struct RxSlot {
    // Safety invariant: initialized only for a prepared/published payload owner;
    // ranges belong to that owner's fixed byte storage or final retained frame.
    meta: UnsafeCell<MaybeUninit<SlotMeta>>,
    // Safety invariant: initialized exactly when live meta.retained is true
    // (or during exclusive preparation before meta is written). Never moved or
    // accessed by owner callbacks after publication; destructor completes first.
    frame: UnsafeCell<MaybeUninit<RetainedFrame>>,
}

/// One admitted wire for a typed route.
pub(crate) struct TypedWire<T: 'static> {
    pub(crate) type_id: core::any::TypeId,
    pub(crate) wire: &'static dyn Wire<T>,
    pub(crate) info: crate::wire::WireInfo,
}

impl<T> Clone for TypedWire<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TypedWire<T> {}

/// What a receive route does with frames.
#[derive(Clone, Copy)]
pub(crate) enum RxKind<T: 'static> {
    /// Decode into a startup slot through one of the admitted wires.
    Encoded { wires: &'static [TypedWire<T>] },
    /// Keep the retained frame and borrow it, checked.
    InPlace { identity: WirePayloadIdentity },
}

/// A receive route for messages of type `T`.
pub struct TypedInbound<T: 'static, M: WireMode = Encoded> {
    pub(crate) common: RxCommon,
    kind: RxKind<T>,
    // Safety invariant: encoded slots are valid/vacant as tracked by DecodedSlot;
    // only the current producer/releaser may mutate. Published Received leases
    // read an initialized value. InPlace has no decoded values/accessors.
    values: &'static [super::decoded::DecodedSlot<T>],
    // Safety invariant: RX-owner: free, ready and slots have equal capacity.
    // One raw pool obligation is held by producer, queue, or Received. Initialized
    // meta is published before transfer; retained=true means one live frame at
    // its final address. Meta's ranges remain immutable until all lease borrows
    // end, then frame destruction completes before the pool obligation returns.
    slots: &'static [RxSlot],
    /// Per-slot copy of the whole frame, on a handoff route.
    frames: Option<ByteSlots>,
    /// Per-slot copy of the token and traceparent, on a lending encoded route,
    /// whose frame goes back as soon as it is decoded.
    text: Option<ByteSlots>,
    free: FreeIndices,
    ready: ReadyQueue,
    /// Existing DropNewest/Wait receive/decode credits, including in-flight pulls.
    /// DropOldest membership/occupancy is authoritative only in ready.root.
    queued: AtomicU32,
    /// Leases currently held by the application.
    held: AtomicU32,
    max_held: u32,
    depth: u32,
    overflow: Overflow,
    native: bool,
    _mode: PhantomData<fn() -> M>,
}

/// Unpublished receive owner. Its seed cleanup cannot outlive installation or
/// coexist with a usable route handle: callers get reports until install consumes
/// the guard, and the route reference is returned only after seed ownership moves.
pub(crate) struct PreparedInbound<T: 'static, M: WireMode> {
    route: &'static TypedInbound<T, M>,
    seeds: Option<super::decoded::InitialValues<T>>,
}

impl<T: Send + Sync + 'static, M: WireMode> PreparedInbound<T, M> {
    pub(crate) fn report(&self) -> RouteReport { self.route.report() }

    /// # Safety-usable invariant
    /// On success adds the complete route and consumes seed cleanup before a
    /// usable route handle escapes. add_rx has no fallible step after installing
    /// the dispatch entry. On error the route was not installed and seeds remain
    /// solely owned by this wrapper; its Drop clears them before returning.
    pub(crate) fn install(self, setup: &super::mode::TransportSetup)
        -> Result<&'static TypedInbound<T, M>, StartupError>
    {
        setup.add_rx(self.route)?;
        if let Some(seeds) = self.seeds { seeds.commit(); }
        Ok(self.route)
    }
}

impl<T: 'static, M: WireMode> fmt::Debug for TypedInbound<T, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TypedInbound")
            .field("route", self.common.name)
            .finish_non_exhaustive()
    }
}

// SAFETY: slots and values are reachable only through the index that a producer
// popped from `free` or a consumer popped from `ready`, so each is used by one
// owner at a time. DecodedSlot flags and values are mutated only under that
// unique slot obligation; free/ready transfer uses the existing Release/Acquire
// protocol. Setup seeds are hidden until their guard commits. `T: Send + Sync`
// covers moving and sharing values across threads. The retained frames are `Send` by the `Frame` contract and are never
// shared by reference.
unsafe impl<T: Send + Sync, M: WireMode> Send for TypedInbound<T, M> {}
// SAFETY: see `Send`.
unsafe impl<T: Send + Sync, M: WireMode> Sync for TypedInbound<T, M> {}

impl<T: Send + Sync + 'static, M: WireMode> TypedInbound<T, M> {
    /// Reserves everything a receive route needs. `wire_for_init` seeds each
    /// encoded value; its cleanup remains guarded until route installation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn place(
        memory: &crate::memory::StartupMemory,
        common: RxCommon,
        kind: RxKind<T>,
        depth: usize,
        max_held: usize,
        frame_len: usize,
        text_len: usize,
        native: bool,
        wire_for_init: Option<&'static dyn Wire<T>>,
    ) -> Result<PreparedInbound<T, M>, StartupError> {
        let count = depth.saturating_add(max_held).saturating_add(1);
        let slot_count = u32::try_from(count).map_err(|_| {
            StartupError::Memory(crate::memory::StartupError::MemoryExhausted {
                requested: count,
                remaining: 0,
            })
        })?;
        let free = FreeIndices::new_in(memory, count)?.with_policy(common.shared.lifetime_policy());
        let ready = ReadyQueue::new_in(memory, slot_count, depth as u32, max_held)?;
        let mut slots = memory.fixed_vec::<RxSlot>(count)?;
        for _ in 0..count {
            slots.push(RxSlot {
                meta: UnsafeCell::new(MaybeUninit::uninit()),
                frame: UnsafeCell::new(MaybeUninit::uninit()),
            })?;
        }
        let seeds = wire_for_init
            .map(|wire| super::decoded::initialize(memory, wire, count)).transpose()?;
        let values = seeds.as_ref().map_or(&[][..], |seeds| seeds.slots());
        let frames = if frame_len == 0 {
            None
        } else {
            Some(ByteSlots::new_in(memory, count, frame_len)?)
        };
        let text = if text_len == 0 {
            None
        } else {
            Some(ByteSlots::new_in(memory, count, text_len)?)
        };
        let overflow = common.qos.overflow;
        let route = memory.place(Self {
            common,
            kind,
            values,
            slots: slots.into_static_slice(),
            frames,
            text,
            free,
            ready,
            queued: AtomicU32::new(0),
            held: AtomicU32::new(0),
            max_held: u32::try_from(max_held).unwrap_or(u32::MAX),
            depth: u32::try_from(depth).unwrap_or(u32::MAX),
            overflow,
            native,
            _mode: PhantomData,
        })?;
        Ok(PreparedInbound { route: &*route, seeds })
    }

    /// Acquires actual payload storage. Full is the existing pool's exact
    /// all-leased observation, distinct from unread-depth overflow. Preparation
    /// never destroys an unread owner merely to speculate on a new frame.
    /// # Safety-usable invariant
    /// Ok transfers FreeIndices.pop's unique index obligation for this route;
    /// RX-owner geometry makes it in range of slots/values/byte regions.
    fn claim_slot(&'static self) -> Result<u32, ReceiveError> {
        match self.free.pop() {
            Ok(index) => Ok(index),
            Err(super::PoolError::PoolRetired) => Err(ReceiveError::PoolRetired),
            Err(super::PoolError::Full) => {
                add(&self.common.counters.slots_exhausted);
                Err(ReceiveError::SlotsExhausted)
            }
        }
    }

    /// Reserves queue depth for `DropNewest` and `Wait`, counting a refusal: a
    /// dropped frame under `DropNewest`, and under `Wait` backpressure (`busy`),
    /// since the sender keeps the frame and retries.
    fn reserve_depth(&self) -> bool {
        if self.try_reserve_depth() {
            return true;
        }
        if matches!(self.overflow, Overflow::Wait { .. }) {
            add(&self.common.counters.busy);
        } else {
            add(&self.common.counters.dropped_newest);
        }
        false
    }

    /// Reserves queue depth, counting nothing. An admission with a compare-exchange,
    /// confirmed with a read-modify-write before refusing, so a stale read never
    /// refuses a frame there is room for.
    fn try_reserve_depth(&self) -> bool {
        loop {
            let queued = self.queued.load(Ordering::Acquire);
            if queued < self.depth {
                if self
                    .queued
                    .compare_exchange_weak(
                        queued,
                        queued.wrapping_add(1),
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    return true;
                }
                continue;
            }
            // Full by a plain read. Confirm against the newest value.
            if self.queued.fetch_add(0, Ordering::AcqRel) >= self.depth {
                return false;
            }
        }
    }

    /// Returns a slot to the pool, dropping any retained frame it held.
    /// # Safety
    /// Caller owns RX-owner's initialized index exclusively with no live views.
    /// Drop completes the retained owner and any owned decoded value before
    /// consuming the one pool obligation; unwind keeps that index withheld.
    unsafe fn release_slot(&self, index: u32) {
        // SAFETY: RX-owner and the caller's unique initialized index prove meta's
        // assume_init_ref and retained frame's assume_init_drop. No callback can
        // obtain this index before free.push; if Drop unwinds the index is not
        // returned. DecodedSlot::clear receives the same unique index after every
        // value borrow ended; its flag excludes a second owned-value destructor.
        // Completed cleanup and no live views discharge push's contract.
        unsafe {
            if let Some(slot) = self.slots.get(index as usize) {
                // SAFETY: the caller owns `index` (popped from `ready` or a dropping
                // lease), and its meta was written before it was published.
                let retained = { (*slot.meta.get()).assume_init_ref().retained };
                if retained {
                    // SAFETY: as above; a retained slot holds an initialized frame,
                    // and this is the only drop of it. The index is released after.
                    { (*slot.frame.get()).assume_init_drop() };
                }
            }
            if let Some(value) = self.values.get(index as usize) { value.clear(); }
            self.free.push(index);
        }
    }

    /// Commits the incoming owner and any exact overflow victim together.
    /// No payload/meta access follows publication: another caller may already
    /// have consumed, returned and reused the incoming slot.
    /// # Safety
    /// Caller uniquely owns an initialized RX-owner slot with no surviving
    /// mutable borrow; reserved_depth names its own outstanding depth credit.
    /// This consumes the index on every normal return, by publication or cleanup.
    unsafe fn publish(&'static self, index: u32, reserved_depth: bool) -> Result<(), ReceiveError> {
        // SAFETY: RX-owner gives index bounds, initialized meta and exclusive
        // producer ownership. slot_meta reads before transfer. ready.offer's
        // contract is satisfied; success ends incoming access and Replaced gives
        // exclusive victim ownership. Full preserves incoming ownership, allowing
        // release_slot. Each release completes cleanup before pool return.
        unsafe {
            match self.ready.offer(index, matches!(self.overflow, Overflow::DropOldest)) {
                ReadyOffer::Inserted => {},
                ReadyOffer::Replaced(victim) => {
                    add(&self.common.counters.dropped_oldest);
                    self.release_slot(victim);
                }
                ReadyOffer::Full => {
                    self.unreserve(reserved_depth);
                    if matches!(self.overflow, Overflow::Wait { .. }) {
                        add(&self.common.counters.busy);
                    } else { add(&self.common.counters.dropped_newest); }
                    self.release_slot(index);
                    self.common.wake_waiters();
                    return Err(ReceiveError::SlotsExhausted);
                }
            }
            add(&self.common.counters.carried);
            self.common.wake_waiters();
            Ok(())
        }
    }

    fn parse_and_admit<'a>(
        &self,
        metadata: &'a [u8],
        payload: PayloadBytes<'a>,
        peer: PeerRef,
    ) -> Result<(ParsedFrame<'a>, usize), ReceiveError> {
        let parsed = self
            .common
            .envelope
            .parse(self.common.admitted, metadata, payload, &self.common.limits)
            .map_err(|error| {
                add(&self.common.counters.refused);
                ReceiveError::Header(error)
            })?;
        self.common.admit_addressing(&parsed, peer)?;
        let wire_index = self.common.wire_index(&parsed)?;
        Ok((parsed, wire_index))
    }

    fn refuse_wire(&self, error: WireError) -> ReceiveError {
        add(&self.common.counters.refused);
        ReceiveError::Wire(error)
    }

    /// `decode_into_slot` with its slot and, if `reserved`, its frame's depth under
    /// a [`DecodeGuard`]: a wire whose `decode` unwinds costs that frame, not the
    /// route.
    /// # Safety
    /// Caller exclusively owns this valid encoded index and its optional depth
    /// credit; no retained frame is stored. Keeps ownership on normal return,
    /// but consumes/returns it on decode unwind. No other access may overlap.
    unsafe fn decode_guarded(
        &self,
        index: u32,
        wire_index: usize,
        parsed: &ParsedFrame<'_>,
        body: &[u8],
        reserved: bool,
    ) -> Result<(), ReceiveError> {
        // SAFETY: the explicit caller contract initializes DecodeGuard's unique
        // index obligation and permits decode_into_slot's exclusive initialized T
        // borrow. Safe decode can unwind/reenter; the guard remains sole return
        // owner on unwind, and returned() suppresses its drop after normal return.
        unsafe {
            let guard = DecodeGuard {
                route: self,
                index,
                reserved,
            };
            let decoded = self.decode_into_slot(index, wire_index, parsed, body);
            guard.returned();
            decoded
        }
    }

    /// Decodes `body` through the selected wire into this slot's value.
    // [impl->req~ssr-unsafe-wire-untrusted~1]
    /// # Safety
    /// Caller exclusively owns this encoded slot throughout initialization and
    /// decode, including arbitrary safe callback reentry. No value borrow overlaps
    /// that interval. A vacant value may be initialized here; success leaves valid
    /// T. Init unwind can leave vacant/leaked storage; decode error/unwind after
    /// successful initialization leaves valid T, without semantic success.
    unsafe fn decode_into_slot(
        &self,
        index: u32,
        wire_index: usize,
        parsed: &ParsedFrame<'_>,
        body: &[u8],
    ) -> Result<(), ReceiveError> {
        // SAFETY: checked values.get and the RX-owner obligation give exclusive
        // typed storage for the complete temporary borrow. get_or_init establishes
        // initialized T before decode sees &mut T. Safe decode may replace/panic,
        // but cannot invalidate T. Ordinary assignment through &mut T installs
        // its replacement even when the old value's Drop unwinds (Rust's
        // drop-and-replace cleanup), so logical decode success is not validity.
        // Initialization unwind is handled as vacant by DecodeGuard; encoding
        // semantics and a no-panic promise are not memory-safety premises.
        {
            let RxKind::Encoded { wires } = self.kind else {
                return Err(ReceiveError::Unsupported("not an encoded route"));
            };
            let encoding = parsed
                .header
                .payload_encoding
                .ok_or_else(|| self.refuse_wire(WireError::Malformed("no payload")))?;
            let wire = wires
                .get(wire_index)
                .ok_or(ReceiveError::UnknownEncoding(encoding.id()))?;
            let value = self
                .values
                .get(index as usize)
                .ok_or(ReceiveError::SlotsExhausted)?;
            // SAFETY: caller holds this unpublished index through the complete
            // mutable decode borrow and every initializer/decode callback. No
            // value view or pool return can overlap; the borrow ends at return.
            let into = unsafe { value.get_or_init(wire.wire) }
                .map_err(|error| self.refuse_wire(error))?;
            wire.wire
                .decode(encoding, body, into)
                .map_err(|error| self.refuse_wire(error))
        }
    }

    /// Records the frame's views in the slot. Each `ByteRange` must stay valid
    /// until the slot is released: `store_views` takes them from bytes the slot
    /// keeps, and `copy_text` from the slot's own text region.
    /// # Safety
    /// Caller exclusively owns index; every views range must satisfy
    /// ByteRange::as_slice for every later lease borrow until complete slot
    /// release. Under retained=true a matching live final-address frame must
    /// exist; otherwise ranges belong to this index's fixed byte/text storage.
    /// Replaces no live owned metadata; success establishes RX-owner publication.
    unsafe fn store_meta(
        &self,
        index: u32,
        parsed: &ParsedFrame<'_>,
        views: SlotViews,
        peer: PeerRef,
        reply: ReplyToken,
        retained: bool,
    ) -> Result<(), ReceiveError> {
        // SAFETY: slots.get checks bounds; exclusive producer ownership permits
        // writing MaybeUninit<SlotMeta>. views/retained obligations are the caller
        // contract; writing valid scalar/header metadata initializes exactly one
        // slot and does not publish it yet. No arbitrary callback receives storage.
        {
            let SlotViews {
                text: (token, traceparent),
                payload,
            } = views;
            let slot = self
                .slots
                .get(index as usize)
                .ok_or(ReceiveError::SlotsExhausted)?;
            // SAFETY: this caller holds the slot's index exclusively until publish.
            unsafe {
                (*slot.meta.get()).write(SlotMeta {
                    header: parsed.header.clone().with_text(None, None),
                    token,
                    traceparent,
                    payload,
                    peer,
                    reply,
                    retained,
                });
            }
            Ok(())
        }
    }

    /// The header's text and payload views, for a frame the slot itself will keep.
    fn store_views(parsed: &ParsedFrame<'_>, body: Option<&[u8]>) -> SlotViews {
        SlotViews {
            text: (
                parsed
                    .header
                    .token()
                    .map(|text| ByteRange::of(text.as_bytes())),
                parsed
                    .header
                    .traceparent()
                    .map(|text| ByteRange::of(text.as_bytes())),
            ),
            payload: body.map(ByteRange::of),
        }
    }

    /// Copies the token and traceparent into the slot's text region, so `header`
    /// keeps working after the frame has gone back to the transport.
    /// # Safety
    /// Caller exclusively owns this index and has no existing views into its
    /// text region. Returned raw ranges belong to disjoint initialized subranges
    /// in permanent slot storage; keep the slot owned until their last use.
    unsafe fn copy_text(
        &self,
        index: u32,
        parsed: &ParsedFrame<'_>,
    ) -> (Option<ByteRange>, Option<ByteRange>) {
        // SAFETY: the caller's index ownership discharges ByteSlots.slot_mut.
        // Checked split_at_mut partitions disjoint pieces; copying initialized
        // str bytes preserves initialization. ByteRange::of records each piece
        // without dereferencing after its mutable borrow ends; no later piece
        // write overlaps an earlier range. Missing/short storage returns None.
        {
            let Some(region) = &self.text else {
                return (None, None);
            };
            // SAFETY: this caller holds the slot's index exclusively until publish.
            let mut rest: &mut [u8] = unsafe { region.slot_mut(index) };
            // Each copy takes its own split-off piece, so copying the second text
            // cannot invalidate the view of the first.
            let mut copy = |text: &str| -> Option<ByteRange> {
                let bytes = text.as_bytes();
                let (target, tail) = core::mem::take(&mut rest).split_at_mut_checked(bytes.len())?;
                rest = tail;
                target.copy_from_slice(bytes);
                Some(ByteRange::of(target))
            };
            let token = parsed.header.token().and_then(&mut copy);
            let traceparent = parsed.header.traceparent().and_then(&mut copy);
            (token, traceparent)
        }
    }

    /// The most frames one call examines on this route, refused ones included:
    /// the route's depth plus one, as on a carrier, so a peer that keeps sending
    /// cannot hold a receive call.
    fn pull_bound(&self) -> usize {
        (self.depth as usize).saturating_add(1)
    }

    /// Pulls frames until one is ready for the application or the bound is reached.
    ///
    /// Each pull takes one frame, so a binding that holds frames itself, lending or
    /// handoff, keeps them in its own queue, where its overflow policy applies (R44).
    /// A push binding's frames arrive in the inbox, where this route's policy
    /// applies. [`TransportRuntime::pump`](super::TransportRuntime::pump) leaves
    /// this route alone, so a pull route has the one queue (R2-2).
    /// # Safety
    /// Caller exclusively owns an unreleased ready consumer reservation for
    /// this entire call. Some transfers a fully initialized RX-owner index;
    /// reservation remains held and its protections are cleared between pops.
    unsafe fn next_ready(&'static self, context: usize) -> Option<u32> {
        // SAFETY: the context contract passes unchanged to pop_ready, including
        // after pump callbacks. Reentrant receivers must acquire their own context.
        // Only successful pop returns payload ownership; no arbitrary binding can
        // forge a slot index through the safe InboundDispatch interface.
        unsafe {
            let budget = super::ReceiveBudget::new(self.pull_bound());
            loop {
                if let Some(index) = self.pop_ready(context) {
                    return Some(index);
                }
                if self.free.lifetime() == super::LifetimeState::Retired || self.common.shared.receive_fault(self.common.binding_route).is_some() { return None; }
                if budget.remaining() == 0 {
                    // More may be waiting: make sure a parked caller looks again.
                    self.common.wake_waiters();
                    return None;
                }
                if self
                    .common
                    .shared
                    .pump_route_bounded(self.common.binding_route, self, 1, &budget)
                    == 0
                {
                    return self.pop_ready(context);
                }
            }
        }
    }

    /// Takes the next received frame, if any.
    /// # Safety
    /// Caller holds this ready queue's exclusive consumer reservation throughout.
    /// Some transfers sole RX-owner index ownership, meta initialized; None
    /// transfers none. Every skipped expired payload is completely released.
    unsafe fn pop_ready(&'static self, context: usize) -> Option<u32> {
        // SAFETY: ready.pop has the reserved context contract; Some gives sole
        // initialized payload ownership by RX-owner and queue publication. Checked
        // slot access and that ownership justify assume_init_ref. TTL inspection
        // uses this initialized header; release_slot consumes only an expired index.
        unsafe {
            // TTL'd frames are skipped inside the bound of the slot count.
            let bound = self.slots.len().saturating_add(1);
            for _ in 0..bound {
                let index = self.ready.pop(context)?;
                if self.reserves_depth() { self.queued.fetch_sub(1, Ordering::AcqRel); }
                let slot = self.slots.get(index as usize)?;
                // SAFETY: this caller popped `index` from `ready`, and its meta was
                // written before it was published.
                let header = { &(*slot.meta.get()).assume_init_ref().header };
                match self.common.check_ttl(header) {
                    Ok(()) => return Some(index),
                    Err(_) => self.release_slot(index),
                }
            }
            None
        }
    }

    /// The route's waker storage, for the async layer.
    #[doc(hidden)]
    /// Returns process-lifetime wake storage for this live call/route, or None for an invalid
    /// slot. Only the owning adapter registers or clears it; completion threads may wake it. A
    /// static storage lifetime does not extend slot ownership: clear on Call/Recv drop, never
    /// use the registration after reuse. Register before waiting and poll again after
    /// registration and each wake. This accessor neither blocks nor uses the heap.
    // [impl->req~ssr-async-seam~1]
    pub fn __wake(&'static self) -> &'static RouteWake {
        &self.common.wake
    }

    /// `try_recv` for the L2 handle.
    ///
    /// # Errors
    /// `MaxHeldExceeded` when the application already holds every lease.
    pub(crate) fn try_receive(&'static self) -> Result<Option<Received<T, M>>, ReceiveError> {
        let taken = self.take_lease();
        // Check after this attempt. The binding separately reports real queue
        // ingress, including refused frames; taking buffered data is not arrival.
        self.common.check_deadline();
        taken
    }

    /// Reserves a lease, then takes the next frame, so a lease exists for it before
    /// the frame is handed over and concurrent consumers cannot exceed `max_held`.
    fn take_lease(&'static self) -> Result<Option<Received<T, M>>, ReceiveError> {
        // SAFETY: the successful held CAS reserves one existing max_held credit.
        // reserve_consumer supplies a unique context; LeaseReservation owns it on
        // all exits. next_ready receives that exact context; Some gives RX-owner
        // slot ownership for Received, while reservation.taken ends context use.
        unsafe {
            let mut held = self.held.load(Ordering::Acquire);
            loop {
                if held >= self.max_held {
                    return Err(ReceiveError::MaxHeldExceeded);
                }
                match self.held.compare_exchange_weak(
                    held,
                    held.saturating_add(1),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break,
                    Err(actual) => held = actual,
                }
            }
            // Given back unless a `Received` takes it over: also when a wire's `decode`
            // unwinds out of the pull below (R2-4).
            let context = self.ready.reserve_consumer();
            let reservation = LeaseReservation { route: self, context };
            let Some(index) = self.next_ready(context) else {
                if self.free.lifetime() == super::LifetimeState::Retired { return Err(ReceiveError::PoolRetired); }
                if let Some(fault) = self.common.shared.receive_fault(self.common.binding_route) { return Err(fault); }
                return Ok(None);
            };
            reservation.taken();
            Ok(Some(Received {
                route: self,
                index,
                _mode: PhantomData,
            }))
        }
    }

    /// # Safety
    /// Caller holds an initialized RX-owner index for this self borrow and
    /// excludes mutation/drop/reuse of its meta. Returns that slot's metadata
    /// with only the self-borrow lifetime; this does not grant owner callbacks.
    unsafe fn slot_meta(&self, index: u32) -> &SlotMeta {
        // SAFETY: RX-owner plus caller contract gives in-range initialized meta
        // and no writer for this borrow. assume_init_ref thus reads one valid
        // SlotMeta; the missing-index branch is excluded by the explicit bound.
        {
            let slot = self.slots.get(index as usize);
            // SAFETY: a `Received` exists only for a published slot, and it owns the
            // index until dropped, so the meta is initialized and not being written.
            match slot {
                // SAFETY: as the comment above: this index was published, and the
                // lease holds it until dropped.
                Some(slot) => unsafe { (*slot.meta.get()).assume_init_ref() },
                None => {
                    // A `Received` is only ever created for an index popped from
                    // `ready`, which is below the slot count.
                    // SAFETY: unreachable by construction; see above.
                    unsafe { core::hint::unreachable_unchecked() }
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn __review_queue_snapshot(&self) -> (usize, u32, Option<u32>) { self.ready.review_snapshot() }

    #[cfg(test)]
    pub(crate) fn __queued_for_test(&self) -> u32 {
        if self.reserves_depth() { self.queued.load(Ordering::Acquire) }
        else { self.ready.unread_for_test() }
    }

    /// Whether this route holds queue depth for each frame it queues: `DropNewest`
    /// and `Wait` refuse an arrival when full; `DropOldest` makes room instead.
    fn reserves_depth(&self) -> bool {
        !matches!(self.overflow, Overflow::DropOldest)
    }

    /// Gives back depth reserved for a frame that will not be queued.
    fn unreserve(&self, held: bool) {
        if held {
            self.queued.fetch_sub(1, Ordering::AcqRel);
        }
    }

    /// Queues a retained frame. `pulled` says the frame was taken from a pull
    /// binding after [`InboundDispatch::begin_pull`] reserved its depth.
    fn deliver_retained_inner(
        &'static self,
        frame: RetainedFrame,
        pulled: bool,
    ) -> Result<(), ReceiveError> {
        // SAFETY: claim_slot's documented postcondition supplies the unique
        // in-range producer index. Before publish no other code receives it.
        // decode_guarded retains it on normal return and returns it on unwind;
        // store_meta/copy_text use only that exclusive slot. retain_in_slot moves
        // frame to final storage before views and cleans it on Err. The local
        // release closure is called only for this index with no retained owner.
        // publish is called only after successful initialization and consumes it.
        // Stack-local frame views end before moving/dropping the frame.
        unsafe {
            let reserved = self.reserves_depth();
            // Depth this frame already holds, given back on any refusal below,
            // including closure racing an already admitted pull.
            let held = reserved && pulled;
            if self.free.lifetime() == super::LifetimeState::Retired {
                self.unreserve(held);
                return Err(ReceiveError::PoolRetired);
            }
            let peer = frame.peer();
            // Views taken from `frame` here are used only while it stays in this
            // frame of the stack: the owner may hold its bytes inline, and it has not
            // reached its final address yet.
            // SAFETY: the owner retains these bytes; `frame` is alive for this call.
            let metadata: &[u8] = { &*frame.metadata_ptr() };
            let parts = frame.payload_parts();
            // SAFETY: as above.
            let payload_bytes = parts.map(|(bytes, _)| { &*bytes });
            let payload = payload_bytes.map_or(PayloadBytes::Absent, PayloadBytes::one);
            // Reject a malformed or foreign frame before it can evict anything.
            let (parsed, wire_index) = match self.parse_and_admit(metadata, payload, peer) {
                Ok(parsed) => parsed,
                Err(error) => {
                    self.unreserve(held);
                    drop(frame);
                    return Err(error);
                }
            };
            if let RxKind::InPlace { identity } = self.kind {
                if parsed.header.payload_encoding != Some(identity.encoding)
                    || parsed.header.native_type_token != identity.native_type_token
                {
                    add(&self.common.counters.refused);
                    self.unreserve(held);
                    drop(frame);
                    return Err(ReceiveError::UnknownEncoding(
                        parsed.header.payload_encoding.map_or(0, |e| e.id()),
                    ));
                }
            }
            // A pushed frame reserves depth here. `Wait` is bounded: the frame is
            // refused and the binding keeps it and waits.
            if reserved && !held && !self.reserve_depth() {
                drop(frame);
                return Err(ReceiveError::SlotsExhausted);
            }
            let release = |this: &Self, index: u32| {
                // Unique unpublished index; decode borrows ended. Clear owned T
                // before pool return so destructor reentry cannot reacquire it.
                if let Some(value) = this.values.get(index as usize) { value.clear(); }
                this.free.push(index);
                if reserved {
                    this.queued.fetch_sub(1, Ordering::AcqRel);
                }
            };
            let index = match self.claim_slot() {
                Ok(index) => index,
                Err(error) => {
                    if reserved {
                        self.queued.fetch_sub(1, Ordering::AcqRel);
                    }
                    drop(frame);
                    return Err(error);
                }
            };
            let body = parsed.payload.as_contiguous();
            let stored = match self.kind {
                RxKind::Encoded { .. } => {
                    // Decode into the startup slot from the frame where it is, then
                    // send the frame straight back. No view of it survives this block.
                    let decoded = match body {
                        Some(body) => self.decode_guarded(index, wire_index, &parsed, body, reserved),
                        None => {
                            add(&self.common.counters.refused);
                            Err(ReceiveError::Wire(WireError::Malformed("no payload")))
                        }
                    };
                    let stored = decoded.and_then(|()| {
                        self.store_meta(
                            index,
                            &parsed,
                            SlotViews {
                                text: self.copy_text(index, &parsed),
                                payload: None,
                            },
                            peer,
                            ReplyToken::default(),
                            false,
                        )
                    });
                    drop(frame);
                    stored
                }
                RxKind::InPlace { .. } => self.retain_in_slot(index, frame, peer),
            };
            if let Err(error) = stored {
                release(self, index);
                return Err(error);
            }
            self.publish(index, reserved)
        }
    }

    /// Moves the owner to the slot, its final address, and only then takes the
    /// views a lease will read. An owner may keep its bytes inline, so a view
    /// taken before this move could dangle.
    /// # Safety
    /// Caller exclusively owns an empty RX-owner index, with no outstanding views.
    /// Success initializes its retained frame/meta at final address; Err leaves
    /// no live frame and preserves the raw index for caller return. Unwind may
    /// leak the owned slot, but cannot publish or return a partially stored owner.
    unsafe fn retain_in_slot(
        &'static self,
        index: u32,
        frame: RetainedFrame,
        peer: PeerRef,
    ) -> Result<(), ReceiveError> {
        // SAFETY: checked slots.get and unique empty index permit frame.write.
        // RetainedFrame's raw-view postconditions apply while that final-address
        // owner is immutably borrowed. parse's returned references borrow those
        // bytes (or longer-lived safe data); no mutable owner callback occurs.
        // store_meta receives exactly these owner-bound ranges, retained=true.
        // On Err the only frame destructor runs before caller may return the index.
        unsafe {
            let slot = self
                .slots
                .get(index as usize)
                .ok_or(ReceiveError::SlotsExhausted)?;
            // SAFETY: this caller holds the slot's index exclusively until publish.
            let owner: &RetainedFrame = { (*slot.frame.get()).write(frame) };
            // SAFETY: the owner is at its final address and stays alive until the
            // slot is released, which is after every view below is last used.
            let metadata: &[u8] = { &*owner.metadata_ptr() };
            let parts = owner.payload_parts();
            // SAFETY: as above.
            let bytes = parts.map(|(bytes, _)| { &*bytes });
            let payload = bytes.map_or(PayloadBytes::Absent, PayloadBytes::one);
            let stored = self
                .common
                .envelope
                .parse(self.common.admitted, metadata, payload, &self.common.limits)
                .map_err(|error| {
                    add(&self.common.counters.refused);
                    ReceiveError::Header(error)
                })
                .and_then(|parsed| {
                    let views = Self::store_views(&parsed, parsed.payload.as_contiguous());
                    self.store_meta(index, &parsed, views, peer, ReplyToken::default(), true)
                });
            if stored.is_err() {
                // SAFETY: the owner was written above and nothing else can reach
                // this slot; this is its only drop.
                { (*slot.frame.get()).assume_init_drop() };
            }
            stored
        }
    }
}

/// Gives back a slot and, if `reserved`, its frame's queue depth when a wire's
/// `decode` unwinds (R2-4), as the carrier's `CarryEnd` does for a sink. Only an
/// unwind reaches its drop: a decode that returns calls `returned`, and the caller
/// publishes or releases the slot itself.
struct DecodeGuard<'r, T: 'static, M: WireMode> {
    route: &'r TypedInbound<T, M>,
    // Safety invariant: unique unpublished encoded index; value may be valid or
    // vacant, and no retained frame exists. Drop alone clears/returns it unless
    // returned() consumes the guard. Unwind never permits overlapping access.
    index: u32,
    reserved: bool,
}

impl<T: 'static, M: WireMode> DecodeGuard<'_, T, M> {
    fn returned(self) {
        core::mem::forget(self);
    }
}

impl<T: 'static, M: WireMode> Drop for DecodeGuard<'_, T, M> {
    fn drop(&mut self) {
        // Meta was never written, so this guard owns no retained frame.
        add(&self.route.common.counters.refused);
        // SAFETY: the guard owns this unpublished index, and every temporary
        // initialization/decode borrow has unwound. DecodedSlot tracks validity;
        // clear destroys only completed owned values before return. If Drop also
        // unwinds, the index remains withheld. No callback can acquire it early.
        unsafe {
            if let Some(value) = self.route.values.get(self.index as usize) { value.clear(); }
            self.route.free.push(self.index);
        }
        if self.reserved {
            self.route.queued.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

/// A lease reserved in `held` for a frame not taken yet. Given back on drop, unless
/// a `Received` takes it over (`taken`), which gives it back when it is dropped.
struct LeaseReservation<'r, T: 'static, M: WireMode> {
    route: &'r TypedInbound<T, M>,
    // Safety invariant: one unreleased consumer reservation of route.ready and
    // one held credit; no pop survives Drop/taken. Forget leaks both, never reuses.
    context: usize,
}

impl<T: 'static, M: WireMode> LeaseReservation<'_, T, M> {
    fn taken(self) {
        // SAFETY: LeaseReservation.context holds this queue's unique reservation;
        // the pop call has ended and Drop/taken are mutually exclusive consumers.
        unsafe { self.route.ready.release_consumer(self.context) };
        // The existing held count transfers to Received; its Drop returns it.
        core::mem::forget(self);
    }
}

impl<T: 'static, M: WireMode> Drop for LeaseReservation<'_, T, M> {
    fn drop(&mut self) {
        // SAFETY: LeaseReservation.context holds this queue's unique reservation;
        // the pop call has ended and Drop/taken are mutually exclusive consumers.
        unsafe { self.route.ready.release_consumer(self.context) };
        self.route.held.fetch_sub(1, Ordering::AcqRel);
    }
}

impl<T: Send + Sync + 'static, M: WireMode> TypedInbound<T, M> {
    /// Queues a copy of a borrowed frame. `pulled` says the frame came from a pull
    /// binding after [`InboundDispatch::begin_pull`] reserved its depth.
    fn deliver_copied(
        &'static self,
        frame: InboundFrame<'_>,
        pulled: bool,
    ) -> Result<(), ReceiveError> {
        // SAFETY: claim_slot grants the unique in-range index. frames.slot_mut
        // is borrowed only before publication; copy_frame's initialized-u8 API
        // cannot deinitialize it. Checked copied slices stay in the same fixed
        // slot and parse returns references to those or longer-lived safe data.
        // decode_guarded owns its unwind return; local release consumes the same
        // index only after normal failure and no retained owner. store_meta records
        // views of this fixed slot; publish consumes the fully initialized index.
        unsafe {
            let reserved = self.reserves_depth();
            // Depth this frame already holds, given back on any refusal below,
            // including closure racing an already admitted pull.
            let held = reserved && pulled;
            if self.free.lifetime() == super::LifetimeState::Retired {
                self.unreserve(held);
                return Err(ReceiveError::PoolRetired);
            }
            let Some(frames) = self.frames.as_ref() else {
                add(&self.common.counters.refused);
                self.unreserve(held);
                return Err(ReceiveError::Unsupported(
                    "route holds retained frames only",
                ));
            };
            // A frame that cannot possibly fit is refused before it can evict anything.
            let needed = frame.metadata.len().saturating_add(frame.payload.len());
            if needed > frames.slot_len() {
                add(&self.common.counters.refused);
                self.unreserve(held);
                return Err(ReceiveError::TooLarge);
            }
            // Reject a malformed or foreign frame before claiming a slot. Chunked
            // input is parsed by the envelope with its chunks intact; only the
            // payload is gathered contiguous when it is copied below.
            let contiguous = frame.payload.as_contiguous();
            let (parsed, wire_index) =
                match self.parse_and_admit(frame.metadata, frame.payload, frame.peer) {
                    Ok(admitted) => admitted,
                    Err(error) => {
                        self.unreserve(held);
                        return Err(error);
                    }
                };
            // A pushed frame reserves depth here. `Wait` is bounded: the frame is
            // refused and the binding keeps it and waits.
            if reserved && !held && !self.reserve_depth() {
                return Err(ReceiveError::SlotsExhausted);
            }
            let release = |this: &Self, index: u32| {
                // Unique unpublished index; decode borrows ended. Clear owned T
                // before pool return so destructor reentry cannot reacquire it.
                if let Some(value) = this.values.get(index as usize) { value.clear(); }
                this.free.push(index);
                if reserved {
                    this.queued.fetch_sub(1, Ordering::AcqRel);
                }
            };
            let index = self.claim_slot().inspect_err(|_| {
                if reserved {
                    self.queued.fetch_sub(1, Ordering::AcqRel);
                }
            })?;
            // SAFETY: this caller holds the slot's index exclusively until publish.
            let bytes = { frames.slot_mut(index) };
            let copied = copy_frame(&frame, bytes);
            let (metadata_len, payload_len) = match copied {
                Ok(lengths) => lengths,
                Err(error) => {
                    release(self, index);
                    add(&self.common.counters.refused);
                    return Err(error);
                }
            };
            // The copy is contiguous, so parse the copy (zero-length chunks are fine).
            let Some((head, tail)) = bytes.split_at_checked(metadata_len) else {
                release(self, index);
                add(&self.common.counters.refused);
                return Err(ReceiveError::TooLarge);
            };
            let tail = tail.get(..payload_len).unwrap_or_default();
            let payload = if frame.payload.is_present() {
                PayloadBytes::one(tail)
            } else {
                PayloadBytes::Absent
            };
            let copied_parse =
                self.common
                    .envelope
                    .parse(self.common.admitted, head, payload, &self.common.limits);
            let copied_parsed = match copied_parse {
                Ok(parsed) => parsed,
                Err(error) => {
                    release(self, index);
                    add(&self.common.counters.refused);
                    return Err(ReceiveError::Header(error));
                }
            };
            let body = copied_parsed.payload.as_contiguous();
            let decoded = match self.kind {
                RxKind::Encoded { .. } => match body {
                    Some(body) => {
                        self.decode_guarded(index, wire_index, &copied_parsed, body, reserved)
                    }
                    None => Err(self.refuse_wire(WireError::Malformed("no payload"))),
                },
                RxKind::InPlace { .. } => {
                    add(&self.common.counters.refused);
                    Err(ReceiveError::Unsupported(
                        "native routes need a lending transport",
                    ))
                }
            };
            if let Err(error) = decoded {
                release(self, index);
                return Err(error);
            }
            let views = Self::store_views(&copied_parsed, body);
            let stored = self.store_meta(index, &copied_parsed, views, frame.peer, frame.reply, false);
            if let Err(error) = stored {
                release(self, index);
                return Err(error);
            }
            let _ = contiguous;
            let _ = parsed;
            self.publish(index, reserved)
        }
    }
}

impl<T: Send + Sync + 'static, M: WireMode> InboundDispatch for TypedInbound<T, M> {
    fn deliver(&'static self, frame: InboundFrame<'_>) -> Result<(), ReceiveError> {
        self.deliver_copied(frame, false)
    }

    fn deliver_retained(&'static self, frame: RetainedFrame) -> Result<(), ReceiveError> {
        self.deliver_retained_inner(frame, false)
    }

    fn begin_pull(&'static self) -> bool {
        self.free.lifetime() != super::LifetimeState::Retired && (!self.reserves_depth() || self.try_reserve_depth())
    }

    fn end_pull_unused(&'static self) {
        self.unreserve(self.reserves_depth());
    }

    fn deliver_pulled(&'static self, frame: InboundFrame<'_>) -> Result<(), ReceiveError> {
        self.deliver_copied(frame, true)
    }

    fn deliver_retained_pulled(&'static self, frame: RetainedFrame) -> Result<(), ReceiveError> {
        self.deliver_retained_inner(frame, true)
    }

    fn readable(&'static self) {
        self.common.wake_waiters();
    }

    fn peer_lost(&'static self, peer: PeerRef) {
        self.common.peers.forget(peer);
    }

    fn common(&self) -> &RxCommon { &self.common }

    fn service(&'static self, now: Monotonic) -> usize {
        usize::from(self.common.check_deadline_at(now))
    }

    fn wake(&'static self) {
        self.common.wake_waiters();
    }

    fn pull_limit(&self) -> usize {
        self.pull_bound()
    }

    fn binding_route(&self) -> BindingRoute {
        self.common.binding_route
    }

    fn startup_check(&self) -> Result<(), StartupError> {
        if self.native {
            return Ok(());
        }
        let native_payload = self.common.admitted.iter().any(|wire| wire.native_type_token.is_some());
        if let RxKind::Encoded { wires } = self.kind {
            for wire in wires {
                if native_payload {
                    if let RuntimeHeap::Allocates(reason) = wire.info.decode_heap() {
                        return Err(StartupError::RouteNeedsHeapAtRuntime {
                            route: self.common.name.clone(), wire: reason,
                        });
                    }
                }
                if matches!(wire.info.decode_heap(), RuntimeHeap::Middleware(_)) {
                    return Err(StartupError::InvalidConfig(
                        "a wire must not declare a middleware heap class",
                    ));
                }
            }
        }
        if native_payload && mem::needs_drop::<T>() {
            return Err(StartupError::RouteNeedsHeapAtRuntime {
                route: self.common.name.clone(),
                wire: "the decoded type owns heap memory",
            });
        }
        Ok(())
    }

    /// Seeds control state only while the complete pool is exclusively in setup.
    /// # Safety
    /// Caller owns initialization rights for this pool and must exclude every
    /// acquisition, queue operation, observer and other reset for the call.
    /// No held/queued/retiring lease or payload reference may exist, including a
    /// forgotten lease or an outstanding raw-index return obligation. The pool
    /// has never entered runtime; reset is not a way to reclaim a live tenure.
    /// On success the selected FREE counter is initialized to `even`; ordinary
    /// acquisition may begin only after this call and setup publication complete.
    #[allow(unsafe_code)]
    #[cfg(any(test, up_rust_test_hooks))]
    unsafe fn seed_for_setup(&self, index: usize, even: u64) -> Result<(), super::pool::CapacityError> {
        // SAFETY: this function's caller supplies exclusive, never-runtime setup
        // rights for this same pool, excluding every live owner and observer.
        unsafe { self.free.seed_for_setup(index, even) }
    }
    fn pool_lifetime(&self) -> super::LifetimeState { self.free.lifetime() }

    fn report(&self) -> RouteReport {
        self.common.report(
            self.slots.len(),
            self.frames.as_ref().map_or(0, ByteSlots::slot_len),
        )
    }

    fn counters(&self) -> &Counters {
        &self.common.counters
    }

    fn id(&self) -> RouteId {
        self.common.id
    }
}

/// Erased inbound route, reached from `RuntimePort`.
pub(crate) trait InboundDispatch: Send + Sync {
    fn common(&self) -> &RxCommon;

    fn queue_arrived(&self, sampled: Monotonic) { self.common().queue_arrived(sampled); }

    fn deadline_schedule(&self, previous: u64, sampled: u64)
        -> Option<super::deadline_monitor::CountSchedule>
    {
        self.common().monitor.schedule(previous, sampled)
    }

    fn rpc_deadline(&self) -> Option<Monotonic> { None }
    fn rpc_capacity(&self) -> usize { 0 }

    /// Seeds control state only while the complete pool is exclusively in setup.
    /// # Safety
    /// Caller owns initialization rights for this pool and must exclude every
    /// acquisition, queue operation, observer and other reset for the call.
    /// No held/queued/retiring lease or payload reference may exist, including a
    /// forgotten lease or an outstanding raw-index return obligation. The pool
    /// has never entered runtime; reset is not a way to reclaim a live tenure.
    /// On success the selected FREE counter is initialized to `even`; ordinary
    /// acquisition may begin only after this call and setup publication complete.
    #[allow(unsafe_code)]
    #[cfg(any(test, up_rust_test_hooks))]
    unsafe fn seed_for_setup(&self, _index: usize, _even: u64) -> Result<(), super::pool::CapacityError> { Ok(()) }

    fn pool_lifetime(&self) -> super::LifetimeState { super::LifetimeState::Healthy }
    fn deliver(&'static self, frame: InboundFrame<'_>) -> Result<(), ReceiveError>;

    fn deliver_retained(&'static self, frame: RetainedFrame) -> Result<(), ReceiveError> {
        drop(frame);
        Err(ReceiveError::Unsupported("route does not retain frames"))
    }

    /// Before a frame is taken from a pull binding: whether this route has room
    /// for it. A route that refuses an arrival when full (`DropNewest`, `Wait`)
    /// reserves the room here, so a frame is never taken only to be dropped: left
    /// in the binding it keeps its place, and a `Wait` sender keeps waiting. The
    /// reservation goes with the next [`Self::deliver_pulled`] or
    /// [`Self::deliver_retained_pulled`], or back through [`Self::end_pull_unused`].
    fn begin_pull(&'static self) -> bool {
        true
    }

    /// Gives back what [`Self::begin_pull`] reserved, when no frame came.
    fn end_pull_unused(&'static self) {}

    /// Delivers a frame pulled after [`Self::begin_pull`] said yes.
    fn deliver_pulled(&'static self, frame: InboundFrame<'_>) -> Result<(), ReceiveError> {
        self.deliver(frame)
    }

    /// Delivers a retained frame pulled after [`Self::begin_pull`] said yes.
    fn deliver_retained_pulled(&'static self, frame: RetainedFrame) -> Result<(), ReceiveError> {
        self.deliver_retained(frame)
    }

    /// Counts a frame the binding's own queue discarded under this route's policy,
    /// as the SDK counts a discard in its own queue: an evicted oldest frame was
    /// received, then dropped; an arriving newest one is only dropped.
    fn binding_dropped(&'static self, overflow: Overflow) {
        match overflow {
            Overflow::DropOldest => {
                add(&self.counters().carried);
                add(&self.counters().dropped_oldest);
            }
            Overflow::DropNewest => add(&self.counters().dropped_newest),
            // A full `Wait` queue refuses its sender; nothing was dropped.
            Overflow::Wait { .. } => {}
        }
    }

    fn readable(&'static self) {}

    fn peer_restarted(&'static self, _peer: PeerRef) {}

    fn peer_lost(&'static self, _peer: PeerRef) {}

    /// Applies deadlines. Returns how many completed with an error.
    fn service(&'static self, _now: Monotonic) -> usize {
        0
    }


    fn wake(&'static self) {}

    /// The most frames one call of
    /// [`TransportRuntime::pump`](super::TransportRuntime::pump), or of
    /// `TransportRuntime::receive` on a carrier, examines on this route.
    fn pull_limit(&self) -> usize;

    fn binding_route(&self) -> BindingRoute;

    /// Whether [`TransportRuntime::pump`](super::TransportRuntime::pump) serves
    /// this route. Only an RPC response route says yes.
    ///
    /// A typed route's queue is the binding's, read by `try_recv` with the route's
    /// QoS; moving frames into the inbox ahead of it would put two queues of the
    /// route's depth in series (R2-2). A carrier route takes frames under its
    /// forwarder's capacity check, by `TransportRuntime::receive`.
    fn pumped(&self) -> bool {
        false
    }

    fn startup_check(&self) -> Result<(), StartupError>;

    fn report(&self) -> RouteReport;

    fn counters(&self) -> &Counters;

    fn id(&self) -> RouteId;
}

/// A received value in its startup slot. Dropping it returns the slot.
///
/// The handle itself contains permanent route state and an index. Native/in-place
/// release retains the no-heap/free contract. Encoded serialized release may
/// destroy declared owned fields and free their allocations; a destructor unwind
/// withholds the index. Resource/panic qualification covers the full lifecycle.
pub struct Received<T: 'static, M: WireMode = Encoded> {
    // Safety invariant: this handle uniquely owns the current initialized slot
    // removed from route's queue, plus its held-credit reservation. take_lease
    // constructs it only after both transfers. No sibling can fabricate/duplicate
    // the tuple; Drop consumes it once, after every borrow from self ends.
    route: &'static TypedInbound<T, M>,
    index: u32,
    _mode: PhantomData<fn() -> M>,
}

impl<T: 'static, M: WireMode> fmt::Debug for Received<T, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Received")
            .field("index", &self.index)
            .finish()
    }
}

// SAFETY: a `Received` owns its slot index exclusively until dropped. Its only
// shared access goes through `Deref`/accessors that read data written before
// publish, which is safe from any thread when `T: Send + Sync`.
unsafe impl<T: Send + Sync, M: WireMode> Send for Received<T, M> {}
// SAFETY: as above.
unsafe impl<T: Send + Sync, M: WireMode> Sync for Received<T, M> {}

impl<T: Send + Sync + 'static> Deref for Received<T, Encoded> {
    type Target = T;

    fn deref(&self) -> &T {
        let value = self.route.values.get(self.index as usize);
        // SAFETY: a `Received` is created only for an index popped from the
        // ready queue, so the value slot exists and its value was decoded before
        // the slot was published. This lease owns the index until dropped.
        match value {
            // SAFETY: Received's publication invariant establishes initialized T
            // and its live lease excludes clear/reuse for this whole borrow.
            Some(value) => match unsafe { value.get() } {
                Some(value) => value,
                // SAFETY: success-only publication requires initialized=true;
                // no clearer can run while this Received lease remains live.
                None => unsafe { core::hint::unreachable_unchecked() },
            },
            None => {
                // SAFETY: unreachable by construction; see above.
                unsafe { core::hint::unreachable_unchecked() }
            }
        }
    }
}

impl<T: StablePayload + Send + Sync + 'static> Received<T, InPlace> {
    /// Borrows the typed payload from the retained native storage, checked.
    ///
    /// # Errors
    /// Reports a frame whose bytes are not a valid `T`, with a fixed-size reason.
    pub fn payload(&self) -> Result<&T, crate::payload::loan::BorrowError> {
        let bytes = self
            .body_bytes()
            .ok_or(crate::payload::loan::BorrowError::Length {
                expected: mem::size_of::<T>(),
                actual: 0,
            })?;
        borrow_checked::<T>(bytes)
    }
}

impl<T: Send + Sync + 'static, M: WireMode> Received<T, M> {
    /// # Safety-usable invariant
    /// Returns this lease's initialized metadata; RX-owner excludes mutation,
    /// owner movement and destruction while self is borrowed. Its ranges remain
    /// immutable and live for exactly such borrows, not for an arbitrary lifetime.
    fn meta(&self) -> &SlotMeta {
        // SAFETY: Received.index's unique initialized RX-owner obligation and
        // this self borrow exclude return/reuse, discharging slot_meta's contract.
        unsafe { self.route.slot_meta(self.index) }
    }

    fn body_bytes(&self) -> Option<&[u8]> {
        self.meta().payload.map(|range| {
            // SAFETY: meta's postcondition ties this range to this live lease.
            // RX-owner preserves its initialized extent/provenance/final address
            // for the returned self borrow, including nonnull empty ranges.
            unsafe { range.as_slice() }
        })
    }

    /// The validated metadata, with token and traceparent borrowed from the frame.
    #[must_use]
    pub fn header(&self) -> RxHeader<'_> {
        let meta = self.meta();
        let text = |range: Option<ByteRange>| {
            range.and_then(|range| {
                // SAFETY: token/traceparent come from self.meta; its postcondition
                // keeps the full initialized range immutable through this borrow.
                core::str::from_utf8(unsafe { range.as_slice() }).ok()
            })
        };
        meta.header
            .clone()
            .with_text(text(meta.token), text(meta.traceparent))
    }

    /// The payload bytes as received, when the slot kept them.
    ///
    /// `None` on a lending route that decoded the payload: its frame went back to
    /// the transport as soon as it was decoded, and the value is what remains.
    #[must_use]
    pub fn payload_bytes(&self) -> Option<&[u8]> {
        self.body_bytes()
    }

    /// Who sent this frame, as reported by the transport.
    #[must_use]
    pub fn peer(&self) -> PeerRef {
        self.meta().peer
    }

    pub(crate) fn reply(&self) -> ReplyToken {
        self.meta().reply
    }
}

impl<T: 'static, M: WireMode> Drop for Received<T, M> {
    fn drop(&mut self) {
        self.route.held.fetch_sub(1, Ordering::AcqRel);
        // SAFETY: Received.index owns the sole initialized RX-owner obligation;
        // &mut self at Drop excludes surviving safe borrows of this lease.
        unsafe { self.route.release_lease(self.index) };
    }
}

impl<T: 'static, M: WireMode> TypedInbound<T, M> {
    /// # Safety
    /// Caller owns the sole Received obligation for this initialized RX-owner
    /// index; all derived borrows have ended. Consumes the slot after full drop.
    unsafe fn release_lease(&self, index: u32) {
        // SAFETY: RX-owner and caller contract prove initialized meta/frame and
        // exclusive destruction of frame and decoded value. DecodedSlot::clear
        // receives the unique index after all value borrows ended. FreeIndices.push
        // runs only after complete drop;
        // on unwind it does not run. No view may survive the consumed lease.
        unsafe {
            if let Some(slot) = self.slots.get(index as usize) {
                // SAFETY: a dropping lease owns `index`, whose meta was written before publish.
                let retained = { (*slot.meta.get()).assume_init_ref().retained };
                if retained {
                    // SAFETY: a retained slot holds an initialized frame, and this
                    // is the only drop of it; the index is released just after.
                    { (*slot.frame.get()).assume_init_drop() };
                }
            }
            if let Some(value) = self.values.get(index as usize) { value.clear(); }
            self.free.push(index);
        }
    }
}
