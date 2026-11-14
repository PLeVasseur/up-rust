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

//! What a transport binding implements: one setup trait, one runtime trait per family.
//!
//! A lending binding implements [`BindingSetup`] and [`LendingRuntime`]: it lends
//! transmit storage, publishes it and hands back received frames. A handoff
//! binding implements [`BindingSetup`] and [`HandoffRuntime`]: it receives a
//! finished frame and copies only what it must. The SDK owns the mode state
//! machine, startup slots, wire selection, envelopes, RPC call slots and receive
//! dispatch (R5).
//!
//! Rules a binding must keep, all checked by `assert_binding_contract`:
//!
//! * **Lazy state.** First lock at binding construction or in `prepare_route`.
//!   `into_running` may allocate, since it is initialization. Every lazy primitive
//!   must be forced, and every thread it starts must have reached its idle loop,
//!   before it returns.
//! * **Peers.** A peer id is stable across restarts. Incarnation starts at 1 and
//!   only rises.
//! * **`Frame` drop** neither frees heap nor blocks.
//! * **No panic** on any peer input.
//! * **Truncation.** [`Capabilities::truncate_payload`] says whether the binding
//!   can carry a payload shorter than its loan. If it can, `used..loaned` is
//!   initialized before publish or only `used` bytes are published.
//! * **Carrier ingress.** A route with no message kind is a carrier ingress. It
//!   is pulled: [`LendingRuntime::receive`] or [`HandoffRuntime::receive`], or
//!   pushed retained with [`RuntimePort::deliver_frame`]. A borrowed
//!   [`RuntimePort::deliver`] is refused on it: those bytes live only for that
//!   call, and a carrier may not stage a copy (R24).
//! * **`has_capacity`** answers for what the binding can see, and its default of
//!   `true` means it cannot tell. A `true` is not a promise: the route's overflow
//!   policy still applies at the send.

use core::fmt;

use super::envelope::Envelope;
use super::error::{ReceiveError, RouteDirection};
use super::mode::RuntimeCore;
use super::qos::{Overflow, Qos, QosSupport, SendReport};
use super::{Frame, InitializedTx, LoanError, PreparedTxStorage, ReceiveBudget, SendError, TxFrameLayout};
use crate::frame::header::RxHeader;
use crate::uattributes::{PayloadEncoding, UMessageType};
use crate::ucode::UCode;
use crate::uri::UUri;
use crate::wire::{RuntimeHeap, WireIdentity};

/// A binding's index for one prepared route, returned by `prepare_route`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct BindingRoute(pub u32);

/// The index a binding passes to [`RuntimePort::deliver`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct InboundRoute(pub(crate) u32);

impl InboundRoute {
    /// The numeric index.
    #[must_use]
    pub fn value(self) -> u32 {
        self.0
    }
}

/// A binding-issued correlation value carried from a request to its response.
///
/// A transport whose middleware correlates responses, such as a SOME/IP binding
/// with its session and client IDs, returns this value on the response dispatch.
/// Zero when the binding does not use one.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ReplyToken(pub u64);

/// The identity of a remote participant, supplied by the binding on each frame.
///
/// `peer` zero means the binding has no notion of peer identity, and stale-peer
/// checks are skipped. A restarted peer must present a higher `incarnation`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct PeerRef {
    /// Binding-scoped peer identifier. Stable across restarts. Zero means unknown.
    pub peer: u64,
    /// Monotonic incarnation: at least 1, and it only rises. A binding may
    /// start it anywhere; the SDK only compares it.
    pub incarnation: u64,
}

impl PeerRef {
    /// No peer identity known: deadlines only.
    pub const UNKNOWN: PeerRef = PeerRef {
        peer: 0,
        incarnation: 0,
    };
}

/// What a sink tells a receive loop to do next.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Flow {
    /// Hand over the next frame.
    Continue,
    /// Stop now. Frames not yet taken stay with the transport.
    Stop,
}

/// Payload bytes to send or carry: absent, or one or more borrowed chunks.
///
/// The chunks are borrowed from the caller (an ingress frame, typically), so
/// forwarding never gathers them into a staged buffer.
#[derive(Clone, Copy, Debug, Default)]
pub enum PayloadBytes<'a> {
    /// No payload. Distinct from a present empty payload.
    #[default]
    Absent,
    /// The first chunk and any further chunks, in order.
    Present {
        /// The first chunk.
        first: &'a [u8],
        /// Remaining chunks.
        rest: &'a [&'a [u8]],
    },
}

