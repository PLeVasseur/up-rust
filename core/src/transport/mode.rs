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

//! The two modes: [`TransportSetup`] during initialization, [`TransportRuntime`] after.
//!
//! Both are non-generic, so transports chosen from configuration can sit in a
//! list (R5). The transition is one-way: [`TransportSetup::enter_runtime`]
//! consumes the setup, so no setup call can be written afterwards, and every
//! traffic operation lives on a handle activated from a [`TransportRuntime`] (R2).
//!
//! Everything a route needs is placed in startup memory as it is declared. When
//! `enter_runtime` runs it only freezes pointers. A participant leaves by process
//! exit; runtime mode has no release path (R38).

use core::cell::{Cell, RefCell, UnsafeCell};
use core::fmt;
use core::mem::MaybeUninit;
use crate::platform::atomic::{AtomicBool, AtomicPtr, Ordering};
use core::time::Duration;

use super::binding::{
    BindingRefusal, BindingRoute, BindingSetup, HandoffFailure, Capabilities, Flow, HandoffRuntime, InboundRoute,
    LendingRuntime, OutgoingFrame, PayloadBytes, PreparedRoute, ReplyToken, RouteDeclaration,
    RouteKind, RuntimePort, SendRefusal,
};
use super::carrier::{CarriedFrame, CarrierInbound, EgressRoute, IngressRoute};
use super::envelope::{AdmittedWire, Envelope, PayloadSize};
use super::error::{RouteDirection, RouteName, StartupError};
use super::frame::RetainedFrame;
use super::ids::IdGenerator;
use super::inbound::InboundDispatch;
use super::qos::{Qos, QosSupport, SendReport};
use super::report::{add, Counters, RouteCounters, RouteId, RouteReport, SendMode};
use super::slots::{as_uninit_mut, SlotPool};
use super::waiver::RuntimeWaiver;
use super::{InitializedTx, PreparedTxStorage, ReceiveBudget, ReceiveError, SendError, TxFrameLayout};
use crate::frame::header::{TxHeaderParts, UFrameHeaderError, WrittenHeader};
use crate::memory::{FixedVec, SetupClaim, StartupMemory};
use crate::platform::{Monotonic, Platform, PlatformInit};
use crate::signal::{Signal, SignalWait, WaitToken};
use crate::uattributes::{UMessageType, UPriority};
use crate::ucode::UCode;
use crate::uri::UUri;
use crate::uuid::UUID;
use crate::wire::{EncodedSize, PayloadWriter, RuntimeHeap, WireError};

/// The monotonic-time and signal facts shared by every route of one transport.
pub(crate) struct TransportShared {
    pub(crate) platform: &'static dyn Platform,
    // Safety invariant: a nonnull, aligned pointer with provenance for a fully
    // initialized, permanently shared Signal. Targets are never reclaimed;
    // their interior mutation is exclusively through Signal's atomic API.
    signal: AtomicPtr<Signal>,
    binding: &'static dyn ErasedBinding,
    lifetime_policy: PublishedLifetimePolicy,
    pub(crate) deadline_signal: Signal,
    pub(crate) deadline_cancelled: AtomicBool,
}

impl TransportShared {
    pub(crate) fn notify_deadline_service(&self) {
        self.deadline_signal.notify_on(self.platform);
    }

    pub(crate) fn cancel_deadline_service(&self) {
        self.deadline_cancelled.store(true, Ordering::Release);
        self.notify_deadline_service();
    }

    pub(crate) fn lifetime_policy(&self) -> super::LifetimePolicy {
        self.lifetime_policy.get()
    }
    pub(crate) fn receive_fault(&self, route: BindingRoute) -> Option<ReceiveError> { self.binding.receive_fault(route) }
    pub(crate) fn binding_lifetime(&self, route: BindingRoute) -> super::LifetimeState { self.binding.pool_lifetime(route) }

    /// The transport's current signal.
    /// # Safety-usable invariant
    /// Returns one initialized permanent Signal. Replacement cannot invalidate
    /// an earlier result; no mutable reference or storage ownership escapes.
    pub(crate) fn signal(&self) -> &'static Signal {
        // SAFETY: forming &Signal requires nonnullness, alignment, initialized
        // readable storage and shared access for the returned lifetime. The
        // signal field invariant supplies these over size_of::<Signal>() bytes
        // in one allocation. Acquire observes initialization published by the
        // Release store, or the initial value published with TransportShared.
        // Replacing the pointer does not reclaim the old target. Only Signal's
        // atomics mutate its contents; no exclusive reference is created here.
        unsafe { &*self.signal.load(Ordering::Acquire) }
    }

    /// Bumps the transport's signal, waking any waiter.
    pub(crate) fn notify(&self) {
        self.signal().notify_on(self.platform);
    }

    /// Takes frames from a carrier route's binding on behalf of `receive`.
    pub(crate) fn receive_carried(
        &self,
        route: &'static CarrierInbound,
        sink: &mut dyn FnMut(CarriedFrame<'_>) -> Flow,
    ) -> usize {
        self.binding.receive_carried(route, sink)
    }

    /// Runs one bounded pull of `route` on behalf of a receive call.
    pub(crate) fn pump_route(
        &self,
        route: BindingRoute,
        inbound: &'static dyn InboundDispatch,
        limit: usize,
    ) -> usize {
        let budget = ReceiveBudget::new(limit.min(inbound.pull_limit()));
        self.pump_route_bounded(route, inbound, limit, &budget)
    }

    /// Reuses the outer receive call's allowance; never starts a nested budget.
    pub(crate) fn pump_route_bounded(
        &self,
        route: BindingRoute,
        inbound: &'static dyn InboundDispatch,
        limit: usize,
        budget: &ReceiveBudget,
    ) -> usize {
        let delivered = self.binding.pump_route(route, inbound, limit, budget);
        if budget.remaining() == 0 {
            // Exhaustion is not an empty observation. Include RPC call wakers.
            inbound.readable();
        }
        delivered
    }
}

/// Per-message fields beyond the route's fixed ones.
#[derive(Clone, Copy, Debug, Default)]
pub struct MessageFields<'a> {
    /// An explicit message ID; otherwise one is generated.
    pub id: Option<&'a UUID>,
    /// The correlated request ID, on a response.
    pub reqid: Option<&'a UUID>,
    /// An override of the route's priority.
    pub priority: Option<UPriority>,
    /// An access token, if the route admits one.
    pub token: Option<&'a str>,
    /// A traceparent, if the route admits one.
    pub traceparent: Option<&'a str>,
    /// A communication status, on a response.
    pub comm_status: Option<UCode>,
}

/// The per-route send state fixed during initialization.
pub(crate) struct TxRoute {
    pub(crate) name: &'static RouteName,
    pub(crate) id: RouteId,
    pub(crate) binding_route: BindingRoute,
    pub(crate) kind: UMessageType,
    pub(crate) source: UUri,
    pub(crate) sink: Option<UUri>,
    pub(crate) priority: Option<UPriority>,
    pub(crate) ttl: Option<Duration>,
    pub(crate) permission_level: Option<u32>,
    pub(crate) envelope: Envelope,
    pub(crate) admitted: &'static [AdmittedWire],
    /// Encode heap class per admitted wire, with the wire's name.
    pub(crate) wire_heap: &'static [(RuntimeHeap, &'static str)],
    /// Startup slots for encoding a value, on a handoff route. `None` on a lending route.
    pub(crate) value_slots: Option<SlotPool>,
    /// Small slots for metadata on the already-encoded path. Never hold a payload.
    pub(crate) scratch: Option<SlotPool>,
    pub(crate) max_payload_len: usize,
    /// Largest head (everything before the payload) reserved on this route.
    pub(crate) max_head: usize,
    pub(crate) max_token_len: usize,
    pub(crate) allow_traceparent: bool,
    pub(crate) alignment: usize,
    pub(crate) ids: IdGenerator,
    pub(crate) qos: Qos,
    pub(crate) counters: Counters,
    pub(crate) heap: RuntimeHeap,
    pub(crate) deviations: &'static [&'static super::Deviation],
    pub(crate) lending: bool,
    pub(crate) native: bool,
    pub(crate) shared: &'static TransportShared,
    pub(crate) binding: &'static dyn ErasedBinding,
}

impl fmt::Debug for TxRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TxRoute")
            .field("name", self.name)
            .finish_non_exhaustive()
    }
}

/// Refuses a per-message field the route was not set up for, before any work.
fn check_fields(route: &TxRoute, fields: &MessageFields<'_>) -> Result<(), SendError> {
    if let Some(token) = fields.token {
        if route.max_token_len == 0 {
            return Err(SendError::TokenNotAllowed);
        }
        if token.len() > route.max_token_len {
            return Err(SendError::TokenTooLong);
        }
    }
    if fields
        .traceparent
        .is_some_and(|value| !route.allow_traceparent || value.len() > 63)
    {
        return Err(SendError::TraceparentRejected);
    }
    // [impl->dsn~up-attributes-request-priority~1]
    let priority = fields.priority.or(route.priority);
    if matches!(route.kind, UMessageType::Request | UMessageType::Response)
        && priority.is_none_or(|value| value < UPriority::CS4)
    {
        return Err(SendError::PriorityRejected);
    }
    if route.kind == UMessageType::Response && fields.reqid.is_none() {
        return Err(SendError::MissingRequestId);
    }
    Ok(())
}

impl TxRoute {
    /// A fresh message ID, or `Err` on the (impossible for UUIDv7) failure.
    pub(crate) fn next_id(&self) -> Result<UUID, SendError> {
        self.ids
            .next_id(self.shared.platform)
            .ok_or(SendError::IdGeneration)
    }

    /// Header fields for this route, with this message's values.
    pub(crate) fn parts<'a>(
        &'a self,
        id: &'a UUID,
        fields: &MessageFields<'a>,
    ) -> Result<TxHeaderParts<'a>, SendError> {
        check_fields(self, fields)?;
        Ok(TxHeaderParts {
            kind: self.kind,
            priority: fields.priority.or(self.priority),
            id,
            reqid: fields.reqid,
            ttl: self.ttl,
            comm_status: fields.comm_status,
            permission_level: self.permission_level,
            source: &self.source,
            sink: self.sink.as_ref(),
            token: fields.token,
            traceparent: fields.traceparent,
            payload_encoding: None,
            native_type_token: None,
        })
    }

    pub(crate) fn report(&self) -> RouteReport {
        let (slots, slot_len) = self
            .value_slots
            .as_ref()
            .map_or((0, 0), |pool| (pool.slots(), pool.slot_len()));
        let send_mode = if self.lending {
            SendMode::Lend
        } else if self.value_slots.is_some() {
            SendMode::SlotCopy
        } else {
            SendMode::Direct
        };
        RouteReport {
            route: self.name,
            id: self.id,
            send_mode: Some(send_mode),
            slots,
            slot_len,
            envelope: self.envelope,
            qos: self.qos,
            heap: self.heap,
            native_payload: self.native
                || self.admitted.iter().any(|wire| wire.native_type_token.is_some()),
            deviations: self.deviations,
            lifetime_policy: self.shared.lifetime_policy(),
            deadline_timing: super::DeadlineTimingReport::pending(None),
        }
    }

    fn startup_check(&self) -> Result<(), StartupError> {
        for (heap, name) in self.wire_heap {
            match heap {
                RuntimeHeap::None => {}
                RuntimeHeap::Allocates(reason) => {
                    return Err(StartupError::RouteNeedsHeapAtRuntime {
                        route: self.name.clone(),
                        wire: reason,
                    });
                }
                RuntimeHeap::Middleware(_) => {
                    return Err(StartupError::RouteNeedsHeapAtRuntime {
                        route: self.name.clone(),
                        wire: name,
                    });
                }
            }
        }
        Ok(())
    }
}

pub(crate) type EncodeFn<'a> = &'a mut dyn FnMut(&mut PayloadWriter<'_>) -> Result<(), WireError>;

/// One value to encode into a loan or slot.
pub(crate) struct ValueFrame<'a> {
    pub(crate) parts: TxHeaderParts<'a>,
    pub(crate) wire: &'a AdmittedWire,
    pub(crate) size: EncodedSize,
    pub(crate) encode: EncodeFn<'a>,
    pub(crate) reply: ReplyToken,
}

