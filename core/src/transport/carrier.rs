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

//! Carrier routes: the primitives a Streamer is built from (design 3.4, 3.7, 9.3).
//!
//! A carrier route forwards a frame from one transport to another with its
//! identifiers and payload bytes unchanged (R24). There is no codec here and
//! no negotiation: a carrier parses metadata only to check that a frame
//! belongs on the route, is not expired and comes from a known peer. What
//! the frame carries is untouched.
//!
//! A route is declared with [`TransportSetup::carrier_ingress`] and
//! [`TransportSetup::carrier_egress`](super::TransportSetup::carrier_egress),
//! which exist only during initialization. Traffic exists only on
//! [`TransportRuntime::receive`](super::TransportRuntime::receive) and
//! [`TransportRuntime::send`](super::TransportRuntime::send) (R2).
//!
//! # Copies
//!
//! Already-encoded input is never staged. A byte-only (handoff) egress receives
//! the ingress bytes directly, chunk structure included. A lending egress takes
//! one copy into its loan. Both legs carrying the same envelope pass the metadata
//! through unchanged; a different egress envelope re-encodes the metadata only,
//! and the payload is still the ingress bytes (R24).
//!
//! # Delivery on a carrier ingress
//!
//! A carrier ingress pulls frames: [`LendingRuntime::receive`](super::LendingRuntime::receive)
//! on a lending binding, [`HandoffRuntime::receive`](super::HandoffRuntime::receive)
//! on a handoff binding. A binding that pushes retained frames with
//! [`RuntimePort::deliver_frame`](super::RuntimePort::deliver_frame) also works:
//! the frame stays in transport memory and only its owner moves.
//! [`RuntimePort::deliver`](super::RuntimePort::deliver) on a carrier route is refused,
//! because its bytes live only for that call: with no egress in hand there is
//! nowhere to put them except a staged copy, which R24 forbids. A push-only handoff
//! binding that needs a carrier ingress implements the pull form or delivers a
//! retained frame.
//!
//! # Backpressure
//!
//! A native queue cannot put a frame back, so a forwarder asks
//! [`has_capacity`](super::TransportRuntime::has_capacity) *before* taking. If the
//! egress fills between that check and the send, a frame from a lending ingress
//! can be kept with [`CarriedFrame::park`], in fixed storage, within the route's
//! retained budget. A handoff ingress frame cannot be parked: its bytes are
//! borrowed for the call, so the frame is dropped and the egress refusal counted.
//!
//! # Release
//!
//! A frame's storage outlives the sink call it was handed to, whatever the sink
//! does. The sink's argument borrows the bytes for the whole call, so a frame
//! parked and then dropped before the sink returns, on its thread or another, goes
//! back to the transport when the sink returns, before `receive` does. A parked
//! frame dropped after that goes back at once.
//!
//! No worker ever blocks. `Overflow::Wait` on a carrier egress is refused at
//! setup, because the worker would be the one waiting; on an ingress it means the
//! upstream sender waits, and the worker simply does not take the frame.

use core::cell::UnsafeCell;
use core::fmt;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use crate::platform::atomic::{AtomicU32, AtomicU8, Ordering};

use super::binding::{Flow, InboundFrame, PayloadBytes, PeerRef, ReplyToken, RouteDeclaration};
use super::envelope::{AdmittedWire, Envelope, ParsedFrame};
use super::error::{ReceiveError, RouteDirection, RouteName, StartupError};
use super::frame::RetainedFrame;
use super::inbound::{ByteRange, InboundDispatch, PeerTable, RxCommon};
use super::mode::{
    route_kind, route_name, EncodedFrame, Family, PassthroughFrame, TransportSetup, TxRoute,
};
use super::qos::{check_qos, Overflow, Qos, SendReport};
use super::report::{add, Counters, RouteId, RouteReport};
use super::slots::{FreeIndices, ReadyQueue, ReadyOffer, SlotPool};
use super::SendError;
use crate::frame::header::{HeaderLimits, RxHeader};
use crate::memory::{FixedVec, StartupMemory};
use crate::platform::Monotonic;
use crate::uattributes::{PayloadEncoding, UMessageType};
use crate::uri::UUri;
use crate::wire::{WireIdentity, WireInfo, WirePayloadIdentity};

/// How many eviction attempts a producer makes before refusing under contention.

/// One wire a carrier route agrees to carry, in a closed set (R24).
///
/// The carrier never encodes or decodes with it: the set decides which frames
/// are admitted and renders the selected-wire prefix a re-encoded head needs.
#[derive(Clone, Copy, Debug)]
pub struct CarriedWire {
    /// The selected wire identity.
    pub wire_id: WireIdentity,
    /// The payload family.
    pub payload_family: WireIdentity,
    /// The payload identity, including a native type token if the frame has one.
    pub identity: WirePayloadIdentity,
    /// The payload's required alignment on a lending egress.
    pub alignment: usize,
}

impl CarriedWire {
    /// The carried identity of an existing wire.
    #[must_use]
    pub const fn of(info: &WireInfo) -> Self {
        Self {
            wire_id: info.wire_id,
            payload_family: info.payload_family,
            identity: info.identity,
            alignment: info.alignment,
        }
    }
}

/// One carrier route: filters, limits, slot count and QoS. One value declares
/// both legs, ingress on one transport and egress on another.
///
/// Addresses are checked at setup: a lending route names one physical service, so
/// a wildcard filter on a lending leg needs [`physical_source`](Self::physical_source)
/// or is refused.
#[derive(Clone, Copy, Debug)]
pub struct CarrierRoute<'a> {
    /// The message kind this route carries, or `None` for every kind.
    pub kind: Option<UMessageType>,
    /// Frames whose source matches are carried; may be a wildcard.
    pub source_filter: &'a UUri,
    /// Frames whose sink matches are carried; `None` matches frames with no sink.
    pub sink_filter: Option<&'a UUri>,
    /// On a lending leg, the address of the one service this route attaches to.
    /// Defaults to the source filter, which must then be exact.
    pub physical_source: Option<&'a UUri>,
    /// On a lending leg, the physical sink, for a route that has one.
    /// Defaults to the sink filter.
    pub physical_sink: Option<&'a UUri>,
    /// The closed set of wires this route carries. Required.
    pub carried: &'a [CarriedWire],
    /// Largest payload in bytes. Required: a carrier has no wire to bound it.
    pub max_payload: usize,
    /// Longest access token. The default of 0 refuses a token-bearing frame.
    pub max_token: usize,
    /// Retained frames this route may hold at once on its ingress leg: one being
    /// carried, plus any parked. Loan or metadata slots on its egress leg. Default 2.
    pub slots: usize,
    /// Peers a route can track. Default 8, the SDK's `.max_peers(n)` default.
    pub max_peers: usize,
    /// QoS, checked separately against each leg's binding at setup.
    pub qos: Qos,
    /// Overrides this leg's transport default envelope, if the transport carries it.
    pub envelope: Option<Envelope>,
}

impl<'a> CarrierRoute<'a> {
    /// A route carrying `carried` frames from `source_filter`, with every
    /// optional setting at its default.
    #[must_use]
    pub const fn new(
        source_filter: &'a UUri,
        carried: &'a [CarriedWire],
        max_payload: usize,
    ) -> Self {
        Self {
            kind: None,
            source_filter,
            sink_filter: None,
            physical_source: None,
            physical_sink: None,
            carried,
            max_payload,
            max_token: 0,
            slots: 2,
            max_peers: 8,
            qos: Qos::DEFAULT,
            envelope: None,
        }
    }
}

/// A carrier ingress route. `Copy`, holds `&'static` data, dropping it does nothing.
///
/// Issued during initialization and used in runtime mode. A token used on a
/// runtime other than its own is refused at run time, naming nothing that needs
/// the heap (R2's backstop, for what the types cannot reach).
#[derive(Clone, Copy)]
pub struct IngressRoute {
    pub(crate) owner: u64,
    pub(crate) route: &'static CarrierInbound,
}