impl<'a> PayloadBytes<'a> {
    /// One contiguous payload.
    #[must_use]
    pub fn one(bytes: &'a [u8]) -> Self {
        Self::Present {
            first: bytes,
            rest: &[],
        }
    }

    /// A chunked payload.
    #[must_use]
    pub fn chunked(chunks: &'a [&'a [u8]]) -> Self {
        match chunks.split_first() {
            Some((first, rest)) => Self::Present { first, rest },
            None => Self::Present {
                first: &[],
                rest: &[],
            },
        }
    }

    /// Whether a payload is present, including a present empty payload.
    #[must_use]
    pub fn is_present(&self) -> bool {
        matches!(self, Self::Present { .. })
    }

    /// Total bytes across all chunks.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Absent => 0,
            Self::Present { first, rest } => rest.iter().fold(first.len(), |total, chunk| {
                total.saturating_add(chunk.len())
            }),
        }
    }

    /// Whether the payload is absent or zero bytes long.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The chunks, in order.
    pub fn chunks(&self) -> impl Iterator<Item = &'a [u8]> + 'a {
        let (first, rest): (Option<&'a [u8]>, &'a [&'a [u8]]) = match *self {
            Self::Absent => (None, &[]),
            Self::Present { first, rest } => (Some(first), rest),
        };
        first.into_iter().chain(rest.iter().copied())
    }

    /// The bytes as one slice when there is exactly one chunk.
    #[must_use]
    pub fn as_contiguous(&self) -> Option<&'a [u8]> {
        match self {
            Self::Present { first, rest: [] } => Some(first),
            Self::Absent | Self::Present { .. } => None,
        }
    }
}

/// One frame handed to the transport. The transport copies only what it must.
///
/// Addressing is passed as borrowed URIs, so a byte-only binding need not decode
/// metadata. `metadata` and `payload` belong to the SDK for the duration of the call.
#[derive(Clone, Copy, Debug)]
pub struct OutgoingFrame<'a> {
    /// Message kind.
    pub kind: UMessageType,
    /// Source address.
    pub source: &'a UUri,
    /// Sink address.
    pub sink: Option<&'a UUri>,
    /// Encoded metadata, or the head of a whole-frame envelope.
    pub metadata: &'a [u8],
    /// Payload bytes, or the tail of a whole-frame envelope.
    pub payload: PayloadBytes<'a>,
    /// Correlation returned by the binding for a response.
    pub reply: ReplyToken,
}

/// Why a handoff transport refused a frame. Fixed size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SendRefusal {
    /// The native pool incarnation permanently retired. Not capacity or discard.
    PoolRetired,
    /// The transport has no capacity now and will call [`RuntimePort::ready`].
    Busy,
    /// The frame is larger than the transport carries.
    TooLarge,
    /// The transport failed the frame.
    Failed {
        /// Status to report.
        code: UCode,
        /// Named reason.
        detail: &'static str,
    },
}

/// A handoff refusal together with any known effects before the refusal.
/// The prefix is not rollback: already accepted frames remain accepted.
/// No report means the binding supplies no counts; it does not mean zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HandoffFailure {
    /// Original fixed-size refusal, unchanged by reporting.
    pub refusal: SendRefusal,
    /// Available per-send effects, including exact zero when established.
    pub report: Option<SendReport>,
}
impl HandoffFailure {
    /// Refusal before attempting any receiver: exactly zero observed effects.
    pub const fn before_send(refusal: SendRefusal) -> Self {
        Self { refusal, report: Some(SendReport::exact(0, 0)) }
    }
    /// Refusal with no available observation of receiver effects.
    pub const fn unknown(refusal: SendRefusal) -> Self { Self { refusal, report: None } }
    /// Refusal after the binding recorded the supplied prefix.
    pub const fn after(refusal: SendRefusal, report: SendReport) -> Self {
        Self { refusal, report: Some(report) }
    }
}
impl From<SendRefusal> for HandoffFailure {
    fn from(refusal: SendRefusal) -> Self { Self::unknown(refusal) }
}

/// A binding's fixed-size setup refusal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BindingRefusal(pub &'static str);