/// A frame whose payload is already encoded, borrowed from its origin.
pub(crate) struct EncodedFrame<'a> {
    pub(crate) parts: TxHeaderParts<'a>,
    pub(crate) wire: Option<&'a AdmittedWire>,
    pub(crate) payload: PayloadBytes<'a>,
    pub(crate) reply: ReplyToken,
}

pub(crate) type FillFn<'a> = &'a mut dyn FnMut(&mut [MaybeUninit<u8>]) -> Result<(), SendError>;

/// An already-encoded frame whose metadata bytes are carried unchanged.
pub(crate) struct PassthroughFrame<'a> {
    pub(crate) kind: UMessageType,
    pub(crate) source: &'a UUri,
    pub(crate) sink: Option<&'a UUri>,
    pub(crate) metadata: &'a [u8],
    pub(crate) token: Option<(usize, usize)>,
    pub(crate) payload: PayloadBytes<'a>,
    pub(crate) reply: ReplyToken,
}

/// In-place construction carrying the private fill-initialization guarantee.
pub(crate) struct InPlaceFrame<'a> {
    parts: TxHeaderParts<'a>,
    wire: &'a AdmittedWire,
    payload_len: usize,
    // Safety invariant: every successful fill initializes this exact complete
    // supplied range with the declared native representation. Construction is
    // restricted to from_checked_fill; forwarding preserves this capability.
    fill: FillFn<'a>,
}

impl<'a> InPlaceFrame<'a> {
    /// Carries an initializer whose successful result can certify a native loan.
    ///
    /// # Safety
    /// For every invocation returning Ok, fill must have initialized every byte
    /// of the exact supplied payload_len-byte range, with the valid representation
    /// and field bits for wire's admitted native type. Any typed witness used as
    /// evidence must refer to that same base and complete range. No successful
    /// path may substitute a different range or leave/deinitialize any byte.
    /// No retained alias or deferred action may mutate/deinitialize that range
    /// after success before the binding takes ownership. This obligation lasts
    /// as long as this borrowed callback can be invoked through the frame.
    /// Err or unwind supplies no initialization certificate; the loan remains
    /// cancellable by its owner and the callback must not publish or release it.
    pub(crate) unsafe fn from_checked_fill(
        parts: TxHeaderParts<'a>,
        wire: &'a AdmittedWire,
        payload_len: usize,
        fill: FillFn<'a>,
    ) -> Self {
        Self { parts, wire, payload_len, fill }
    }
}

fn map_refusal(route: &TxRoute, failure: HandoffFailure) -> SendError {
    let refusal = failure.refusal;
    let cause = match refusal {
        SendRefusal::PoolRetired => SendError::PoolRetired,
        SendRefusal::Busy => {
            add(&route.counters.busy);
            SendError::Busy
        }
        SendRefusal::TooLarge => SendError::PayloadTooLarge,
        SendRefusal::Failed { code, detail } => {
            add(&route.counters.refused);
            SendError::Rejected { code, detail }
        }
    };
    match failure.report {
        Some(report) if report.delivered != 0 || report.dropped_full != 0 || !report.counts_exact =>
            SendError::PartialHandoff { refusal, report },
        None => SendError::PartialHandoff { refusal, report: SendReport::unknown() },
        _ => cause,
    }
}

fn map_encode(error: WireError) -> SendError {
    if matches!(error, WireError::BufferTooSmall { .. }) {
        SendError::PayloadTooLarge
    } else {
        SendError::Wire(error)
    }
}

/// Encodes a value into `out`, checking the writer the wire was given afterwards.
// [impl->req~ssr-unsafe-wire-untrusted~1]
/// # Safety-usable invariant
/// Every output byte initialized on entry stays initialized on all exits, including
/// callback unwind or writer replacement: PayloadWriter::new's closed safe API
/// never exposes MaybeUninit or deinitializes bytes. Success reports the checked
/// initialized prefix of this exact original output, after owns(base,len).
fn encode_into(out: &mut [MaybeUninit<u8>], encode: EncodeFn<'_>) -> Result<usize, SendError> {
    let base = out.as_ptr();
    let len = out.len();
    let mut writer = PayloadWriter::new(out);
    let encoded = encode(&mut writer);
    if !writer.owns(base, len) {
        return Err(SendError::Wire(WireError::WriterReplaced));
    }
    let used = writer.written();
    encoded.map_err(map_encode)?;
    Ok(used)
}

/// A lending transport's erased send path.
fn lend_value<R: LendingRuntime>(
    running: &R,
    route: &'static TxRoute,
    frame: ValueFrame<'_>,
) -> Result<SendReport, SendError> {
    let (payload_len, size) = match frame.size {
        EncodedSize::Exact(len) => (len, PayloadSize::Exact(len)),
        EncodedSize::AtMost(len) => {
            let bound = len.min(route.max_payload_len);
            (bound, PayloadSize::AtMost(bound))
        }
        EncodedSize::Unknown => (
            route.max_payload_len,
            PayloadSize::AtMost(route.max_payload_len),
        ),
    };
    if payload_len > route.max_payload_len {
        return Err(SendError::PayloadTooLarge);
    }
    let metadata_len = route
        .envelope
        .head_len(Some(frame.wire), &frame.parts, Some(payload_len))
        .map_err(SendError::Header)?;
    let layout = TxFrameLayout {
        metadata_len,
        payload_len: Some(payload_len),
        payload_alignment: route.alignment,
    };
    let mut tx = running
        .loan(route.binding_route, layout)
        .map_err(|e| if e == super::LoanError::PoolRetired { SendError::PoolRetired } else { SendError::Loan(e) })?;
    let head = {
        let metadata = tx.metadata_uninit_mut();
        if metadata.len() != metadata_len {
            return Err(SendError::PayloadStorage);
        }
        route
            .envelope
            .write_head(Some(frame.wire), &frame.parts, size, metadata)
            .map_err(SendError::Header)?
    };
    // The envelope initialized its reported prefix. A safe custom callback
    // may change its length answer; only the complete agreed metadata extent
    // can be certified for this loan, regardless of either callback's claim.
    if head.len != metadata_len {
        return Err(SendError::Header(UFrameHeaderError::Invalid(
            "envelope header length changed",
        )));
    }
    tx.record_header(WrittenHeader {
        len: head.len,
        token: head.token,
    })?;
    let out = tx.payload_uninit_mut().ok_or(SendError::PayloadStorage)?;
    if out.len() != payload_len || !(out.as_ptr() as usize).is_multiple_of(route.alignment) {
        return Err(SendError::PayloadStorage);
    }
    let used = encode_into(out, frame.encode)?;
    if matches!(size, PayloadSize::Exact(len) if len != used) {
        return Err(SendError::Wire(WireError::Unencodable(
            "the wire's exact size hint was wrong",
        )));
    }
    if used != payload_len {
        tx.truncate_payload(used)?;
    }
    route
        .envelope
        .finish(Some(used), tx.metadata_uninit_mut())
        .map_err(SendError::Header)?;
    // SAFETY: the header and payload views were written above: `write_head`
    // initialized the metadata extent (checked to be exact) and recorded it, the
    // `PayloadWriter` wrote `used` bytes which is the whole payload or was
    // truncated to it, and the envelope finished. The writer's storage was
    // checked to be this loan's own. PreparedTxStorage's unsafe contract keeps
    // these disjoint views owned, stable and initialized across record_header,
    // subsequent accessors and finish; successful truncate_payload either
    // preserves/initializes the suffix or publishes only the initialized prefix.
    // Safe custom codec semantics are not a memory-safety premise.
    let initialized = unsafe { InitializedTx::from_initialized(tx) };
    running.publish(route.binding_route, initialized)
}

/// A lending transport's erased in-place path.
fn lend_in_place<R: LendingRuntime>(
    running: &R,
    route: &'static TxRoute,
    frame: InPlaceFrame<'_>,
) -> Result<SendReport, SendError> {
    let metadata_len = route
        .envelope
        .head_len(Some(frame.wire), &frame.parts, Some(frame.payload_len))
        .map_err(SendError::Header)?;
    let layout = TxFrameLayout {
        metadata_len,
        payload_len: Some(frame.payload_len),
        payload_alignment: route.alignment,
    };
    let mut tx = running
        .loan(route.binding_route, layout)
        .map_err(|e| if e == super::LoanError::PoolRetired { SendError::PoolRetired } else { SendError::Loan(e) })?;
    let head = {
        let metadata = tx.metadata_uninit_mut();
        if metadata.len() != metadata_len {
            return Err(SendError::PayloadStorage);
        }
        route
            .envelope
            .write_head(
                Some(frame.wire),
                &frame.parts,
                PayloadSize::Exact(frame.payload_len),
                metadata,
            )
            .map_err(SendError::Header)?
    };
    // The envelope initialized its reported prefix. A safe custom callback
    // may change its length answer; only the complete agreed metadata extent
    // can be certified for this loan, regardless of either callback's claim.
    if head.len != metadata_len {
        return Err(SendError::Header(UFrameHeaderError::Invalid(
            "envelope header length changed",
        )));
    }
    tx.record_header(WrittenHeader {
        len: head.len,
        token: head.token,
    })?;
    let out = tx.payload_uninit_mut().ok_or(SendError::PayloadStorage)?;
    if out.len() != frame.payload_len || !(out.as_ptr() as usize).is_multiple_of(route.alignment) {
        return Err(SendError::PayloadStorage);
    }
    (frame.fill)(out)?;
    // SAFETY: write_head initialized the complete checked metadata extent and
    // record_header recorded it. InPlaceFrame::from_checked_fill's unsafe
    // contract makes this successful fill initialize the exact whole payload
    // with its valid admitted native representation. PreparedTxStorage's unsafe
    // ownership/preservation contract keeps both views stable and disjoint
    // through record_header and the payload accessor. No operation after fill
    // deinitializes either view; Err/unwind never reaches this certificate.
    // This does not certify arbitrary custom metadata codec semantics.
    let initialized = unsafe { InitializedTx::from_initialized(tx) };
    running.publish(route.binding_route, initialized)
}

/// A lending transport's erased already-encoded path: one copy into the loan.
fn lend_encoded<R: LendingRuntime>(
    running: &R,
    route: &'static TxRoute,
    frame: EncodedFrame<'_>,
) -> Result<SendReport, SendError> {
    let total = frame.payload.len();
    if total > route.max_payload_len {
        return Err(SendError::PayloadTooLarge);
    }
    let payload_len = frame.payload.is_present().then_some(total);
    let metadata_len = route
        .envelope
        .head_len(frame.wire, &frame.parts, payload_len)
        .map_err(SendError::Header)?;
    let layout = TxFrameLayout {
        metadata_len,
        payload_len,
        payload_alignment: route.alignment,
    };
    let mut tx = running
        .loan(route.binding_route, layout)
        .map_err(|e| if e == super::LoanError::PoolRetired { SendError::PoolRetired } else { SendError::Loan(e) })?;
    let size = payload_len.map_or(PayloadSize::Absent, PayloadSize::Exact);
    let head = {
        let metadata = tx.metadata_uninit_mut();
        if metadata.len() != metadata_len {
            return Err(SendError::PayloadStorage);
        }
        route
            .envelope
            .write_head(frame.wire, &frame.parts, size, metadata)
            .map_err(SendError::Header)?
    };
    // The envelope initialized its reported prefix. A safe custom callback
    // may change its length answer; only the complete agreed metadata extent
    // can be certified for this loan, regardless of either callback's claim.
    if head.len != metadata_len {
        return Err(SendError::Header(UFrameHeaderError::Invalid(
            "envelope header length changed",
        )));
    }
    tx.record_header(WrittenHeader {
        len: head.len,
        token: head.token,
    })?;
    if let Some(expected) = payload_len {
        let out = tx.payload_uninit_mut().ok_or(SendError::PayloadStorage)?;
        if out.len() != expected {
            return Err(SendError::PayloadStorage);
        }
        let mut rest = out;
        for chunk in frame.payload.chunks() {
            let (part, tail) = rest
                .split_at_mut_checked(chunk.len())
                .ok_or(SendError::PayloadStorage)?;
            for (slot, byte) in part.iter_mut().zip(chunk) {
                slot.write(*byte);
            }
            rest = tail;
        }
        if !rest.is_empty() {
            return Err(SendError::PayloadStorage);
        }
    }
    // SAFETY: the complete checked metadata extent was initialized/recorded and
    // every present payload byte was written by the checked chunk loop; the
    // empty remainder proves no tail is left uninitialized. PreparedTxStorage's
    // unsafe contract keeps these views disjoint, stable and owned across
    // record_header and the later payload accessor, preserving earlier writes.
    // These are encoded bytes, not a claimed typed native representation;
    // custom metadata semantics must still be validated by their consumers.
    let initialized = unsafe { InitializedTx::from_initialized(tx) };
    running.publish(route.binding_route, initialized)
}

