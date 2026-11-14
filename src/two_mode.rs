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

//! The one merged two-mode API, re-exported from `up-rust-core` at the paths a
//! `std` application uses, plus the heap-backed constructors (R15).
//!
//! ```rust
//! use up_rust::two_mode::*;
//! ```

#![allow(clippy::result_large_err)] // Setup-only: these never run in runtime mode.

#[cfg(up_rust_test_hooks)]
use std::mem::MaybeUninit;

pub use up_rust_core::communication::two_mode::{
    InPlaceInit, Method, MethodResource, NotificationListener, NotificationListenerBuilder,
    NotificationListenerSetup, NotificationResource, NotificationTopic, Notifier, NotifierBuilder,
    NotifierSetup, PublishResource, PublishTopic, Publisher, PublisherBuilder, PublisherSetup,
    Request, RpcClient, RpcClientBuilder, RpcClientSetup, RpcServer, RpcServerBuilder,
    RpcServerSetup, Subscriber, SubscriberBuilder, SubscriberSetup,
};
pub use up_rust_core::communication::LocalUriProvider;
pub use up_rust_core::memory::{
    FixedVec, MemoryReport, RegionSource, StartupMemory, StartupRegion,
};
pub use up_rust_core::payload::agreement::{AgreementError, NativeAgreement, NativeAgreementEntry};
pub use up_rust_core::payload::loan::BorrowError;
pub use up_rust_core::payload::stable::{
    InitializedStablePayload, StablePayload, StablePayloadInit,
};
pub use up_rust_core::platform::{Monotonic, Platform, PlatformInit};
pub use up_rust_core::signal::{Signal, SignalWait, WaitToken};
pub use up_rust_core::transport::{
    check_qos, enter_runtime_all, BindingRefusal, BindingRoute, BindingSetup, Call, CallError,
    Capabilities, CarriedFrame, CarriedWire, CarrierRoute, CustomEnvelope, Deadline, Deviation,
    DeviationKind, EgressRoute, Envelope, Flow, Frame, HandoffRuntime, HeapOperation, InboundFrame,
    InboundRoute, IngressRoute, InitializedTx, LendingRuntime, LoanError, LoanedPayload,
    MessageFields, NoWire, OutgoingFrame, Overflow, ParkedFrame, PayloadBytes,
    PayloadLoanProvenance, PeerRef, PreparedRoute, PreparedTxStorage, Qos, QosRefused, QosSetting,
    QosSupport, ReceiveBudget, ReceiveError, Received, Reliability, Reply, ReplyToken, RetainedFrame,
    RouteCounterDelta, RouteCounters, RouteDeclaration, RouteDirection, RouteId, RouteKind,
    RouteName, RouteReport, RouteView, RuntimePermit, RuntimePort, RuntimeWaiver, SendError,
    SendMode, SendRefusal, HandoffFailure, SendReport, SetupCheck, StartupError, TransportRuntime, TransportSetup,
    TransportSource, TransportView, TxFrameLayout, Wired, RETAINED_FRAME_ALIGN,
    RETAINED_FRAME_CAPACITY,
};
pub use up_rust_core::wire::{
    Encoded, EncodedSize, InPlace, NativeWire, PayloadWriter, Ready, ReservedRange, RoleWire,
    RpcRoleWire, RuntimeHeap, StartupSlot, Wire, WireError, WireIdentity, WireInfo, WireMode,
    WirePayloadIdentity, ZeroSlot,
};
pub use up_rust_core::{runtime_waiver, wire_table};
#[cfg(feature = "zero-copy-transport")]
pub use up_rust_macros::ZeroSlot;

// Preserve the facade's existing extension-trait path while allowing slim
// consumers to depend directly on up-rust-std.
pub use up_rust_std::TransportSetupExt;

/// Bridges a classic [`crate::LocalUriProvider`] to the core trait.
#[derive(Debug)]
pub struct ClassicUri<'a, P: ?Sized>(pub &'a P);

impl<P: crate::LocalUriProvider + ?Sized> LocalUriProvider for ClassicUri<'_, P> {
    fn get_resource_uri(&self, resource_id: u16) -> up_rust_core::uri::UUri {
        self.0.get_resource_uri(resource_id)
    }

    fn get_source_uri(&self) -> up_rust_core::uri::UUri {
        self.0.get_source_uri()
    }
}

/// Writes through a [`PayloadWriter`] with `std::io::Write`, for encoders that
/// expect it (design section 5; `impl io::Write for PayloadWriter` cannot live in core).
#[derive(Debug)]
pub struct WriterIo<'w, 'a>(pub &'w mut PayloadWriter<'a>);

impl std::io::Write for WriterIo<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .put(bytes)
            .map(|()| bytes.len())
            .map_err(|_| std::io::ErrorKind::WriteZero.into())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Builds a [`NativeAgreement`] table from a classic agreement. Leaks the table:
/// this runs once, during initialization, and the table lives for the process (R38).
///
/// # Panics
/// If the agreement is empty of targets; agreements carry at least one type.
#[cfg(feature = "native-identity")]
#[must_use]
pub fn native_agreement_table(
    agreement: &crate::NativeProfileAgreement,
) -> &'static NativeAgreement {
    let profile = agreement.profile();
    let mut entries = Vec::new();
    let mut triple = String::new();
    let mut pointer_width = (usize::BITS) as u8;
    for representation in profile.representations() {
        triple.clear();
        triple.push_str(representation.target().triple());
        pointer_width = representation.target().pointer_width();
        let digest = up_rust_core::payload::sha256::sha256(&representation.canonical_bytes());
        entries.push(NativeAgreementEntry {
            type_name: Box::leak(representation.name().to_string().into_boxed_str()),
            revision: representation.revision(),
            digest,
        });
    }
    let little_endian = cfg!(target_endian = "little");
    Box::leak(Box::new(NativeAgreement {
        domain: Box::leak(profile.domain().to_string().into_boxed_str()),
        version: profile.version(),
        encoding: profile.encoding(),
        target_triple: Box::leak(triple.into_boxed_str()),
        pointer_width,
        little_endian,
        entries: Box::leak(entries.into_boxed_slice()),
    }))
}

/// Places `count` startup-initialized slots of `T`, through `wire`, for contract
/// tests that exercise a wire over its real startup path.
///
/// # Errors
/// As [`StartupMemory`]'s placement.
#[doc(hidden)]
#[cfg(up_rust_test_hooks)]
pub fn startup_values<T: 'static, W: Wire<T>>(
    memory: &StartupMemory,
    wire: &W,
    count: usize,
) -> Result<&'static mut [T], up_rust_core::memory::StartupError> {
    let slots: &'static mut [MaybeUninit<T>] = memory.__slice_for_tests::<T>(count)?;
    for slot in slots.iter_mut() {
        let ready = wire.init_slot(up_rust_core::wire::__contract_slot(slot));
        let _ = up_rust_core::wire::__contract_value(ready);
    }
    // SAFETY: every slot was just initialized by `init_slot`; `MaybeUninit<T>`
    // has the layout of `T`; the memory lives for the rest of the process.
    Ok(unsafe { &mut *(core::ptr::from_mut(slots) as *mut [T]) })
}

/// Advanced binding-owned finite index storage and lifetime diagnostics.
pub use up_rust_core::transport::{pool, LifetimePolicy, LifetimeState, PoolError};