/// Which kind of route a QoS question is about, for [`BindingSetup::qos_support`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteKind {
    /// Whether the route sends or receives.
    pub direction: RouteDirection,
    /// The message kind it carries, or `None` for a carrier route.
    pub message: Option<UMessageType>,
}

/// The route an SDK role asks a binding to prepare.
#[derive(Clone, Copy, Debug)]
pub struct RouteDeclaration<'a> {
    /// Validated setup margin for binding-owned pools; never an early close threshold.
    pub lifetime_policy: super::LifetimePolicy,
    /// Whether the route sends or receives.
    pub direction: RouteDirection,
    /// Message kind, or `None` for a carrier route that forwards every kind.
    pub kind: Option<UMessageType>,
    /// Exact source when sending; a source filter when receiving.
    pub source: &'a UUri,
    /// Sink for sends; sink filter for receives.
    pub sink: Option<&'a UUri>,
    /// The inbound route to deliver on, for receive routes.
    pub inbound: Option<InboundRoute>,
    /// The transport's envelope for this route.
    pub envelope: Envelope,
    /// The closed set of payload encodings this route carries.
    pub encodings: &'a [PayloadEncoding],
    /// The closed set of payload families this route carries.
    pub families: &'a [WireIdentity],
    /// Largest head (everything before the payload) the route will produce.
    pub max_metadata_len: usize,
    /// Largest payload the route will carry.
    pub max_payload_len: usize,
    /// The payload's required alignment.
    pub payload_alignment: usize,
    /// Reserved startup slots for this route.
    pub slots: usize,
    /// Per-route QoS, already checked against [`BindingSetup::qos_support`].
    pub qos: Qos,
    /// Whether the payload is `T` in native representation.
    pub native: bool,
    /// Peers this route keeps apart: the size of the SDK's peer table for it, at
    /// least the deployment's declared clients. Zero on a send route.
    ///
    /// A binding that cannot serve this many peers on this route refuses it in
    /// `prepare_route`, so a configuration the transport cannot honor fails at
    /// setup, naming the binding, and not by dropping the extra peers later (R29).
    pub max_peers: usize,
}

/// What a route needs from its binding, returned by [`BindingSetup::prepare_route`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct PreparedRoute {
    /// The binding's handle for this route.
    pub route: BindingRoute,
    /// The route's heap class. `Middleware` needs a waiver; `Allocates` is refused.
    pub heap: RuntimeHeap,
    /// Deviations that actually apply to this route. Empty unless declared by the binding.
    pub deviations: &'static [&'static super::Deviation],
}

impl PreparedRoute {
    /// A prepared binding route with its explicit heap declaration.
    #[must_use]
    pub const fn new(route: BindingRoute, heap: RuntimeHeap) -> Self {
        Self {
            route,
            heap,
            deviations: &[],
        }
    }

    /// Records the binding's deviations for this particular route.
    #[must_use]
    pub const fn with_deviations(
        mut self,
        deviations: &'static [&'static super::Deviation],
    ) -> Self {
        self.deviations = deviations;
        self
    }
}

/// What a transport can carry, declared during initialization.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Capabilities {
    /// Binding implementation version; None explicitly means not declared.
    pub version: Option<&'static str>,
    /// The envelope for routes that do not choose one.
    pub default_envelope: Envelope,
    /// The envelopes this transport carries.
    pub envelopes: &'static [Envelope],
    /// Whether each frame may carry its own payload encoding.
    ///
    /// When false (for instance SOME/IP, which has no encoding field), a route
    /// must declare exactly one encoding and refuses a per-message choice.
    pub per_message_encoding: bool,
    /// Largest head plus payload per frame.
    pub max_frame_len: usize,
    /// Whether this binding can carry a payload shorter than its loan.
    pub truncate_payload: bool,
    /// Whether RPC servers on this binding must declare their clients at setup.
    pub declared_clients: bool,
    /// Whether the binding may deliver chunked frames to typed routes.
    pub chunked_receive: bool,
    /// Whether the binding offers native (in-place) loans.
    pub native_loans: bool,
}

impl Capabilities {
    /// Names the binding implementation version for initialization policy reports.
    #[must_use]
    pub const fn with_version(mut self, version: &'static str) -> Self {
        self.version = Some(version);
        self
    }