/// A carrier egress route. `Copy`, holds `&'static` data, dropping it does nothing.
#[derive(Clone, Copy)]
pub struct EgressRoute {
    pub(crate) owner: u64,
    pub(crate) route: &'static TxRoute,
}

const _: () = {
    assert!(!core::mem::needs_drop::<IngressRoute>());
    assert!(!core::mem::needs_drop::<EgressRoute>());
};

impl fmt::Debug for IngressRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IngressRoute")
            .field("route", self.route.common.name)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for EgressRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EgressRoute")
            .field("route", self.route.name)
            .finish_non_exhaustive()
    }
}

impl IngressRoute {
    /// This route's id, for [`TransportRuntime::counters`](super::TransportRuntime::counters).
    #[must_use]
    pub fn id(self) -> RouteId {
        self.route.common.id
    }
}

impl EgressRoute {
    /// This route's id, for [`TransportRuntime::counters`](super::TransportRuntime::counters).
    #[must_use]
    pub fn id(self) -> RouteId {
        self.route.id
    }
}

/// One slot of a carrier ingress: a retained frame and its views.
struct CarrierSlot {
    frame: UnsafeCell<MaybeUninit<RetainedFrame>>,
    meta: UnsafeCell<MaybeUninit<CarrierMeta>>,
    /// Who may release the frame: one of the `HOLD_*` states.
    hold: AtomicU8,
}

// Who may release a carrier slot's frame (`CarrierSlot::hold`).
//
// A frame handed to a sink is borrowed by the sink's argument, and in Rust's model
// that borrow is protected until the sink returns: freeing or reusing the storage
// during the call is undefined behaviour even if nothing reads it again. So during
// the call only the carry that handed the frame may release it, and it does so
// after the sink has returned or unwound. A release asked for inside the call, by
// dropping the parked frame on any thread, is recorded here and done then.

/// Free or queued: not handed to a sink, not parked.
const HOLD_NONE: u8 = 0;
/// Handed to a sink and not parked: the carry releases it when the sink returns.
const HOLD_HANDED: u8 = 1;
/// Parked during the sink call, and the parked frame is alive.
const HOLD_PARKED_IN_CALL: u8 = 2;
/// Parked and the parked frame dropped, both during the sink call: the carry
/// releases it when the sink returns.
const HOLD_RELEASE_ON_RETURN: u8 = 3;
/// Parked, and its sink call has returned: dropping the parked frame releases it.
const HOLD_PARKED: u8 = 4;

/// What a slot remembers about the frame it holds. Copied here when the frame
/// reaches the slot, so a parked frame never reaches back into its owner.
struct CarrierMeta {
    // Safety invariant: all borrowed text is removed from this static header;
    // token/traceparent/metadata/payload point to immutable ranges retained by
    // the matching final-address C-owner frame until complete release.
    header: RxHeader<'static>,
    token: Option<ByteRange>,
    traceparent: Option<ByteRange>,
    metadata: ByteRange,
    payload: Option<ByteRange>,
    encoding: Option<PayloadEncoding>,
    token_extent: Option<(usize, usize)>,
    reply: ReplyToken,
}

/// A carrier ingress: the route's slots and its admission state.
pub(crate) struct CarrierInbound {
    pub(crate) common: RxCommon,
    // Safety invariant: C-owner: slots/free/ready have equal payload capacity.
    // Each index is uniquely owned by producer, unread queue, or a carry/park
    // state machine. Published slots hold one final-address initialized frame
    // and initialized meta; every meta range stays immutable until that owner is
    // fully destroyed. All byte views end before pool return/reuse.
    slots: &'static [CarrierSlot],
    free: FreeIndices,
    ready: ReadyQueue,
    queued: AtomicU32,
    depth: u32,
    overflow: Overflow,
    parked: AtomicU32,
    max_parked: u32,
    /// Frames currently held: being carried, or parked. Bounds what a pull takes.
    retained: AtomicU32,
    budget: u32,
    max_payload: usize,
    pull_limit: usize,
}

// SAFETY: slots are reachable only through the index a thread popped from `free`
// or `ready`, so each is used by one owner at a time; a frame handed to a sink and
// parked has two possible releasers, its carry and its parked frame, and the slot's
// `hold` word lets exactly one of them release it. Retained frames are `Send` by
// the `Frame` contract and are never shared by reference.
unsafe impl Send for CarrierInbound {}
// SAFETY: as above; everything else is atomics or immutable after startup.
unsafe impl Sync for CarrierInbound {}

