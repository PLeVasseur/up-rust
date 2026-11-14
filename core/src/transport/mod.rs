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

//! L1 contracts for the transports: received frames, lent transmit storage,
//! their fixed-size errors, the two modes and the binding contracts.
//!
//! A transport starts as a [`TransportSetup`], where routes, wires and startup
//! memory are declared and admitted. [`TransportSetup::enter_runtime`] consumes
//! it and returns a [`TransportRuntime`]. Handles activated from that value are
//! the only way to send or receive, so traffic is impossible until runtime mode
//! and setup is impossible after it (R2). Both types are non-generic: a Streamer
//! holds a list of transports chosen from configuration at run time (R5).
//!
//! Native runtime paths neither acquire nor release heap memory (R3). Serialized
//! paths may declare allocation, including owned decoded values released with
//! their leases. Runtime backing remains permanent; refusals are fixed-size
//! values. A peer dying or restarting is normal input (R40).

#![allow(clippy::result_large_err)]

use core::fmt;
use core::mem::MaybeUninit;

use crate::frame::header::{UFrameHeaderError, WrittenHeader};
use crate::ucode::UCode;
use crate::wire::WireError;

mod decoded;
mod deadline_monitor;
mod deadline_service;
mod deadline_timing;
pub use deadline_service::{DeadlineServiceEntry, DeadlineServiceRequest, PreparedDeadlineService};
pub use deadline_timing::{ArrivalTiming, CounterReadContract, DeadlineConfiguration, DeadlineTimingReport,
    ServiceRoute, TimingEvidencePins, TimingGap, TimingIdentity, TimingStatus, ValidatedJ};
pub mod binding;
mod receive_budget;
pub mod carrier;
pub mod envelope;
pub mod error;
pub mod frame;
#[doc(hidden)]
pub mod local;
mod metadata;
pub mod ids;
#[doc(hidden)]
#[cfg(up_rust_test_hooks)]
pub use metadata::__native32_records_control;
pub mod inbound;
pub mod mode;
pub mod qos;
pub mod report;
pub mod rpc;
pub mod slots;
pub mod pool;
pub use pool::{LifetimePolicy, LifetimeState, PoolError};
mod strict;
pub mod waiver;
pub mod wake;
pub use strict::{RouteView, SetupCheck, TransportView};

#[cfg(test)]
pub(crate) mod tests;

pub use receive_budget::ReceiveBudget;
pub use binding::{
    BindingRefusal, BindingRoute, BindingSetup, Capabilities, Flow, HandoffRuntime, InboundFrame,
    InboundRoute, LendingRuntime, OutgoingFrame, PayloadBytes, PeerLossOutcome, PeerRef, PreparedRoute, ReplyToken,
    RouteDeclaration, RouteKind, RuntimePort, SendRefusal, HandoffFailure,
};
pub use carrier::{
    CarriedFrame, CarriedWire, CarrierRoute, EgressRoute, IngressRoute, ParkedFrame,
};
pub use envelope::{
    CustomEnvelope, Envelope, NATIVE_ENVELOPE_HEADER_LEN, NATIVE_ENVELOPE_MAGIC,
    NATIVE_ENVELOPE_VERSION,
};
pub use error::{CallError, ReceiveError, RouteDirection, RouteName, StartupError};
pub use frame::{RetainedFrame, RETAINED_FRAME_ALIGN, RETAINED_FRAME_CAPACITY};
pub use inbound::{Received, TypedInbound};
pub use mode::{
    enter_runtime_all, MessageFields, NoWire, TransportRuntime, TransportSetup, TransportSource,
    Wired,
};
pub use qos::{
    check_qos, Deadline, Overflow, Qos, QosRefused, QosSetting, QosSupport, Reliability, SendReport,
};
pub use report::{DeadlineReport, RouteCounterDelta, RouteCounters, RouteId, RouteReport, SendMode};
pub use rpc::{Call, CallTable, Reply};
pub use waiver::{Deviation, DeviationKind, HeapOperation, RuntimePermit, RuntimeWaiver};
pub use wake::RouteWake;

