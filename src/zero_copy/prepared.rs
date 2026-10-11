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
//! Prepared-route mechanics: storage is reserved at setup and transmit is synchronous.

use std::{future::Future, mem::MaybeUninit, sync::Arc};

use crate::{Frame, RegistrationInbox, UFrameHeaderError, UUri, WrittenHeader};

/// Synchronous encoded delivery into setup-owned receiver state.
///
/// The sink may be a bounded registration inbox or an RPC pending table. Dynamic
/// erasure happens at setup and does not create an async future per message.
pub trait FrameSink<F: Frame>: Send + Sync {
    /// Accepts or locally rejects one frame, releasing rejected storage.
    fn push(&self, frame: F);
    /// Closes admission and wakes any local waiters without revoking retained frames.
    fn close(&self);
}

impl<F: Frame> FrameSink<F> for RegistrationInbox<F> {
    fn push(&self, frame: F) {
        let _ = RegistrationInbox::push(self, frame);
    }
    fn close(&self) {
        RegistrationInbox::close(self);
    }
}

/// Setup failure when reserving a route, registration or storage table.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum SetupError {
    /// A configuration value is unusable.
    #[error("invalid setup: {0}")]
    InvalidConfig(&'static str),
    /// A setup allocation could not be satisfied.
    #[error("setup allocation failed")]
    Allocation,
    /// A preallocated route or registration table is exhausted.
    #[error("reserved table is full")]
    TableFull,
    /// Route metadata does not satisfy the message kind.
    #[error("invalid route: {0}")]
    InvalidRoute(&'static str),
    /// The selected stable type is not agreed or has an invalid representation.
    #[error("payload type is not agreed")]
    UnknownPayloadType,
    /// The metadata codec cannot prepare the selected prefix.
    #[error("metadata prefix cannot be prepared")]
    Prefix,
    /// A registration worker cannot be created in the current environment.
    #[error("receive worker cannot be created")]
    Worker,
}

/// A fixed-size failure to obtain reserved transmit storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum LoanError {
    /// Every preallocated slot is still in use.
    #[error("transmit pool is exhausted")]
    PoolExhausted,
    /// The requested header/payload layout cannot fit one slot.
    #[error("transmit slot is too small")]
    SlotTooSmall,
    /// The route is closed or belongs to a different core instance.
    #[error("route is not owned by this core")]
    InvalidRoute,
}

/// A fixed-size per-message send failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum SendError {
    /// No suitable preallocated loan is available.
    #[error("transmit loan failed: {0}")]
    Loan(LoanError),
    /// The route's reserved token capacity was exceeded.
    #[error("token exceeds route capacity")]
    TokenTooLong,
    /// Tokens are disabled for this route.
    #[error("route does not accept tokens")]
    TokenNotAllowed,
    /// Traceparent is disabled or exceeds the profile limit.
    #[error("traceparent is not accepted")]
    TraceparentRejected,
    /// An RPC priority override is below CS4.
    #[error("RPC priority is too low")]
    PriorityRejected,
    /// A response needs its correlated request ID.
    #[error("response is missing its request ID")]
    MissingRequestId,
    /// The binding's payload storage does not meet the requested layout.
    #[error("invalid payload storage")]
    PayloadStorage,
    /// The initializer's witness belongs to a different byte range.
    #[error("initialization witness does not match the loan")]
    WitnessMismatch,
    /// The initializer produced an invalid field representation.
    #[error("invalid initialized field bits")]
    InvalidFieldBits,
    /// Header encoding rejected a per-message value.
    #[error("header encoding failed: {0}")]
    Header(UFrameHeaderError),
    /// Delivery could not complete under the binding's bounded policy.
    #[error("frame delivery failed")]
    Delivery,
}

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
}

/// A loan whose SDK-visible header and payload were initialized and checked.
///
/// Ordinary construction is performed by the checked frame filler. Advanced
/// producers must satisfy the explicit unsafe constructor contract. A binding
/// still owns initialization of native framing/padding outside those visible views.
pub struct InitializedTx<T> {
    pub(crate) storage: T,
}

impl<T> InitializedTx<T> {
    /// Wraps a loan initialized outside the checked SDK filler.
    ///
    /// Ordinary sends use the checked route initializer instead.
    ///
    /// # Safety
    /// Every SDK-visible metadata and present payload byte must be initialized;
    /// metadata must satisfy the selected codec and payload-presence contract.
    /// Any typed payload's representation and field bits must be valid, and its
    /// initialization witness must refer to this loan's exact payload range.
    /// The caller must have recorded the correct header/token extent with the
    /// storage owner. This proves nothing about unrelated native framing/padding.
    pub unsafe fn from_initialized(storage: T) -> Self {
        Self { storage }
    }

    /// Consumes the checked wrapper for the binding's physical publish operation.
    #[must_use]
    pub fn into_storage(self) -> T {
        self.storage
    }
}

impl<T> std::fmt::Debug for InitializedTx<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InitializedTx").finish_non_exhaustive()
    }
}

/// Physical prepared-route mechanics composed with SDK-owned wire semantics.
///
/// A core reserves its route/fan-out/registration tables and storage at creation.
/// Routes contain fixed physical addressing and capacities. Transmit operations
/// are synchronous and must not allocate after setup, including their failures.
/// Remaining asynchronous registration work uses a concrete future, not async-trait.
pub trait PreparedTransportCore: Send + Sync + 'static {
    /// Instance-bound route handle; releasing it returns its reserved table entry.
    type Route: Send + Sync;
    /// Exclusive transmit loan, allowed to borrow its route/core owners.
    type Tx<'a>: PreparedTxStorage
    where
        Self: 'a;
    /// Receive storage retaining all physical owners independently.
    type Rx: Frame;
    /// Instance-bound detachment handle, created during registration setup.
    type Subscription: Send;

    /// Reserves a route entry from the preallocated table, matching fan-out now.
    ///
    /// # Errors
    /// Returns a fixed-size error for invalid capacities or table exhaustion.
    fn prepare_route(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        limits: RouteLimits,
    ) -> Result<Self::Route, SetupError>;

    /// Borrows reserved storage for one message without allocating.
    ///
    /// # Errors
    /// Reports route mismatch, exhaustion or an unsupported physical layout.
    fn loan<'a>(
        &'a self,
        route: &'a Self::Route,
        layout: TxFrameLayout,
    ) -> Result<Self::Tx<'a>, LoanError>;

    /// Publishes an initialized loan under the core's bounded delivery policy.
    ///
    /// # Errors
    /// Reports delivery failure without constructing an owned error string.
    fn publish<'a>(
        &'a self,
        route: &'a Self::Route,
        frame: InitializedTx<Self::Tx<'a>>,
    ) -> Result<(), SendError>;

    /// Attaches a synchronous inbox sink using setup-reserved registration capacity.
    ///
    /// A None sink filter matches sinkless frames, not every destination.
    ///
    /// # Errors
    /// Reports invalid setup or registration-table exhaustion without leaving a
    /// partially attached sink behind.
    fn attach(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        inbox: Arc<dyn FrameSink<Self::Rx>>,
    ) -> impl Future<Output = Result<Self::Subscription, SetupError>> + Send;

    /// Detaches the exact instance-bound registration; existing frames stay valid.
    ///
    /// # Errors
    /// Rejects a foreign or already removed registration handle.
    fn detach(&self, subscription: &mut Self::Subscription) -> Result<(), SetupError>;
}