impl CarrierInbound {
    fn place(
        memory: &StartupMemory,
        common: RxCommon,
        depth: usize,
        slots: usize,
        max_parked: usize,
        max_payload: usize,
    ) -> Result<&'static Self, StartupError> {
        let count = depth.saturating_add(slots).saturating_add(1);
        let slot_count = u32::try_from(count).map_err(|_| {
            StartupError::Memory(crate::memory::StartupError::MemoryExhausted {
                requested: count,
                remaining: 0,
            })
        })?;
        let free = FreeIndices::new_in(memory, count)?.with_policy(common.shared.lifetime_policy());
        let ready = ReadyQueue::new_in(memory, slot_count, depth as u32, slots)?;
        let mut table = memory.fixed_vec::<CarrierSlot>(count)?;
        for _ in 0..count {
            table.push(CarrierSlot {
                frame: UnsafeCell::new(MaybeUninit::uninit()),
                meta: UnsafeCell::new(MaybeUninit::uninit()),
                hold: AtomicU8::new(HOLD_NONE),
            })?;
        }
        let overflow = common.qos.overflow;
        let route = memory.place(Self {
            common,
            slots: table.into_static_slice(),
            free,
            ready,
            queued: AtomicU32::new(0),
            depth: u32::try_from(depth).unwrap_or(u32::MAX),
            overflow,
            parked: AtomicU32::new(0),
            max_parked: u32::try_from(max_parked).unwrap_or(u32::MAX),
            retained: AtomicU32::new(0),
            budget: u32::try_from(slots).unwrap_or(u32::MAX),
            max_payload,
            pull_limit: depth.saturating_add(1),
        })?;
        Ok(&*route)
    }

    fn slot(&self, index: u32) -> Option<&CarrierSlot> {
        self.slots.get(index as usize)
    }

    /// # Safety
    /// Caller must satisfy ByteRange::as_slice's full range/initialization and
    /// immutability contract for 'a with a live C-owner witness. This does not
    /// create a static lifetime or keep the slot alive by itself.
    unsafe fn bytes<'a>(range: ByteRange) -> &'a [u8] {
        // SAFETY: the caller supplies as_slice's exact contract and lifetime'a.
        unsafe { range.as_slice() }
    }

    /// Returns a slot's frame to the transport and frees the slot. The caller owns
    /// `index`, and the slot's `hold` is already `HOLD_NONE`: a frame handed to a
    /// sink is released only by [`Self::end_carry`] or by its parked frame.
    /// # Safety
    /// Caller owns this initialized C-owner index's sole release obligation;
    /// hold is HOLD_NONE and every payload/meta/frame borrow has ended. Consumes
    /// the frame and pool obligation exactly once, after full destructor return.
    unsafe fn release_slot(&self, index: u32) {
        // SAFETY: C-owner and caller contract give a live uniquely owned frame in
        // the checked slot. assume_init_drop has exclusive access/no remaining
        // views. On unwind push is skipped. On return push consumes the raw pool
        // obligation only after the frame's entire destructor has completed.
        unsafe {
            if let Some(slot) = self.slot(index) {
                // SAFETY: the caller owns `index` and the frame was written before it
                // was published; this is the only drop of it.
                { (*slot.frame.get()).assume_init_drop() };
            }
            self.free.push(index);
        }
    }

    /// Takes one parked-frame reservation, if the route's budget allows it.
    fn reserve_park(&self) -> bool {
        let mut held = self.parked.load(Ordering::Acquire);
        loop {
            if held >= self.max_parked {
                return false;
            }
            match self.parked.compare_exchange_weak(
                held,
                held.saturating_add(1),
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => held = actual,
            }
        }
    }

    /// Takes one retained-frame reservation for a frame about to be pulled, if
    /// the route's budget allows it. Frames being carried and parked frames count.
    fn reserve_retained(&self) -> bool {
        let mut held = self.retained.load(Ordering::Acquire);
        loop {
            if held >= self.budget {
                return false;
            }
            match self.retained.compare_exchange_weak(
                held,
                held.saturating_add(1),
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => held = actual,
            }
        }
    }

    /// Parses `metadata` and `payload` and admits the frame's address and peer,
    /// counting any refusal.
    fn admit<'a>(
        &self,
        metadata: &'a [u8],
        payload: PayloadBytes<'a>,
        peer: PeerRef,
    ) -> Result<ParsedFrame<'a>, ReceiveError> {
        let parsed = self
            .common
            .envelope
            .parse(self.common.admitted, metadata, payload, &self.common.limits)
            .map_err(|error| {
                add(&self.common.counters.refused);
                ReceiveError::Header(error)
            })?;
        self.common.admit_addressing(&parsed, peer)?;
        // A frame with no payload names no encoding: the prefix already matched.
        if parsed.header.payload_encoding.is_some() {
            self.common.wire_index(&parsed)?;
        }
        if parsed.payload.len() > self.max_payload {
            add(&self.common.counters.refused);
            return Err(ReceiveError::TooLarge);
        }
        Ok(parsed)
    }

    /// Moves the owner to the slot, its final address, and only then takes the
    /// views a sink or a parked frame will read.
    /// # Safety
    /// Caller exclusively owns an empty C-owner index, without any live views.
    /// Success installs frame/meta at final address and retains that ownership;
    /// Err leaves no live frame. Unwind may withhold the slot, never publish it.
    unsafe fn store(&self, index: u32, frame: RetainedFrame) -> Result<(), ReceiveError> {
        // SAFETY: slot lookup checks bounds and the caller exclusively owns its
        // uninitialized frame cell. write moves exactly one RetainedFrame into its
        // final address before metadata/payload callbacks. Those pointer contracts
        // are valid during the shared owner borrow; C-owner keeps it immobile until
        // all resulting views end. parse/admit returns lifetime-bound safe slices,
        // not arbitrary raw pointers. Each Err path drops the only frame before
        // return; successful meta.write records those exact ranges without transfer.
        {
            let Some(slot) = self.slot(index) else {
                return Err(ReceiveError::SlotsExhausted);
            };
            let peer = frame.peer();
            // SAFETY: this caller holds `index` exclusively, from `free` or an eviction.
            let owner: &RetainedFrame = unsafe { (*slot.frame.get()).write(frame) };
            // SAFETY: the owner is at its final address and stays alive until the
            // slot is released, which is after every view below is last used.
            let metadata: &[u8] = unsafe { &*owner.metadata_ptr() };
            let parts = owner.payload_parts();
            // SAFETY: as above.
            let bytes: Option<&[u8]> = parts.map(|(bytes, _)| unsafe { &*bytes });
            let payload = bytes.map_or(PayloadBytes::Absent, PayloadBytes::one);
            let parsed = match self.admit(metadata, payload, peer) {
                Ok(parsed) => parsed,
                Err(error) => {
                    // SAFETY: the owner was written above and nothing else can reach
                    // this slot; this is its only drop.
                    unsafe { (*slot.frame.get()).assume_init_drop() };
                    return Err(error);
                }
            };
            let Some(head) = head_of(metadata, payload, &parsed.payload) else {
                add(&self.common.counters.refused);
                // SAFETY: as above.
                unsafe { (*slot.frame.get()).assume_init_drop() };
                return Err(ReceiveError::Unsupported("no head before the payload"));
            };
            let token_extent = text_extent(head, parsed.header.token());
            let token = parsed
                .header
                .token()
                .map(|text| ByteRange::of(text.as_bytes()));
            let traceparent = parsed
                .header
                .traceparent()
                .map(|text| ByteRange::of(text.as_bytes()));
            let encoding = parsed.header.payload_encoding;
            let payload = parsed.payload.as_contiguous().map(ByteRange::of);
            let header = parsed.header.clone().with_text(None, None);
            // SAFETY: this caller holds `index` exclusively until publish.
            unsafe {
                (*slot.meta.get()).write(CarrierMeta {
                    header,
                    token,
                    traceparent,
                    metadata: ByteRange::of(head),
                    payload,
                    encoding,
                    token_extent,
                    reply: ReplyToken::default(),
                });
            }
            Ok(())
        }
    }

    /// Hands a frame held in slot `index` to the sink. The frame goes back to the
    /// transport here, once the sink has returned or unwound, unless the sink kept
    /// it parked: nothing the sink does releases it while the sink still runs.
    /// # Safety
    /// Caller owns an initialized C-owner index and one retained budget credit;
    /// no prior carried/parked owner exists. Consumes that ownership into the
    /// carry/park state machine or TTL cleanup, with complete cleanup before reuse.
    unsafe fn carry_from_slot(
        &'static self,
        index: u32,
        sink: &mut dyn FnMut(CarriedFrame<'_>) -> Flow,
        handed: &mut usize,
    ) -> Flow {
        // SAFETY: C-owner and the unique index prove initialized meta access.
        // Before sink callback HOLD_HANDED and CarryEnd establish the only release
        // path. All bytes() calls use ranges from that meta, whose final-address
        // owner is retained until sink return/unwind and all callback views end.
        // Park consumes CarriedFrame; dropping it in the callback requests deferred
        // release rather than freeing the borrowed argument. TTL failure instead
        // ends the sole owner without creating any views.
        unsafe {
            let Some(slot) = self.slot(index) else {
                return Flow::Continue;
            };
            // SAFETY: this caller holds `index`, and its meta was written in `store`.
            let meta = { (*slot.meta.get()).assume_init_ref() };
            if self.common.check_ttl(&meta.header).is_err() {
                self.release_slot(index);
                self.retained.fetch_sub(1, Ordering::AcqRel);
                return Flow::Continue;
            }
            // This caller holds the index, so no parked frame of this slot exists.
            slot.hold.store(HOLD_HANDED, Ordering::Release);
            // Ends the carry when the sink returns, and when it unwinds.
            let carry = CarryEnd { route: self, index };
            let flow = {
                let text = |range: Option<ByteRange>| {
                    range.and_then(|range| core::str::from_utf8(Self::bytes(range)).ok())
                };
                let header = meta
                    .header
                    .clone()
                    .with_text(text(meta.token), text(meta.traceparent));
                let payload = meta.payload.map_or(PayloadBytes::Absent, |range| {
                    PayloadBytes::one(Self::bytes(range))
                });
                let frame = CarriedFrame {
                    metadata: Self::bytes(meta.metadata),
                    payload,
                    encoding: meta.encoding,
                    context: FrameContext {
                        header: &header,
                        envelope: self.common.envelope,
                        token: meta.token_extent,
                        reply: meta.reply,
                        park: Some(ParkHandle { route: self, index }),
                        parked_from: None,
                    },
                    _on_this_thread: PhantomData,
                };
                add(&self.common.counters.carried);
                *handed = handed.saturating_add(1);
                sink(frame)
            };
            // The sink has returned and every view taken above is out of scope.
            drop(carry);
            flow
        }
    }

    /// `HOLD_HANDED` to `HOLD_PARKED_IN_CALL`: the sink is keeping the frame.
    /// # Safety
    /// Caller holds the unique CarriedFrame's live ParkHandle for this index's
    /// current carry, plus one reserved park credit. Success transfers its park
    /// obligation; failure creates no ParkedFrame or byte lifetime extension.
    unsafe fn mark_parked(&self, index: u32) -> bool {
        // SAFETY: the live ParkHandle contract fixes this incarnation. Only a
        // HOLD_HANDED->HOLD_PARKED_IN_CALL winner may create a ParkedFrame;
        // other states refuse. The CAS leaves the carry's byte lifetime intact.
        {
            self.slot(index).is_some_and(|slot| {
                slot.hold
                    .compare_exchange(
                        HOLD_HANDED,
                        HOLD_PARKED_IN_CALL,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
            })
        }
    }

    /// The sink call for slot `index` has returned or unwound. Releases the frame,
    /// unless a parked frame still holds it: then that frame's drop releases it.
    /// # Safety
    /// Caller owns the only CarryEnd for this index and the sink call has ended
    /// (including unwind). All its argument borrows are over. Consumes the carry
    /// obligation once; a surviving parked owner keeps its own release duty.
    unsafe fn end_carry(&self, index: u32) {
        // SAFETY: C-owner and the CarryEnd contract identify this live hold word.
        // The CAS grants sole cleanup when HANDED or RELEASE_ON_RETURN transitions
        // to NONE, after all callback borrows have ended. PARKED_IN_CALL instead
        // transfers cleanup to the parked owner. release_slot sees HOLD_NONE and
        // exclusive initialized frame ownership only on the release branches.
        unsafe {
            let Some(slot) = self.slot(index) else {
                return;
            };
            let mut hold = slot.hold.load(Ordering::Acquire);
            loop {
                let next = match hold {
                    HOLD_HANDED | HOLD_RELEASE_ON_RETURN => HOLD_NONE,
                    HOLD_PARKED_IN_CALL => HOLD_PARKED,
                    // Unreachable: this carry set `HOLD_HANDED`, and only its frame's
                    // park and that parked frame's drop change it before this runs.
                    // Keeping the frame is the choice that cannot release it twice.
                    _ => return,
                };
                match slot
                    .hold
                    .compare_exchange_weak(hold, next, Ordering::AcqRel, Ordering::Acquire)
                {
                    Ok(_) => break,
                    Err(actual) => hold = actual,
                }
            }
            if hold == HOLD_PARKED_IN_CALL {
                return;
            }
            if hold == HOLD_RELEASE_ON_RETURN {
                self.parked.fetch_sub(1, Ordering::AcqRel);
            }
            self.release_slot(index);
            self.retained.fetch_sub(1, Ordering::AcqRel);
        }
    }

    /// Carries frames a binding pushed with `deliver_frame`, oldest first.
    fn carry_queued(
        &'static self,
        sink: &mut dyn FnMut(CarriedFrame<'_>) -> Flow,
        handed: &mut usize,
        budget: &super::ReceiveBudget,
    ) -> Flow {
        // SAFETY: reserve_retained admits one existing consumer credit; the exact
        // reserve_consumer result is uniquely held across pop and released once
        // after it. Some carries initialized C-owner ownership. carry_from_slot
        // consumes that exact owner and retained credit; None returns only credit.
        unsafe {
            while budget.remaining() != 0 {
                // Same admitted retained-frame budget as the pull path. A pending
                // consumer is already reserved before it can remove an unread owner.
                if !self.reserve_retained() { break; }
                let context = self.ready.reserve_consumer();
                let index = self.ready.pop(context);
                self.ready.release_consumer(context);
                let Some(index) = index else {
                    self.retained.fetch_sub(1, Ordering::AcqRel);
                    break;
                };
                let _ = budget.try_take();
                if !matches!(self.overflow, Overflow::DropOldest) {
                    self.queued.fetch_sub(1, Ordering::AcqRel);
                }
                if matches!(self.carry_from_slot(index, sink, handed), Flow::Stop) {
                    return Flow::Stop;
                }
            }
            Flow::Continue
        }
    }

    /// Carries frames pulled from a lending binding one at a time.
    pub(crate) fn carry_pulled(
        &'static self,
        pull: &mut dyn FnMut(&super::ReceiveBudget) -> Option<RetainedFrame>,
        sink: &mut dyn FnMut(CarriedFrame<'_>) -> Flow,
    ) -> usize {
        // SAFETY: reserve_retained admits one credit and free.pop transfers the
        // unique empty slot. A missing frame returns that unchanged slot; store's
        // Err leaves no frame before push. Success establishes C-owner for
        // carry_from_slot. Safe pull/sink callbacks can reenter or unwind but never
        // obtain this unpublished index. Unwind can leak it, not publish invalid state.
        unsafe {
            let mut handed = 0_usize;
            let budget = super::ReceiveBudget::new(self.pull_limit);
            if matches!(self.carry_queued(sink, &mut handed, &budget), Flow::Stop) {
                return handed;
            }
            while budget.remaining() != 0 {
                // Take the retained budget and a slot before pulling: with the budget spent
                // the frame stays in the binding's queue, where backpressure belongs.
                if !self.reserve_retained() {
                    break;
                }
                let Ok(index) = self.free.pop() else {
                    self.retained.fetch_sub(1, Ordering::AcqRel);
                    break;
                };
                let Some(frame) = pull(&budget) else {
                    self.free.push(index);
                    self.retained.fetch_sub(1, Ordering::AcqRel);
                    break;
                };
                if self.store(index, frame).is_err() {
                    self.free.push(index);
                    self.retained.fetch_sub(1, Ordering::AcqRel);
                    continue;
                }
                if matches!(self.carry_from_slot(index, sink, &mut handed), Flow::Stop) {
                    break;
                }
            }
            if budget.remaining() == 0 {
                self.common.wake_waiters();
            }
            handed
        }
    }

    /// How many frames one `receive` call examines, so a flood of frames that are
    /// refused or forwarded cannot hold a worker on this route.
    pub(crate) fn pull_limit(&self) -> usize {
        self.pull_limit
    }

    /// Carries one frame a handoff binding lent for the duration of its `receive`.
    pub(crate) fn carry_borrowed(
        &'static self,
        frame: InboundFrame<'_>,
        sink: &mut dyn FnMut(CarriedFrame<'_>) -> Flow,
        handed: &mut usize,
    ) -> Flow {
        let InboundFrame {
            metadata,
            payload,
            peer,
            reply,
        } = frame;
        let Ok(parsed) = self.admit(metadata, payload, peer) else {
            return Flow::Continue;
        };
        if self.common.check_ttl(&parsed.header).is_err() {
            return Flow::Continue;
        }
        let Some(head) = head_of(metadata, payload, &parsed.payload) else {
            add(&self.common.counters.refused);
            return Flow::Continue;
        };
        let token = text_extent(head, parsed.header.token());
        let carried = CarriedFrame {
            metadata: head,
            payload: parsed.payload,
            encoding: parsed.header.payload_encoding,
            context: FrameContext {
                header: &parsed.header,
                envelope: self.common.envelope,
                token,
                reply,
                park: None,
                parked_from: None,
            },
            _on_this_thread: PhantomData,
        };
        add(&self.common.counters.carried);
        *handed = handed.saturating_add(1);
        sink(carried)
    }

    /// Acquire actual storage without evicting speculatively for preparation.
    /// # Safety-usable invariant
    /// Ok transfers FreeIndices.pop's one live index obligation. C-owner geometry
    /// puts it in slots/ready; its prior frame destruction has completed.
    fn claim_for_push(&'static self) -> Result<u32, ReceiveError> {
        match self.free.pop() {
            Ok(index) => Ok(index),
            Err(super::PoolError::PoolRetired) => Err(ReceiveError::PoolRetired),
            Err(super::PoolError::Full) => {
                add(&self.common.counters.slots_exhausted);
                Err(ReceiveError::SlotsExhausted)
            }
        }
    }

    /// Reserves queue depth for `DropNewest` and `Wait`, refusing when full. A
    /// refusal is a dropped frame under `DropNewest`, and backpressure (`busy`)
    /// under `Wait`, where the binding keeps the frame and retries.
    fn reserve_depth(&self) -> bool {
        loop {
            let queued = self.queued.load(Ordering::Acquire);
            if queued < self.depth {
                if self
                    .queued
                    .compare_exchange_weak(
                        queued,
                        queued.saturating_add(1),
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    return true;
                }
                continue;
            }
            if self.queued.fetch_add(0, Ordering::AcqRel) >= self.depth {
                if matches!(self.overflow, Overflow::Wait { .. }) {
                    add(&self.common.counters.busy);
                } else {
                    add(&self.common.counters.dropped_newest);
                }
                return false;
            }
        }
    }
}

impl InboundDispatch for CarrierInbound {
    fn deliver(&'static self, frame: InboundFrame<'_>) -> Result<(), ReceiveError> {
        // A borrowed frame cannot wait for a `receive` call without a staged copy.
        let _ = frame;
        add(&self.common.counters.refused);
        Err(ReceiveError::Unsupported(
            "a carrier ingress takes frames by pull receive or retained delivery",
        ))
    }

    fn deliver_retained(&'static self, frame: RetainedFrame) -> Result<(), ReceiveError> {
        // SAFETY: local frame views are used only before the frame moves.
        // claim_for_push's unique raw pool index and C-owner geometry permit store.
        // Store Err completes cleanup before push. Success supplies initialized
        // exclusive input to ready.offer; Inserted/Replaced transfers input with
        // no later input access, Replaced grants sole victim, Full keeps input.
        // release_slot consumes only the exclusively owned victim/refused input.
        unsafe {
            // Reject a malformed or foreign frame before it can evict anything.
            let peer = frame.peer();
            {
                // SAFETY: the owner is alive for this block, and these views are
                // used only here: it has not reached its final address yet.
                let metadata: &[u8] = { &*frame.metadata_ptr() };
                let parts = frame.payload_parts();
                // SAFETY: as above.
                let bytes = parts.map(|(bytes, _)| { &*bytes });
                let payload = bytes.map_or(PayloadBytes::Absent, PayloadBytes::one);
                self.admit(metadata, payload, peer)?;
            }
            let reserved = !matches!(self.overflow, Overflow::DropOldest);
            if reserved && !self.reserve_depth() {
                return Err(ReceiveError::SlotsExhausted);
            }
            let index = match self.claim_for_push() {
                Ok(index) => index,
                Err(error) => {
                    if reserved {
                        self.queued.fetch_sub(1, Ordering::AcqRel);
                    }
                    return Err(error);
                }
            };
            if let Err(error) = self.store(index, frame) {
                self.free.push(index);
                if reserved {
                    self.queued.fetch_sub(1, Ordering::AcqRel);
                }
                return Err(error);
            }
            match self.ready.offer(index, !reserved) {
                ReadyOffer::Inserted => {},
                ReadyOffer::Replaced(victim) => {
                    add(&self.common.counters.dropped_oldest);
                    self.release_slot(victim);
                }
                ReadyOffer::Full => {
                    if reserved { self.queued.fetch_sub(1, Ordering::AcqRel); }
                    self.release_slot(index);
                    if matches!(self.overflow, Overflow::Wait { .. }) {
                        add(&self.common.counters.busy);
                    } else { add(&self.common.counters.dropped_newest); }
                    return Err(ReceiveError::SlotsExhausted);
                }
            }
            self.common.wake_waiters();
            Ok(())
        }
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
        self.pull_limit
    }

    fn binding_route(&self) -> super::binding::BindingRoute {
        self.common.binding_route
    }

    fn startup_check(&self) -> Result<(), StartupError> {
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
        self.common.report(self.slots.len(), 0)
    }

    fn counters(&self) -> &Counters {
        &self.common.counters
    }

    fn id(&self) -> RouteId {
        self.common.id
    }
}

/// Everything before the payload: the metadata, or for a whole-frame envelope
/// the head of its one chunk.
fn head_of<'a>(
    metadata: &'a [u8],
    payload: PayloadBytes<'a>,
    body: &PayloadBytes<'a>,
) -> Option<&'a [u8]> {
    if !metadata.is_empty() {
        return Some(metadata);
    }
    let whole = payload.as_contiguous()?;
    whole.get(..whole.len().checked_sub(body.len())?)
}

/// Where `text` sits inside `head`, for a binding's token bookkeeping.
fn text_extent(head: &[u8], text: Option<&str>) -> Option<(usize, usize)> {
    let text = text?;
    let start = (text.as_ptr() as usize).checked_sub(head.as_ptr() as usize)?;
    let end = start.checked_add(text.len())?;
    (end <= head.len()).then_some((start, text.len()))
}

/// Lets the sink holding a lending frame keep it in its slot.
#[derive(Clone, Copy)]
struct ParkHandle {
    route: &'static CarrierInbound,
    // Safety invariant: this index belongs to the associated live carry/park
    // state in route. It is never fabricated from an unrelated or returned slot.
    index: u32,
}

/// Ends one carry on every way out of its sink call, unwinding included.
struct CarryEnd {
    route: &'static CarrierInbound,
    // Safety invariant: this index belongs to the associated live carry/park
    // state in route. It is never fabricated from an unrelated or returned slot.
    index: u32,
}

impl Drop for CarryEnd {
    fn drop(&mut self) {
        // SAFETY: CarryEnd is the sole guard for this sink call and is dropped
        // only after the call's normal/unwind completion ends its argument borrows.
        unsafe { self.route.end_carry(self.index) };
    }
}

#[derive(Clone, Copy)]
struct FrameContext<'a> {
    header: &'a RxHeader<'a>,
    envelope: Envelope,
    token: Option<(usize, usize)>,
    reply: ReplyToken,
    /// Present while a lending frame is held by its slot and can still be parked.
    park: Option<ParkHandle>,
    /// The ingress this view came from, when the frame was parked: its time to
    /// live is checked again before it goes out.
    parked_from: Option<&'static CarrierInbound>,
}

/// One frame in a forwarder's hands, borrowed for the duration of the sink call.
///
/// Identifier and payload bytes are exactly the ingress bytes (R24). `send` takes
/// this by reference, so a frame can still be parked after a refusal. Every byte
/// view comes from an accessor that borrows this value, and [`park`](Self::park)
/// consumes it, so no view can be held across a park (or across anything else that
/// ends this frame's claim on its bytes).
///
/// The bytes stay valid until the sink returns, whatever it does with this value:
/// a frame parked and then dropped inside the sink goes back to the transport when
/// the sink returns, before `receive` does.
pub struct CarriedFrame<'a> {
    metadata: &'a [u8],
    payload: PayloadBytes<'a>,
    encoding: Option<PayloadEncoding>,
    context: FrameContext<'a>,
    /// A carried frame stays on its sink's thread (`!Send`, `!Sync`), as it always has.
    _on_this_thread: PhantomData<*const ()>,
}

