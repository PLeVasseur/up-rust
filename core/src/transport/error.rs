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

//! Refusals: fixed-size at runtime, and naming the route during initialization.
//!
//! `SendError`, [`ReceiveError`] and [`CallError`] are `Copy` and carry no heap
//! data, so reporting a refused send cannot touch the heap. `up-rust` converts
//! only [`StartupError`], from initialization, into a `UStatus`.

use core::fmt;

use super::qos::QosSetting;
use super::SendError;
use crate::frame::header::UFrameHeaderError;
use crate::memory;
use crate::uattributes::UMessageType;
use crate::ucode::UCode;
use crate::uri::UUri;
use crate::wire::WireError;

/// Which way a route carries frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteDirection {
    /// The route sends frames.
    Send,
    /// The route receives frames.
    Receive,
}

/// Names one route in a refusal or report. Fixed size: URIs are inline.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteName {
    /// Direction of the route.
    pub direction: RouteDirection,
    /// Message kind, or `None` for a carrier route that forwards every kind.
    pub kind: Option<UMessageType>,
    /// Source URI or source filter.
    pub source: UUri,
    /// Sink URI or sink filter.
    pub sink: Option<UUri>,
}

impl fmt::Display for RouteName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} {:?} {}", self.direction, self.kind, self.source)?;
        if let Some(sink) = &self.sink {
            write!(f, " -> {sink}")?;
        }
        Ok(())
    }
}

/// A refusal raised during initialization, naming the offending route.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum StartupError {
    /// A strict policy refused initialization, with an actionable diagnosis.
    Strict {
        /// Route, or None for a transport-wide finding.
        route: Option<RouteName>,
        /// Stable rule identifier.
        rule: &'static str,
        /// Normative requirement identifier.
        requirement: &'static str,
        /// What the integrator must change.
        fix: &'static str,
    },
    /// A route's wire would need the heap in runtime mode.
    RouteNeedsHeapAtRuntime {
        /// The refused route.
        route: RouteName,
        /// The wire's named reason.
        wire: &'static str,
    },
    /// A route's middleware needs a waiver that was not given.
    MiddlewareHeapNeedsWaiver {
        /// The refused route.
        route: RouteName,
        /// The permit's identifier.
        permit: &'static str,
    },
    /// A waiver was rejected.
    WaiverRefused {
        /// The waiver's identifier.
        waiver: &'static str,
        /// Why it was refused.
        problem: &'static str,
    },
    /// The transport cannot honor the route's QoS. Never silently downgraded.
    QosNotHonored {
        /// The refused route.
        route: RouteName,
        /// The setting that cannot be honored.
        setting: QosSetting,
        /// The value asked for.
        requested: u32,
        /// The most the binding can do; 0 where the setting is unsupported.
        available: u32,
    },
    /// The role requires lending and this transport copies at handoff (R16).
    LendingRequired {
        /// The refused route.
        route: RouteName,
    },
    /// A native route on a handoff transport (R20: no silent fallback).
    NativeNeedsLending {
        /// The refused route.
        route: RouteName,
    },
    /// The transport does not carry this envelope.
    EnvelopeNotAccepted {
        /// The refused route.
        route: RouteName,
    },
    /// The transport carries one encoding per route, and several were listed.
    SingleEncodingOnly {
        /// The refused route.
        route: RouteName,
    },
    /// Two wires of one type on a route: `publish_using::<W>` could not pick.
    DuplicateWireType {
        /// The refused route.
        route: RouteName,
    },
    /// The route's reservation exceeds what the transport carries.
    FrameTooLarge {
        /// The refused route.
        route: RouteName,
        /// The route's largest frame.
        requested: usize,
        /// The transport's largest frame.
        maximum: usize,
    },
    /// A wire with no size bound and no `.max_payload` on this route.
    PayloadUnbounded {
        /// The refused route.
        route: RouteName,
    },
    /// A variable-size wire on a binding that cannot carry a short payload.
    TruncationNotSupported {
        /// The refused route.
        route: RouteName,
    },
    /// A server on a binding that declares clients at setup, built without `.clients(..)`.
    ClientsRequired {
        /// The refused route.
        route: RouteName,
    },
    /// A carrier route was refused, naming the route, the binding and the reason.
    CarrierRefused {
        /// The refused route.
        route: RouteName,
        /// The binding that was asked.
        binding: &'static str,
        /// Why: an SDK rule or the binding's own reason.
        reason: &'static str,
    },
    /// Deployment configuration or a route declaration was refused (R43).
    InvalidConfig(&'static str),
    /// The binding refused during setup.
    Binding {
        /// The binding.
        binding: &'static str,
        /// The binding's reason.
        reason: &'static str,
    },
    /// A role was activated against a runtime other than the one it was set up on.
    WrongTransport,
    /// Exact deadline admission failure; an optional initialization join error
    /// is retained alongside the primary refusal.
    DeadlineTiming {
        /// Missing or mismatched timing prerequisite.
        gap: super::TimingGap,
        /// Retained initialization cleanup error, if cleanup also failed.
        cleanup_code: Option<i32>,
    },
    /// Actual pthread preparation failure, preserving any secondary attribute or
    /// join cleanup error and an underlying startup-memory refusal.
    DeadlineWorker {
        /// Backend operation that failed during initialization.
        operation: &'static str,
        /// Error code returned by that operation.
        code: i32,
        /// Error from cleaning up worker attributes, if any.
        attribute_cleanup: Option<i32>,
        /// Error from joining the initialization worker, if any.
        join_cleanup: Option<i32>,
        /// Associated startup memory reservation failure, if any.
        memory: Option<memory::StartupError>,
    },
    /// Startup memory or a fixed table.
    Memory(memory::StartupError),
}