    /// Declares the supported envelopes and frame bound. Optional capabilities
    /// start disabled and are enabled explicitly by the binding.
    #[must_use]
    pub const fn new(
        default_envelope: Envelope,
        envelopes: &'static [Envelope],
        max_frame_len: usize,
    ) -> Self {
        Self {
            version: None,
            default_envelope,
            envelopes,
            max_frame_len,
            per_message_encoding: false,
            truncate_payload: false,
            declared_clients: false,
            chunked_receive: false,
            native_loans: false,
        }
    }
    /// Sets [`Self::per_message_encoding`].
    #[must_use]
    pub const fn with_per_message_encoding(mut self, value: bool) -> Self {
        self.per_message_encoding = value;
        self
    }
    /// Sets [`Self::truncate_payload`].
    #[must_use]
    pub const fn with_truncate_payload(mut self, value: bool) -> Self {
        self.truncate_payload = value;
        self
    }
    /// Sets [`Self::declared_clients`].
    #[must_use]
    pub const fn with_declared_clients(mut self, value: bool) -> Self {
        self.declared_clients = value;
        self
    }
    /// Sets [`Self::chunked_receive`].
    #[must_use]
    pub const fn with_chunked_receive(mut self, value: bool) -> Self {
        self.chunked_receive = value;
        self
    }
    /// Sets [`Self::native_loans`].
    #[must_use]
    pub const fn with_native_loans(mut self, value: bool) -> Self {
        self.native_loans = value;
        self
    }
}

/// Initialization side of a binding.
pub trait BindingSetup: Send + 'static {
    /// The binding's runtime form, a [`LendingRuntime`] or a [`HandoffRuntime`].
    type Running: Send + Sync + 'static;

    /// Name, for refusals and reports.
    fn name(&self) -> &'static str;

    /// What this transport carries.
    fn capabilities(&self) -> Capabilities;

    /// What this binding can honor for one kind of route.
    fn qos_support(&self, kind: RouteKind) -> QosSupport;

    /// Cleans up after a crashed predecessor (R38). Runs in the constructor,
    /// before any route.
    ///
    /// # Errors
    /// Reports a predecessor state that cannot be recovered.
    fn recover_predecessor(&mut self) -> Result<(), BindingRefusal> {
        Ok(())
    }

    /// Reserves every binding resource for one route.
    ///
    /// # Errors
    /// Refuses a route the transport cannot carry.
    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal>;

    /// Consumes setup. The binding keeps `port` to deliver frames and signal readiness.
    ///
    /// May allocate: this is initialization. Force anything lazy here.
    ///
    /// # Errors
    /// Reports a binding that cannot enter runtime.
    fn into_running(self, port: RuntimePort) -> Result<Self::Running, BindingRefusal>;
}