/// A lending transport's carried frame: metadata unchanged, one payload copy.
fn lend_passthrough<R: LendingRuntime>(
    running: &R,
    route: &'static TxRoute,
    frame: PassthroughFrame<'_>,
) -> Result<SendReport, SendError> {
    let total = frame.payload.len();
    let payload_len = frame.payload.is_present().then_some(total);
    let layout = TxFrameLayout {
        metadata_len: frame.metadata.len(),
        payload_len,
        payload_alignment: route.alignment,
    };
    let mut tx = running
        .loan(route.binding_route, layout)
        .map_err(|e| if e == super::LoanError::PoolRetired { SendError::PoolRetired } else { SendError::Loan(e) })?;
    {
        let metadata = tx.metadata_uninit_mut();
        if metadata.len() != frame.metadata.len() {
            return Err(SendError::PayloadStorage);
        }
        for (slot, byte) in metadata.iter_mut().zip(frame.metadata) {
            slot.write(*byte);
        }
    }
    tx.record_header(WrittenHeader {
        len: frame.metadata.len(),
        token: frame.token,
    })?;
    if let Some(expected) = payload_len {
        let out = tx.payload_uninit_mut().ok_or(SendError::PayloadStorage)?;
        if out.len() != expected {
            return Err(SendError::PayloadStorage);
        }
        let mut rest = out;
        for chunk in frame.payload.chunks() {
            let (part, tail) = rest
                .split_at_mut_checked(chunk.len())
                .ok_or(SendError::PayloadStorage)?;
            for (slot, byte) in part.iter_mut().zip(chunk) {
                slot.write(*byte);
            }
            rest = tail;
        }
        if !rest.is_empty() {
            return Err(SendError::PayloadStorage);
        }
    }
    // SAFETY: the metadata extent was fully written above and recorded, and
    // every payload byte was written by the copy loop, whose lengths were checked.
    let initialized = unsafe { InitializedTx::from_initialized(tx) };
    running.publish(route.binding_route, initialized)
}

/// The runtime half of the erased binding.
pub(crate) trait ErasedBinding: Send + Sync {
    fn receive_fault(&self, route: BindingRoute) -> Option<ReceiveError>;
    fn pool_lifetime(&self, route: BindingRoute) -> super::LifetimeState;

