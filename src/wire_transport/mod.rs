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

#![cfg_attr(
    not(any(
        feature = "transport-implementer-api",
        feature = "selected-wire-user-api"
    )),
    allow(dead_code)
)]

//! Selected-wire transport adapter core.
//!
//! Product transports implement the small core traits in this module for their
//! physical mechanics. A core accepts metadata bytes already prepared by the
//! selected metadata codec, returns encoded metadata bytes on receive, and
//! validates any physical mirror fields against decoded metadata before public
//! exposure. Product transport modules should not import, match on, or branch
//! by concrete wire families such as `UProtocolNativeWire`; the selected wire is
//! owned by [`UWireTransport<TCore, W, C>`].
//!
//! ## Using the adapter
//!
//! 1. Implement [`UOwnedTransportCore`] for physical storage and encoded
//!    metadata carriage. Core methods do not choose a wire.
//! 2. Wrap the core with [`UWithNativePrefixWire::into_native_prefix_wire_transport`]
//!    for an external or deployment-specific wire, or use
//!    [`UWithNativePrefixWire::into_protobuf_transport`].
//! 3. Use the resulting adapter through the public semantic owned-transport
//!    traits. The adapter encodes metadata before core TX, checks selected
//!    identities on RX, validates the frame, and only then exposes it.
//!
//! Cores own storage and physical carriage; wires and codecs own representations.
//! Identity admission precedes decoded frame exposure. The README provisional
//! policy applies to these adapter contracts.
//!
//! Prepared native and opaque carriage uses `PreparedTransportCore` over a shared
//! core. The adapter prepares route templates and generic receive registrations;
//! each registration retains its own physical handle instead of an adapter-owned
//! erased listener map. Native typed admission retains its explicit agreement.

#[cfg(any(
    all(feature = "zero-copy-transport", feature = "transport-implementer-api"),
    feature = "owned-frame-transport"
))]
use std::sync::Arc;
#[cfg(feature = "owned-frame-transport")]
use std::{any::Any, collections::HashMap, sync::Mutex};
use std::{io::Read, marker::PhantomData};

#[cfg(feature = "owned-frame-transport")]
use async_trait::async_trait;
#[cfg(feature = "owned-frame-transport")]
use bytes::Bytes;
#[cfg(feature = "owned-frame-transport")]
use tracing::warn;

#[cfg(feature = "zero-copy-transport")]
use crate::payload::loan::BorrowPayload;
use crate::payload::{
    codec::{PayloadDecodeLimit, ReadDecodePayload},
    UWireError,
};
use crate::wire::ProtobufWire;
#[cfg(feature = "zero-copy-transport")]
use crate::wire::StableContainerWire;
use crate::wire::UFrameFieldsMetadataCodec;
use crate::wire::{UWire, UWireMetadataCodecFor, UWireMetadataContext, UWirePayload};
#[cfg(feature = "owned-frame-transport")]
use crate::UCode;
#[cfg(any(
    all(feature = "zero-copy-transport", feature = "transport-implementer-api"),
    feature = "owned-frame-transport"
))]
use crate::UUri;
use crate::{validate_frame_view_for_transport, UFrameMetadata, UFrameView, UStatus};
#[cfg(feature = "zero-copy-transport")]
use crate::{LoanedPayload, ULoanedContiguousZeroCopyRxFrame, UZeroCopyRxLease};
#[cfg(feature = "owned-frame-transport")]
use crate::{UOwnedFrame, UOwnedListener, UOwnedTransportImpl};