/// Provenance class for one contiguous receive loan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadLoanProvenance {
    /// Storage is owned by a public receive lease.
    OwnedReceiveLease,
    /// Storage is loaned by an opaque physical transport.
    OpaqueTransportLoan,
}

/// Contiguous payload bytes carrying explicit loan provenance.
#[derive(Clone, Copy, Debug)]
pub struct LoanedPayload<'a> {
    bytes: &'a [u8],
    provenance: PayloadLoanProvenance,
}

impl<'a> LoanedPayload<'a> {
    /// Creates a loan after the caller has established its provenance.
    ///
    /// `provenance` is a statement of fact by the transport: `bytes` is the
    /// receive storage the transport actually loaned, not a copy, an allocation
    /// or a coalesced buffer. No memory-safety property depends on it (every
    /// typed borrow re-checks length, alignment and bit validity), so the
    /// constructor is safe; a wrong value misreports zero-copy evidence through
    /// [`provenance`](Self::provenance) and is a transport bug.
    #[must_use]
    pub const fn new(bytes: &'a [u8], provenance: PayloadLoanProvenance) -> Self {
        Self { bytes, provenance }
    }

    /// Returns the contiguous payload bytes.
    #[must_use]
    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Returns the loan provenance.
    #[must_use]
    pub const fn provenance(self) -> PayloadLoanProvenance {
        self.provenance
    }
}

/// Binding-owned immutable metadata and optional contiguous payload storage.
///
/// The frame retains every resource required by its byte views, including after
/// listener detachment or transport destruction. An absent payload differs from
/// a present empty payload. Implementations report provenance truthfully and do
/// not allocate, coalesce or copy storage to produce these views.
///
/// The SDK moves an owner to its final slot before taking the views it will keep,
/// so an owner may hold its bytes inline. Once there it is not moved again, and the
/// SDK reads the views without calling back into the owner.
pub trait Frame: Send + 'static {
    /// Selected-wire metadata bytes, immutable while the frame is retained.
    fn encoded_metadata(&self) -> &[u8];
    /// Optional payload loan, including a present zero-length payload.
    fn payload(&self) -> Option<LoanedPayload<'_>>;
    /// Who sent this frame. Required (R40): `PeerRef::UNKNOWN` means the binding
    /// has deadlines only.
    fn peer(&self) -> PeerRef;
}

/// Setup failure when reserving a route, registration or storage table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SetupError {
    /// A configuration value is unusable.
    InvalidConfig(&'static str),
    /// A setup allocation could not be satisfied.
    Allocation,
    /// A preallocated route or registration table is exhausted.
    TableFull,
    /// Route metadata does not satisfy the message kind.
    InvalidRoute(&'static str),
    /// The selected stable type is not agreed or has an invalid representation.
    UnknownPayloadType,
    /// The metadata codec cannot prepare the selected prefix.
    Prefix,
    /// A registration worker cannot be created in the current environment.
    Worker,
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(reason) => write!(f, "invalid setup: {reason}"),
            Self::Allocation => f.write_str("setup allocation failed"),
            Self::TableFull => f.write_str("reserved table is full"),
            Self::InvalidRoute(reason) => write!(f, "invalid route: {reason}"),
            Self::UnknownPayloadType => f.write_str("payload type is not agreed"),
            Self::Prefix => f.write_str("metadata prefix cannot be prepared"),
            Self::Worker => f.write_str("receive worker cannot be created"),
        }
    }
}

impl core::error::Error for SetupError {}

/// A fixed-size failure to obtain reserved transmit storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum LoanError {
    /// This pool incarnation is permanently closed; existing owners remain valid.
    PoolRetired,
    /// Every preallocated slot is still in use.
    PoolExhausted,
    /// The requested header/payload layout cannot fit one slot.
    SlotTooSmall,
    /// The route is closed or belongs to a different core instance.
    InvalidRoute,
}