    fn name(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;
    fn qos_support(&self, kind: RouteKind) -> QosSupport;
    fn is_lending(&self) -> bool;
    fn prepare_route(&self, route: &RouteDeclaration<'_>) -> Result<PreparedRoute, BindingRefusal>;
    fn start(&'static self, port: RuntimePort) -> Result<(), BindingRefusal>;

    fn send_value(
        &'static self,
        route: &'static TxRoute,
        frame: ValueFrame<'_>,
    ) -> Result<SendReport, SendError>;
    fn send_encoded(
        &'static self,
        route: &'static TxRoute,
        frame: EncodedFrame<'_>,
    ) -> Result<SendReport, SendError>;
    fn send_in_place(
        &'static self,
        route: &'static TxRoute,
        frame: InPlaceFrame<'_>,
    ) -> Result<SendReport, SendError>;
    fn send_passthrough(
        &'static self,
        route: &'static TxRoute,
        frame: PassthroughFrame<'_>,
    ) -> Result<SendReport, SendError>;
    fn has_capacity(&'static self, route: &'static TxRoute) -> bool;

    fn receive_carried(
        &'static self,
        route: &'static CarrierInbound,
        sink: &mut dyn FnMut(CarriedFrame<'_>) -> Flow,
    ) -> usize;

    fn receive_retained(
        &'static self, route: BindingRoute, budget: &ReceiveBudget,
    ) -> Option<RetainedFrame>;
    fn pump_route(
        &'static self,
        route: BindingRoute,
        inbound: &'static dyn InboundDispatch,
        limit: usize,
        budget: &ReceiveBudget,
    ) -> usize;
}

const NOT_RUNNING: SendError = SendError::Rejected {
    code: UCode::FailedPrecondition,
    detail: "transport is not running",
};

/// The running-value cell: written once by `enter_runtime`, read by sends.
struct RunningCell<R> {
    // Safety invariant: uninitialized until its unique set call writes one R.
    // The write completes before ready becomes true. Afterwards R is never
    // replaced, moved out or dropped through this cell, and only shared borrows
    // of it are exposed; their lifetimes are bounded by a borrow of this cell.
    value: UnsafeCell<MaybeUninit<R>>,
    // Safety invariant: false until the sole initializer's Release publication;
    // true is permanent and implies a fully initialized, shared-only value.
    ready: AtomicBool,
}

impl<R> RunningCell<R> {
    /// # Safety-usable invariant
    /// Establishes both field invariants with no initialized R and ready=false.
    const fn new() -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            ready: AtomicBool::new(false),
        }
    }

    /// Sends read this. `deliver` never does.
    /// # Safety-usable invariant
    /// Success returns an initialized shared R bounded by self's borrow, after
    /// Acquire publication. Failure never accesses value, including during set.
    fn get(&self) -> Result<&R, SendError> {
        if self.ready.load(Ordering::Acquire) {
            // SAFETY: UnsafeCell::get retains the live aligned field's provenance;
            // the implicit &MaybeUninit<R> covers exactly that field for self's
            // borrow. Acquire seeing true synchronizes with set's Release, so
            // the value invariant gives a fully initialized valid R and no
            // overlapping exclusive access. MaybeUninit::assume_init_ref thus
            // returns only a shared R; no move, drop or ownership transfer occurs.
            Ok(unsafe { (*self.value.get()).assume_init_ref() })
        } else {
            Err(NOT_RUNNING)
        }
    }

    /// # Safety-usable invariant
    /// The same reference/publication guarantees as get, with refusal as None.
    fn try_get(&self) -> Option<&R> {
        self.get().ok()
    }

    /// Writes the running value exactly once.
    ///
    /// # Safety
    /// The caller must own the sole initialization right for this cell: no prior
    /// or concurrent set, and no other direct access to its value storage. Calls
    /// to get/try_get may overlap; they read value only after ready publication.
    /// Consumes that right even if the owning setup is later abandoned. On return
    /// value is fully initialized, permanently shared-only, and Release-published.
    unsafe fn set(&self, value: R) {
        // SAFETY: UnsafeCell::get supplies nonnull aligned provenance for the
        // complete MaybeUninit<R> field for self's borrow. The caller's unique
        // initialization right excludes every other write. By ready's invariant,
        // get cannot touch these bytes until the subsequent Release store. The
        // implicit &mut MaybeUninit<R> is therefore exclusive for write; it never
        // denotes an uninitialized R. MaybeUninit::write moves value here without
        // dropping old storage; no callback occurs, and the mutable borrow ends
        // before publication. No later write is permitted by the consumed right.
        unsafe { (*self.value.get()).write(value) };
        self.ready.store(true, Ordering::Release);
    }
}

/// Setup cell of one binding: the setup value until it enters runtime.
struct SetupCell<S> {
    // Safety invariant: one initialized Option<S>; a successful busy CAS grants
    // its only mutable borrow, bounded by the with callback. No reference into
    // the Option escapes that borrow. It is accessed only while busy is held.
    value: UnsafeCell<Option<S>>,
    // Safety invariant: false means no value access is outstanding. One CAS
    // winner holds true; normal callback return releases it. Unwind leaves true
    // permanently, so later calls cannot access possibly partially changed S.
    busy: AtomicBool,
}

impl<S> SetupCell<S> {
    /// # Safety-usable invariant
    /// Establishes exclusive-cell invariants with Some(value) and busy=false.
    const fn new(value: S) -> Self {
        Self {
            value: UnsafeCell::new(Some(value)),
            busy: AtomicBool::new(false),
        }
    }

    /// Runs `f` with exclusive access. Re-entrance is refused, not a panic.
    /// # Safety-usable invariant
    /// Only the successful CAS owner calls f, with one exclusive Option<S>
    /// borrow. The callback may reenter or unwind; a failed CAS never accesses
    /// value, and unwind leaves it inaccessible. Normal return publishes changes
    /// with Release before another callback's Acquire. f cannot return a borrow
    /// of the Option through T, which is independent of the callback's borrow.
    fn with<T>(&self, f: impl FnOnce(&mut Option<S>) -> T) -> Result<T, BindingRefusal> {
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(BindingRefusal("re-entrant binding setup"));
        }
        // SAFETY: UnsafeCell::get supplies a nonnull aligned live Option<S> field.
        // The successful CAS and busy invariant exclude any overlapping content
        // access; Acquire observes the prior owner's Release changes. The value
        // invariant supplies initialization, and the callback borrow cannot
        // outlive this call. Caller code may mutate, reenter or unwind: competing
        // with calls fail before access, and unwind cannot reset busy or expose
        // an alias. Only this inner field, not the shared SetupCell, is &mut.
        let result = f(unsafe { &mut *self.value.get() });
        self.busy.store(false, Ordering::Release);
        Ok(result)
    }
}

struct LendingCell<S: BindingSetup> {
    // Safety invariant: initially Some(S); only start removes it, using
    // SetupCell::with(Option::take). No path restores Some after removal. That
    // unique successful removal owns running's sole initialization right.
    setup: SetupCell<S>,
    running: RunningCell<S::Running>,
    name: &'static str,
    capabilities: Capabilities,
}

// SAFETY: Sync permits concurrent shared access on different threads.
// SetupCell's field invariants and with postcondition serialize content access
// with CAS Acquire/Release, even for reentrant/unwinding caller code. S: Send
// (BindingSetup's supertrait) permits transfer between those exclusive users;
// no shared S reference escapes. The setup field invariant gives at most one
// running initializer. RunningCell's invariants/get/set contracts publish one
// immutable S::Running, whose Send + Sync bounds are enforced by BindingSetup.
// name/capabilities are immutable Sync values. No caller thread-affinity promise
// or safe trait's behavioral honesty is used for memory safety.
unsafe impl<S: BindingSetup> Sync for LendingCell<S> {}
// SAFETY: ownership transfer needs S: Send and S::Running: Send, both enforced
// by BindingSetup. Safe movement cannot overlap a borrow by with/get; remaining
// fields are immutable Send values or atomics. Dropping the owned wrapper may
// drop Option<S> on the receiving thread, which S: Send permits; MaybeUninit
// never implicitly drops the running value. No borrowed content can outlive it.
unsafe impl<S: BindingSetup> Send for LendingCell<S> {}

impl<S> ErasedBinding for LendingCell<S>
where
    S: BindingSetup,
    S::Running: LendingRuntime,
{
    fn receive_fault(&self, route: BindingRoute) -> Option<ReceiveError> { self.running.try_get().and_then(|r| r.receive_fault(route)) }
    fn pool_lifetime(&self, route: BindingRoute) -> super::LifetimeState { self.running.try_get().map_or(super::LifetimeState::Healthy, |r| r.pool_lifetime(route)) }

    fn name(&self) -> &'static str {
        self.name
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    fn qos_support(&self, kind: RouteKind) -> QosSupport {
        self.setup
            .with(|setup| setup.as_ref().map(|setup| setup.qos_support(kind)))
            .ok()
            .flatten()
            .unwrap_or(QosSupport::NONE)
    }

    fn is_lending(&self) -> bool {
        true
    }

    fn prepare_route(&self, route: &RouteDeclaration<'_>) -> Result<PreparedRoute, BindingRefusal> {
        self.setup.with(|setup| match setup {
            Some(setup) => setup.prepare_route(route),
            None => Err(BindingRefusal("transport already entered runtime")),
        })?
    }

    fn start(&'static self, port: RuntimePort) -> Result<(), BindingRefusal> {
        let setup = self
            .setup
            .with(Option::take)?
            .ok_or(BindingRefusal("transport already entered runtime"))?;
        let running = setup.into_running(port)?;
        // SAFETY: SetupCell::with grants the exclusive Option borrow, and the
        // standard Option::take moves Some(S) out and leaves None. LendingCell's
        // setup invariant forbids restoration, so only this invocation can
        // reach set. Reentrant/concurrent starts see None or busy. into_running
        // refusal/unwind consumes the same right without setting; success owns
        // an initialized S::Running. Concurrent get is permitted by set's
        // contract and cannot access the value until its Release publication.
        unsafe { self.running.set(running) };
        Ok(())
    }

    fn send_value(
        &'static self,
        route: &'static TxRoute,
        frame: ValueFrame<'_>,
    ) -> Result<SendReport, SendError> {
        let running = self.running.get()?;
        let result = lend_value(running, route, frame);
        record_send(route, result)
    }

    fn send_encoded(
        &'static self,
        route: &'static TxRoute,
        frame: EncodedFrame<'_>,
    ) -> Result<SendReport, SendError> {
        let running = self.running.get()?;
        let result = lend_encoded(running, route, frame);
        record_send(route, result)
    }

    fn send_in_place(
        &'static self,
        route: &'static TxRoute,
        frame: InPlaceFrame<'_>,
    ) -> Result<SendReport, SendError> {
        let running = self.running.get()?;
        let result = lend_in_place(running, route, frame);
        record_send(route, result)
    }

    fn send_passthrough(
        &'static self,
        route: &'static TxRoute,
        frame: PassthroughFrame<'_>,
    ) -> Result<SendReport, SendError> {
        let running = self.running.get()?;
        let result = lend_passthrough(running, route, frame);
        record_send(route, result)
    }

    fn has_capacity(&'static self, route: &'static TxRoute) -> bool {
        self.running
            .try_get()
            .is_some_and(|running| running.has_capacity(route.binding_route))
    }

    fn receive_carried(
        &'static self,
        route: &'static CarrierInbound,
        sink: &mut dyn FnMut(CarriedFrame<'_>) -> Flow,
    ) -> usize {
        let Some(running) = self.running.try_get() else {
            return 0;
        };
        let binding = route.common.binding_route;
        route.carry_pulled(
            &mut |budget| running.receive_bounded(binding, budget).map(RetainedFrame::new),
            sink,
        )
    }

    fn receive_retained(
        &'static self, route: BindingRoute, budget: &ReceiveBudget,
    ) -> Option<RetainedFrame> {
        let running = self.running.try_get()?;
        running.receive_bounded(route, budget).map(RetainedFrame::new)
    }

    fn pump_route(
        &'static self,
        route: BindingRoute,
        inbound: &'static dyn InboundDispatch,
        limit: usize,
        budget: &ReceiveBudget,
    ) -> usize {
        let mut pulled = 0_usize;
        while pulled < limit && budget.remaining() != 0 {
            // Room first. A frame left in the binding keeps its place there, and a
            // `Wait` sender keeps waiting; one taken with no room would be dropped.
            if !inbound.begin_pull() {
                break;
            }
            let Some(frame) = self.receive_retained(route, budget) else {
                inbound.end_pull_unused();
                break;
            };
            let _ = inbound.deliver_retained_pulled(frame);
            pulled = pulled.saturating_add(1);
        }
        pulled
    }
}

fn record_send(
    route: &'static TxRoute,
    result: Result<SendReport, SendError>,
) -> Result<SendReport, SendError> {
    match &result {
        Ok(_) => add(&route.counters.carried),
        Err(SendError::SlotsExhausted | SendError::Loan(super::LoanError::PoolExhausted)) => {
            add(&route.counters.slots_exhausted);
        }
        Err(SendError::Busy) => add(&route.counters.busy),
        Err(_) => add(&route.counters.refused),
    }
    result
}

struct HandoffCell<S: BindingSetup> {
    // Safety invariant: initially Some(S); only start removes it, using
    // SetupCell::with(Option::take). No path restores Some after removal. That
    // unique successful removal owns running's sole initialization right.
    setup: SetupCell<S>,
    running: RunningCell<S::Running>,
    name: &'static str,
    capabilities: Capabilities,
}

// SAFETY: SetupCell::with's Acquire/Release exclusivity and unwind containment
// permit sharing its S: Send across exclusive users, never exposing shared S.
// The setup field invariant gives one running initializer; RunningCell::get
// observes its Release publication before returning S::Running: Send + Sync.
// Other fields are immutable Sync values. Reentrant safe bindings cannot bypass
// either invariant, regardless of their semantic behavior.
unsafe impl<S: BindingSetup> Sync for HandoffCell<S> {}
// SAFETY: BindingSetup enforces S: Send and S::Running: Send. Safe movement
// excludes outstanding with/get borrows. Other fields are immutable Send data
// or atomics. An owned wrapper may drop Option<S> on the receiving thread, as
// allowed by S: Send; MaybeUninit never implicitly drops the running value.
unsafe impl<S: BindingSetup> Send for HandoffCell<S> {}

impl<S> ErasedBinding for HandoffCell<S>
where
    S: BindingSetup,
    S::Running: HandoffRuntime,
{
    fn receive_fault(&self, route: BindingRoute) -> Option<ReceiveError> { self.running.try_get().and_then(|r| r.receive_fault(route)) }
    fn pool_lifetime(&self, route: BindingRoute) -> super::LifetimeState { self.running.try_get().map_or(super::LifetimeState::Healthy, |r| r.pool_lifetime(route)) }

    fn name(&self) -> &'static str {
        self.name
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    fn qos_support(&self, kind: RouteKind) -> QosSupport {
        self.setup
            .with(|setup| setup.as_ref().map(|setup| setup.qos_support(kind)))
            .ok()
            .flatten()
            .unwrap_or(QosSupport::NONE)
    }

    fn is_lending(&self) -> bool {
        false
    }

    fn prepare_route(&self, route: &RouteDeclaration<'_>) -> Result<PreparedRoute, BindingRefusal> {
        self.setup.with(|setup| match setup {
            Some(setup) => setup.prepare_route(route),
            None => Err(BindingRefusal("transport already entered runtime")),
        })?
    }

    fn start(&'static self, port: RuntimePort) -> Result<(), BindingRefusal> {
        let setup = self
            .setup
            .with(Option::take)?
            .ok_or(BindingRefusal("transport already entered runtime"))?;
        let running = setup.into_running(port)?;
        // SAFETY: SetupCell::with's exclusive callback and Option::take leave
        // None before into_running is called. HandoffCell's setup invariant
        // forbids restoration, so only this call owns running's initialization
        // right; failed/reentrant/concurrent starts cannot reach set. The
        // returned S::Running is initialized. get may overlap and reads value
        // only after set publishes ready; no external safe code sees its storage.
        unsafe { self.running.set(running) };
        Ok(())
    }

    fn send_value(
        &'static self,
        route: &'static TxRoute,
        frame: ValueFrame<'_>,
    ) -> Result<SendReport, SendError> {
        let running = self.running.get()?;
        let pool = route.value_slots.as_ref().ok_or(SendError::Rejected {
            code: UCode::FailedPrecondition,
            detail: "route carries already-encoded bytes only",
        })?;
        let mut slot = pool.acquire().map_err(|error| {
            if error == super::PoolError::Full { add(&route.counters.slots_exhausted); }
            SendError::from(error)
        })?;
        // The slot's bytes hold a token the moment the head is written, so every
        // way out of this send clears them. On failure the head may be partial
        // and the token's place unknown, so the whole slot is cleared; it is
        // bounded by the route's reserved slot. A wire's `encode` that unwinds is
        // a way out too: the guard clears the slot then, before it is released.
        let guard = WipeUnlessReturned(&mut slot);
        let sent = send_value_from_slot(running, route, frame, &mut *guard.0);
        guard.returned();
        match sent {
            Ok((result, token)) => {
                wipe_token(&mut slot, token);
                match result {
                    Ok(report) => {
                        add(&route.counters.carried);
                        Ok(report)
                    }
                    Err(refusal) => Err(map_refusal(route, refusal)),
                }
            }
            Err(error) => {
                slot.bytes_mut().fill(0);
                Err(error)
            }
        }
    }

    fn send_encoded(
        &'static self,
        route: &'static TxRoute,
        frame: EncodedFrame<'_>,
    ) -> Result<SendReport, SendError> {
        let running = self.running.get()?;
        if frame.payload.len() > route.max_payload_len {
            return Err(SendError::PayloadTooLarge);
        }
        let pool = route.scratch.as_ref().ok_or(SendError::Rejected {
            code: UCode::FailedPrecondition,
            detail: "route has no metadata scratch",
        })?;
        let mut scratch = pool.acquire().map_err(|error| {
            if error == super::PoolError::Full { add(&route.counters.slots_exhausted); }
            SendError::from(error)
        })?;
        let size = if frame.payload.is_present() {
            PayloadSize::Exact(frame.payload.len())
        } else {
            PayloadSize::Absent
        };
        let head = route
            .envelope
            .write_head(
                frame.wire,
                &frame.parts,
                size,
                // SAFETY: only Envelope::write_head receives this view; its
                // safety-usable invariant preserves every initialized byte on
                // success/error/unwind and exposes only initialized u8 to callbacks.
                unsafe { as_uninit_mut(scratch.bytes_mut()) },
            )
            .inspect_err(|_| scratch.bytes_mut().fill(0))
            .map_err(SendError::Header)?;
        let metadata = scratch.bytes().get(..head.len).unwrap_or_default();
        let result = running.send(
            route.binding_route,
            OutgoingFrame {
                kind: frame.parts.kind,
                source: frame.parts.source,
                sink: frame.parts.sink,
                metadata,
                payload: frame.payload,
                reply: frame.reply,
            },
        );
        wipe_token(&mut scratch, head.token);
        match result {
            Ok(report) => {
                        add(&route.counters.carried);
                        Ok(report)
                    }
            Err(refusal) => Err(map_refusal(route, refusal)),
        }
    }

    fn send_in_place(
        &'static self,
        route: &'static TxRoute,
        _frame: InPlaceFrame<'_>,
    ) -> Result<SendReport, SendError> {
        add(&route.counters.refused);
        Err(SendError::Rejected {
            code: UCode::Unimplemented,
            detail: "native carriage needs a lending transport",
        })
    }

    fn send_passthrough(
        &'static self,
        route: &'static TxRoute,
        frame: PassthroughFrame<'_>,
    ) -> Result<SendReport, SendError> {
        let running = self.running.get()?;
        let result = running.send(
            route.binding_route,
            OutgoingFrame {
                kind: frame.kind,
                source: frame.source,
                sink: frame.sink,
                metadata: frame.metadata,
                payload: frame.payload,
                reply: frame.reply,
            },
        );
        match result {
            Ok(report) => {
                        add(&route.counters.carried);
                        Ok(report)
                    }
            Err(refusal) => Err(map_refusal(route, refusal)),
        }
    }

    fn has_capacity(&'static self, route: &'static TxRoute) -> bool {
        let scratch_free = route.scratch.as_ref().is_none_or(SlotPool::has_free);
        scratch_free
            && self
                .running
                .try_get()
                .is_some_and(|running| running.has_capacity(route.binding_route))
    }

    fn receive_carried(
        &'static self,
        route: &'static CarrierInbound,
        sink: &mut dyn FnMut(CarriedFrame<'_>) -> Flow,
    ) -> usize {
        let Some(running) = self.running.try_get() else {
            return 0;
        };
        let mut handed = 0_usize;
        let mut examined = 0_usize;
        let limit = route.pull_limit();
        let budget = ReceiveBudget::new(limit);
        running.receive_bounded(route.common.binding_route, &budget, &mut |frame| {
            let flow = route.carry_borrowed(frame, sink, &mut handed);
            examined = examined.saturating_add(1);
            if examined >= limit {
                Flow::Stop
            } else {
                flow
            }
        });
        if budget.remaining() == 0 {
            route.common.wake_waiters();
        }
        handed
    }

    fn receive_retained(
        &'static self, _route: BindingRoute, _budget: &ReceiveBudget,
    ) -> Option<RetainedFrame> {
        None
    }

    fn pump_route(
        &'static self,
        route: BindingRoute,
        inbound: &'static dyn InboundDispatch,
        limit: usize,
        budget: &ReceiveBudget,
    ) -> usize {
        let Some(running) = self.running.try_get() else {
            return 0;
        };
        // Room for the first frame before asking for any: a frame the binding
        // lends is taken, so one with no room would be dropped (R44).
        if limit == 0 || budget.remaining() == 0 || !inbound.begin_pull() {
            return 0;
        }
        // The peer sets the pace, so the route's bound is what ends this call. A
        // frame refused at admission counts toward it.
        let mut examined = 0_usize;
        let mut room = true;
        running.receive_bounded(route, budget, &mut |frame| {
            if room {
                let _ = inbound.deliver_pulled(frame);
            } else {
                // A binding that lends a frame after `Stop`: the usual delivery.
                let _ = inbound.deliver(frame);
            }
            room = false;
            examined = examined.saturating_add(1);
            if examined >= limit {
                return Flow::Stop;
            }
            // Room for the next frame, or stop: the binding keeps it.
            room = inbound.begin_pull();
            if room {
                Flow::Continue
            } else {
                Flow::Stop
            }
        });
        if room {
            inbound.end_pull_unused();
        }
        examined
    }
}

/// What [`send_value_from_slot`] leaves for its caller to clear.
type SlotSend = (Result<SendReport, HandoffFailure>, Option<(usize, usize)>);

/// Writes one value frame into `slot` and hands it to the transport. On `Err` the
/// binding was never called and the slot may hold part of a head.
fn send_value_from_slot<R: HandoffRuntime>(
    running: &R,
    route: &'static TxRoute,
    frame: ValueFrame<'_>,
    slot: &mut super::slots::SlotGuard,
) -> Result<SlotSend, SendError> {
    let size = match frame.size {
        EncodedSize::Exact(len) => PayloadSize::Exact(len),
        EncodedSize::AtMost(_) | EncodedSize::Unknown => PayloadSize::AtMost(route.max_payload_len),
    };
    let head = route
        .envelope
        .write_head(
            Some(frame.wire),
            &frame.parts,
            size,
            // SAFETY: only Envelope::write_head receives this view; its
                // safety-usable invariant preserves every initialized byte on
                // success/error/unwind and exposes only initialized u8 to callbacks.
                unsafe { as_uninit_mut(slot.bytes_mut()) },
        )
        .map_err(SendError::Header)?;
    let token = head.token;
    let (head_region, area) = slot
        .bytes_mut()
        .split_at_mut_checked(head.len)
        .ok_or(SendError::PayloadTooLarge)?;
    let limit = route.max_payload_len.min(area.len());
    let area = area.get_mut(..limit).unwrap_or_default();
    // SAFETY: encode_into's safety-usable invariant and PayloadWriter's closed
    // methods preserve all initialized bytes even if the safe callback replaces
    // the writer, reenters, or unwinds. area stays exclusively reborrowed here.
    let used = encode_into(unsafe { as_uninit_mut(area) }, frame.encode)?;
    if matches!(size, PayloadSize::Exact(len) if len != used) {
        return Err(SendError::Wire(WireError::Unencodable(
            "the wire's exact size hint was wrong",
        )));
    }
    route
        .envelope
        .finish(Some(used),
            // SAFETY: finish's documented invariant only writes initialized u8;
            // it preserves the initialized head on all exits and has no callbacks.
            unsafe { as_uninit_mut(head_region) })
        .map_err(SendError::Header)?;
    let bytes = slot.bytes();
    let (metadata, rest) = bytes
        .split_at_checked(head.len)
        .ok_or(SendError::PayloadStorage)?;
    let payload = rest.get(..used).unwrap_or_default();
    let result = running.send(
        route.binding_route,
        OutgoingFrame {
            kind: frame.parts.kind,
            source: frame.parts.source,
            sink: frame.parts.sink,
            metadata,
            payload: PayloadBytes::one(payload),
            reply: frame.reply,
        },
    );
    Ok((result, token))
}

/// Clears a value slot's bytes unless `returned` is called: so a send that unwinds
/// out of a wire's `encode` leaves no token in the slot it gives back (L4, R2-4).
pub(crate) struct WipeUnlessReturned<'s>(pub(crate) &'s mut super::slots::SlotGuard);

impl WipeUnlessReturned<'_> {
    /// The send returned; its own path clears what it must.
    pub(crate) fn returned(self) {
        core::mem::forget(self);
    }
}

impl Drop for WipeUnlessReturned<'_> {
    fn drop(&mut self) {
        self.0.bytes_mut().fill(0);
    }
}