#[cfg(feature = "owned-frame-transport")]
mod owned;
#[cfg(all(test, feature = "owned-frame-transport", feature = "up-core-api"))]
use owned::selected_wire_core_source_filter_for;
#[cfg(feature = "owned-frame-transport")]
use owned::WireListenerKey;
#[cfg(all(
    feature = "owned-frame-transport",
    any(test, feature = "transport-implementer-api")
))]
pub use owned::{EncodedOwnedFrame, PreparedOwnedFrame};
#[cfg(feature = "owned-frame-transport")]
pub use owned::{UEncodedOwnedListener, UOwnedTransportCore};
#[cfg(all(feature = "zero-copy-transport", feature = "transport-implementer-api"))]
mod prepared;
mod rx;
#[cfg(all(
    feature = "zero-copy-transport",
    any(
        feature = "transport-implementer-api",
        all(test, feature = "owned-frame-transport")
    )
))]
pub use rx::UEncodedLoanedRxFrame;
#[cfg(any(
    feature = "transport-implementer-api",
    all(test, feature = "owned-frame-transport")
))]
pub use rx::UEncodedRxFrame;
#[cfg(any(
    feature = "transport-implementer-api",
    feature = "selected-wire-user-api",
    all(test, feature = "owned-frame-transport", feature = "up-core-api")
))]
pub use rx::UWireRx;

/// Generic selected-wire transport adapter.
pub struct UWireTransport<TCore, W, C>
where
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
    core: TCore,
    wire: W,
    metadata_codec: C,
    native_agreement: Option<crate::NativeProfileAgreement>,
    #[cfg(feature = "owned-frame-transport")]
    owned_listeners: Mutex<HashMap<WireListenerKey, Arc<dyn Any + Send + Sync>>>,
}

impl<TCore, W, C> core::fmt::Debug for UWireTransport<TCore, W, C>
where
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UWireTransport").finish_non_exhaustive()
    }
}

impl<TCore, W, C> UWireTransport<TCore, W, C>
where
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
    /// Creates an adapter around a transport core, selected wire marker, and metadata codec.
    ///
    /// Ordinary construction is available only for a wire that does not require
    /// a native agreement. That requirement is checked when the generic
    /// constructor is instantiated during code generation, not by `cargo check`.
    /// Native wires use [`Self::with_native_agreement`].
    #[cfg_attr(
        all(
            feature = "wire-implementer-api",
            feature = "transport-implementer-api"
        ),
        doc = r#"
A standalone wire can use this constructor:

```no_run
use up_rust::{UFrameFieldsMetadataCodec, UWire, UWireTransport, WireIdentity};
struct Standalone;
impl UWire for Standalone {
    const WIRE_ID: WireIdentity = WireIdentity::new("example.wire", 0x8123);
    const PAYLOAD_FAMILY_ID: WireIdentity = WireIdentity::new("example.family", 0x8124);
    const FORMAT_VERSION: u16 = 1;
    const REQUIRES_NATIVE_AGREEMENT: bool = false;
}
let _ = UWireTransport::new((), Standalone, UFrameFieldsMetadataCodec);
```

