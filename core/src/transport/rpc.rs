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

//! RPC call slots: reserved before send, completed by response, deadline or peer change.
//!
//! The table is built during initialization and lives until process exit. A
//! call's slot is claimed before its request is sent, so a response that races
//! the send still finds its reservation. Every call completes: with a response,
//! when its deadline passes, or when its server restarts or is reported gone.
//! Correlation is the full key: the route fixes source and sink, and the slot
//! records the request ID. Dropping a [`Call`] releases any owned decoded value
//! before returning its slot.
//!
//! A response is decoded once, into a startup slot the size of `Resp`, when
//! `try_response` takes it; a large response never crosses the stack. A retained
//! response frame is dropped by whoever leaves `COMPLETE`, or by the deliverer
//! when the call was abandoned while the response was being delivered.
//!
//! On a native route nothing is decoded. The response stays in the transport's
//! storage until its [`Call`] is dropped, and the caller borrows it checked.

use core::cell::UnsafeCell;
use core::fmt;
use core::marker::PhantomData;
use core::mem::{self, MaybeUninit};
use core::ops::Deref;
use crate::platform::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use super::binding::{BindingRoute, InboundFrame, PayloadBytes, PeerRef};
use super::envelope::ParsedFrame;
use super::error::{CallError, ReceiveError, StartupError};
use super::frame::RetainedFrame;
use super::inbound::{InboundDispatch, PeerAdmission, RxCommon, TypedWire};
use super::report::{add, Counters, RouteId, RouteReport};
use super::slots::{ByteSlots, FreeIndices};
use super::wake::RouteWake;
use crate::memory::StartupMemory;
use crate::payload::loan::{borrow_checked, BorrowError};
use crate::payload::stable::StablePayload;
use crate::platform::Monotonic;
use crate::uattributes::PayloadEncoding;
use crate::ucode::UCode;
use crate::uuid::UUID;
use crate::wire::{Encoded, InPlace, RuntimeHeap, Wire, WireMode};

const FREE: u64 = 0;
const RESERVED: u64 = 1;
const ARMED: u64 = 2;
const DELIVERING: u64 = 3;
const COMPLETE: u64 = 4;
const TIMED_OUT: u64 = 5;
const PEER_RESTARTED: u64 = 6;
const PEER_LOST: u64 = 7;
const ABANDONED: u64 = 8;
const TAKEN: u64 = 9;
/// The owner is releasing the slot: its frame and waker are still the owner's.
const RELEASING: u64 = 10;
const PHASE_MASK: u64 = 0xff;

/// The only terminal failures a shared observer may publish from ARMED.
#[derive(Clone, Copy)]
enum CompletionFailure {
    TimedOut,
    PeerRestarted,
    PeerLost,
}

impl CompletionFailure {
    fn phase(self) -> u64 {
        match self {
            Self::TimedOut => TIMED_OUT,
            Self::PeerRestarted => PEER_RESTARTED,
            Self::PeerLost => PEER_LOST,
        }
    }
}

fn phase(state: u64) -> u64 {
    state & PHASE_MASK
}

fn with_phase(state: u64, next: u64) -> u64 {
    (state & !PHASE_MASK) | next
}

/// The slot's free state: the generation in the high bits advances with each reuse.
fn free_state(state: u64) -> u64 {
    ((state >> 8).saturating_add(1) << 8) | FREE
}

const NO_ENCODING: u32 = u32::MAX;

struct CallSlot {
    state: AtomicU64,
    msb: AtomicU64,
    lsb: AtomicU64,
    deadline: AtomicU64,
    incarnation: AtomicU64,
    status: AtomicI32,
    encoding: AtomicU32,
    response_len: AtomicUsize,
    // Safety invariant: true Release-publishes an initialized final-address frame
    // and this tenure's payload extent (null/zero on absence or parse error)
    // after all non-atomic response writes end. It never publishes an earlier
    // tenure's tuple. Only the current Call taker or exclusive release/delivery
    // owner may clear/drop it;
    // live shared Call borrows exclude those transitions.
    retained: AtomicBool,
    frame: UnsafeCell<MaybeUninit<RetainedFrame>>,
    /// The retained response's payload, for the decode at `try_response`.
    payload: UnsafeCell<(*const u8, usize)>,
    wake: RouteWake,
}

/// The preallocated call table of one RPC client route.
pub struct CallTable<Resp: 'static> {
    free: FreeIndices,
    // Safety invariant: equal capacity with free/values/response regions; one
    // raw pool obligation exists per live Call. DELIVERING grants one writer;
    // completed payload stays immutable until its unique Call releases/takes it.
    slots: &'static [CallSlot],
    responses: Option<ByteSlots>,
    max_response_len: usize,
    // Safety invariant: encoded values are valid/vacant under DecodedSlot.
    // Only the current exclusive Call decode/release owner may mutate them;
    // a successful Reply borrows an initialized slot until that Call is dropped.
    values: &'static [super::decoded::DecodedSlot<Resp>],
    wires: &'static [TypedWire<Resp>],
}

// SAFETY: immutable setup geometry bounds every slot/value/response access.
// DELIVERING grants one frame/payload writer. retained's Release store publishes
// the current final-address owner and tuple after those writes; an Acquire can
// therefore expose native bytes even before COMPLETE. A live shared Call borrow
// prevents its unique mutable take/release/drop and hence owner destruction.
// Encoded Resp initialization/decoding occurs under &mut Call after COMPLETE;
// a successful Reply reads that value in TAKEN. DecodedSlot flag/value cleanup
// occurs under the unique releasing claim before FreeIndices return. Its existing
// Release/Acquire transfer publishes the vacant/valid state to the next owner. Resp: Send + Sync covers value movement/sharing.
// Retained owner callbacks are not invoked concurrently through shared views;
// readers use the captured immutable extent. Unique release/delivery claims
// arbitrate destruction, including the ABANDONED handoff. Private Call fields
// and unsafe adoption contracts prevent duplicate reservation ownership.
unsafe impl<Resp: Send + Sync> Send for CallTable<Resp> {}
// SAFETY: see `Send`.
unsafe impl<Resp: Send + Sync> Sync for CallTable<Resp> {}

/// Owns unpublished RPC seeds through response-owner allocation and admission.
pub(crate) struct PreparedCallTable<Resp: 'static> {
    table: &'static CallTable<Resp>,
    seeds: Option<super::decoded::InitialValues<Resp>>,
}

/// Exposes reports only; no usable table/response handle escapes before install.
pub(crate) struct PreparedClientResponses<Resp: 'static> {
    responses: &'static ClientResponses<Resp>,
    seeds: Option<super::decoded::InitialValues<Resp>>,
}

impl<Resp: Send + Sync + 'static> PreparedCallTable<Resp> {
    /// # Safety-usable invariant
    /// Keeps sole seed cleanup in the returned unpublished wrapper. Allocation
    /// refusal destroys completed seeds before return; no table handle escapes.
    pub(crate) fn with_responses(self, memory: &StartupMemory, common: RxCommon)
        -> Result<PreparedClientResponses<Resp>, StartupError>
    {
        let responses = memory.place(ClientResponses { common, table: self.table })?;
        Ok(PreparedClientResponses { responses, seeds: self.seeds })
    }
}