fn wipe_token(slot: &mut super::slots::SlotGuard, token: Option<(usize, usize)>) {
    if let Some((offset, len)) = token {
        if let Some(bytes) = slot.bytes_mut().get_mut(offset..offset.saturating_add(len)) {
            bytes.fill(0);
        }
    }
}

/// The runtime-mode state of one transport. Lives until process exit.
pub(crate) struct RuntimeCore {
    pub(crate) activation_sample_ns: u64,
    pub(crate) id: u64,
    pub(crate) binding: &'static str,
    pub(crate) shared: &'static TransportShared,
    pub(crate) tx: &'static [&'static TxRoute],
    pub(crate) rx: &'static [&'static dyn InboundDispatch],
    pub(crate) waivers: &'static [&'static RuntimeWaiver],
    pub(crate) reports: &'static [RouteReport],
}

impl fmt::Debug for RuntimeCore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeCore")
            .field("id", &self.id)
            .field("binding", &self.binding)
            .finish_non_exhaustive()
    }
}

impl RuntimeCore {
    pub(crate) fn inbound(&self, route: InboundRoute) -> Option<&'static dyn InboundDispatch> {
        self.rx.get(route.0 as usize).copied()
    }

    pub(crate) fn notify(&self) {
        self.shared.notify();
    }
}

/// A transport in runtime mode. Only traffic exists here.
///
/// It is `Copy`, holds `&'static` data only and has no `Drop`: there is no
/// teardown path (R38, R39). Setup operations are not offered on this type.
#[derive(Clone, Copy, Debug)]
pub struct TransportRuntime {
    pub(crate) core: &'static RuntimeCore,
}

const _: () = assert!(!core::mem::needs_drop::<TransportRuntime>());

impl TransportRuntime {
    /// Samples SDK and binding pool lifetime for one route without allocating.
    /// Use `routes()` for its name and configured margin. States are latched;
    /// this is not a synchronized snapshot or a monitor scheduling guarantee.
    pub fn pool_lifetime(&self, route: RouteId) -> Option<super::LifetimeState> {
        for tx in self.core.tx { if tx.id == route {
            let mut state = self.core.shared.binding_lifetime(tx.binding_route);
            for pool in [tx.value_slots.as_ref(), tx.scratch.as_ref()].into_iter().flatten() { state = state.max(pool.lifetime()); }
            return Some(state);
        } }
        for rx in self.core.rx { if rx.id() == route { return Some(rx.pool_lifetime().max(self.core.shared.binding_lifetime(rx.binding_route()))); } }
        None
    }
    /// Latched binding receive fault. Poll alongside service/diagnostics even
    /// when `pump` returns zero, which is a work count rather than empty proof.
    pub fn receive_fault(&self, route: RouteId) -> Option<ReceiveError> {
        for rx in self.core.rx { if rx.id() == route {
            return if rx.pool_lifetime() == super::LifetimeState::Retired { Some(ReceiveError::PoolRetired) }
                   else { self.core.shared.receive_fault(rx.binding_route()) };
        } }
        Some(ReceiveError::UnknownRoute)
    }

    /// The waivers recorded when this transport entered runtime (R41).
    #[must_use]
    pub fn waivers(&self) -> &'static [&'static RuntimeWaiver] {
        self.core.waivers
    }

    /// Reports each route's mode and reservations (R16). Fixed at startup.
    #[must_use]
    pub fn routes(&self) -> &'static [RouteReport] {
        self.core.reports
    }

    /// Independent relaxed counter loads; this does not service a monitor or
    /// produce a cross-field atomic snapshot. A deadline load is a qualifying
    /// next read only under its report's `qualifying_read` contract and evidence.
    /// COUNTED is not publication. Acquire alone would not force visibility;
    /// callers' later observation/reaction time is additional to the reported J.
    #[must_use]
    pub fn counters(&self, route: RouteId) -> RouteCounters {
        for tx in self.core.tx {
            if tx.id == route {
                return tx.counters.snapshot();
            }
        }
        for rx in self.core.rx {
            if rx.id() == route {
                return rx.counters().snapshot();
            }
        }
        RouteCounters::default()
    }

    /// A token to wait on the transport's signal. Take it before polling.
    #[must_use]
    pub fn wait_token(&self) -> WaitToken {
        self.core.shared.signal().token()
    }

    /// Blocks until a frame arrives or a capacity change happens, or `timeout` elapses.
    ///
    /// Always bounded (R40). A deadline-monitored route's check runs here.
    pub fn wait(&self, token: WaitToken, timeout: Duration) -> SignalWait {
        super::deadline_service::wait(self.core, token, timeout)
    }

    /// Moves the RPC responses pull bindings are holding to their calls, at most a
    /// response route's bound of frames from each. Returns the number of frames handed to SDK admission, including SDK
    /// refusals. Native-internal rejections and empty/error attempts consume the
    /// raw-attempt allowance but are excluded from this return value. Zero is
    /// not proof that the native queue is empty.
    ///
    /// A response completes a call that may have nobody polling it: a call
    /// awaited on another task, woken when its response arrives, or a call
    /// already dropped, whose late response would otherwise keep its place in the
    /// binding's queue. A call that is polled pulls its own response.
    ///
    /// Every other route is left alone. A typed route (a subscriber, a listener,
    /// a server's requests) is read by `try_recv`, which pulls with the route's
    /// QoS: its queue is the binding's, and moving frames ahead of it would let
    /// the route hold twice its depth (R2-2). A carrier route takes frames under
    /// its forwarder's capacity check, by [`Self::receive`].
    ///
    /// Deadlines use actual binding-queue arrivals reported through RuntimePort;
    /// a pull/read does not renew them and introduces no application polling duty.
    ///
    /// Raw attempts are bounded by the smaller of route depth plus one and
    /// call-table capacity plus one, including binding-internal rejections. A
    /// flooding peer therefore cannot hold this call. One call is the whole drain: call it once per tick,
    /// not in a loop until it returns 0, which ends only when the peer stops.
    pub fn pump(&self) -> usize {
        let mut delivered = 0_usize;
        for route in self.core.rx {
            if !route.pumped() {
                continue;
            }
            delivered = delivered.saturating_add(self.core.shared.pump_route(
                route.binding_route(),
                *route,
                route.pull_limit(),
            ));
        }
        delivered
    }

    /// Completes one whole service pass, including RPC expiry and notifications.
    /// Qualified deadline configurations own an autonomous initialization-created
    /// service; applications need not call this to satisfy the reporting promise.
    /// This manual entry performs useful work but is not timing qualification.
    pub fn service(&self) -> usize {
        super::deadline_service::service_pass(self.core, self.core.activation_sample_ns)
            .completed_items
    }

    /// The transport's binding name.
    #[must_use]
    pub fn binding_name(&self) -> &'static str {
        self.core.binding
    }

    /// Whether `route` can take a frame right now. Call it *before* taking from
    /// an ingress: a native queue cannot put a frame back (design 3.4).
    ///
    /// It says only what the binding can see. On a lending transport that means a
    /// free transmit loan. On a binding whose middleware reports nothing about a
    /// receiver's queue, iceoryx2 for one, a full receiver shows only at
    /// [`send`](Self::send), where the route's overflow policy applies, so `true`
    /// here is not a promise. `false` is one: the send would be refused.
    #[must_use]
    pub fn has_capacity(&self, route: EgressRoute) -> bool {
        route.owner == self.core.id && route.route.binding.has_capacity(route.route)
    }

    /// Takes frames from a carrier ingress and hands each to `sink`, until the
    /// queue is empty, `sink` returns [`Flow::Stop`] or the route's bound is reached.
    /// Returns how many frames were handed over.
    ///
    /// Each frame is checked first: its address and peer against the route, and
    /// its time to live against the wall clock, failing closed when the clock
    /// cannot be read. A refused frame is dropped and counted, never handed over.
    ///
    /// A frame goes back to its transport after `sink` has returned, or unwound,
    /// for it: even one parked and dropped inside `sink` is returned only then,
    /// before this returns. Only a parked frame kept past `sink` stays out longer.
    ///
    /// # Errors
    /// [`ReceiveError::UnknownRoute`] if `route` belongs to another transport.
    pub fn receive(
        &self,
        route: IngressRoute,
        sink: &mut dyn FnMut(CarriedFrame<'_>) -> Flow,
    ) -> Result<usize, ReceiveError> {
        if route.owner != self.core.id {
            return Err(ReceiveError::UnknownRoute);
        }
        let handed = self.core.shared.receive_carried(route.route, sink);
        // Taking a frame is not an arrival. Binding ingress already reports the
        // authoritative arrival; this check supplements autonomous deadline service.
        route.route.common.check_deadline();
        if handed == 0 {
            if route.route.pool_lifetime() == super::LifetimeState::Retired { return Err(ReceiveError::PoolRetired); }
            if let Some(fault) = self.core.shared.receive_fault(route.route.common.binding_route) { return Err(fault); }
        }
        Ok(handed)
    }

    /// Forwards one frame on a carrier egress: identifier and payload bytes
    /// unchanged, at most one payload copy (R24). Never blocks; a parked frame's
    /// time to live is checked again here.
    ///
    /// # Errors
    /// [`SendError::Busy`], `SlotsExhausted` or `Loan(PoolExhausted)` when the
    /// egress has no capacity; `Expired` when a parked frame ran out of time. The
    /// frame is borrowed, so a capacity refusal can still [`park`](CarriedFrame::park) it.
    pub fn send(
        &self,
        route: EgressRoute,
        frame: &CarriedFrame<'_>,
    ) -> Result<SendReport, SendError> {
        if route.owner != self.core.id {
            add(&route.route.counters.refused);
            return Err(SendError::Rejected {
                code: UCode::InvalidArgument,
                detail: "this egress route belongs to another transport",
            });
        }
        super::carrier::send_carried(route.route, frame)
    }
}