/// Runtime side of a lending binding.
///
/// No method may allocate, panic on capacity or block without bound. Each runs
/// only after `into_running` returned.
pub trait LendingRuntime: Send + Sync + 'static {
    /// Latched receive fault, sampled before and after an SDK pull. Never clear
    /// PoolRetired or encode it as an empty queue. No allocation or callback.
    fn receive_fault(&self, route: BindingRoute) -> Option<ReceiveError> { let _ = route; None }
    /// Lifetime of this binding route's native pools. A point-in-time observation;
    /// a monitor must run within the deployment's qualified response budget.
    fn pool_lifetime(&self, route: BindingRoute) -> super::LifetimeState { let _ = route; super::LifetimeState::Healthy }

    /// Exclusive transmit loan, allowed to borrow this runtime.
    type Tx<'a>: PreparedTxStorage
    where
        Self: 'a;
    /// Receive storage owner, retained independently of any port.
    type Rx: Frame;

    /// Borrows reserved storage for one message on a prepared route.
    ///
    /// # Errors
    /// Reports route mismatch, exhaustion or an unsupported layout.
    fn loan(&self, route: BindingRoute, layout: TxFrameLayout) -> Result<Self::Tx<'_>, LoanError>;

    /// Publishes an initialized loan under the route's QoS.
    ///
    /// # Errors
    /// Reports delivery failure without constructing an owned error string.
    fn publish<'a>(
        &'a self,
        route: BindingRoute,
        frame: InitializedTx<Self::Tx<'a>>,
    ) -> Result<SendReport, SendError>;

    /// Pulls one received frame; `None` for a push binding, which delivers
    /// through [`RuntimePort::deliver_frame`] instead.
    ///
    /// A pull binding calls [`RuntimePort::readable`] when a frame becomes
    /// available, so a task or thread waiting on the route is woken to pull it.
    ///
    /// A pull binding's queue is the route's queue: it holds at most the route's
    /// `depth` and applies its overflow policy. Where it can see a discard, it
    /// reports it with [`RuntimePort::dropped`], so the route's counters show it.
    /// The SDK pulls only when it has room (so a `Wait` frame stays here) and at
    /// most the route's bound per call. `run_qos_contract` in `test-util` checks
    /// the queue through a plain `try_recv` reader, and the counts through the
    /// route's counters unless the binding declares its middleware discards out of
    /// its sight (`route_counts_drops`).
    fn receive(&self, route: BindingRoute) -> Option<Self::Rx>;

    /// SDK pull entry point, sharing the caller's raw-attempt allowance.
    ///
    /// Required explicitly: a forwarding wrapper must forward this SAME budget.
    /// Take one unit before every raw receive attempt, including internal
    /// discards and empty/error attempts. A single-attempt implementation may
    /// explicitly debit once and call `receive`; a filtering loop must debit
    /// inside that loop. Push-only implementations can return None directly.
    ///
    /// Preserve native empty/error/credit stops and ownership. Do not charge SDK
    /// validation again. None at budget exhaustion is not proof of an empty
    /// queue; the SDK arranges another wake. No caller-created/reset budget.
    fn receive_bounded(&self, route: BindingRoute, budget: &ReceiveBudget) -> Option<Self::Rx>;

    /// Whether a send on `route` would find transmit capacity right now.
    ///
    /// Honest about what the binding can see. The default is `true`, meaning the
    /// binding cannot tell: a middleware that says nothing about a full receiver
    /// (iceoryx2 is one) answers from the loan pool alone, and the route's
    /// overflow policy applies at the send, where the fullness finally shows.
    fn has_capacity(&self, route: BindingRoute) -> bool {
        let _ = route;
        true
    }
}

/// Runtime side of a handoff binding.
///
/// No method may allocate, panic on capacity or block without bound.
pub trait HandoffRuntime: Send + Sync + 'static {
    /// Latched receive fault, sampled before and after an SDK pull. Never clear
    /// PoolRetired or encode it as an empty queue. No allocation or callback.
    fn receive_fault(&self, route: BindingRoute) -> Option<ReceiveError> { let _ = route; None }
    /// Lifetime of this binding route's native pools. A point-in-time observation;
    /// a monitor must run within the deployment's qualified response budget.
    fn pool_lifetime(&self, route: BindingRoute) -> super::LifetimeState { let _ = route; super::LifetimeState::Healthy }

    /// Hands one frame to the transport, copying only what it must.
    ///
    /// # Errors
    /// [`SendRefusal::Busy`] means retry after [`RuntimePort::ready`].
    /// Reports available acceptance/discard counts. Use `SendReport::unknown()`
    /// when the middleware only reports send acceptance. Never invent a receiver.
    /// On refusal preserve any known prefix in `HandoffFailure`; do not retry it.
    fn send(&self, route: BindingRoute, frame: OutgoingFrame<'_>) -> Result<SendReport, HandoffFailure>;

    /// Pull form of receive, for a binding that holds received frames itself.
    ///
    /// Each frame passed to `sink` is delivered to the route; when `sink` returns
    /// [`Flow::Stop`] the binding stops and keeps the rest. Returns how many.
    /// The default delivers none; a push binding uses [`RuntimePort::deliver`].
    /// A binding that holds frames calls [`RuntimePort::readable`] when one arrives.
    ///
    /// A frame's bytes must stay valid and unchanged until `sink` returns for it,
    /// even if `sink` calls back into the SDK: the SDK borrows them for that call.
    ///
    /// The SDK asks for at most a route's bound of frames per call, so a peer
    /// cannot hold the caller, and stops (returns [`Flow::Stop`]) before a frame
    /// it has no room for, so a `Wait` frame is never taken only to be dropped.
    /// The rest stay here. So the binding's own queue is the route's queue, as a
    /// lending binding's is: it holds at most the route's `depth` and applies its
    /// overflow policy (R44), and reports each discard it makes with
    /// [`RuntimePort::dropped`]. `run_qos_contract` in `test-util` checks the queue
    /// through a plain `try_recv` reader, and the counts through the route's
    /// counters.
    fn receive(
        &self,
        route: BindingRoute,
        sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow,
    ) -> usize {
        let _ = (route, sink);
        0
    }

    /// SDK pull entry point with one allowance across rejection layers.
    ///
    /// Required explicitly. Filtering paths take budget before EVERY raw
    /// attempt, even when no sink callback follows. Stop on exhaustion or
    /// Flow::Stop. A wrapper forwards this SAME budget to its inner binding.
    /// Push-only bindings may return zero without examining a native frame.
    ///
    /// A deliberately selected sink-visible adapter can prepay an attempt and
    /// prepay each next attempt only after a Continue callback; it is valid only
    /// when every examined raw frame reaches that callback. No provided default
    /// can silently assert this property for an existing implementation.
    fn receive_bounded(
        &self,
        route: BindingRoute,
        budget: &ReceiveBudget,
        sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow,
    ) -> usize;

    /// Whether a send on `route` would be accepted right now, as far as the
    /// binding can tell. The default is `true`: the binding cannot tell, and
    /// [`SendRefusal::Busy`] at the send is the answer.
    fn has_capacity(&self, route: BindingRoute) -> bool {
        let _ = route;
        true
    }
}