impl fmt::Display for LoanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PoolRetired => f.write_str("transmit pool incarnation retired"),
            Self::PoolExhausted => f.write_str("transmit pool is exhausted"),
            Self::SlotTooSmall => f.write_str("transmit slot is too small"),
            Self::InvalidRoute => f.write_str("route is not owned by this core"),
        }
    }
}

impl core::error::Error for LoanError {}

/// A fixed-size per-message send failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SendError {
    /// This pool incarnation is permanently closed; existing owners remain valid.
    PoolRetired,
    /// No suitable preallocated loan is available.
    Loan(LoanError),
    /// The route's reserved token capacity was exceeded.
    TokenTooLong,
    /// Tokens are disabled for this route.
    TokenNotAllowed,
    /// Traceparent is disabled or exceeds the profile limit.
    TraceparentRejected,
    /// An RPC priority override is below CS4.
    PriorityRejected,
    /// A response needs its correlated request ID.
    MissingRequestId,
    /// The binding's payload storage does not meet the requested layout.
    PayloadStorage,
    /// The initializer's witness belongs to a different byte range.
    WitnessMismatch,
    /// The initializer produced an invalid field representation.
    InvalidFieldBits,
    /// Header encoding rejected a per-message value.
    Header(UFrameHeaderError),
    /// Delivery could not complete under the binding's bounded policy.
    Delivery,
    /// Every startup slot of this route is in use.
    SlotsExhausted,
    /// The transport cannot accept a frame now; it signals when it can.
    Busy,
    /// The frame exceeds the capacity reserved at setup.
    PayloadTooLarge,
    /// The wire refused the value.
    Wire(WireError),
    /// The payload encoding is not in the route's closed set.
    EncodingNotAdmitted(u32),
    /// The transport refused the frame.
    Rejected {
        /// The status the transport reports.
        code: UCode,
        /// Its reason.
        detail: &'static str,
    },
    /// Handoff refused with observed or unknown prior effects. The refusal remains
    /// available through `cause()`; this report is not a rollback or retry token.
    PartialHandoff {
        /// Original handoff refusal, preserving status and detail identity.
        refusal: binding::SendRefusal,
        /// Effects reported before refusal, with explicit count completeness.
        report: SendReport,
    },
    /// The wait for capacity passed its deadline.
    TimedOut,
    /// A request's reply window has closed.
    Expired,
    /// A message ID could not be generated.
    IdGeneration,
    /// A receive route's wire selection was given a wire type it did not admit.
    WireNotAdmitted,
    /// The wall clock could not be read, so the frame's expiry cannot be judged.
    ClockUnavailable,
}

impl SendError {
    /// Original error identity, excluding an attached handoff prefix report.
    pub fn cause(self) -> Self {
        if let Self::PartialHandoff { refusal, .. } = self {
            match refusal {
                binding::SendRefusal::PoolRetired => Self::PoolRetired,
                binding::SendRefusal::Busy => Self::Busy,
                binding::SendRefusal::TooLarge => Self::PayloadTooLarge,
                binding::SendRefusal::Failed { code, detail } => Self::Rejected { code, detail },
            }
        } else {
            self
        }
    }
    /// Available prefix effects. None is no supplied report, never proof of zero.
    pub fn delivery_report(self) -> Option<SendReport> {
        if let Self::PartialHandoff { report, .. } = self { Some(report) } else { None }
    }