impl<Resp: Send + Sync + 'static> PreparedClientResponses<Resp> {
    pub(crate) fn report(&self) -> RouteReport { self.responses.report() }

    /// # Safety-usable invariant
    /// Installs the complete dispatch entry and consumes seed cleanup before
    /// returning its usable handle. add_rx has no fallible step after dispatch
    /// insertion; errors leave no installed table and this guard clears seeds.
    pub(crate) fn install(self, setup: &super::mode::TransportSetup)
        -> Result<&'static ClientResponses<Resp>, StartupError>
    {
        setup.add_rx(self.responses)?;
        if let Some(seeds) = self.seeds { seeds.commit(); }
        Ok(self.responses)
    }
}

impl<Resp: Send + Sync + 'static> CallTable<Resp> {
    /// Reserves the table in startup memory. `init_wire` gives every decode slot
    /// its first valid value, through a pointer.
    // [impl->req~ssr-unsafe-wire-untrusted~1]
    pub(crate) fn prepare(
        memory: &StartupMemory,
        capacity: usize,
        max_response_len: usize,
        retained: bool,
        wires: &'static [TypedWire<Resp>],
        init_wire: Option<&'static dyn Wire<Resp>>,
        policy: super::LifetimePolicy,
    ) -> Result<PreparedCallTable<Resp>, StartupError> {
        let free = FreeIndices::new_in(memory, capacity)?.with_policy(policy);
        let mut slots = memory.fixed_vec::<CallSlot>(capacity)?;
        for _ in 0..capacity {
            slots.push(CallSlot {
                state: AtomicU64::new(FREE),
                msb: AtomicU64::new(0),
                lsb: AtomicU64::new(0),
                deadline: AtomicU64::new(0),
                incarnation: AtomicU64::new(0),
                status: AtomicI32::new(0),
                encoding: AtomicU32::new(NO_ENCODING),
                response_len: AtomicUsize::new(0),
                retained: AtomicBool::new(false),
                frame: UnsafeCell::new(MaybeUninit::uninit()),
                payload: UnsafeCell::new((core::ptr::null(), 0)),
                wake: RouteWake::new(),
            })?;
        }
        let responses = if retained {
            None
        } else {
            Some(ByteSlots::new_in(memory, capacity, max_response_len)?)
        };
        let seeds = init_wire
            .map(|wire| super::decoded::initialize(memory, wire, capacity)).transpose()?;
        let values = seeds.as_ref().map_or(&[][..], |seeds| seeds.slots());
        let table = memory.place(Self {
            free,
            slots: slots.into_static_slice(),
            responses,
            max_response_len,
            values,
            wires,
        })?;
        Ok(PreparedCallTable { table: &*table, seeds })
    }

    // Preserve existing private state-machine fixtures without giving production
    // builders a way around their unpublished seed guard.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn place(
        memory: &StartupMemory,
        capacity: usize,
        max_response_len: usize,
        retained: bool,
        wires: &'static [TypedWire<Resp>],
        init_wire: Option<&'static dyn Wire<Resp>>,
        policy: super::LifetimePolicy,
    ) -> Result<&'static Self, StartupError> {
        let prepared = Self::prepare(memory, capacity, max_response_len, retained,
            wires, init_wire, policy)?;
        if let Some(seeds) = prepared.seeds { seeds.commit(); }
        Ok(prepared.table)
    }

    pub(crate) fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// Wakes every waiting call. Bounded by the table.
    pub(crate) fn wake_all(&self) {
        for slot in self.slots {
            slot.wake.wake();
        }
    }

    pub(crate) fn slot_len(&self) -> usize {
        self.max_response_len
    }

    /// Claims a slot for `id` before its request is sent.
    /// # Safety-usable invariant
    /// Ok transfers exactly one current raw pool obligation for slots[index].
    /// The index is in range; ARMED records the initialized request state and
    /// no retained frame. Only one Call::from_reserved may adopt that obligation.
    pub(crate) fn reserve(&self, id: &UUID, deadline: Monotonic, incarnation: u64) -> Result<u32, CallError> {
        let index = self.free.pop()?;
        // Both tables have the same immutable setup capacity; a pool index is
        // therefore in range. Keep a checked, non-panicking boundary even when
        // codegen cannot carry that invariant across the pool abstraction.
        let Some(slot) = self.slots.get(index as usize) else {
            // SAFETY: reserve just acquired this raw pool obligation; no slot
            // payload was touched. This consumes that unique obligation once.
            unsafe { self.free.push(index) };
            return Err(CallError::InternalState);
        };
        let state = slot.state.load(Ordering::Acquire);
        let (msb, lsb) = id.as_u64_pair();
        slot.state.store(with_phase(state, RESERVED), Ordering::Release);
        slot.msb.store(msb, Ordering::Relaxed);
        slot.lsb.store(lsb, Ordering::Relaxed);
        slot.deadline.store(deadline.0, Ordering::Relaxed);
        slot.incarnation.store(incarnation, Ordering::Relaxed);
        slot.status.store(UCode::Ok.value(), Ordering::Relaxed);
        slot.encoding.store(NO_ENCODING, Ordering::Relaxed);
        slot.response_len.store(0, Ordering::Relaxed);
        slot.retained.store(false, Ordering::Relaxed);
        slot.state.store(with_phase(state, ARMED), Ordering::Release);
        Ok(index)
    }

    /// # Safety
    /// Caller holds exclusive payload/destructor access for this live slot and
    /// no views survive. It must be the current delivery/release owner or Call's
    /// unique mutable response operation. If retained was true, consumes that
    /// initialized frame exactly once; no pool index is returned by this helper.
    unsafe fn drop_retained(&self, slot: &CallSlot) {
        // SAFETY: the exclusive-state contract prevents concurrent payload/view
        // access. retained.swap(false) acquires initialization and selects its only
        // destructor. assume_init_drop consumes that live frame; if Drop unwinds no
        // second drop is permitted and no index-return operation here can run.
        {
            if slot.retained.swap(false, Ordering::AcqRel) {
                // SAFETY: `retained` was true, so `frame` holds an initialized owner
                // written before the release store; swapping it to false makes this
                // the only drop. No other thread touches the storage in this phase.
                unsafe { (*slot.frame.get()).assume_init_drop() };
            }
        }
    }

    /// # Safety-usable invariant
    /// Success changes exactly this observed ARMED tenure to a terminal failure.
    /// It never opens payload access, re-arms a taken slot, or changes generation.
    /// All other expected phases fail without a state change or wake.
    fn try_complete(&self, slot: &CallSlot, state: u64, next: CompletionFailure) -> bool {
        if phase(state) != ARMED {
            return false;
        }
        if slot
            .state
            .compare_exchange(
                state,
                with_phase(state, next.phase()),
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            slot.wake.wake();
            true
        } else {
            false
        }
    }

    /// Planning snapshot only, never a payload/ownership witness. Reuse may
    /// make an event early or change it; Call::from_reserved notifies the service
    /// after ARMED publication. All loads are from existing atomic metadata.
    pub(crate) fn next_expiry(&self) -> Option<Monotonic> {
        let mut earliest = None::<u64>;
        for slot in self.slots {
            if phase(slot.state.load(Ordering::Acquire)) == ARMED {
                let deadline = slot.deadline.load(Ordering::Relaxed);
                earliest = Some(earliest.map_or(deadline, |old| old.min(deadline)));
            }
        }
        earliest.map(Monotonic)
    }

    /// Completes every armed call whose deadline has passed.
    pub(crate) fn expire(&self, now: Monotonic) -> usize {
        let mut expired = 0_usize;
        for slot in self.slots {
            let state = slot.state.load(Ordering::Acquire);
            if phase(state) == ARMED
                && slot.deadline.load(Ordering::Relaxed) <= now.0
                && self.try_complete(slot, state, CompletionFailure::TimedOut)
            {
                expired = expired.saturating_add(1);
            }
        }
        expired
    }

    /// Fails calls reserved against an earlier incarnation of the server.
    fn fail_older(&self, incarnation: u64) {
        for slot in self.slots {
            let state = slot.state.load(Ordering::Acquire);
            if phase(state) == ARMED && slot.incarnation.load(Ordering::Relaxed) < incarnation {
                self.try_complete(slot, state, CompletionFailure::PeerRestarted);
            }
        }
    }

    /// Fails every outstanding call: the server has been reported gone.
    fn fail_all(&self) {
        for slot in self.slots {
            let state = slot.state.load(Ordering::Acquire);
            if phase(state) == ARMED {
                self.try_complete(slot, state, CompletionFailure::PeerLost);
            }
        }
    }

    /// Takes the DELIVERING phase of the call whose reservation holds this request ID.
    /// # Safety-usable invariant
    /// Ok(index,state) transfers the sole DELIVERING claim for this table's
    /// slots[index] at generation state. Only that claimant writes response/frame
    /// storage; its completion consumes this claim. Concurrent Call drop can only
    /// mark ABANDONED, transferring final cleanup to this claimant.
    fn begin_delivery(&self, parsed: &ParsedFrame<'_>) -> Result<(usize, u64), ReceiveError> {
        let reqid = parsed
            .header
            .reqid
            .as_ref()
            .ok_or(ReceiveError::NoMatchingCall)?;
        let (msb, lsb) = reqid.as_u64_pair();
        for (index, slot) in self.slots.iter().enumerate() {
            let state = slot.state.load(Ordering::Acquire);
            if phase(state) != ARMED
                || slot.msb.load(Ordering::Relaxed) != msb
                || slot.lsb.load(Ordering::Relaxed) != lsb
                || slot
                    .state
                    .compare_exchange(
                        state,
                        with_phase(state, DELIVERING),
                        Ordering::AcqRel,
                        Ordering::Relaxed,
                    )
                    .is_err()
            {
                continue;
            }
            return Ok((index, state));
        }
        Err(ReceiveError::NoMatchingCall)
    }

    /// # Safety
    /// Caller owns this table's unique begin_delivery result (index,state), slot
    /// is exactly slots[index], and all response writes are complete. No borrowed
    /// view survives this call. Consumes the DELIVERING obligation, publishing
    /// COMPLETE or finishing abandoned cleanup before returning the raw pool index.
    unsafe fn finish_delivery(&self, index: usize, slot: &CallSlot, state: u64) {
        // SAFETY: the caller binds current index/state/slot and unique DELIVERING
        // ownership. No state other than ABANDONED may defeat the COMPLETE CAS for
        // this generation. That failed branch owns complete cleanup; drop_retained
        // and wake.clear finish before finish_return consumes the one pool index.
        // The success branch Release-publishes writes and never returns that index.
        unsafe {
            if slot
                .state
                .compare_exchange(
                    with_phase(state, DELIVERING),
                    with_phase(state, COMPLETE),
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                slot.wake.wake();
            } else {
                // The call was dropped while delivering; this deliverer finishes its release.
                self.drop_retained(slot);
                slot.wake.clear();
                // SAFETY: finish_delivery's state machine granted sole ABANDONED
                // completion; retained destruction/wake cleanup above have finished.
                // Its index/slot pair is from the same checked delivery reservation.
                { self.finish_return(index as u32, slot, state) };
            }
        }
    }

    fn record_header(&self, slot: &CallSlot, parsed: &ParsedFrame<'_>) {
        let status = parsed.header.comm_status.unwrap_or(UCode::Ok).value();
        slot.status.store(status, Ordering::Relaxed);
        slot.encoding.store(
            parsed
                .header
                .payload_encoding
                .map_or(NO_ENCODING, PayloadEncoding::id),
            Ordering::Relaxed,
        );
    }

    /// Stores a copied response for the call whose reservation holds this request ID.
    fn complete_copied(&self, parsed: &ParsedFrame<'_>) -> Result<(), ReceiveError> {
        let (index, state) = self.begin_delivery(parsed)?;
        let slot = self.slots.get(index).ok_or(ReceiveError::NoMatchingCall)?;
        self.record_header(slot, parsed);
        let mut len = 0_usize;
        let mut fits = true;
        if let Some(responses) = &self.responses {
            // SAFETY: DELIVERING grants this thread exclusive use of the buffer
            // until the state below is published.
            let out = unsafe { responses.slot_mut(index as u32) };
            for chunk in parsed.payload.chunks() {
                let end = len.saturating_add(chunk.len());
                let Some(target) = out.get_mut(len..end) else {
                    fits = false;
                    break;
                };
                for (slot, byte) in target.iter_mut().zip(chunk) {
                    *slot = *byte;
                }
                len = end;
            }
        } else {
            fits = false;
        }
        slot.response_len
            .store(if fits { len } else { usize::MAX }, Ordering::Relaxed);
        // SAFETY: begin_delivery returned this exact unique (index,state) and
        // checked slots.get bound slot to it. Writes are complete; no views escape.
        unsafe { self.finish_delivery(index, slot, state) };
        Ok(())
    }

    /// Stores a retained response frame for the call matching this request ID.
    ///
    /// The owner moves to the call slot, its final address, first; the payload view
    /// is taken from it there. An owner may keep its bytes inline, so a view taken
    /// before the move could dangle.
    /// # Safety
    /// delivering must be this table's unique live begin_delivery result, not a
    /// copied/reused tuple. Its response/frame cells are exclusively writable
    /// and hold no previous retained owner. Consumes that delivery obligation on
    /// normal completion; unwind may withhold it, never publish partial state.
    unsafe fn complete_retained(
        &self,
        common: &RxCommon,
        delivering: (usize, u64),
        frame: RetainedFrame,
    ) -> Result<(), ReceiveError> {
        // SAFETY: the exact live delivery claim fixes bounds/current slot/phase.
        // Frame writes occur only under that exclusive claim. Views are captured
        // after final placement, obey RetainedFrame postconditions, and are not
        // used after finish_delivery publishes/returns ownership. retained Release
        // publishes the initialized frame; finish_delivery consumes the exact pair.
        {
            let (index, state) = delivering;
            let Some(slot) = self.slots.get(index) else {
                drop(frame);
                return Err(ReceiveError::NoMatchingCall);
            };
            // SAFETY: DELIVERING grants exclusive payload access, and retained is
            // still false for this reservation. Clear the old tuple BEFORE any
            // owner callback or fallible parse. A safe stateful parser may reject
            // the second parse even though the pre-admission parse succeeded.
            // Success replaces this absent extent; error can publish only absence,
            // never a pointer from a destroyed prior tenure. Unwind before retained
            // publication withholds ownership rather than exposing partial state.
            unsafe { *slot.payload.get() = (core::ptr::null(), 0) };
            slot.response_len.store(0, Ordering::Relaxed);
            // SAFETY: DELIVERING grants this thread exclusive use of the frame and
            // payload cells until the state below is published.
            let owner: &RetainedFrame = unsafe { (*slot.frame.get()).write(frame) };
            // SAFETY: the owner is at its final address and alive until the call
            // releases it; the views are kept only while it is retained.
            let metadata: &[u8] = unsafe { &*owner.metadata_ptr() };
            let parts = owner.payload_parts();
            // SAFETY: as above.
            let bytes = parts.map(|(bytes, _)| unsafe { &*bytes });
            let payload = bytes.map_or(PayloadBytes::Absent, PayloadBytes::one);
            match common
                .envelope
                .parse(common.admitted, metadata, payload, &common.limits)
            {
                Ok(parsed) => {
                    self.record_header(slot, &parsed);
                    let (pointer, len) = parsed
                        .payload
                        .as_contiguous()
                        .map_or((core::ptr::null(), 0), |bytes| {
                            (bytes.as_ptr(), bytes.len())
                        });
                    slot.response_len.store(len, Ordering::Relaxed);
                    // SAFETY: as above: exclusive until the state below is published.
                    unsafe { *slot.payload.get() = (pointer, len) };
                }
                Err(_) => {
                    // A safe parser/view provider may change its answer between
                    // calls. Keep the current owner for normal cleanup but publish
                    // the absent extent initialized above, never a stale tuple.
                    slot.status
                        .store(UCode::Internal.value(), Ordering::Relaxed);
                }
            }
            slot.retained.store(true, Ordering::Release);
            // SAFETY: begin_delivery returned this exact unique (index,state) and
            // checked slots.get bound slot to it. Writes are complete; no views escape.
            unsafe { self.finish_delivery(index, slot, state) };
            Ok(())
        }
    }

    /// Checks a call's state at `now`, completing it if its deadline has passed.
    /// # Safety
    /// Caller holds this table's current unique Call reservation at index for
    /// the full returned borrow. No response take/drop/reuse may overlap. Some
    /// Response borrows only immutable completed response storage of that Call.
    unsafe fn check_outcome(&self, index: u32, now: Monotonic) -> Option<Outcome<'_>> {
        // SAFETY: the current Call contract prevents release/reuse. The Acquire
        // state load must observe COMPLETE before outcome's raw access; slot is
        // from this index's checked lookup. Other states access atomics only.
        unsafe {
            let Some(slot) = self.slots.get(index as usize) else {
                return Some(Outcome::Failed(CallError::SlotsExhausted));
            };
            let state = slot.state.load(Ordering::Acquire);
            match phase(state) {
                COMPLETE => return Some(self.outcome(index, slot)),
                TIMED_OUT => return Some(Outcome::Failed(CallError::TimedOut)),
                PEER_RESTARTED => return Some(Outcome::Failed(CallError::PeerRestarted)),
                PEER_LOST => return Some(Outcome::Failed(CallError::PeerLost)),
                TAKEN => return Some(Outcome::Failed(CallError::AlreadyTaken)),
                _ => {}
            }
            if phase(state) == ARMED
                && slot.deadline.load(Ordering::Relaxed) <= now.0
                && self.try_complete(slot, state, CompletionFailure::TimedOut)
            {
                return Some(Outcome::Failed(CallError::TimedOut));
            }
            None
        }
    }

    /// The call slot's waker storage, for the async layer.
    #[doc(hidden)]
    /// Returns process-lifetime wake storage for this live call/route, or None for an invalid
    /// slot. Only the owning adapter registers or clears it; completion threads may wake it. A
    /// static storage lifetime does not extend slot ownership: clear on Call/Recv drop, never
    /// use the registration after reuse. Register before waiting and poll again after
    /// registration and each wake. This accessor neither blocks nor uses the heap.
    // [impl->req~ssr-async-seam~1]
    pub fn __wake(&'static self, index: u32) -> Option<&'static RouteWake> {
        self.slots.get(index as usize).map(|slot| &slot.wake)
    }

    /// # Safety
    /// Caller holds the current Call reservation for index; slot is slots[index]
    /// and an Acquire read observed COMPLETE. Hold the Call and exclude mutable
    /// take/drop/reuse throughout the returned response borrow.
    unsafe fn outcome(&self, index: u32, slot: &CallSlot) -> Outcome<'_> {
        // SAFETY: the explicit Call/COMPLETE contract orders response writes and
        // excludes release/mutation while borrowed. Checked lengths and null absence
        // precede raw slice creation; stored pointers came from the final retained
        // owner. ByteSlots.slot receives the same live read-only index obligation.
        {
            let status = slot.status.load(Ordering::Relaxed);
            if status != UCode::Ok.value() {
                return Outcome::Failed(CallError::Rejected(
                    UCode::from_i32(status).unwrap_or(UCode::Unknown),
                ));
            }
            let len = slot.response_len.load(Ordering::Relaxed);
            if len == usize::MAX {
                return Outcome::Failed(CallError::ResponseTooLarge);
            }
            let encoding = slot.encoding.load(Ordering::Relaxed);
            if encoding == NO_ENCODING {
                return Outcome::Failed(CallError::UnknownEncoding);
            }
            if slot.retained.load(Ordering::Acquire) {
                // SAFETY: COMPLETE fixes the payload pointer until the owner drops
                // the call; the retained frame keeps those bytes alive.
                let (pointer, stored) = unsafe { *slot.payload.get() };
                let payload = if pointer.is_null() || len != stored {
                    &[][..]
                } else {
                    // SAFETY: as above; the range was taken from a live borrow at delivery.
                    unsafe { core::slice::from_raw_parts(pointer, stored) }
                };
                return Outcome::Response { encoding, payload };
            }
            let Some(responses) = &self.responses else {
                return Outcome::Failed(CallError::ResponseTooLarge);
            };
            // SAFETY: COMPLETE fixes the buffer until the call's owner drops it.
            let bytes = unsafe { responses.slot(index) };
            Outcome::Response {
                encoding,
                payload: bytes.get(..len).unwrap_or_default(),
            }
        }
    }

    // [impl->req~ssr-unsafe-wire-untrusted~1]
    /// # Safety
    /// Caller uniquely mutably borrows the current Call for this index, after
    /// response completion, with no live typed value borrows. Exclusive access
    /// lasts through initialization, decode and every temporary returned borrow,
    /// including safe callback reentry. Success leaves valid T. Initialization
    /// unwind may leave vacant/leaked storage; after successful initialization,
    /// decode error/unwind leaves valid T without implying semantic success.
    unsafe fn decode_into_value(
        &self,
        index: u32,
        encoding: u32,
        payload: &[u8],
    ) -> Result<(), CallError> {
        // SAFETY: the current Call exclusively owns this checked value slot for
        // the full initialization/decode borrow. get_or_init establishes valid
        // Resp before a safe decoder sees it. Initialization unwind leaves its
        // flag false; otherwise safe &mut Resp preserves validity on all exits.
        // In particular, Rust's assignment cleanup installs the replacement if
        // old-value Drop unwinds. A panic can break semantic decode success, not
        // the safe reference's value validity. No no-panic/trait-law premise is used.
        {
            let encoding =
                PayloadEncoding::try_from_wire_id(encoding).map_err(|_| CallError::UnknownEncoding)?;
            let wire = self
                .wires
                .iter()
                .find(|wire| wire.info.identity.encoding == encoding)
                .ok_or(CallError::UnknownEncoding)?;
            let value = self
                .values
                .get(index as usize)
                .ok_or(CallError::UnknownEncoding)?;
            // SAFETY: exclusive &mut Call and completed phase exclude every
            // other value reader/writer until this temporary borrow ends after
            // decode. get_or_init's postcondition supplies a valid initialized T.
            let into = unsafe { value.get_or_init(wire.wire) }.map_err(CallError::Decode)?;
            wire.wire
                .decode(encoding, payload, into)
                .map_err(CallError::Decode)
        }
    }
}

