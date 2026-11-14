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

use std::{future::Future, sync::Arc};

use crate::{Frame, RegistrationInbox, UUri};

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

pub use up_rust_core::transport::{
    InitializedTx, LoanError, PreparedTxStorage, RouteLimits, SendError, SetupError, TxFrameLayout,
};

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

/// Physical native-loan capability for an already configured middleware core.
///
/// Implement only for native-only binding types whose constructor admits direct
/// middleware TX loans and publication of that exact storage, and whose RX frames
/// are native-backed and independently retain every middleware owner. A network
/// mode or heap-backed prepared pool must not implement this trait. Dynamic
/// configuration must refuse unsupported TX/RX modes before returning the core.
/// Zenoh implementations must require SHM TX and reject non-SHM RX.
///
/// No blanket implementation exists. This contract is an implementation obligation,
/// not evidence of physical zero-copy; real loan, lifetime and allocation gates
/// remain mandatory. Explicit test-only core wrappers confer no production credit.
pub trait NativeLoanCore: PreparedTransportCore {}