/// Enters runtime mode on every transport in `setups`, in order.
///
/// Stops at the first refusal and returns it, naming that transport's route.
/// There is no way to take a transport that already started back out of runtime
/// mode (R38): a refusal here means the process exits and is restarted.
///
/// Collects into whatever container the caller names, typically a `Vec` on `std`.
///
/// # Errors
/// As [`TransportSetup::enter_runtime`], for the first transport that refuses.
pub fn enter_runtime_all<C, I>(setups: I) -> Result<C, StartupError>
where
    I: IntoIterator<Item = TransportSetup>,
    C: FromIterator<TransportRuntime>,
{
    setups
        .into_iter()
        .map(TransportSetup::enter_runtime)
        .collect()
}

struct SetupInner {
    views: GrowTable<super::RouteView>,
    tx: GrowTable<&'static TxRoute>,
    rx: GrowTable<&'static dyn InboundDispatch>,
    reports: GrowTable<RouteReport>,
    waivers: GrowTable<&'static RuntimeWaiver>,
    next_id: u32,
}

/// A table in startup memory that grows by reallocating into a larger reserved
/// block; the old block stays reserved for the life of the process.
struct GrowTable<T: 'static> {
    items: Option<FixedVec<T>>,
    capacity: usize,
}

impl<T: 'static + Clone> GrowTable<T> {
    const fn new() -> Self {
        Self {
            items: None,
            capacity: 0,
        }
    }

    fn len(&self) -> usize {
        self.items.as_ref().map_or(0, FixedVec::len)
    }

    fn slice(&self) -> &[T] {
        self.items.as_ref().map_or(&[], FixedVec::as_slice)
    }

    fn push(&mut self, memory: &StartupMemory, value: T) -> Result<(), StartupError> {
        if self.len() == self.capacity {
            let capacity = self.capacity.saturating_mul(2).max(8);
            let mut next = memory.fixed_vec::<T>(capacity)?;
            if let Some(items) = self.items.take() {
                for item in items {
                    next.push(item)?;
                }
            }
            self.capacity = capacity;
            self.items = Some(next);
        }
        match &mut self.items {
            Some(items) => items.push(value).map_err(StartupError::from),
            None => Err(StartupError::InvalidConfig("internal: no route table")),
        }
    }

    /// # Safety-usable invariant
    /// Consumes the table's mutable capability. FixedVec::into_static_slice
    /// supplies its initialized permanent shared prefix, or an empty static
    /// slice when absent. No Clone callback, element drop or reclamation occurs.
    fn into_static(self) -> &'static [T] {
        self.items.map_or(&[], FixedVec::into_static_slice)
    }
}

// Immutable policies have process lifetime, just like the transport's startup
// storage. This is a single-pointer publication, NOT a multiword atomic integer.
static DEFAULT_LIFETIME_POLICY: super::LifetimePolicy = super::LifetimePolicy::DEFAULT;

struct PublishedLifetimePolicy {
    // Safety invariant: current is nonnull, aligned for LifetimePolicy, with
    // provenance for one fully initialized immutable LifetimePolicy covering
    // size_of::<LifetimePolicy>() bytes, live for 'static. No published target
    // is mutated, moved, dropped or reclaimed. All pointer accesses are atomic.
    current: AtomicPtr<super::LifetimePolicy>,
}

impl PublishedLifetimePolicy {
    /// # Safety-usable invariant
    /// The returned value satisfies the current field invariant and initially
    /// reads LifetimePolicy::DEFAULT. No caller-provided code is invoked.
    fn new() -> Self {
        // A reference to this immutable static enforces alignment, liveness,
        // validity and provenance. cast_mut changes pointer type, not access
        // permission; the pointee is never written through the pointer.
        Self {
            current: AtomicPtr::new(core::ptr::from_ref(&DEFAULT_LIFETIME_POLICY).cast_mut()),
        }
    }

    /// # Safety-usable invariant
    /// Returns a copy of one complete published policy. No reference or owner
    /// escapes, no pointee is modified, and no publication target is reclaimed.
    fn get(&self) -> super::LifetimePolicy {
        let current = self.current.load(Ordering::Acquire);
        // SAFETY: Operation: ptr::read (core::ptr::read's contract). It requires
        // an aligned pointer valid to read size_of::<LifetimePolicy>() bytes
        // containing an initialized LifetimePolicy. The current field invariant
        // provides that range in one live allocation, alignment and provenance;
        // new/publish's Safety-usable invariants maintain it. Acquire observes
        // the Release-published initialization (or the initialized static).
        // No pointee has interior mutability, and no target is ever reclaimed,
        // including if another writer republishes between load and read. No
        // callback intervenes. LifetimePolicy is Copy with a single u64 field:
        // this typed read duplicates a value, not a destructor/deallocation
        // responsibility, and does not read padding as bytes. Postcondition:
        // the field invariant and original initialized policy remain unchanged.
        unsafe { current.read() }
    }

    /// # Safety-usable invariant
    /// Preserves the current field invariant and publishes an initialized policy
    /// with Release ordering. All previous targets remain valid indefinitely.
    /// Safe even with concurrent readers/writers; no caller safety obligation.
    fn publish(&self, policy: &'static super::LifetimePolicy) {
        // The signature enforces initialized, aligned, immutable process-lived
        // storage. LifetimePolicy contains no UnsafeCell or borrowed ownership.
        // Release publishes that initialization; no pointee mutation occurs.
        self.current.store(core::ptr::from_ref(policy).cast_mut(), Ordering::Release);
    }

}

/// Space for the runtime core, reserved at construction.
struct CoreCell(
    // Safety invariant: at most one TransportSetup owns initialization of this
    // cell. Before publication the MaybeUninit may be uninitialized and no
    // RuntimeCore reference exists. Its single initialization completes before
    // any RuntimeCore reference escapes; afterwards its fields are never
    // reassigned and live until process exit. Referenced objects retain their
    // own synchronization invariants. Identity access never reads the fields.
    UnsafeCell<MaybeUninit<RuntimeCore>>,
);
// SAFETY: Sync requires sharing &CoreCell across threads without data races.
// The field invariant permits one initializer, whose capability is held by the
// non-Clone, non-Sync TransportSetup. All other safe access is identity(), which
// reads no cell contents. enter_runtime consumes that sole capability, writes
// before publishing any RuntimeCore reference, then never writes again. The
// assertion below checks RuntimeCore: Send + Sync; its shared subobjects must
// therefore uphold their own Sync contracts. No thread-affinity premise is used.
unsafe impl Sync for CoreCell {}
// SAFETY: Send permits ownership transfer between threads. The same field
// invariant gives either uninitialized MaybeUninit storage or an initialized
// Send RuntimeCore (checked below), with no active mutable alias during a move.
// A 'static borrow prevents moving/reclaiming a placed CoreCell in safe Rust.
unsafe impl Send for CoreCell {}

// Type checks, not runtime assertions or evidence of a completed compile.
const _: fn() = || {
    fn require_send_sync<T: Send + Sync>() {}
    require_send_sync::<RuntimeCore>();
};

impl CoreCell {
    /// # Safety-usable invariant
    /// Returns the full address of this process-lived, nonzero-sized cell as a
    /// u64 on supported 32/64-bit targets. Does not read or initialize its
    /// contents, reconstruct a pointer, or extend a borrow. Distinct disjoint
    /// cells have distinct tokens; the token has no ordering/wire-ID meaning.
    fn identity(&'static self) -> u64 {
        // StartupMemory::place's Safety-usable invariant establishes that it places this nonzero-sized cell in disjoint storage
        // which is never reused, even if setup/start fails or handles are
        // dropped. Its address is therefore a process-local ownership token.
        // No pointer is reconstructed from the token. On supported 32/64-bit
        // targets this conversion preserves ALL address bits; the u64 owner
        // fields, equality checks, UUIDs and wire identities stay unchanged.
        core::ptr::from_ref(self).addr() as u64
    }
}

// CoreCell address equality needs a positive-size object on every supported
// target. This compile-time assertion introduces no runtime branch or limit.
const _: () = assert!(core::mem::size_of::<CoreCell>() != 0);

/// A transport in initialization mode. Only setup exists here.
///
/// One line constructs this from a binding, and nothing after that line names a
/// concrete transport. Setup takes `&self`, so a Streamer can add routes on two
/// entries of one list.
pub struct TransportSetup {
    // The full permanent CoreCell address, used only as an opaque equality token.
    id: u64,
    memory: StartupMemory,
    /// Seals `memory` when the last setup sharing it has entered runtime.
    _claim: SetupClaim,
    init: &'static dyn PlatformInit,
    shared: &'static TransportShared,
    binding: &'static dyn ErasedBinding,
    inner: RefCell<SetupInner>,
    // Safety invariant: this setup alone owns the one initialization capability
    // for core. The core is a distinct StartupMemory placement and is never
    // reused, including on refusal, unwind, forgetting or setup abandonment.
    core: &'static CoreCell,
    strict: Option<&'static dyn super::SetupCheck>,
    // A binding can retain resources even when preparation or SDK admission
    // fails. Individual route commits clear this; a composite operation's guard
    // restores it on failure after any of its routes has committed.
    incomplete_preparation: Cell<bool>,
    deadline_configuration: Option<super::deadline_timing::DeadlineConfiguration>,
}