impl fmt::Debug for CarriedFrame<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CarriedFrame")
            .field("metadata_len", &self.metadata.len())
            .field("payload_len", &self.payload.len())
            .field("encoding", &self.encoding)
            .finish_non_exhaustive()
    }
}

impl<'a> CarriedFrame<'a> {
    /// Everything before the payload, as the ingress carried it.
    #[must_use]
    pub fn metadata(&self) -> &[u8] {
        self.metadata
    }

    /// The payload bytes, unchanged, including chunk structure.
    #[must_use]
    pub fn payload(&self) -> PayloadBytes<'_> {
        self.payload
    }

    /// The frame's payload encoding; `None` for a frame with no payload.
    #[must_use]
    pub fn encoding(&self) -> Option<PayloadEncoding> {
        self.encoding
    }

    /// The message kind.
    #[must_use]
    pub fn kind(&self) -> UMessageType {
        self.context.header.kind
    }

    /// The frame's source address.
    #[must_use]
    pub fn source(&self) -> &UUri {
        &self.context.header.source
    }

    /// The frame's sink address, if it has one.
    #[must_use]
    pub fn sink(&self) -> Option<&UUri> {
        self.context.header.sink.as_ref()
    }

    /// Keeps this frame in fixed storage until it can be sent, on a lending
    /// ingress: nothing moves, and the frame holds the route's retained budget.
    ///
    /// Call it after a send refused for capacity (`Busy`, exhausted slots). It
    /// fails with the frame back when the frame is borrowed from a handoff
    /// binding, or the route's park budget is spent; the frame is then dropped
    /// when the sink returns, and the refusal already counted.
    ///
    /// Dropping the parked frame returns the frame to the transport. Dropped before
    /// the sink returns, on any thread, it is returned when the sink returns.
    ///
    /// # Errors
    /// The frame, when it cannot be parked.
    pub fn park(self) -> Result<ParkedFrame, Self> {
        let Some(handle) = self.context.park else {
            return Err(self);
        };
        if !handle.route.reserve_park() {
            return Err(self);
        }
        // SAFETY: CarriedFrame's live ParkHandle identifies this current carry;
        // self is consumed, and reserve_park above supplies its unique park credit.
        if !unsafe { handle.route.mark_parked(handle.index) } {
            // Unreachable: a carry hands out one frame, and `park` consumes it.
            handle.route.parked.fetch_sub(1, Ordering::AcqRel);
            return Err(self);
        }
        add(&handle.route.common.counters.parked);
        // SAFETY: RxHeader's only borrowed fields are token/traceparent. Both
        // came from the current C-owner frame after final placement. Successful
        // mark_parked keeps that owner until this ParkedFrame drops (and defers
        // release until the sink ends). No owner callback mutates those bytes.
        // Transmute changes only lifetimes, not representation or ownership;
        // ParkedFrame's field invariant prevents any 'static borrowed view from
        // escaping. frame()/accessors reborrow through &self; forget withholds
        // the static slot rather than releasing it under an escaped view.
        let header = unsafe {
            core::mem::transmute::<RxHeader<'a>, RxHeader<'static>>(self.context.header.clone())
        };
        Ok(ParkedFrame {
            route: handle.route,
            index: handle.index,
            header,
            metadata: ByteRange::of(self.metadata),
            payload: self.payload.as_contiguous().map(ByteRange::of),
            present: self.payload.is_present(),
            encoding: self.encoding,
            token: self.context.token,
            reply: self.context.reply,
            envelope: self.context.envelope,
        })
    }
}