    /// The uProtocol status code for this refusal. Heap-free.
    #[must_use]
    pub fn code(&self) -> UCode {
        match self {
            Self::PartialHandoff { .. } => self.cause().code(),
            Self::PoolRetired | Self::Loan(LoanError::PoolRetired) => UCode::FailedPrecondition,
            Self::SlotsExhausted | Self::Busy | Self::PayloadTooLarge | Self::Loan(_) => {
                UCode::ResourceExhausted
            }
            Self::Wire(_)
            | Self::EncodingNotAdmitted(_)
            | Self::Header(_)
            | Self::TokenTooLong
            | Self::TokenNotAllowed
            | Self::TraceparentRejected
            | Self::PriorityRejected
            | Self::MissingRequestId
            | Self::PayloadStorage
            | Self::WitnessMismatch
            | Self::InvalidFieldBits
            | Self::WireNotAdmitted => UCode::InvalidArgument,
            Self::Rejected { code, .. } => *code,
            Self::TimedOut | Self::Expired | Self::ClockUnavailable => UCode::DeadlineExceeded,
            Self::Delivery | Self::IdGeneration => UCode::Internal,
        }
    }
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PartialHandoff { report, .. } => write!(f, "{}; prior delivery report: {:?}", self.cause(), report),
            Self::PoolRetired => f.write_str("pool incarnation retired; use qualified supervisor recovery"),
            Self::Loan(error) => write!(f, "transmit loan failed: {error}"),
            Self::TokenTooLong => f.write_str("token exceeds route capacity"),
            Self::TokenNotAllowed => f.write_str("route does not accept tokens"),
            Self::TraceparentRejected => f.write_str("traceparent is not accepted"),
            Self::PriorityRejected => f.write_str("RPC priority is too low"),
            Self::MissingRequestId => f.write_str("response is missing its request ID"),
            Self::PayloadStorage => f.write_str("invalid payload storage"),
            Self::WitnessMismatch => f.write_str("initialization witness does not match the loan"),
            Self::InvalidFieldBits => f.write_str("invalid initialized field bits"),
            Self::Header(error) => write!(f, "header encoding failed: {error}"),
            Self::Delivery => f.write_str("frame delivery failed"),
            Self::SlotsExhausted => f.write_str("every startup slot is in use"),
            Self::Busy => f.write_str("the transport is busy"),
            Self::PayloadTooLarge => {
                f.write_str("the frame exceeds the capacity reserved at setup")
            }
            Self::Wire(error) => write!(f, "wire refused the value: {error}"),
            Self::EncodingNotAdmitted(encoding) => {
                write!(f, "payload encoding {encoding} is not admitted")
            }
            Self::Rejected { detail, .. } => write!(f, "transport refused: {detail}"),
            Self::TimedOut => f.write_str("the deadline passed before the transport was ready"),
            Self::Expired => f.write_str("the request has expired"),
            Self::IdGeneration => f.write_str("a message ID could not be generated"),
            Self::WireNotAdmitted => f.write_str("the wire is not admitted on this route"),
            Self::ClockUnavailable => {
                f.write_str("the wall clock could not be read, so expiry cannot be judged")
            }
        }
    }
}

impl core::error::Error for SendError {}

/// Fixed-size rejection before a received payload reaches its typed handler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AdmissionError {
    /// Header structure, metadata invariants or token admission failed.
    Header(UFrameHeaderError),
    /// Encoded identity and carrier payload presence disagree.
    PayloadPresence,
    /// The encoded identity is not the registered payload type.
    WrongPayloadType,
    /// The payload is not exactly the expected size.
    PayloadLength,
    /// The payload address does not satisfy the declared type's alignment.
    PayloadAlignment,
    /// At least one payload field has an invalid bit pattern.
    PayloadFieldBits,
}

impl fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Header(error) => write!(f, "header admission failed: {error}"),
            Self::PayloadPresence => f.write_str("payload presence disagrees with metadata"),
            Self::WrongPayloadType => f.write_str("wrong payload identity"),
            Self::PayloadLength => f.write_str("wrong payload length"),
            Self::PayloadAlignment => f.write_str("wrong payload alignment"),
            Self::PayloadFieldBits => f.write_str("invalid payload field bits"),
        }
    }
}

impl core::error::Error for AdmissionError {}

/// Exact logical extents requested from a physical transmit core.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TxFrameLayout {
    /// Selected-wire metadata extent, including prefix and length word.
    pub metadata_len: usize,
    /// Payload byte length; None means absent, Some(0) means present empty.
    pub payload_len: Option<usize>,
    /// Required payload alignment, positive and a power of two.
    pub payload_alignment: usize,
}