impl From<memory::StartupError> for StartupError {
    fn from(value: memory::StartupError) -> Self {
        Self::Memory(value)
    }
}

impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Strict { route, rule, requirement, fix } => {
                if let Some(route) = route { write!(f, "route {route}: ")?; }
                write!(f, "strict {rule} ({requirement}): {fix}")
            }
            Self::RouteNeedsHeapAtRuntime { route, wire } => {
                write!(f, "route {route}: wire {wire} needs the heap at runtime")
            }
            Self::MiddlewareHeapNeedsWaiver { route, permit } => write!(
                f,
                "route {route}: middleware permit {permit} needs a named waiver; add `.with_waiver(..)`"
            ),
            Self::WaiverRefused { waiver, problem } => write!(f, "waiver {waiver}: {problem}"),
            Self::QosNotHonored {
                route,
                setting,
                requested,
                available,
            } => write!(
                f,
                "route {route}: the transport cannot honor {setting}: requested {requested}, available {available}"
            ),
            Self::LendingRequired { route } => write!(
                f,
                "route {route}: the role requires lending, and this transport copies at handoff"
            ),
            Self::NativeNeedsLending { route } => {
                write!(f, "route {route}: native carriage needs a lending transport")
            }
            Self::EnvelopeNotAccepted { route } => {
                write!(f, "route {route}: the transport does not carry this envelope")
            }
            Self::SingleEncodingOnly { route } => {
                write!(f, "route {route}: the transport carries a single declared encoding per route")
            }
            Self::DuplicateWireType { route } => write!(
                f,
                "route {route}: two wires of one type; `publish_using` could not choose"
            ),
            Self::FrameTooLarge {
                route,
                requested,
                maximum,
            } => write!(
                f,
                "route {route}: frame of {requested} bytes exceeds transport maximum {maximum}"
            ),
            Self::PayloadUnbounded { route } => write!(
                f,
                "route {route}: the wire has no size bound; set `.max_payload(n)`"
            ),
            Self::TruncationNotSupported { route } => write!(
                f,
                "route {route}: this binding cannot carry a payload shorter than its loan; use an exact-size wire"
            ),
            Self::ClientsRequired { route } => write!(
                f,
                "route {route}: this binding declares clients at setup; add `.clients(..)` from deployment data"
            ),
            Self::CarrierRefused {
                route,
                binding,
                reason,
            } => write!(f, "carrier route {route} on {binding}: {reason}"),
            Self::InvalidConfig(reason) => write!(f, "invalid configuration: {reason}"),
            Self::Binding { binding, reason } => write!(f, "transport {binding}: {reason}"),
            Self::WrongTransport => f.write_str("activation against a different transport"),
            Self::DeadlineTiming { gap, cleanup_code } =>
                write!(f, "deadline admission: {gap:?}; cleanup: {cleanup_code:?}"),
            Self::DeadlineWorker { operation, code, attribute_cleanup, join_cleanup, memory } =>
                write!(f, "deadline worker {operation}: {code}; attribute cleanup: {attribute_cleanup:?}; join cleanup: {join_cleanup:?}; memory: {memory:?}"),
            Self::Memory(error) => write!(f, "{error}"),
        }
    }
}