/// A frame kept on a lending ingress until it can be sent (design 3.4).
///
/// A lease like `Received`: dropping it returns the frame to the transport and
/// frees its slot, and neither frees heap memory nor blocks (R39). Dropped while
/// the sink call that parked it is still running, it leaves the return to that
/// call, which makes it when the sink returns: the sink's argument still borrows
/// the bytes until then.
pub struct ParkedFrame {
    route: &'static CarrierInbound,
    // Safety invariant: this index belongs to the associated live carry/park
    // state in route. It is never fabricated from an unrelated or returned slot.
    index: u32,
    // Safety invariant: borrowed header text and raw extents belong to this
    // ParkedFrame's C-owner. The static annotation is erased storage only;
    // public views borrow self and no static text reference is exposed.
    header: RxHeader<'static>,
    metadata: ByteRange,
    payload: Option<ByteRange>,
    present: bool,
    encoding: Option<PayloadEncoding>,
    token: Option<(usize, usize)>,
    reply: ReplyToken,
    envelope: Envelope,
}

impl fmt::Debug for ParkedFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParkedFrame")
            .field("metadata_len", &self.metadata.len)
            .finish_non_exhaustive()
    }
}

// SAFETY: a parked frame owns its slot index; the retained frame is `Send` by
// the `Frame` contract. It is dropped exactly once: by this lease, or by the carry
// of the sink call that parked it when the lease went first, as the slot's `hold`
// word decides with a compare-exchange.
unsafe impl Send for ParkedFrame {}
// SAFETY: as above; `frame` only reads views of immutable retained bytes.
unsafe impl Sync for ParkedFrame {}