/// Contains a composite setup failure after its first route has committed.
/// Individual route admission remains usable while the operation prepares its
/// other legs. Any early return restores the failure latch, even when it occurs
/// before the next binding preparation. This guard exists only during setup.
#[must_use]
#[derive(Debug)]
pub(crate) struct CompositePreparation<'a> {
    incomplete: &'a Cell<bool>,
    complete: bool,
}

impl CompositePreparation<'_> {
    pub(crate) fn complete(mut self) {
        self.complete = true;
    }
}

impl Drop for CompositePreparation<'_> {
    fn drop(&mut self) {
        if !self.complete {
            self.incomplete.set(true);
        }
    }
}

impl fmt::Debug for TransportSetup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransportSetup")
            .field("id", &self.id)
            .field("binding", &self.binding.name())
            .finish_non_exhaustive()
    }
}

/// Which family a route belongs to, for the role builders.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Family {
    Lending,
    Handoff,
}

impl TransportSetup {
    /// Supplies the exact deployment/evidence lookup inputs. This does not accept
    /// a J value or validate the platform; enter_runtime matches reviewed proof.
    pub fn with_deadline_configuration(
        mut self, configuration: super::DeadlineConfiguration,
    ) -> Self {
        self.deadline_configuration = Some(configuration);
        self
    }

    /// Test instrumentation: seed one free control in each admitted SDK pool,
    /// exclusively during initialization. Not a production API or live reset.
    #[cfg(any(test, up_rust_test_hooks))]
    pub fn __seed_pool_free_for_test(&self, index: usize, even: u64) -> Result<(), super::pool::CapacityError> {
        let inner = self.inner.try_borrow_mut().map_err(|_| super::pool::CapacityError)?;
        // SAFETY: TransportSetup retains the unique, unpublished initialization
        // capability. enter_runtime consumes it, so no RuntimePort/active role can
        // coexist with this borrow. The exclusive SetupInner borrow excludes
        // route construction/reentry; the crate-owned forwarders invoke no caller
        // callbacks. These pools have no acquisition/queue/return path before
        // activation, including no forgotten runtime lease. Initialization-only
        // controls remain private; the borrow covers the complete seeding pass.
        unsafe {
            for tx in inner.tx.slice() {
                for pool in [tx.value_slots.as_ref(), tx.scratch.as_ref()].into_iter().flatten() {
                    pool.seed_for_setup(index, even)?;
                }
            }
            for rx in inner.rx.slice() { rx.seed_for_setup(index, even)?; }
        }
        Ok(())
    }

    /// Sets a validated lifetime warning margin before declaring any route.
    /// The policy does not change the defensive terminal limit. Qualify hottest-
    /// slot rate and monitor/response latency; a suspended monitor may miss it.
    /// A changed policy reserves one immutable policy value in startup memory;
    /// repeating the current policy reserves nothing. Earlier values stay valid
    /// until process exit. Exhausted startup memory is an ordinary setup error.
    pub fn with_lifetime_policy(self, policy: super::LifetimePolicy) -> Result<Self, StartupError> {
        if self.inner.try_borrow().map_err(|_| StartupError::InvalidConfig("re-entrant setup"))?.next_id != 0 { return Err(StartupError::InvalidConfig("set lifetime policy before declaring routes")); }
        if self.shared.lifetime_policy() != policy {
            // Allocation/placement is setup-only. StartupMemory::place's
            // Safety-usable invariant guarantees a fully initialized, aligned,
            // exclusive, process-lived value. Coercion to &'static shares it
            // immutably; we retain no mutable alias. Publish only on success,
            // so failure leaves the previous complete policy intact.
            let policy: &'static super::LifetimePolicy = self.memory.place(policy)?;
            self.shared.lifetime_policy.publish(policy);
        }
        Ok(self)
    }