enum Outcome<'a> {
    Response { encoding: u32, payload: &'a [u8] },
    Failed(CallError),
}

/// The inbound route that delivers responses to one RPC client's table.
pub(crate) struct ClientResponses<Resp: 'static> {
    pub(crate) common: RxCommon,
    pub(crate) table: &'static CallTable<Resp>,
}

impl<Resp: Send + Sync + 'static> ClientResponses<Resp> {
    /// Filters a parsed response, handling restarts, then completes its call.
    fn admit_response(&self, parsed: &ParsedFrame<'_>, peer: PeerRef) -> Result<(), ReceiveError> {
        let admission = self.common.admit_addressing(parsed, peer)?;
        // A status-only response carries no payload encoding to match.
        let status_only = parsed.header.payload_encoding.is_none()
            && parsed
                .header
                .comm_status
                .is_some_and(|code| code != UCode::Ok);
        if !status_only {
            self.common.wire_index(parsed)?;
        }
        if admission == PeerAdmission::Restarted {
            self.table.fail_older(peer.incarnation);
        }
        Ok(())
    }
}

impl<Resp: Send + Sync + 'static> InboundDispatch for ClientResponses<Resp> {
    fn deliver(&'static self, frame: InboundFrame<'_>) -> Result<(), ReceiveError> {
        let parsed = self
            .common
            .envelope
            .parse(
                self.common.admitted,
                frame.metadata,
                frame.payload,
                &self.common.limits,
            )
            .map_err(|error| {
                add(&self.common.counters.refused);
                ReceiveError::Header(error)
            })?;
        self.admit_response(&parsed, frame.peer)?;
        let outcome = self.table.complete_copied(&parsed);
        match outcome {
            Ok(()) => add(&self.common.counters.carried),
            Err(_) => add(&self.common.counters.refused),
        }
        self.common.wake_waiters();
        outcome
    }

    fn deliver_retained(&'static self, frame: RetainedFrame) -> Result<(), ReceiveError> {
        // SAFETY: the owner retains these bytes; `frame` is alive for this call
        // and the ranges are kept only while the slot keeps the owner.
        let metadata: &[u8] = unsafe { &*frame.metadata_ptr() };
        let parts = frame.payload_parts();
        let payload_bytes: Option<&[u8]> = parts.map(|(bytes, _)| {
            // SAFETY: as above.
            unsafe { &*bytes }
        });
        let payload = payload_bytes.map_or(PayloadBytes::Absent, PayloadBytes::one);
        let peer = frame.peer();
        let parsed = match self.common.envelope.parse(
            self.common.admitted,
            metadata,
            payload,
            &self.common.limits,
        ) {
            Ok(parsed) => parsed,
            Err(error) => {
                add(&self.common.counters.refused);
                drop(frame);
                return Err(ReceiveError::Header(error));
            }
        };
        if let Err(error) = self.admit_response(&parsed, peer) {
            drop(frame);
            return Err(error);
        }
        let delivering = match self.table.begin_delivery(&parsed) {
            Ok(found) => found,
            Err(error) => {
                add(&self.common.counters.refused);
                drop(frame);
                self.common.wake_waiters();
                return Err(error);
            }
        };
        // SAFETY: delivering is the unique result just obtained from this same
        // table's begin_delivery. No copy is consumed elsewhere; frame's local
        // preliminary views are no longer used after it moves into final storage.
        let outcome = unsafe {
            self.table.complete_retained(&self.common, delivering, frame)
        };
        match outcome {
            Ok(()) => add(&self.common.counters.carried),
            Err(_) => add(&self.common.counters.refused),
        }
        self.common.wake_waiters();
        outcome
    }

    fn peer_restarted(&'static self, peer: PeerRef) {
        if self.common.peers.admit_event(peer) {
            self.table.fail_older(peer.incarnation);
            self.common.wake_waiters();
        }
    }

    fn peer_lost(&'static self, peer: PeerRef) {
        self.common.peers.forget(peer);
        self.table.fail_all();
        self.common.wake_waiters();
    }

    fn readable(&'static self) {
        // A pull binding has a response for some call: wake them all to look.
        self.table.wake_all();
        self.common.wake_waiters();
    }

    fn common(&self) -> &RxCommon { &self.common }
    fn rpc_deadline(&self) -> Option<Monotonic> { self.table.next_expiry() }
    fn rpc_capacity(&self) -> usize { self.table.capacity() }

    fn service(&'static self, now: Monotonic) -> usize {
        let counted = usize::from(self.common.check_deadline_at(now));
        counted.saturating_add(self.table.expire(now))
    }

    fn wake(&'static self) {
        self.common.wake_waiters();
    }

    fn pull_limit(&self) -> usize {
        self.table.capacity().saturating_add(1)
            .min(usize::from(self.common.qos.depth).saturating_add(1))
    }

    fn binding_route(&self) -> BindingRoute {
        self.common.binding_route
    }

    fn pumped(&self) -> bool {
        // A response completes a call that may have no poller: one awaited on
        // another task, or one dropped whose late response holds a place.
        true
    }

    fn startup_check(&self) -> Result<(), StartupError> {
        check_decoders(self.table.wires, || self.common.name.clone())
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
        unsafe { self.table.free.seed_for_setup(index, even) }
    }
    fn pool_lifetime(&self) -> super::LifetimeState { self.table.free.lifetime() }

    fn report(&self) -> RouteReport {
        self.common
            .report(self.table.capacity(), self.table.slot_len())
    }

    fn counters(&self) -> &Counters {
        &self.common.counters
    }

    fn id(&self) -> RouteId {
        self.common.id
    }
}