impl StartupError {
    /// The uProtocol status code for this refusal. Heap-free.
    #[must_use]
    pub fn code(&self) -> UCode {
        match self {
            Self::DeadlineTiming { .. } => UCode::FailedPrecondition,
            Self::DeadlineWorker { .. } => UCode::Internal,
            Self::Memory(_) => UCode::ResourceExhausted,
            Self::WrongTransport | Self::Strict { .. } => UCode::FailedPrecondition,
            Self::Binding { .. } => UCode::Internal,
            Self::RouteNeedsHeapAtRuntime { .. }
            | Self::MiddlewareHeapNeedsWaiver { .. }
            | Self::WaiverRefused { .. }
            | Self::QosNotHonored { .. }
            | Self::LendingRequired { .. }
            | Self::NativeNeedsLending { .. }
            | Self::EnvelopeNotAccepted { .. }
            | Self::SingleEncodingOnly { .. }
            | Self::DuplicateWireType { .. }
            | Self::FrameTooLarge { .. }
            | Self::PayloadUnbounded { .. }
            | Self::TruncationNotSupported { .. }
            | Self::ClientsRequired { .. }
            | Self::CarrierRefused { .. }
            | Self::InvalidConfig(_) => UCode::InvalidArgument,
        }
    }
}

impl core::error::Error for StartupError {}

/// Why an inbound frame was not accepted. Reported to the delivering binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReceiveError {
    /// This pool incarnation retired. Held frames and calls remain valid.
    PoolRetired,
    /// The inbound route index is unknown.
    UnknownRoute,
    /// Every startup slot of the route is in use.
    SlotsExhausted,
    /// The frame does not fit the route's receive slots.
    TooLarge,
    /// Header validation failed.
    Header(UFrameHeaderError),
    /// The frame's addressing or kind does not belong on this route.
    WrongRoute,
    /// The frame's payload encoding is not in the route's closed set.
    UnknownEncoding(u32),
    /// The wire could not decode the payload.
    Wire(WireError),
    /// The sender is an earlier incarnation of a peer.
    StalePeer,
    /// The route's fixed peer table is full.
    PeerTableFull,
    /// A response matched no outstanding call. It may be late or duplicated.
    NoMatchingCall,
    /// The route is not ready for this delivery form.
    Unsupported(&'static str),
    /// The frame's time-to-live expired before it could be processed.
    Expired,
    /// The wall clock could not be read, so expiry cannot be judged (fail closed).
    ClockUnavailable,
    /// The caller holds every lease the route reserved. Drop one and try again.
    MaxHeldExceeded,
    /// A request came from a client outside the deployment's `.clients(..)` list.
    UnknownClient,
}

impl fmt::Display for ReceiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRoute => f.write_str("unknown inbound route"),
            Self::PoolRetired => f.write_str("pool incarnation retired"),
            Self::SlotsExhausted => f.write_str("every receive slot is in use"),
            Self::TooLarge => f.write_str("the frame exceeds the receive slot"),
            Self::Header(error) => write!(f, "{error}"),
            Self::WrongRoute => f.write_str("frame does not match its route"),
            Self::UnknownEncoding(encoding) => {
                write!(
                    f,
                    "payload encoding {encoding} is not admitted on this route"
                )
            }
            Self::Wire(error) => write!(f, "payload could not be decoded: {error}"),
            Self::StalePeer => f.write_str("frame is from a stale peer incarnation"),
            Self::PeerTableFull => f.write_str("the route's peer table is full"),
            Self::NoMatchingCall => f.write_str("no outstanding call matches this response"),
            Self::Unsupported(reason) => write!(f, "route cannot accept this delivery: {reason}"),
            Self::Expired => f.write_str("the message expired before it could be processed"),
            Self::ClockUnavailable => {
                f.write_str("the wall clock could not be read to judge expiry")
            }
            Self::MaxHeldExceeded => f.write_str("every lease of this route is in use"),
            Self::UnknownClient => f.write_str("the request came from an undeclared client"),
        }
    }
}