/// One received frame, borrowed for the duration of the delivery call.
#[derive(Clone, Copy, Debug, Default)]
pub struct InboundFrame<'a> {
    /// Encoded metadata; empty for a whole-frame envelope.
    pub metadata: &'a [u8],
    /// Payload bytes, or a whole frame in the first chunk.
    pub payload: PayloadBytes<'a>,
    /// Who sent it, for stale-peer rejection.
    pub peer: PeerRef,
    /// Returned unchanged on a response's send.
    pub reply: ReplyToken,
}

/// Handed to a binding when it enters runtime. Delivers frames and signals readiness.
///
/// Every method is heap-free and never blocks. It works during `into_running`,
/// and `deliver` never reads the running-value cell. A binding keeps this value
/// for its whole life; dropping it does nothing.
#[derive(Clone, Copy)]
pub struct RuntimePort {
    pub(crate) core: &'static RuntimeCore,
}

impl fmt::Debug for RuntimePort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimePort").finish_non_exhaustive()
    }
}

impl RuntimePort {
    /// Samples this transport's actual platform clock for a queue-ingress report.
    /// Call at the binding's real arrival boundary; taking an old queued sample
    /// and sampling now cannot reconstruct its original arrival time.
    pub fn arrival_now(&self) -> crate::platform::Monotonic {
        self.core.shared.platform.now()
    }

    /// D7: report one actual arrival at a binding-owned receive queue, including
    /// arrivals discarded by overflow and frames later refused by SDK admission.
    /// Sample the SAME platform clock at the actual evidence boundary. Wake
    /// hints, old timestamps from a deep-queue take, and duplicated push reports
    /// are not valid evidence. The accepted arrival proof bounds reporting delay
    /// as L; J never excuses an arrival report that would cause a false miss.
    /// `deliver`/`deliver_frame` already report direct push arrivals. Pull delivery
    /// does not report again. This retains native storage; it adds no unread queue.
    /// Bad safe binding reports can break timing/metrics, never Rust memory safety.
    // [impl->req~ssr-qos-deadline~3]
    pub fn arrived(&self, route: InboundRoute, sampled: crate::platform::Monotonic)
        -> Result<(), ReceiveError>
    {
        let inbound = self.core.inbound(route).ok_or(ReceiveError::UnknownRoute)?;
        inbound.queue_arrived(sampled);
        Ok(())
    }