impl ParkedFrame {
    /// This frame, to try another `send` with. Its time to live is checked there.
    #[must_use]
    pub fn frame(&self) -> CarriedFrame<'_> {
        let payload = if self.present {
            let bytes = self.payload.map_or(&[][..], |range| {
                // SAFETY: ParkedFrame's C-owner obligation keeps this captured
                // initialized range immutable/live through the borrow of self.
                unsafe { CarrierInbound::bytes(range) }
            });
            PayloadBytes::one(bytes)
        } else {
            PayloadBytes::Absent
        };
        CarriedFrame {
            // SAFETY: the same ParkedFrame obligation preserves metadata's
            // complete range/provenance/final address for this self borrow.
            metadata: unsafe { CarrierInbound::bytes(self.metadata) },
            payload,
            encoding: self.encoding,
            context: FrameContext {
                header: &self.header,
                envelope: self.envelope,
                token: self.token,
                reply: self.reply,
                park: None,
                parked_from: Some(self.route),
            },
            _on_this_thread: PhantomData,
        }
    }
}

impl Drop for ParkedFrame {
    fn drop(&mut self) {
        let Some(slot) = self.route.slot(self.index) else {
            return;
        };
        let mut hold = slot.hold.load(Ordering::Acquire);
        loop {
            let next = match hold {
                // The sink call that parked it is still running: that call's
                // carry releases the frame when the sink returns.
                HOLD_PARKED_IN_CALL => HOLD_RELEASE_ON_RETURN,
                // That call is over: release it here.
                HOLD_PARKED => HOLD_NONE,
                // Unreachable while this parked frame exists. Keeping the frame
                // is the choice that cannot release it twice.
                _ => return,
            };
            match slot
                .hold
                .compare_exchange_weak(hold, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => break,
                Err(actual) => hold = actual,
            }
        }
        if hold == HOLD_PARKED {
            self.route.parked.fetch_sub(1, Ordering::AcqRel);
            // SAFETY: this Drop won PARKED->NONE after the sink ended; C-owner
            // and exclusive &mut self exclude all remaining view borrows. The
            // losing carry cannot release again, so this is the sole frame drop.
            unsafe { self.route.release_slot(self.index) };
            self.route.retained.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

/// A carrier leg's admission: envelope, wires and reserved lengths, checked once.
struct CarrierAdmission {
    envelope: Envelope,
    admitted: &'static [AdmittedWire],
    encodings: FixedVec<PayloadEncoding>,
    families: FixedVec<WireIdentity>,
    head: usize,
    alignment: usize,
}

/// Refuses a carrier route for a binding-level reason, naming the route.
fn carrier_refusal(
    setup: &TransportSetup,
    route: &RouteName,
    reason: &'static str,
) -> StartupError {
    StartupError::CarrierRefused {
        route: route.clone(),
        binding: setup.binding_name(),
        reason,
    }
}

fn admit_carrier(
    setup: &TransportSetup,
    route: &CarrierRoute<'_>,
    name: &RouteName,
    direction: RouteDirection,
) -> Result<CarrierAdmission, StartupError> {
    let memory = setup.memory();
    let capabilities = setup.capabilities();
    let envelope = route.envelope.unwrap_or(capabilities.default_envelope);
    if !capabilities.envelopes.contains(&envelope) {
        return Err(StartupError::EnvelopeNotAccepted {
            route: name.clone(),
        });
    }
    let support = setup.qos_support(route_kind(direction, route.kind));
    if let Err(refused) = check_qos(&route.qos, &support) {
        return Err(StartupError::QosNotHonored {
            route: name.clone(),
            setting: refused.setting,
            requested: refused.requested,
            available: refused.available,
        });
    }
    if route.carried.is_empty() {
        return Err(carrier_refusal(
            setup,
            name,
            "a carrier route must list the wires it carries",
        ));
    }
    if !capabilities.per_message_encoding && route.carried.len() > 1 {
        return Err(StartupError::SingleEncodingOnly {
            route: name.clone(),
        });
    }
    let count = route.carried.len();
    let mut admitted: FixedVec<AdmittedWire> = memory.fixed_vec(count)?;
    let mut encodings: FixedVec<PayloadEncoding> = memory.fixed_vec(count)?;
    let mut families: FixedVec<WireIdentity> = memory.fixed_vec(count)?;
    let mut prefix_len = 0_usize;
    let mut alignment = 1_usize;
    for carried in route.carried {
        let wire = AdmittedWire::admit(
            memory,
            envelope,
            carried.wire_id,
            carried.payload_family,
            carried.identity,
        )
        .map_err(|_| StartupError::EnvelopeNotAccepted {
            route: name.clone(),
        })?;
        prefix_len = prefix_len.max(wire.prefix.len());
        admitted.push(wire)?;
        encodings.push(carried.identity.encoding)?;
        families.push(carried.payload_family)?;
        alignment = alignment.max(carried.alignment.max(1));
    }
    let head = envelope.max_head_len(prefix_len, route.max_token);
    let requested = head.saturating_add(route.max_payload);
    if requested > capabilities.max_frame_len {
        return Err(StartupError::FrameTooLarge {
            route: name.clone(),
            requested,
            maximum: capabilities.max_frame_len,
        });
    }
    Ok(CarrierAdmission {
        envelope,
        admitted: admitted.into_static_slice(),
        encodings,
        families,
        head,
        alignment,
    })
}

/// The address a leg names to its binding: a lending route names one service.
fn physical_address<'a>(
    setup: &TransportSetup,
    route: &CarrierRoute<'a>,
    name: &RouteName,
) -> Result<(&'a UUri, Option<&'a UUri>), StartupError> {
    if setup.family() != Family::Lending {
        return Ok((route.source_filter, route.sink_filter));
    }
    let source = route.physical_source.unwrap_or(route.source_filter);
    let sink = route.physical_sink.or(route.sink_filter);
    let exact = |uri: &UUri| uri.check_no_wildcards().is_ok();
    if !exact(source) || sink.is_some_and(|sink| !exact(sink)) {
        return Err(carrier_refusal(
            setup,
            name,
            "a lending route names one physical service; set physical_source to the exact address",
        ));
    }
    if !route.source_filter.matches(source)
        || sink.is_some_and(|sink| {
            route
                .sink_filter
                .is_some_and(|filter| !filter.matches(sink))
        })
    {
        return Err(carrier_refusal(
            setup,
            name,
            "the physical address is outside the route's filter",
        ));
    }
    Ok((source, sink))
}

impl TransportSetup {
    /// Declares the receiving leg of a carrier route.
    ///
    /// # Errors
    /// Names the route for anything the binding cannot carry, a wildcard filter on
    /// a lending leg with no physical address, a QoS it cannot honor, or an empty
    /// wire set.
    pub fn carrier_ingress(&self, route: &CarrierRoute<'_>) -> Result<IngressRoute, StartupError> {
        let name = route_name(
            RouteDirection::Receive,
            route.kind,
            route.source_filter,
            route.sink_filter,
        );
        let admission = admit_carrier(self, route, &name, RouteDirection::Receive)?;
        let (source, sink) = physical_address(self, route, &name)?;
        // The binding is told exactly the table the SDK builds: none of zero entries.
        if route.max_peers == 0 {
            return Err(carrier_refusal(
                self,
                &name,
                "max_peers 0 leaves no entry for any identified sender",
            ));
        }
        let id = self.next_route_id()?;
        let inbound = self.next_inbound()?;
        let slots = route.slots.max(1);
        let depth = usize::from(route.qos.depth);
        let prepared = self
            .prepare_route(&RouteDeclaration {
            lifetime_policy: self.shared().lifetime_policy(),
                direction: RouteDirection::Receive,
                kind: route.kind,
                source,
                sink,
                inbound: Some(inbound),
                envelope: admission.envelope,
                encodings: admission.encodings.as_slice(),
                families: admission.families.as_slice(),
                max_metadata_len: admission.head,
                max_payload_len: route.max_payload,
                payload_alignment: admission.alignment,
                slots: depth.saturating_add(slots).saturating_add(1),
                qos: route.qos,
                native: false,
                max_peers: route.max_peers,
            })
            .map_err(|error| match error {
                StartupError::Binding { reason, .. } => carrier_refusal(self, &name, reason),
                other => other,
            })?;
        let memory = self.memory();
        let peers = PeerTable::new_in(memory, route.max_peers)?;
        let limits = HeaderLimits {
            max_token_len: route.max_token,
        };
        let name_ref: &'static RouteName = memory.place(name)?;
        let common = RxCommon::new(
            name_ref,
            id,
            prepared.route,
            route.kind,
            route.source_filter.clone(),
            route.sink_filter.cloned(),
            &[],
            admission.envelope,
            admission.admitted,
            limits,
            peers,
            route.qos,
            prepared.heap,
            prepared.deviations,
            self.shared(),
        );
        // A route parks at most `slots - 1` frames, so one slot stays free for the
        // frame being carried; a route of one slot parks the frame it holds.
        let max_parked = slots.saturating_sub(1).max(1);
        let placed =
            CarrierInbound::place(memory, common, depth, slots, max_parked, route.max_payload)?;
        self.check_route(super::RouteView {
            report: placed.report(),
            wires: &[],
            max_peers: route.max_peers,
            clients: &[],
            ttl: None,
            max_in_flight: 0,
        })?;
        self.add_rx(placed)?;
        Ok(IngressRoute {
            owner: self.id(),
            route: placed,
        })
    }

    /// Declares the sending leg of a carrier route.
    ///
    /// # Errors
    /// Names the route for anything the binding cannot carry, a wildcard filter on
    /// a lending leg with no physical address, a QoS it cannot honor, or
    /// `Overflow::Wait`: a carrier worker never blocks.
    pub fn carrier_egress(&self, route: &CarrierRoute<'_>) -> Result<EgressRoute, StartupError> {
        let name = route_name(
            RouteDirection::Send,
            route.kind,
            route.source_filter,
            route.sink_filter,
        );
        if matches!(route.qos.overflow, Overflow::Wait { .. }) {
            return Err(carrier_refusal(
                self,
                &name,
                "a carrier worker never blocks: Overflow::Wait is refused on an egress; \
                 declare Wait on the ingress, where the upstream sender waits",
            ));
        }
        let admission = admit_carrier(self, route, &name, RouteDirection::Send)?;
        let (source, sink) = physical_address(self, route, &name)?;
        let id = self.next_route_id()?;
        let slots = route.slots.max(1);
        let prepared = self
            .prepare_route(&RouteDeclaration {
            lifetime_policy: self.shared().lifetime_policy(),
                direction: RouteDirection::Send,
                kind: route.kind,
                source,
                sink,
                inbound: None,
                envelope: admission.envelope,
                encodings: admission.encodings.as_slice(),
                families: admission.families.as_slice(),
                max_metadata_len: admission.head,
                max_payload_len: route.max_payload,
                payload_alignment: admission.alignment,
                slots,
                qos: route.qos,
                native: false,
                max_peers: 0,
            })
            .map_err(|error| match error {
                StartupError::Binding { reason, .. } => carrier_refusal(self, &name, reason),
                other => other,
            })?;
        let memory = self.memory();
        let lending = self.family() == Family::Lending;
        let scratch = if lending {
            None
        } else {
            Some(SlotPool::new_in(memory, slots, admission.head)?.with_policy(self.shared().lifetime_policy()))
        };
        let name_ref: &'static RouteName = memory.place(name)?;
        let source_owned = source.clone();
        let placed: &'static TxRoute = memory.place(TxRoute {
            name: name_ref,
            id,
            binding_route: prepared.route,
            // A carrier route has no fixed message kind: the kind goes out per
            // frame, from its own metadata. This value only fills the record.
            kind: route.kind.unwrap_or(UMessageType::Publish),
            source: source_owned,
            sink: sink.cloned(),
            priority: None,
            ttl: None,
            permission_level: None,
            envelope: admission.envelope,
            admitted: admission.admitted,
            wire_heap: &[],
            value_slots: None,
            scratch,
            max_payload_len: route.max_payload,
            max_head: admission.head,
            max_token_len: route.max_token,
            // A traceparent travels in the metadata this route carries unchanged.
            allow_traceparent: true,
            alignment: admission.alignment,
            // Carried frames keep their own message IDs; this generator is unused.
            ids: super::ids::IdGenerator::seeded([0; 16], 0),
            qos: route.qos,
            counters: Counters::default(),
            heap: prepared.heap,
            deviations: prepared.deviations,
            lending,
            native: false,
            shared: self.shared(),
            binding: self.binding(),
        })?;
        self.check_route(super::RouteView {
            report: placed.report(),
            wires: &[],
            max_peers: 0,
            clients: &[],
            ttl: None,
            max_in_flight: 0,
        })?;
        self.add_tx(placed)?;
        Ok(EgressRoute {
            owner: self.id(),
            route: placed,
        })
    }
}