    /// Wraps a lending binding in startup memory on the given platform.
    ///
    /// # Errors
    /// Refuses when memory is exhausted, when it starts no more setups
    /// (`RegionSealed`: sealed, or a transport using it has entered runtime), or when
    /// the binding cannot recover a predecessor.
    pub fn lending_on<S>(
        memory: &StartupMemory,
        init: &'static dyn PlatformInit,
        mut binding: S,
    ) -> Result<Self, StartupError>
    where
        S: BindingSetup,
        S::Running: LendingRuntime,
    {
        // Starting a setup is initialization: the caller's handle refuses it once a
        // transport using this memory has entered runtime, before the binding is
        // touched. Everything this setup needs then goes through its own handle.
        let (memory, claim) = memory.share_for_setup()?;
        binding
            .recover_predecessor()
            .map_err(|BindingRefusal(reason)| StartupError::Binding {
                binding: binding.name(),
                reason,
            })?;
        let name = binding.name();
        let capabilities = binding.capabilities();
        let cell: &'static LendingCell<S> = memory.place(LendingCell {
            setup: SetupCell::new(binding),
            running: RunningCell::new(),
            name,
            capabilities,
        })?;
        Self::assemble(memory, claim, init, cell)
    }

    /// Wraps a handoff binding in startup memory on the given platform.
    ///
    /// # Errors
    /// As [`Self::lending_on`].
    pub fn handoff_on<S>(
        memory: &StartupMemory,
        init: &'static dyn PlatformInit,
        mut binding: S,
    ) -> Result<Self, StartupError>
    where
        S: BindingSetup,
        S::Running: HandoffRuntime,
    {
        // As in `lending_on`: refused before the binding is touched once this memory
        // is past initialization, then everything goes through the setup's own handle.
        let (memory, claim) = memory.share_for_setup()?;
        binding
            .recover_predecessor()
            .map_err(|BindingRefusal(reason)| StartupError::Binding {
                binding: binding.name(),
                reason,
            })?;
        let name = binding.name();
        let capabilities = binding.capabilities();
        let cell: &'static HandoffCell<S> = memory.place(HandoffCell {
            setup: SetupCell::new(binding),
            running: RunningCell::new(),
            name,
            capabilities,
        })?;
        Self::assemble(memory, claim, init, cell)
    }

    /// Wraps a lending binding in startup memory on the native platform.
    ///
    /// # Errors
    /// As [`Self::lending_on`].
    #[cfg(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "nto",
        target_os = "qnx"
    ))]
    pub fn lending_in<S>(memory: &StartupMemory, binding: S) -> Result<Self, StartupError>
    where
        S: BindingSetup,
        S::Running: LendingRuntime,
    {
        Self::lending_on(memory, &crate::platform::NATIVE, binding)
    }

    /// Wraps a handoff binding in startup memory on the native platform.
    ///
    /// # Errors
    /// As [`Self::handoff_on`].
    #[cfg(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "nto",
        target_os = "qnx"
    ))]
    pub fn handoff_in<S>(memory: &StartupMemory, binding: S) -> Result<Self, StartupError>
    where
        S: BindingSetup,
        S::Running: HandoffRuntime,
    {
        Self::handoff_on(memory, &crate::platform::NATIVE, binding)
    }

    /// Builds the setup on its own handle of the shared memory, and its claim.
    fn assemble(
        memory: StartupMemory,
        claim: SetupClaim,
        init: &'static dyn PlatformInit,
        binding: &'static dyn ErasedBinding,
    ) -> Result<Self, StartupError> {
        let default_signal: &'static mut Signal = memory.place(Signal::new())?;
        let shared: &'static mut TransportShared = memory.place(TransportShared {
            platform: init.runtime(),
            signal: AtomicPtr::new(core::ptr::from_mut(default_signal)),
            binding,
            lifetime_policy: PublishedLifetimePolicy::new(),
            deadline_signal: Signal::new(),
            deadline_cancelled: AtomicBool::new(false),
        })?;
        let core: &'static CoreCell =
            memory.place(CoreCell(UnsafeCell::new(MaybeUninit::uninit())))?;
        Ok(Self {
            id: core.identity(),
            memory,
            _claim: claim,
            init,
            shared: &*shared,
            binding,
            inner: RefCell::new(SetupInner {
                views: GrowTable::new(),
                tx: GrowTable::new(),
                rx: GrowTable::new(),
                reports: GrowTable::new(),
                waivers: GrowTable::new(),
                next_id: 0,
            }),
            core,
            strict: None,
            incomplete_preparation: Cell::new(false),
            deadline_configuration: None,
        })
    }

    /// Installs an initialization policy. A late installation still checks every
    /// admitted route at the transition. There is no runtime policy handle.
    #[must_use]
    pub fn strict(mut self, policy: &'static dyn super::SetupCheck) -> Self {
        self.strict = Some(policy);
        self
    }

    fn view<'a>(&'a self, inner: &'a SetupInner) -> super::TransportView<'a> {
        super::TransportView {
            binding: self.binding.name(),
            binding_version: self.capabilities().version,
            capabilities: self.capabilities(),
            routes: inner.views.slice(),
            waivers: inner.waivers.slice(),
            signal: self.shared.signal(),
            memory: self.memory.identity(),
            platform: self.shared.platform,
            panic_abort: cfg!(panic = "abort"),
        }
    }

    // [impl->req~ssr-strict-init-only~1]
    pub(crate) fn check_route(&self, route: super::RouteView) -> Result<(), StartupError> {
        let mut inner = self
            .inner
            .try_borrow_mut()
            .map_err(|_| StartupError::InvalidConfig("re-entrant setup"))?;
        if let Some(policy) = self.strict {
            policy.route(&self.view(&inner), &route)?;
        }
        inner.views.push(&self.memory, route)
    }

    fn check_strict(&self, inner: &SetupInner) -> Result<(), StartupError> {
        // The full locked crate exclusions land in 4S; this is the core transition.
        // [impl->req~ssr-strict-lock~1]
        #[cfg(up_rust_strict)]
        if !self.strict.is_some_and(super::SetupCheck::is_enforcing) {
            return Err(StartupError::Strict {
                route: None,
                rule: "BUILD-LOCK",
                requirement: "req~ssr-strict-lock~1",
                fix: "install an enforcing policy with TransportSetup::strict",
            });
        }
        if let Some(policy) = self.strict {
            let view = self.view(inner);
            for route in inner.views.slice() {
                policy.route(&view, route)?;
            }
            policy.transport(&view)?;
        }
        Ok(())
    }

    /// Records a waiver for a permit one of this transport's routes declares.
    ///
    /// # Errors
    /// Refuses a waiver id given twice.
    pub fn with_waiver(self, waiver: &'static RuntimeWaiver) -> Result<Self, StartupError> {
        {
            let mut inner = self
                .inner
                .try_borrow_mut()
                .map_err(|_| StartupError::InvalidConfig("re-entrant setup"))?;
            if inner
                .waivers
                .slice()
                .iter()
                .any(|known| known.id == waiver.id)
            {
                return Err(StartupError::WaiverRefused {
                    waiver: waiver.id,
                    problem: "the same waiver was given twice",
                });
            }
            inner.waivers.push(&self.memory, waiver)?;
        }
        Ok(self)
    }

    /// Uses `signal` instead of a per-transport one, so one wake serves several
    /// transports.
    pub fn signal_on(&self, signal: &'static Signal) {
        // A typed static shared reference maintains signal's field invariant;
        // Release publishes initialization and no old target is reclaimed.
        self.shared
            .signal
            .store(core::ptr::from_ref(signal).cast_mut(), Ordering::Release);
    }

    /// # Safety-usable invariant
    /// The permanent CoreCell equality token for this setup and its eventual
    /// runtime. It is not a sequence number, wire ID, or reconstructible pointer.
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn binding_name(&self) -> &'static str {
        self.binding.name()
    }

    pub(crate) fn memory(&self) -> &StartupMemory {
        &self.memory
    }

    pub(crate) fn shared(&self) -> &'static TransportShared {
        self.shared
    }

    pub(crate) fn binding(&self) -> &'static dyn ErasedBinding {
        self.binding
    }

    pub(crate) fn family(&self) -> Family {
        if self.binding.is_lending() {
            Family::Lending
        } else {
            Family::Handoff
        }
    }

    pub(crate) fn capabilities(&self) -> Capabilities {
        self.binding.capabilities()
    }

    pub(crate) fn qos_support(&self, kind: RouteKind) -> QosSupport {
        self.binding.qos_support(kind)
    }

    pub(crate) fn platform_init(&self) -> &'static dyn PlatformInit {
        self.init
    }

    pub(crate) fn platform(&self) -> &'static dyn Platform {
        self.shared.platform
    }

    pub(crate) fn prepare_route(
        &self,
        declaration: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, StartupError> {
        self.check_preparation_complete()?;
        self.incomplete_preparation.set(true);
        self.binding
            .prepare_route(declaration)
            .map_err(|BindingRefusal(reason)| StartupError::Binding {
                binding: self.binding.name(),
                reason,
            })
    }

    /// Call immediately after the first route commits, before any further
    /// fallible work. Complete the guard only when the whole role is ready.
    pub(crate) fn guard_composite_preparation(&self) -> CompositePreparation<'_> {
        CompositePreparation {
            incomplete: &self.incomplete_preparation,
            complete: false,
        }
    }

    fn check_preparation_complete(&self) -> Result<(), StartupError> {
        if self.incomplete_preparation.get() {
            return Err(StartupError::InvalidConfig(
                "incomplete route admission: discard and recreate this transport setup",
            ));
        }
        Ok(())
    }

    pub(crate) fn next_route_id(&self) -> Result<RouteId, StartupError> {
        let mut inner = self
            .inner
            .try_borrow_mut()
            .map_err(|_| StartupError::InvalidConfig("re-entrant setup"))?;
        let id = RouteId(inner.next_id);
        inner.next_id = inner
            .next_id
            .checked_add(1)
            .ok_or(StartupError::InvalidConfig("too many routes"))?;
        Ok(id)
    }

    pub(crate) fn next_inbound(&self) -> Result<InboundRoute, StartupError> {
        let inner = self
            .inner
            .try_borrow()
            .map_err(|_| StartupError::InvalidConfig("re-entrant setup"))?;
        u32::try_from(inner.rx.len())
            .map(InboundRoute)
            .map_err(|_| StartupError::InvalidConfig("too many routes"))
    }

    pub(crate) fn add_tx(&self, route: &'static TxRoute) -> Result<(), StartupError> {
        let mut inner = self
            .inner
            .try_borrow_mut()
            .map_err(|_| StartupError::InvalidConfig("re-entrant setup"))?;
        let report = route.report();
        inner.reports.push(&self.memory, report)?;
        inner.tx.push(&self.memory, route)?;
        self.incomplete_preparation.set(false);
        Ok(())
    }

    pub(crate) fn add_rx(&self, route: &'static dyn InboundDispatch) -> Result<(), StartupError> {
        let mut inner = self
            .inner
            .try_borrow_mut()
            .map_err(|_| StartupError::InvalidConfig("re-entrant setup"))?;
        let report = route.report();
        inner.reports.push(&self.memory, report)?;
        inner.rx.push(&self.memory, route)?;
        self.incomplete_preparation.set(false);
        Ok(())
    }

    /// Sets a default wire for roles built from the returned value.
    ///
    /// Desugars to the role-level primitive: each role still takes `.wire(w)`,
    /// which overrides this default, and an individual message may override both.
    #[must_use]
    pub fn with_wire<W>(&self, wire: W) -> Wired<'_, W> {
        Wired { setup: self, wire }
    }

    fn check_waivers(&self, inner: &SetupInner) -> Result<(), StartupError> {
        for waiver in inner.waivers.slice() {
            waiver
                .check()
                .map_err(|problem| StartupError::WaiverRefused {
                    waiver: waiver.id,
                    problem,
                })?;
        }
        // Every route's permit must be named by a waiver, and every waiver must
        // name a permit some route actually declares (R4, R41).
        let reports = inner.reports.slice();
        for report in reports {
            if let RuntimeHeap::Middleware(permit) = report.heap {
                if !inner.waivers.slice().iter().any(|w| w.permit == permit.id) {
                    return Err(StartupError::MiddlewareHeapNeedsWaiver {
                        route: report.route.clone(),
                        permit: permit.id,
                    });
                }
            }
            // Serialized transport paths may declare allocation. Any native
            // member keeps the whole route subject to the native heap rule.
            // Named middleware waivers above remain independently enforced.
            if report.native_payload {
                if let RuntimeHeap::Allocates(reason) = report.heap {
                    return Err(StartupError::RouteNeedsHeapAtRuntime {
                        route: report.route.clone(),
                        wire: reason,
                    });
                }
            }
        }
        for waiver in inner.waivers.slice() {
            let used = reports.iter().any(|report| {
                matches!(report.heap, RuntimeHeap::Middleware(permit) if permit.id == waiver.permit)
            });
            if !used {
                return Err(StartupError::WaiverRefused {
                    waiver: waiver.id,
                    problem: "no route declares this permit",
                });
            }
        }
        Ok(())
    }

    /// Enters runtime mode. Consumes this value.
    ///
    /// After this, the caller's handle of this transport's [`StartupMemory`] places
    /// nothing and starts no setup. Once the last setup still sharing that memory has
    /// entered runtime, it is sealed for every handle. A refused placement or setup
    /// asks for no memory.
    ///
    /// # Errors
    /// Names the first route whose wire needs the heap, a route whose
    /// middleware needs a waiver, or a waiver that is incomplete or unused.
    /// A binding that fails to start is reported; startup failure is a restart.
    /// If binding preparation began but route admission did not complete, discard
    /// and recreate the setup; the binding has no rollback contract.
    pub fn enter_runtime(self) -> Result<TransportRuntime, StartupError> {
        self.check_preparation_complete()?;
        let binding_name = self.binding.name();
        let binding_version = self.capabilities().version;
        let deadline_configuration = self.deadline_configuration;
        let id = self.id;
        let shared = self.shared;
        let binding = self.binding;
        let core_cell = self.core;
        let inner = self
            .inner
            .try_borrow()
            .map_err(|_| StartupError::InvalidConfig("re-entrant setup"))?;
        self.check_strict(&inner)?;
        self.check_waivers(&inner)?;
        for route in inner.tx.slice() {
            route.startup_check()?;
        }
        for route in inner.rx.slice() {
            route.startup_check()?;
        }
        drop(inner);
        // UUIDv7 and TTL are defined in wall time: confirm the clock once.
        if shared.platform.unix_time_ms().is_none() {
            return Err(StartupError::InvalidConfig(
                "the wall clock cannot be read during initialization",
            ));
        }
        let inner = self.inner.into_inner();
        let SetupInner {
            tx,
            rx,
            reports,
            waivers,
            ..
        } = inner;
        let tx = tx.into_static();
        let rx = rx.into_static();
        let waivers = waivers.into_static();
        let original_reports = reports.into_static();
        // Reserve the final reports before the first arm as part of setup resources.
        let mut final_reports = self.memory.fixed_vec::<RouteReport>(original_reports.len())?;
        let prepared = super::deadline_service::PreparedTiming::prepare(
            &self.memory, self.init, rx, deadline_configuration, binding_name, binding_version,
            shared, tx, original_reports)?;
        let (mut worker, activation_sample_ns) = prepared.arm(shared.platform)?;
        // Complete reporting from actual armed routes; preserve declaration order.
        for original in original_reports {
            let mut report = *original;
            for route in rx {
                if route.id() == report.id { report.deadline_timing = route.report().deadline_timing; }
            }
            final_reports.push(report)?;
        }
        let reports = final_reports.into_static_slice();
        let core: &'static RuntimeCore = {
            // SAFETY: UnsafeCell::get yields the reserved aligned MaybeUninit
            // location. Consuming the sole TransportSetup gives this one writer;
            // CoreCell is private and no pointer/reference to its RuntimeCore has
            // been published. The prepared worker cannot access this core before
            // activation publishes it, and no RuntimePort exists yet. Producing
            // &mut MaybeUninit does not read the uninitialized RuntimeCore; write
            // initializes it, then the permanent shared reference ends exclusive
            // access. No future write/reset/destruction is exposed. All borrowed
            // fields are initialized static storage, including every armed monitor.
            let slot = unsafe { &mut *core_cell.0.get() };
            &*slot.write(RuntimeCore {
                activation_sample_ns,
                id,
                binding: binding_name,
                shared,
                tx,
                rx,
                waivers,
                reports,
            })
        };
        // The no-waiter service is released BEFORE the binding can retain a port
        // or invoke a callback. Arm-to-first-dispatch/report assembly is in J.
        if let Some(worker) = worker.as_mut() {
            if let Err(gap) = worker.activate(super::DeadlineServiceEntry::new(core)) {
                shared.cancel_deadline_service();
                return Err(super::deadline_timing::startup_error(gap));
            }
        }
        let port = RuntimePort { core };
        if let Err(BindingRefusal(reason)) = binding.start(port) {
            shared.cancel_deadline_service();
            return Err(StartupError::Binding { binding: binding_name, reason });
        }
        // Commit is an irreversible ownership transition, with no OS work.
        // Binding errors/unwind drop the local preparation and park an activated
        // worker permanently; unactivated failures join only in initialization.
        if let Some(worker) = worker.as_mut() { worker.commit(); }
        self.memory.note_runtime_started();
        Ok(TransportRuntime { core })
    }
}

/// A source for a role: a transport setup, or one with a wire default attached.
pub trait TransportSource {
    /// The wire the role starts with; [`NoWire`] if none was attached.
    type DefaultWire;
    /// The transport setup the role is built on.
    fn setup(&self) -> &TransportSetup;
    /// The default wire value.
    fn default_wire(&self) -> Self::DefaultWire;
}

/// The default wire of a transport with none attached; a role must choose one.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoWire;

impl TransportSource for TransportSetup {
    type DefaultWire = NoWire;
    fn setup(&self) -> &TransportSetup {
        self
    }
    fn default_wire(&self) -> NoWire {
        NoWire
    }
}

/// A transport with a wire attached as the default for roles built from it.
#[derive(Debug)]
pub struct Wired<'a, W> {
    setup: &'a TransportSetup,
    wire: W,
}

impl<W: Clone> TransportSource for Wired<'_, W> {
    type DefaultWire = W;
    fn setup(&self) -> &TransportSetup {
        self.setup
    }
    fn default_wire(&self) -> W {
        self.wire.clone()
    }
}

/// Which direction a binding is asked about, for builders.
#[must_use]
pub(crate) fn route_kind(direction: RouteDirection, message: Option<UMessageType>) -> RouteKind {
    RouteKind { direction, message }
}

/// A fresh monotonic deadline for the given relative timeout.
pub(crate) fn deadline_after(platform: &dyn Platform, ttl: Duration) -> Monotonic {
    let nanos = u64::try_from(ttl.as_nanos()).unwrap_or(u64::MAX);
    platform.now().saturating_add(nanos)
}

/// Builds the route name a refusal will carry.
pub(crate) fn route_name(
    direction: RouteDirection,
    kind: Option<UMessageType>,
    source: &UUri,
    sink: Option<&UUri>,
) -> RouteName {
    RouteName {
        direction,
        kind,
        source: source.clone(),
        sink: sink.cloned(),
    }
}

#[cfg(test)]
#[path = "refusal_tests.rs"]
mod refusal_tests;

#[cfg(test)]
#[path = "identity_publication_tests.rs"]
mod identity_publication_tests;

#[cfg(test)]
#[path = "execution_package_tests.rs"]
mod execution_package_tests;

#[cfg(test)]
#[path = "custom_envelope_tests.rs"]
mod custom_envelope_tests;