/// One outstanding call. Dropping it returns the slot.
///
/// The handle is permanent table state and an index. Dropping it may release
/// owned encoded response fields; native release retains its no-heap contract.
/// A panicking owned-value destructor withholds the slot instead of returning it.
/// Resource/panic qualification covers initialization, decoding and destruction.
/// A late response is rejected as `NoMatchingCall`.
pub struct Call<Resp: 'static, M: WireMode = Encoded> {
    table: &'static CallTable<Resp>,
    responses: &'static dyn InboundDispatch,
    shared: &'static super::mode::TransportShared,
    // Safety invariant: sole live reservation obtained from table.reserve;
    // all Reply/payload borrows derive from this Call. Construction transfers
    // that obligation; no safe clone duplicates it. Forget withholds the index.
    index: u32,
    // Invariant: true only after this Call's encoded decode returned Ok and its
    // retained response was taken. Kept separate from initialized storage and
    // TAKEN: either can exist after a decoder writes a valid T then returns Err.
    decoded_success: bool,
    _marker: PhantomData<fn() -> (Resp, M)>,
}

impl<Resp: 'static, M: WireMode> Call<Resp, M> {
    /// # Safety
    /// Caller must transfer this table's sole live reserve result index, not
    /// copied, released or already adopted. The responses/shared handles must
    /// be the initialized route context for this table. On return Call owns the
    /// sole release obligation; caller must not access/return that incarnation
    /// except through the returned Call and its borrows. Forget withholds it.
    pub(crate) unsafe fn from_reserved(
        table: &'static CallTable<Resp>,
        responses: &'static dyn InboundDispatch,
        shared: &'static super::mode::TransportShared,
        index: u32,
    ) -> Self {
        // The caller contract establishes all private Call field invariants.
        let call = Self { table, responses, shared, index, decoded_success: false, _marker: PhantomData };
        // The fully initialized Call owns the reservation before invoking any
        // safe platform callback. Reentry/unwind cannot observe a partial owner.
        // A stall from ARMED to here is charged in service configuration evidence.
        shared.notify_deadline_service();
        call
    }
}