The repository's code-generation probe builds this ordinary constructor with
the requirement false and verifies an E0080 failure when only that requirement
is changed to true. Checking without code generation is not that proof.
"#
    )]
    #[must_use]
    pub fn new(core: TCore, wire: W, metadata_codec: C) -> Self {
        const {
            assert!(
                !W::REQUIRES_NATIVE_AGREEMENT,
                "this wire requires an explicit native agreement constructor"
            );
        }
        Self::build(core, wire, metadata_codec, None)
    }

    /// Constructs a new adapter with an already confirmed native route agreement.
    /// The generation is immutable and is retained by listeners and receive views.
    pub fn with_native_agreement(
        core: TCore,
        wire: W,
        metadata_codec: C,
        agreement: crate::NativeProfileAgreement,
    ) -> Self {
        Self::build(core, wire, metadata_codec, Some(agreement))
    }

    fn build(
        core: TCore,
        wire: W,
        metadata_codec: C,
        native_agreement: Option<crate::NativeProfileAgreement>,
    ) -> Self {
        Self {
            core,
            wire,
            metadata_codec,
            native_agreement,
            #[cfg(feature = "owned-frame-transport")]
            owned_listeners: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the wrapped physical transport core.
    #[must_use]
    pub fn core(&self) -> &TCore {
        &self.core
    }

    /// Returns the wrapped physical transport core mutably.
    #[must_use]
    pub fn core_mut(&mut self) -> &mut TCore {
        &mut self.core
    }

    /// Returns the selected metadata codec.
    #[must_use]
    pub fn metadata_codec(&self) -> &C {
        &self.metadata_codec
    }

    /// Consumes the adapter and returns its core, wire, metadata codec and agreement.
    #[must_use]
    pub fn into_parts(self) -> (TCore, W, C, Option<crate::NativeProfileAgreement>) {
        (
            self.core,
            self.wire,
            self.metadata_codec,
            self.native_agreement,
        )
    }
}

/// Selected-wire transport using the canonical UFrame metadata field block.
///
/// Every selected-wire constructor in this crate uses the canonical field-block
/// codec; no other metadata profile is implemented.
pub type UNativePrefixWireTransport<TCore, W> = UWireTransport<TCore, W, UFrameFieldsMetadataCodec>;

/// Protocol Buffers selected-wire transport with canonical metadata.
pub type ProtobufWireTransport<TCore> = UNativePrefixWireTransport<TCore, ProtobufWire>;

/// Stable-container selected-wire transport with canonical metadata.
#[cfg(feature = "zero-copy-transport")]
pub type StableContainerWireTransport<TCore> =
    UNativePrefixWireTransport<TCore, StableContainerWire>;

/// Convenience constructors for canonical native-prefix selected-wire transports.
pub trait UWithNativePrefixWire: Sized {
    /// Wraps this core with an external or deployment-specific selected wire using canonical metadata.
    #[must_use]
    fn into_native_prefix_wire_transport<W>(self, wire: W) -> UNativePrefixWireTransport<Self, W>
    where
        W: UWire;

    /// Wraps this core with the Protocol Buffers selected-wire profile.
    #[must_use]
    fn into_protobuf_transport(self) -> ProtobufWireTransport<Self>;

    /// Wraps this core with the stable-container selected wire.
    #[must_use]
    #[cfg(feature = "zero-copy-transport")]
    fn into_stable_container_transport(
        self,
        agreement: crate::NativeProfileAgreement,
    ) -> StableContainerWireTransport<Self>;
}

impl<TCore> UWithNativePrefixWire for TCore {
    fn into_native_prefix_wire_transport<W>(self, wire: W) -> UNativePrefixWireTransport<Self, W>
    where
        W: UWire,
    {
        UWireTransport::new(self, wire, UFrameFieldsMetadataCodec)
    }

    fn into_protobuf_transport(self) -> ProtobufWireTransport<Self> {
        self.into_native_prefix_wire_transport(ProtobufWire)
    }

    #[cfg(feature = "zero-copy-transport")]
    fn into_stable_container_transport(
        self,
        agreement: crate::NativeProfileAgreement,
    ) -> StableContainerWireTransport<Self> {
        UWireTransport::with_native_agreement(
            self,
            StableContainerWire,
            UFrameFieldsMetadataCodec,
            agreement,
        )
    }
}

/// Exposes the concrete selected wire of an adapter value.
pub trait UHasWire {
    /// Concrete selected wire type.
    type Wire: UWire;

    /// Returns the selected wire marker/value.
    #[must_use]
    fn wire(&self) -> &Self::Wire;

    /// Returns the explicitly configured native route agreement, if any.
    fn native_agreement(&self) -> Option<&crate::NativeProfileAgreement> {
        None
    }
}

impl<TCore, W, C> UHasWire for UWireTransport<TCore, W, C>
where
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
    type Wire = W;

    fn wire(&self) -> &Self::Wire {
        &self.wire
    }

    fn native_agreement(&self) -> Option<&crate::NativeProfileAgreement> {
        self.native_agreement.as_ref()
    }
}

#[cfg(all(
    test,
    feature = "zero-copy-transport",
    feature = "transport-implementer-api"
))]
mod prepared_tests;

#[cfg(all(test, feature = "owned-frame-transport", feature = "up-core-api"))]
mod tests;