/// Upper bounds reserved by the core before this route sends any frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteLimits {
    /// Maximum metadata bytes produced by the route.
    pub max_metadata_len: usize,
    /// Maximum application payload bytes.
    pub max_payload_len: usize,
    /// Required payload alignment.
    pub payload_alignment: usize,
}

/// Exclusive binding-owned transmit storage with immutable logical extents.
///
/// The two views must be disjoint, stable and backed by this loan until it is
/// consumed. The core retains all native owners required by them. It must not
/// allocate, copy or resize storage to satisfy these accessors. Any native fields
/// outside these logical views remain the binding's initialization obligation.
///
/// # Safety
/// Each accessor must return storage exclusively owned by this loan for its full
/// lifetime. Metadata and payload extents must be disjoint, stable and immobile;
/// methods, including `record_header`, must not invalidate bytes already written
/// through either view or replace the backing allocation. A successful header
/// record must describe the same metadata extent later consumed by the core.
/// No other thread/owner may mutate or release this unpublished storage. The SDK
/// separately checks lengths/alignment and may reject a malformed layout, but
/// relies on these ownership/preservation guarantees when proving initialization.
pub unsafe trait PreparedTxStorage: Send {
    /// Writable metadata extent of exactly the requested length.
    fn metadata_uninit_mut(&mut self) -> &mut [MaybeUninit<u8>];
    /// Optional writable payload extent, including present empty storage.
    fn payload_uninit_mut(&mut self) -> Option<&mut [MaybeUninit<u8>]>;
    /// Records initialized header/token extents for cancellation and reclamation.
    ///
    /// # Errors
    /// Rejects extents outside this loan. Bindings wipe only memory they actually
    /// own and may not revoke storage retained by a published receiver.
    fn record_header(&mut self, header: WrittenHeader) -> Result<(), SendError>;

    /// Shrinks the logical payload to `used` bytes. Only shrinks.
    ///
    /// Called after a variable-size wire finished encoding: the binding must
    /// either initialize `used..loaned` before publishing or publish only `used`
    /// bytes. The default refuses, for a binding that declares no truncation.
    ///
    /// # Errors
    /// Refuses a grow, or a binding that cannot carry a short payload.
    fn truncate_payload(&mut self, used: usize) -> Result<(), SendError> {
        let _ = used;
        Err(SendError::PayloadStorage)
    }
}

/// A loan whose SDK-visible header and payload were initialized and checked.
///
/// Ordinary construction is performed by the checked frame filler. Advanced
/// producers must satisfy the explicit unsafe constructor contract. A binding
/// still owns initialization of native framing/padding outside those visible views.
pub struct InitializedTx<T> {
    storage: T,
}

impl<T> InitializedTx<T> {
    /// Wraps a loan initialized outside the checked SDK filler.
    ///
    /// Ordinary sends use the checked route initializer instead.
    ///
    /// # Safety
    /// Every SDK-visible metadata and present payload byte must be initialized,
    /// in the exact live, exclusively owned extents that the storage owner will
    /// publish. Those extents must remain initialized until ownership transfers.
    /// Any typed native payload's representation and field bits must be valid,
    /// and its initialization witness must refer to this loan's exact range.
    /// The caller must have recorded the correct header/token extent with the
    /// storage owner. This proves nothing about unrelated native framing/padding.
    ///
    /// This certificate does not establish arbitrary custom-codec correctness,
    /// metadata field validity, or agreement of metadata with payload presence.
    /// Those are protocol requirements requiring checked parsing/validation.
    /// Consumers must not infer a valid typed metadata representation or perform
    /// unchecked metadata reads from this certificate alone; validate any
    /// memory-sensitive metadata before using it for an unsafe operation.
    pub unsafe fn from_initialized(storage: T) -> Self {
        Self { storage }
    }

    /// Consumes the checked wrapper for the binding's physical publish operation.
    #[must_use]
    pub fn into_storage(self) -> T {
        self.storage
    }
}

impl<T> fmt::Debug for InitializedTx<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InitializedTx").finish_non_exhaustive()
    }
}