impl<Resp: 'static, M: WireMode> fmt::Debug for Call<Resp, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Call")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

impl<Resp: 'static, M: WireMode> Drop for Call<Resp, M> {
    fn drop(&mut self) {
        // SAFETY: Call's index invariant holds the one live reservation;
        // &mut self at Drop excludes every Reply/byte borrow from this Call.
        unsafe { self.table.release_any(self.index) };
    }
}

impl<Resp: 'static> fmt::Debug for CallTable<Resp> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallTable").finish_non_exhaustive()
    }
}

impl<Resp: 'static> CallTable<Resp> {
    /// # Safety
    /// Caller owns the unique return of index in this table; slot is exactly
    /// slots[index], with the current exclusively claimed release state. All
    /// payload borrows and frame/waker cleanup have ended. Caller alone may
    /// destroy a remaining decoded value before returning the index; withhold
    /// this obligation throughout destructor reentry/unwind. Normal return consumes
    /// the pool obligation; no access to this incarnation may follow.
    unsafe fn finish_return(&self, index: u32, slot: &CallSlot, state: u64) {
        // SAFETY: caller contract gives current paired slot/state and exclusive
        // completed frame/waker cleanup. clear destroys owned decoded fields
        // before publication; reentry cannot acquire the withheld index and an
        // unwind prevents return. Native tables have no decoded values. Only
        // after cleanup may warning/state publication and final free.push run.
        unsafe {
            if let Some(value) = self.values.get(index as usize) { value.clear(); }
            let generation = state >> 8;
            let remaining = (u64::MAX >> 8) - generation;
            if remaining <= self.free.policy().warning_remaining() { self.free.warn(); }
            if remaining == 0 {
                // Existing phase observers must never see a recycled generation.
                // Close before publishing any availability; other owners stay valid.
                self.free.retire();
            } else { slot.state.store(free_state(state), Ordering::Release); }
            self.free.push(index); // final access to this slot, after all cleanup
        }
    }

    /// The bytes of this call's retained response. Empty if it holds none.
    /// # Safety
    /// Caller holds the unique Call reservation for this current index through
    /// the returned borrow, with no concurrent mutable response take, drop or
    /// reuse. An in-progress delivery may publish retained=true only after its
    /// payload/frame writes end; if that flag is false, returns an empty slice. Returned bytes borrow that live Call;
    /// this function cannot extend storage lifetime from the raw index alone.
    unsafe fn retained_bytes(&self, index: u32) -> &[u8] {
        // SAFETY: checked index bounds and retained Acquire gate the payload read;
        // retained's Release postcondition proves response writes are finished;
        // the Call borrow excludes take/drop/reuse. A remaining completion CAS
        // changes only atomic phase and cannot invalidate this acquired payload. Stored extent comes from the final retained owner's genuine
        // slice, or null for absence (handled before reference creation). Every
        // non-null extent remains initialized and immutable for this borrow.
        {
            let Some(slot) = self.slots.get(index as usize) else {
                return &[];
            };
            if !slot.retained.load(Ordering::Acquire) {
                return &[];
            }
            // SAFETY: `retained` is true, so the range was written before its release
            // store, and the owner it points into stays in the slot until the call's
            // owner releases it. Borrowing the call rules that out while this lives.
            let (pointer, len) = unsafe { *slot.payload.get() };
            if pointer.is_null() {
                return &[];
            }
            // SAFETY: as above; the range was taken from a live borrow at delivery.
            unsafe { core::slice::from_raw_parts(pointer, len) }
        }
    }

    /// Returns a slot. Called exactly once per reservation, by its owner. Generic
    /// over the same `Resp`, so a `Drop` on `Call<Resp>` where `Resp` lacks
    /// `Send + Sync` still returns the slot.
    /// # Safety
    /// Caller must own the sole live Call reservation for index in this table;
    /// every Reply/view borrow has ended. Consumes that obligation once. If
    /// delivery is active it transfers cleanup to the deliverer via ABANDONED;
    /// otherwise it destroys the frame before the one pool return.
    unsafe fn release_any(&self, index: u32) {
        // SAFETY: the Call-owner contract identifies this current incarnation and
        // excludes duplicate releases/live views. Its successful state CAS grants
        // exclusive cleanup or transfers it to the sole DELIVERING claimant.
        // retained.swap(false) selects one initialized frame destructor. Cleanup
        // finishes before finish_return; reentrant callbacks cannot acquire this
        // index before free publication. Unwind withholds rather than duplicates.
        {
            let Some(slot) = self.slots.get(index as usize) else {
                return;
            };
            let mut state = slot.state.load(Ordering::Acquire);
            loop {
                let next = if phase(state) == DELIVERING {
                    with_phase(state, ABANDONED)
                } else {
                    with_phase(state, RELEASING)
                };
                match slot
                    .state
                    .compare_exchange(state, next, Ordering::AcqRel, Ordering::Acquire)
                {
                    Ok(_) => {
                        if phase(next) == RELEASING {
                            // The slot goes free only after its frame and waker are
                            // gone: a new owner could otherwise lose its waker to this
                            // release, or have its reservation lose this frame.
                            if slot.retained.swap(false, Ordering::AcqRel) {
                                // SAFETY: as `drop_retained`: `retained` was true, so the
                                // owner is initialized; this swap makes this the only drop.
                                unsafe { (*slot.frame.get()).assume_init_drop() };
                            }
                            slot.wake.clear();
                            // SAFETY: this release_any CAS uniquely changed the
                            // current owner's state to RELEASING. Frame destruction
                            // and waker cleanup have completed for slots[index].
                            unsafe { self.finish_return(index, slot, next) };
                        }
                        return;
                    }
                    Err(actual) => state = actual,
                }
            }
        }
    }
}