    /// Delivers an arrival already reported at a binding-owned queue boundary.
    /// Buffered push bindings use this when dequeuing; direct push uses `deliver`.
    /// This avoids a second report at take/admission. No receipt buffer is added.
    pub fn deliver_after_arrival(&self, route: InboundRoute, frame: InboundFrame<'_>)
        -> Result<(), ReceiveError>
    {
        self.core.inbound(route).ok_or(ReceiveError::UnknownRoute)?.deliver(frame)
    }

    /// Retained counterpart of `deliver_after_arrival`; the original storage owner
    /// is preserved. Its true arrival must already have been reported once.
    pub fn deliver_frame_after_arrival<F: Frame>(&self, route: InboundRoute, frame: F)
        -> Result<(), ReceiveError>
    {
        self.core.inbound(route).ok_or(ReceiveError::UnknownRoute)?
            .deliver_retained(super::frame::RetainedFrame::new(frame))
    }

    /// Delivers a frame from the transport's own thread.
    ///
    /// # Errors
    /// A refused frame is dropped and counted, without blocking the transport.
    pub fn deliver(
        &self,
        route: InboundRoute,
        frame: InboundFrame<'_>,
    ) -> Result<(), ReceiveError> {
        let inbound = self.core.inbound(route).ok_or(ReceiveError::UnknownRoute)?;
        // Direct push arrival precedes SDK parsing, TTL and capacity refusal.
        inbound.queue_arrived(self.core.shared.platform.now());
        self.deliver_after_arrival(route, frame)
    }

    /// Delivers a frame retained in transport memory until its lease drops.
    ///
    /// # Errors
    /// On refusal, the frame's owner is dropped before this returns.
    pub fn deliver_frame<F: Frame>(
        &self,
        route: InboundRoute,
        frame: F,
    ) -> Result<(), ReceiveError> {
        let inbound = self.core.inbound(route).ok_or(ReceiveError::UnknownRoute)?;
        inbound.queue_arrived(self.core.shared.platform.now());
        self.deliver_frame_after_arrival(route, frame)
    }

    /// Counter-only overflow report. It does not invent an arrival timestamp:
    /// report the actual incoming frame once through `arrived`, including when
    /// the incoming frame is discarded; evicting an older frame is not a new arrival.
    /// Reports a frame the binding's own queue discarded under the route's overflow
    /// policy: its oldest queued frame under `DropOldest`, the arriving one under
    /// `DropNewest`. Counted as the SDK counts a discard in its own queue: an
    /// evicted frame in `carried` and `dropped_oldest`, a refused one in
    /// `dropped_newest`. A `Wait` queue drops nothing. `run_qos_contract` checks
    /// the counts, unless the binding declares that its middleware discards out of
    /// its sight.
    pub fn dropped(&self, route: InboundRoute, overflow: Overflow) {
        if let Some(inbound) = self.core.inbound(route) {
            inbound.binding_dropped(overflow);
        }
    }

    /// Signals that a route previously refused with `Busy` can accept frames.
    pub fn ready(&self, route: BindingRoute) {
        let _ = route;
        self.core.notify();
    }

    /// Signals that a pull route has something to receive. This is a hint only;
    /// report actual queue arrivals separately through `arrived`.
    pub fn readable(&self, route: InboundRoute) {
        if let Some(inbound) = self.core.inbound(route) {
            inbound.readable();
        }
    }

    /// Reports that a peer's incarnation changed, failing calls bound to the old one.
    pub fn peer_restarted(&self, route: InboundRoute, peer: PeerRef) {
        if let Some(inbound) = self.core.inbound(route) {
            inbound.peer_restarted(peer);
        }
    }

    /// Reports that a peer is gone, for instance a lease or SD TTL expiry.
    pub fn peer_lost(&self, route: InboundRoute, peer: PeerRef) {
        if let Some(inbound) = self.core.inbound(route) {
            inbound.peer_lost(peer);
        }
    }
}

/// Copies metadata and payload chunks into `out`, contiguously.
pub(crate) fn copy_frame(
    frame: &InboundFrame<'_>,
    out: &mut [u8],
) -> Result<(usize, usize), ReceiveError> {
    let metadata = frame.metadata;
    let total = metadata
        .len()
        .checked_add(frame.payload.len())
        .ok_or(ReceiveError::TooLarge)?;
    let target = out.get_mut(..total).ok_or(ReceiveError::TooLarge)?;
    let (head, mut rest) = target
        .split_at_mut_checked(metadata.len())
        .ok_or(ReceiveError::TooLarge)?;
    for (slot, byte) in head.iter_mut().zip(metadata) {
        *slot = *byte;
    }
    for chunk in frame.payload.chunks() {
        let (part, remaining) = rest
            .split_at_mut_checked(chunk.len())
            .ok_or(ReceiveError::TooLarge)?;
        for (slot, byte) in part.iter_mut().zip(chunk) {
            *slot = *byte;
        }
        rest = remaining;
    }
    Ok((metadata.len(), frame.payload.len()))
}

/// Header view helper used by the `Frame` based paths.
pub(crate) type StaticHeader = RxHeader<'static>;