/// The route's admitted wire that carries this frame, if any.
fn admitted_wire(route: &'static TxRoute, header: &RxHeader<'_>) -> Option<&'static AdmittedWire> {
    route.admitted.iter().find(|wire| {
        Some(wire.encoding) == header.payload_encoding
            && wire.native_type_token == header.native_type_token
    })
}

/// Maps a refused parked-frame TTL check to the send error a caller sees.
fn map_ttl(error: ReceiveError) -> SendError {
    if matches!(error, ReceiveError::ClockUnavailable) {
        SendError::ClockUnavailable
    } else {
        SendError::Expired
    }
}

/// Counts and returns a refusal from before the binding was reached.
fn refused(route: &'static TxRoute, error: SendError) -> SendError {
    add(&route.counters.refused);
    error
}

/// `TransportRuntime::send`, after checking the route belongs to this transport.
pub(crate) fn send_carried(
    route: &'static TxRoute,
    frame: &CarriedFrame<'_>,
) -> Result<SendReport, SendError> {
    let context = &frame.context;
    if let Some(ingress) = context.parked_from {
        ingress
            .common
            .check_ttl(context.header)
            .map_err(|error| refused(route, map_ttl(error)))?;
    }
    if frame.payload.len() > route.max_payload_len {
        return Err(refused(route, SendError::PayloadTooLarge));
    }
    // The egress leg has its own closed set: a frame is carried only if it is in it.
    if let Some(encoding) = context.header.payload_encoding {
        if admitted_wire(route, context.header).is_none() {
            return Err(refused(
                route,
                SendError::EncodingNotAdmitted(encoding.id()),
            ));
        }
    }
    // On both paths below: an egress never carries a token past its own limit.
    if context
        .header
        .token()
        .is_some_and(|token| token.len() > route.max_token_len)
    {
        return Err(refused(route, SendError::TokenTooLong));
    }
    if context.envelope == route.envelope {
        // Same envelope: metadata and payload bytes go out exactly as they came.
        if frame.metadata.len() > route.max_head {
            return Err(refused(route, SendError::PayloadTooLarge));
        }
        let passthrough = PassthroughFrame {
            kind: context.header.kind,
            source: &context.header.source,
            sink: context.header.sink.as_ref(),
            metadata: frame.metadata,
            token: context.token,
            payload: frame.payload,
            reply: context.reply,
        };
        return route.binding.send_passthrough(route, passthrough);
    }
    // A different envelope re-encodes the metadata only; the payload bytes are
    // still the ingress bytes.
    let header = context.header;
    let wire = admitted_wire(route, header);
    // A frame with no payload has no wire of its own, but a selected-wire head
    // still needs a prefix: use the route's first.
    route.binding.send_encoded(
        route,
        EncodedFrame {
            parts: crate::frame::header::TxHeaderParts::from(header),
            wire: wire.or(route.admitted.first()),
            payload: frame.payload,
            reply: context.reply,
        },
    )
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod whitebox {
    use super::*;
    use crate::communication::LocalUriProvider;
    use crate::memory::StartupRegion;
    use crate::transport::binding::{InboundRoute, RuntimePort};
    use crate::transport::tests::{
        whole_frame, PushLending, StaticFrame, TestUri, CLOCK, ENCODING, FAMILY_ID, WIRE_ID,
    };
    use crate::wire::WirePayloadIdentity;

    static REGION: StartupRegion<{ 1 << 18 }> = StartupRegion::new();

    /// Membership and eviction are observed through delivered payloads, not the
    /// obsolete DropOldest preparation counter. The oldest committed frame alone
    /// is evicted, and the remaining frames retain their order.
    #[test]
    fn drop_oldest_keeps_exact_committed_payload_order() {
        let memory = REGION.claim().unwrap();
        let setup = TransportSetup::lending_on(&memory, &CLOCK, PushLending).unwrap();
        let topic = TestUri(1).get_resource_uri(0x8001);
        let carried = [CarriedWire {
            wire_id: WIRE_ID,
            payload_family: FAMILY_ID,
            identity: WirePayloadIdentity::serialized(ENCODING),
            alignment: 1,
        }];
        let mut route = CarrierRoute::new(&topic, &carried, 64);
        route.kind = Some(UMessageType::Publish);
        route.physical_source = Some(&topic);
        route.slots = 4;
        route.qos = Qos::DEFAULT.with_depth(2).with_overflow(Overflow::DropOldest);
        let ingress = setup.carrier_ingress(&route).unwrap();
        let first = whole_frame(setup.memory(), &topic, &[1, 2, 3, 4]);
        let second = whole_frame(setup.memory(), &topic, &[5, 6, 7, 8]);
        let third = whole_frame(setup.memory(), &topic, &[9, 10, 11, 12]);
        let runtime = setup.enter_runtime().unwrap();
        let port = RuntimePort { core: runtime.core };
        let from = PeerRef {
            peer: 7,
            incarnation: 1,
        };
        port.deliver_frame(
            InboundRoute(0),
            StaticFrame {
                bytes: first,
                peer: from,
            },
        )
        .unwrap();
        port.deliver_frame(
            InboundRoute(0),
            StaticFrame {
                bytes: second,
                peer: from,
            },
        )
        .unwrap();
        assert_eq!(runtime.counters(ingress.id()).dropped_oldest, 0);
        port.deliver_frame(
            InboundRoute(0),
            StaticFrame { bytes: third, peer: from },
        ).unwrap();
        assert_eq!(runtime.counters(ingress.id()).dropped_oldest, 1);
        let expected: [&[u8]; 2] = [&[5, 6, 7, 8], &[9, 10, 11, 12]];
        let mut seen = 0;
        assert_eq!(runtime.receive(ingress, &mut |frame: CarriedFrame<'_>| {
            assert!(seen < expected.len(), "more than the configured depth delivered");
            assert_eq!(frame.payload().as_contiguous(), Some(expected[seen]));
            seen += 1;
            Flow::Continue
        }).unwrap(), 2);
        assert_eq!(seen, 2, "the two surviving committed frames must be delivered");
        assert_eq!(runtime.receive(ingress, &mut |_| {
            panic!("an evicted or already delivered frame reappeared")
        }).unwrap(), 0);
        assert_eq!(runtime.counters(ingress.id()).dropped_oldest, 1);
    }
}