/// A response, borrowing its [`Call`]: a value decoded into a startup slot or, on
/// a native route, the transport's own storage.
pub struct Reply<'c, Resp: 'static, M: WireMode = Encoded> {
    call: &'c Call<Resp, M>,
}

impl<Resp: 'static, M: WireMode> fmt::Debug for Reply<'_, Resp, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reply").finish_non_exhaustive()
    }
}

impl<Resp: Send + Sync + 'static> Deref for Reply<'_, Resp, Encoded> {
    type Target = Resp;

    fn deref(&self) -> &Resp {
        let value = self.call.table.values.get(self.call.index as usize);
        // SAFETY: a `Reply` exists only after `try_response` decoded into this
        // slot and moved the call to `TAKEN`; the call owns the slot until
        // dropped, and the `Reply` borrows the call.
        match value {
            // SAFETY: successful try_response initialized this value; its Reply
            // borrows the live Call, excluding decode/clear/reuse for the borrow.
            Some(value) => match unsafe { value.get() } {
                Some(value) => value,
                // SAFETY: Reply construction requires successful decode, which
                // sets initialized; the live Call keeps it so until every view ends.
                None => unsafe { core::hint::unreachable_unchecked() },
            },
            None => {
                // SAFETY: the index came from `reserve`, which is below the capacity.
                unsafe { core::hint::unreachable_unchecked() }
            }
        }
    }
}