impl core::error::Error for ReceiveError {}

/// A failed RPC call. Fixed size; the slot is reclaimed when the call is dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CallError {
    /// A local call-table invariant failed before the request was sent.
    /// This is neither capacity exhaustion nor a response from the peer.
    InternalState,
    /// This pool incarnation retired. Held frames and calls remain valid.
    PoolRetired,
    /// Every call slot of this client is reserved.
    SlotsExhausted,
    /// The request could not be sent.
    NotSent(SendError),
    /// The deadline passed before a response arrived.
    TimedOut,
    /// The server restarted while this call was outstanding.
    PeerRestarted,
    /// The transport reported the server gone.
    PeerLost,
    /// The server answered with a failure status.
    Rejected(UCode),
    /// The response could not be decoded.
    Decode(WireError),
    /// The response exceeds the reservation made for it at setup.
    ResponseTooLarge,
    /// The response carried an encoding outside the admitted set.
    UnknownEncoding,
    /// The response was already taken.
    AlreadyTaken,
}

impl CallError {
    /// The uProtocol status code for this failure. Heap-free.
    #[must_use]
    pub fn code(&self) -> UCode {
        match self {
            Self::InternalState => UCode::Internal,
            Self::PoolRetired => UCode::FailedPrecondition,
            Self::SlotsExhausted | Self::ResponseTooLarge => UCode::ResourceExhausted,
            Self::NotSent(refusal) => refusal.code(),
            Self::TimedOut => UCode::DeadlineExceeded,
            Self::PeerRestarted | Self::PeerLost => UCode::Unavailable,
            Self::Rejected(code) => *code,
            Self::Decode(_) | Self::UnknownEncoding => UCode::InvalidArgument,
            Self::AlreadyTaken => UCode::FailedPrecondition,
        }
    }
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InternalState => f.write_str("local call-table invariant failed before send"),
            Self::PoolRetired => f.write_str("pool incarnation retired"),
            Self::SlotsExhausted => f.write_str("every call slot is in use"),
            Self::NotSent(refusal) => write!(f, "request was not sent: {refusal}"),
            Self::TimedOut => f.write_str("the call timed out"),
            Self::PeerRestarted => f.write_str("the server restarted"),
            Self::PeerLost => f.write_str("the server is gone"),
            Self::Rejected(code) => write!(f, "the server replied with {code:?}"),
            Self::Decode(error) => write!(f, "the response could not be decoded: {error}"),
            Self::ResponseTooLarge => f.write_str("the response exceeds its reservation"),
            Self::UnknownEncoding => {
                f.write_str("the response used an encoding that is not admitted")
            }
            Self::AlreadyTaken => f.write_str("the response was already taken"),
        }
    }
}

impl core::error::Error for CallError {}

/// Header refusals convert into receive refusals.
impl From<UFrameHeaderError> for ReceiveError {
    fn from(error: UFrameHeaderError) -> Self {
        Self::Header(error)
    }
}

impl From<super::PoolError> for ReceiveError {
    fn from(error: super::PoolError) -> Self { match error { super::PoolError::Full => Self::SlotsExhausted, super::PoolError::PoolRetired => Self::PoolRetired } }
}
impl From<super::PoolError> for super::SendError {
    fn from(error: super::PoolError) -> Self { match error { super::PoolError::Full => Self::SlotsExhausted, super::PoolError::PoolRetired => Self::PoolRetired } }
}
impl From<super::PoolError> for CallError {
    fn from(error: super::PoolError) -> Self { match error { super::PoolError::Full => Self::SlotsExhausted, super::PoolError::PoolRetired => Self::PoolRetired } }
}