impl<Resp: StablePayload + Send + Sync + 'static> Reply<'_, Resp, InPlace> {
    /// Borrows the typed response from the retained native storage, checked.
    ///
    /// # Errors
    /// Reports a frame whose bytes are not a valid `Resp`, with a fixed-size reason.
    pub fn payload(&self) -> Result<&Resp, BorrowError> {
        // SAFETY: Reply holds a borrow of the unique completed Call; its owner
        // cannot be taken/dropped or reused while this payload borrow lives.
        borrow_checked::<Resp>(unsafe { self.call.table.retained_bytes(self.call.index) })
    }
}

// SAFETY: `Reply` only reads a value that is not written again while it lives.
unsafe impl<Resp: Send + Sync, M: WireMode> Send for Reply<'_, Resp, M> {}
// SAFETY: as above.
unsafe impl<Resp: Send + Sync, M: WireMode> Sync for Reply<'_, Resp, M> {}

impl<Resp: Send + Sync + 'static, M: WireMode> Call<Resp, M> {
    /// Runs the binding, then looks at this call: completes it if overdue.
    fn poll_outcome(&self) -> Option<Outcome<'_>> {
        // A pull binding holds responses until someone asks. Bounded by the table.
        self.shared.pump_route(
            self.responses.binding_route(),
            self.responses,
            self.table.capacity(),
        );
        let now = self.shared.platform.now();
        // SAFETY: Call's private index field owns this table's current reservation.
        // The returned Outcome borrows self; no mutable take or Drop can overlap.
        unsafe { self.table.check_outcome(self.index, now) }
    }

    /// The call's waker cell, for the async layer.
    #[doc(hidden)]
    /// Returns process-lifetime wake storage for this live call/route, or None for an invalid
    /// slot. Only the owning adapter registers or clears it; completion threads may wake it. A
    /// static storage lifetime does not extend slot ownership: clear on Call/Recv drop, never
    /// use the registration after reuse. Register before waiting and poll again after
    /// registration and each wake. This accessor neither blocks nor uses the heap.
    // [impl->req~ssr-async-seam~1]
    pub fn __wake(&self) -> Option<&'static RouteWake> {
        self.table.__wake(self.index)
    }
}

impl<Resp: Send + Sync + 'static> Call<Resp, Encoded> {
    /// Polls the call without waiting: `Ok(None)` while it is outstanding.
    ///
    /// A call whose deadline has passed completes here with `TimedOut`. Moves
    /// the call to a terminal taken phase when the response is decoded.
    ///
    /// Takes `&mut self`: the decode writes the call's one value slot, so two
    /// callers must not reach it, and a `Reply` is a shared borrow of the call.
    ///
    /// # Errors
    /// Returns the failure, with a fixed-size reason.
    pub fn try_response(&mut self) -> Result<Option<Reply<'_, Resp>>, CallError> {
        let Some(outcome) = self.poll_outcome() else {
            return Ok(None);
        };
        match outcome {
            Outcome::Failed(error) => Err(error),
            Outcome::Response { encoding, payload } => {
                // SAFETY: try_response has exclusive &mut Call; poll_outcome
                // established completed immutable bytes. No typed Reply exists.
                let decoded = unsafe { self.table.decode_into_value(self.index, encoding, payload) };
                // SAFETY: decode returned and payload is no longer used. This
                // exclusive encoded Call may drop its retained frame before Reply.
                unsafe { self.mark_taken(true) };
                if let Err(error) = decoded {
                    // SAFETY: exclusive Call has no Reply; TAKEN excludes another
                    // decode and the index is withheld until Call destruction.
                    // clear marks an owned value vacant before arbitrary Drop.
                    if let Some(value) = self.table.values.get(self.index as usize) {
                        unsafe { value.clear() };
                    }
                    return Err(error);
                }
                self.decoded_success = true;
                Ok(Some(Reply { call: self }))
            }
        }
    }

    /// The decoded response, once `try_response` has returned it. For the async
    /// layer, which holds the call for as long as the response is read.
    #[doc(hidden)]
    /// Borrows initialized decoded response storage from this call after a successful
    /// try_response. The borrow cannot outlive the call; no mutation or slot reuse is allowed
    /// during it. Callable on threads permitted by the call’s Send/Sync bounds. It neither
    /// blocks nor uses the heap. The adapter retains the call until all response borrows end
    /// and clears its wake registration when dropping it.
    // [impl->req~ssr-async-seam~1]
    pub fn __taken_value(&self) -> Option<&Resp> {
        let slot = self.table.slots.get(self.index as usize)?;
        if !self.decoded_success || phase(slot.state.load(Ordering::Acquire)) != TAKEN {
            return None;
        }
        let value = self.table.values.get(self.index as usize)?;
        // SAFETY: decoded_success proves Ok decode, independent of whether a
        // failed decode left initialized bytes. This live Call owns the taken
        // index; its shared borrow excludes clear, mutation and reuse throughout.
        unsafe { value.get() }
    }
}

impl<Resp: StablePayload + Send + Sync + 'static> Call<Resp, InPlace> {
    /// Polls the call without waiting: `Ok(None)` while it is outstanding.
    ///
    /// A call whose deadline has passed completes here with `TimedOut`. The
    /// response stays in the transport's storage until this call is dropped.
    /// Takes `&mut self`, as on the encoded form.
    ///
    /// # Errors
    /// Returns the failure, with a fixed-size reason.
    pub fn try_response(&mut self) -> Result<Option<Reply<'_, Resp, InPlace>>, CallError> {
        match self.poll_outcome() {
            None => Ok(None),
            Some(Outcome::Failed(error)) => Err(error),
            Some(Outcome::Response { .. }) => {
                // SAFETY: try_response owns &mut Call and creates no Reply until
                // after this transition; false preserves native owner storage.
                unsafe { self.mark_taken(false) };
                Ok(Some(Reply { call: self }))
            }
        }
    }

    /// The taken response, for the async layer, which holds the call.
    ///
    /// # Errors
    /// Reports a frame whose bytes are not a valid `Resp`, with a fixed-size reason.
    #[doc(hidden)]
    /// Borrows checked native response storage retained by this call; validity lasts only for
    /// this borrow of the call. The async adapter normally calls after try_response;
    /// memory safety also holds for an earlier call, which can see only a fully
    /// published current extent or absence. Retain the call until all borrows end.
    /// Callable on threads permitted by the call’s Send/Sync
    /// bounds. It neither blocks nor uses the heap and returns a fixed-size refusal for invalid
    /// bytes. Clear the wake registration on Call drop.
    // [impl->req~ssr-async-seam~1]
    pub fn __taken_payload(&self) -> Result<&Resp, BorrowError> {
        // SAFETY: this Call's private current reservation is held by self for the
        // returned borrow; no mutable take/Drop/reuse can overlap. retained_bytes
        // itself checks the Release-published retained flag before raw access.
        borrow_checked::<Resp>(unsafe { self.table.retained_bytes(self.index) })
    }
}

impl<Resp: Send + Sync + 'static, M: WireMode> Call<Resp, M> {
    /// # Safety
    /// Caller has the exclusive mutable response-taking borrow of this Call.
    /// If release_frame is true, every response byte borrow has ended and no
    /// native Reply/view will be exposed. Success consumes the COMPLETE response
    /// into TAKEN and may destroy its retained owner, never returning the index.
    unsafe fn mark_taken(&self, release_frame: bool) {
        // SAFETY: the caller's unique response-taking borrow excludes existing
        // Reply/views. COMPLETE->TAKEN CAS uniquely authorizes response ownership.
        // release_frame can run drop_retained only after all payload borrows ended;
        // retained memory is not exposed again through an encoded Reply.
        {
            let Some(slot) = self.table.slots.get(self.index as usize) else {
                return;
            };
            let mut state = slot.state.load(Ordering::Acquire);
            loop {
                if phase(state) != COMPLETE {
                    return;
                }
                match slot.state.compare_exchange(
                    state,
                    with_phase(state, TAKEN),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        if release_frame {
                            // Decoded, so the transport's storage can go back to it.
                            // SAFETY: this Call's exclusive response-taking operation
                            // owns the current slot and has finished all response views.
                            unsafe { self.table.drop_retained(slot) };
                        }
                        return;
                    }
                    Err(actual) => state = actual,
                }
            }
        }
    }
}

/// Startup check shared by response decoders.
pub(crate) fn check_decoders<T: 'static>(
    wires: &[TypedWire<T>],
    name: impl Fn() -> super::error::RouteName,
) -> Result<(), StartupError> {
    let native_payload = wires.iter().any(|wire| wire.info.identity.native_type_token.is_some());
    for wire in wires {
        match wire.info.decode_heap() {
            RuntimeHeap::None => {}
            RuntimeHeap::Allocates(reason) if native_payload => {
                return Err(StartupError::RouteNeedsHeapAtRuntime {
                    route: name(), wire: reason,
                });
            }
            RuntimeHeap::Allocates(_) => {}
            RuntimeHeap::Middleware(_) => {
                return Err(StartupError::InvalidConfig(
                    "a wire must not declare a middleware heap class",
                ));
            }
        }
    }
    if native_payload && mem::needs_drop::<T>() {
        return Err(StartupError::RouteNeedsHeapAtRuntime {
            route: name(),
            wire: "the decoded type owns heap memory",
        });
    }
    Ok(())
}

// GCR-3 source-only regression; UNCOMPILED and UNRUN.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod gcr_transition_regressions {
    use super::*;
    use crate::memory::StartupRegion;

    #[test]
    fn rpc_terminal_observer_cannot_reopen_owned_or_incomplete_states() {
        static REGION: StartupRegion<16384> = StartupRegion::new();
        let memory = REGION.claim().unwrap();
        let table = CallTable::<u32>::place(&memory, 1, 4, true, &[], None,
            super::super::LifetimePolicy::new(16).unwrap()).unwrap();
        let slot = table.slots.first().unwrap();
        // Isolated fresh table: no reserved Call, retained owner, value view or
        // other user exists. Synthetic states below authorize no raw payload access.
        for p in [FREE, RESERVED, DELIVERING, COMPLETE, TIMED_OUT, PEER_RESTARTED,
            PEER_LOST, ABANDONED, TAKEN, RELEASING]
        {
            let observed = with_phase(17 << 8, p);
            slot.state.store(observed, Ordering::Relaxed);
            for failure in [CompletionFailure::TimedOut, CompletionFailure::PeerRestarted,
                CompletionFailure::PeerLost]
            {
                assert!(!table.try_complete(slot, observed, failure));
                assert_eq!(slot.state.load(Ordering::Relaxed), observed);
            }
        }
        for failure in [CompletionFailure::TimedOut, CompletionFailure::PeerRestarted,
            CompletionFailure::PeerLost]
        {
            let armed = with_phase(17 << 8, ARMED);
            slot.state.store(armed, Ordering::Relaxed);
            assert!(table.try_complete(slot, armed, failure));
            assert_eq!(slot.state.load(Ordering::Relaxed), with_phase(armed, failure.phase()));
            assert!(!table.try_complete(slot, armed, failure));
        }
        slot.state.store(FREE, Ordering::Relaxed);
    }
}
