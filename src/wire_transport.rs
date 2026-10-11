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

#[cfg(all(feature = "zero-copy-transport", feature = "transport-implementer-api"))]
impl<C: crate::PreparedTransportCore, W: UWire>
    UWireTransport<Arc<C>, W, UFrameFieldsMetadataCodec>
{
    /// Prepares exact in-place metadata admission while leaving payload bytes opaque.
    ///
    /// # Errors
    /// Rejects an invalid prefix or token limit. Custom wire alias callbacks are
    /// not applied by this exact-identity admission API.
    pub fn prepare_header_receiver(
        &self,
        limits: crate::HeaderLimits,
    ) -> Result<crate::HeaderAdmission, UWireError> {
        crate::HeaderAdmission::for_wire::<W>(limits)
    }

    /// Prepares the selected-wire prefix for bounded, explicitly copying opaque routes.
    ///
    /// # Errors
    /// Rejects an invalid selected-wire prefix.
    pub fn prepare_opaque_template(&self) -> Result<crate::OpaqueTemplate, crate::SetupError> {
        crate::OpaqueTemplate::for_wire::<W>()
    }

    /// Attaches an exact-identity opaque handler with decoded source/sink filters.
    ///
    /// Each registration reserves one generic worker. Partial filters are retained
    /// at the core boundary and rechecked after in-place decoding; None is sinkless.
    ///
    /// # Errors
    /// Reports prefix/limit, worker reservation or physical subscription failure.
    pub async fn register_header_handler<H: crate::HeaderHandler<C::Rx>>(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        limits: crate::HeaderLimits,
        config: crate::RegistrationConfig,
        handler: H,
    ) -> Result<crate::WireRegistration<C>, crate::SetupError> {
        let admission = self
            .prepare_header_receiver(limits)
            .map_err(|_| crate::SetupError::Prefix)?;
        let worker = admission
            .register_filtered(source.clone(), sink.cloned(), config, handler, || {})
            .map_err(|_| crate::SetupError::Worker)?;
        crate::WireRegistration::attach(Arc::clone(&self.core), source, sink, worker).await
    }
}

#[cfg(all(feature = "zero-copy-transport", feature = "transport-implementer-api"))]
impl<C> UWireTransport<Arc<C>, StableContainerWire, UFrameFieldsMetadataCodec>
where
    C: crate::PreparedTransportCore,
{
    /// Prepares a stable sender's identity and prefix for route creation.
    ///
    /// # Errors
    /// Rejects a missing agreement or an invalid/unagreed payload representation.
    pub fn prepare_stable_template<P: crate::StablePayload>(
        &self,
    ) -> Result<crate::TxTemplate<P>, crate::SetupError> {
        let agreement = self
            .native_agreement
            .clone()
            .ok_or(crate::SetupError::InvalidConfig(
                "native agreement required",
            ))?;
        crate::TxTemplate::new(agreement)
    }

    /// Creates one fixed stable-native route from the core's reserved tables.
    ///
    /// # Errors
    /// Returns representation, route/configuration or table-capacity failures.
    pub fn prepare_stable_route<P: crate::StablePayload>(
        &self,
        definition: crate::RouteDefinition,
        config: crate::RouteConfig,
    ) -> Result<crate::TxRoute<C, P>, crate::SetupError> {
        self.prepare_stable_template()?
            .route(Arc::clone(&self.core), definition, config)
    }

    /// Prepares in-place stable receive admission using this adapter's agreement.
    ///
    /// # Errors
    /// Rejects a missing agreement or incompatible stable representation.
    pub fn prepare_stable_receiver<P: crate::StablePayload>(
        &self,
        limits: crate::HeaderLimits,
    ) -> Result<crate::RxAdmission<P>, UWireError> {
        let agreement = self
            .native_agreement
            .clone()
            .ok_or_else(|| UWireError::invalid_payload("native agreement required"))?;
        crate::RxAdmission::new(agreement, limits)
    }

    /// Attaches one typed worker with retained agreement and decoded URI filters.
    ///
    /// # Errors
    /// Reports unagreed representation, worker setup or physical subscription failure.
    pub async fn register_stable_handler<P, H>(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        limits: crate::HeaderLimits,
        config: crate::RegistrationConfig,
        handler: H,
    ) -> Result<crate::WireRegistration<C>, crate::SetupError>
    where
        P: crate::StablePayload,
        H: crate::ZeroCopyHandler<P, C::Rx>,
    {
        let admission = self
            .prepare_stable_receiver::<P>(limits)
            .map_err(|_| crate::SetupError::UnknownPayloadType)?;
        let worker = admission
            .register_filtered(source.clone(), sink.cloned(), config, handler, || {})
            .map_err(|_| crate::SetupError::Worker)?;
        crate::WireRegistration::attach(Arc::clone(&self.core), source, sink, worker).await
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

/// Raw encoded receive object returned by a transport core.
///
/// This is an implementation-boundary trait. Raw encoded receive objects should
/// not implement public frame or lease traits directly; public receive paths
/// expose [`UWireRx<Rx, W, C>`] after selected-wire metadata decode and validation.
///
/// The object must retain all resources backing both metadata and payload views
/// for its lifetime, including after listener unregistration or core destruction.
/// Retaining a sample-handle allocation without its native proxy/mapping owner
/// does not satisfy this contract.
pub trait UEncodedRxFrame {
    /// Ordered payload reader type.
    type PayloadReader<'a>: Read + 'a
    where
        Self: 'a;
    /// Ordered payload slices iterator type.
    type PayloadSlices<'a>: Iterator<Item = &'a [u8]> + 'a
    where
        Self: 'a;

    /// Returns selected-wire encoded metadata bytes.
    fn encoded_metadata(&self) -> &[u8];

    /// Returns the visible application payload length.
    fn payload_len(&self) -> usize;

    /// Returns an ordered reader over the application payload bytes.
    fn payload_reader(&self) -> Self::PayloadReader<'_>;

    /// Returns ordered borrowed payload slices.
    fn payload_slices(&self) -> Self::PayloadSlices<'_>;

    /// Returns a contiguous borrowed payload view when available without copying.
    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        None
    }
}

#[cfg(feature = "zero-copy-transport")]
/// Raw encoded receive object that can prove its contiguous payload is loan-backed.
pub trait UEncodedLoanedRxFrame: UEncodedRxFrame {
    /// Returns one contiguous loan-backed application payload view.
    ///
    /// Implementations must not allocate, copy, or coalesce payload bytes to
    /// satisfy this method.
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError>;
}

/// Public zero-copy receive lease after selected-wire metadata validation.
pub struct UWireRx<Rx, W, C>
where
    W: UWire,
{
    metadata: UFrameMetadata,
    native_agreement: Option<crate::NativeProfileAgreement>,
    raw: Rx,
    _wire: PhantomData<W>,
    _metadata_codec: PhantomData<C>,
}

impl<Rx, W, C> core::fmt::Debug for UWireRx<Rx, W, C>
where
    W: UWire,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UWireRx").finish_non_exhaustive()
    }
}

impl<Rx, W, C> UWireRx<Rx, W, C>
where
    Rx: UEncodedRxFrame,
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
    /// Decodes metadata from a raw encoded receive object and validates the public frame view.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata bytes are malformed, selected-wire checks
    /// fail, or the resulting public frame view violates transport invariants.
    pub fn try_from_encoded(raw: Rx, codec: &C) -> Result<Self, UStatus> {
        Self::from_encoded_in_context(raw, codec, None)
    }

    /// Decodes a view while retaining the confirmed agreement for its route.
    /// Payload bytes remain opaque until an explicit typed operation is requested.
    ///
    /// # Errors
    ///
    /// Returns the same framing/metadata errors as [`Self::try_from_encoded`].
    pub fn try_from_encoded_with_native_agreement(
        raw: Rx,
        codec: &C,
        agreement: crate::NativeProfileAgreement,
    ) -> Result<Self, UStatus> {
        Self::from_encoded_in_context(raw, codec, Some(agreement))
    }

    fn from_encoded_in_context(
        raw: Rx,
        codec: &C,
        native_agreement: Option<crate::NativeProfileAgreement>,
    ) -> Result<Self, UStatus> {
        let metadata = codec.decode_frame_metadata(
            UWireMetadataContext::from_wire::<W, C>(),
            raw.encoded_metadata(),
        )?;
        let frame = Self {
            metadata,
            native_agreement,
            raw,
            _wire: PhantomData,
            _metadata_codec: PhantomData,
        };
        validate_frame_view_for_transport(&frame)?;
        Ok(frame)
    }

    /// Returns the raw encoded receive object behind this public wrapper.
    #[must_use]
    pub fn raw(&self) -> &Rx {
        &self.raw
    }

    /// Consumes the public wrapper and returns the raw encoded receive object.
    #[must_use]
    pub fn into_raw(self) -> Rx {
        self.raw
    }

    /// Decodes this frame's payload using the selected wire `W`.
    ///
    /// Prefer this selected-wire helper on receive values produced by
    /// explicit selected-wire adapter construction. Use
    /// [`UFrameView::decode_payload_from_reader_as`] only for
    /// explicit low-level codec selection.
    ///
    /// # Errors
    ///
    /// Returns an error if the frame has missing or incompatible payload encoding,
    /// has no payload, or if the selected wire cannot decode the payload bytes.
    pub fn decode_payload<T>(&self, limit: PayloadDecodeLimit) -> Result<T, UWireError>
    where
        W: UWirePayload<T>,
        <W as UWirePayload<T>>::Codec: ReadDecodePayload<T>,
    {
        <<W as UWirePayload<T>>::Codec as crate::payload::codec::PayloadCodec>::verify_metadata(
            &self.metadata,
            self.native_agreement.as_ref(),
        )?;
        if !self.has_payload() {
            return Err(UWireError::MissingPayload);
        }
        <W as UWirePayload<T>>::Codec::decode_payload_from_reader(
            self.payload_reader(),
            self.payload_len(),
            limit,
        )
    }
}

#[cfg(feature = "zero-copy-transport")]
impl<Rx, W, C> UWireRx<Rx, W, C>
where
    Rx: UEncodedLoanedRxFrame,
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
    /// Borrows this frame's payload through the selected wire mapping.
    ///
    /// # Errors
    ///
    /// Returns an error for missing or incompatible encoding, absent/non-loaned
    /// payload storage, invalid size/alignment or invalid field bits.
    pub fn borrow_payload<T>(&self) -> Result<&T, UWireError>
    where
        W: UWirePayload<T>,
        <W as UWirePayload<T>>::Codec: BorrowPayload<T>,
    {
        <<W as UWirePayload<T>>::Codec as crate::PayloadCodec>::verify_metadata(
            &self.metadata,
            self.native_agreement.as_ref(),
        )?;
        if !self.has_payload() {
            return Err(UWireError::MissingPayload);
        }
        let payload = self.raw.loaned_contiguous_payload()?;
        <<W as UWirePayload<T>>::Codec as BorrowPayload<T>>::borrow_payload(payload.bytes())
    }

    /// Borrows this frame's payload through the selected wire's expert lane.
    ///
    /// Encoding, payload presence and loan-backed contiguity remain checked.
    /// `unchecked` permits but does not guarantee bit-validation elision.
    ///
    /// # Safety
    ///
    /// The caller must prove closed producer provenance, typed construction of
    /// the exact requested Rust representation and recursive bit validity for
    /// `T`. Wire/profile identity, size, alignment and loan-backed contiguity
    /// remain checked; only the recursive bit validator may be elided.
    pub unsafe fn borrow_payload_unchecked<T>(&self) -> Result<&T, UWireError>
    where
        W: UWirePayload<T>,
        <W as UWirePayload<T>>::Codec: BorrowPayload<T>,
    {
        <<W as UWirePayload<T>>::Codec as crate::PayloadCodec>::verify_metadata(
            &self.metadata,
            self.native_agreement.as_ref(),
        )?;
        if !self.has_payload() {
            return Err(UWireError::MissingPayload);
        }
        let payload = self.raw.loaned_contiguous_payload()?;
        // SAFETY: This function's caller must establish closed producer
        // provenance, exact typed construction and recursive validity for T.
        // Metadata, presence and loan-backed contiguity were checked above;
        // the selected codec retains its size/alignment checks.
        unsafe {
            <<W as UWirePayload<T>>::Codec as BorrowPayload<T>>::borrow_payload_unchecked(
                payload.bytes(),
            )
        }
    }
}

impl<Rx, W, C> UFrameView for UWireRx<Rx, W, C>
where
    Rx: UEncodedRxFrame,
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
    type PayloadReader<'a>
        = Rx::PayloadReader<'a>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = Rx::PayloadSlices<'a>
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn native_agreement(&self) -> Option<&crate::NativeProfileAgreement> {
        self.native_agreement.as_ref()
    }

    fn payload_len(&self) -> usize {
        self.raw.payload_len()
    }

    fn has_payload(&self) -> bool {
        self.payload_len() > 0 || self.metadata.payload_encoding().is_some()
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        self.raw.payload_reader()
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        self.raw.payload_slices()
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        self.raw.try_contiguous_payload()
    }
}

#[cfg(feature = "zero-copy-transport")]
impl<Rx, W, C> UZeroCopyRxLease for UWireRx<Rx, W, C>
where
    Rx: UEncodedRxFrame,
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
}

#[cfg(feature = "zero-copy-transport")]
impl<Rx, C> ULoanedContiguousZeroCopyRxFrame for UWireRx<Rx, StableContainerWire, C>
where
    Rx: UEncodedLoanedRxFrame,
    C: UWireMetadataCodecFor<StableContainerWire>,
{
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError> {
        self.raw.loaned_contiguous_payload()
    }
}

#[cfg(feature = "owned-frame-transport")]
fn selected_wire_metadata_matches(
    metadata: &UFrameMetadata,
    source: &UUri,
    sink: Option<&UUri>,
) -> bool {
    source.matches(metadata.source())
        && match (sink, metadata.sink()) {
            (None, None) => true,
            (Some(filter), Some(actual)) => filter.matches(actual),
            _ => false,
        }
}

#[cfg(feature = "owned-frame-transport")]
fn selected_wire_core_source_filter_for(source_filter: &UUri) -> UUri {
    // Preserve useful authority/entity scope even in a partial wildcard. The
    // binding chooses any necessary physical broadening; decoded metadata is
    // still filtered independently before public delivery.
    source_filter.clone()
}

#[cfg(feature = "owned-frame-transport")]
/// Prepared owned frame passed from the adapter to an owned core.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedOwnedFrame {
    metadata: UFrameMetadata,
    encoded_metadata: Vec<u8>,
    payload: Option<Bytes>,
}

#[cfg(feature = "owned-frame-transport")]
impl PreparedOwnedFrame {
    /// Encodes validated owned frame metadata for a selected wire.
    ///
    /// # Errors
    ///
    /// Returns an error if selected-wire metadata encoding fails.
    pub fn from_validated<W, C>(frame: UOwnedFrame, codec: &C) -> Result<Self, UStatus>
    where
        W: UWire,
        C: UWireMetadataCodecFor<W>,
    {
        let (metadata, payload) = frame.into_parts();
        let encoded_metadata =
            codec.encode_frame_metadata(UWireMetadataContext::from_wire::<W, C>(), &metadata)?;
        Ok(Self {
            metadata,
            encoded_metadata,
            payload,
        })
    }

    /// Returns decoded frame metadata.
    #[must_use]
    pub fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    /// Returns selected-wire encoded metadata bytes.
    #[must_use]
    pub fn encoded_metadata(&self) -> &[u8] {
        &self.encoded_metadata
    }

    /// Returns owned payload bytes, if present.
    #[must_use]
    pub fn payload(&self) -> Option<&Bytes> {
        self.payload.as_ref()
    }

    /// Consumes this frame and returns its parts.
    #[must_use]
    pub fn into_parts(self) -> (UFrameMetadata, Vec<u8>, Option<Bytes>) {
        (self.metadata, self.encoded_metadata, self.payload)
    }
}

#[cfg(feature = "owned-frame-transport")]
/// Encoded owned frame returned by an owned core.
#[derive(Clone, Debug, PartialEq)]
pub struct EncodedOwnedFrame {
    encoded_metadata: Vec<u8>,
    payload: Option<Bytes>,
}

#[cfg(feature = "owned-frame-transport")]
impl EncodedOwnedFrame {
    /// Creates an encoded owned frame from selected-wire metadata bytes and payload.
    #[must_use]
    pub fn new(encoded_metadata: impl Into<Vec<u8>>, payload: Option<Bytes>) -> Self {
        Self {
            encoded_metadata: encoded_metadata.into(),
            payload,
        }
    }

    /// Returns selected-wire encoded metadata bytes.
    #[must_use]
    pub fn encoded_metadata(&self) -> &[u8] {
        &self.encoded_metadata
    }

    /// Returns owned payload bytes, if present.
    #[must_use]
    pub fn payload(&self) -> Option<&Bytes> {
        self.payload.as_ref()
    }

    /// Decodes this raw frame into a public owned frame.
    ///
    /// # Errors
    ///
    /// Returns an error if selected-wire metadata decode or owned-frame validation fails.
    pub fn decode<W, C>(self, codec: &C) -> Result<UOwnedFrame, UStatus>
    where
        W: UWire,
        C: UWireMetadataCodecFor<W>,
    {
        let metadata = codec.decode_frame_metadata(
            UWireMetadataContext::from_wire::<W, C>(),
            &self.encoded_metadata,
        )?;
        UOwnedFrame::new(metadata, self.payload).map_err(invalid_metadata)
    }

    /// Consumes this frame and returns its parts.
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, Option<Bytes>) {
        (self.encoded_metadata, self.payload)
    }
}

#[cfg(feature = "owned-frame-transport")]
/// Listener used by cores to deliver raw encoded owned frames.
#[async_trait]
pub trait UEncodedOwnedListener: Send + Sync {
    /// Handles one raw encoded owned frame.
    async fn on_receive_encoded_owned(&self, frame: EncodedOwnedFrame);
}

/// *Role: implemented by transports carrying already-encoded owned frames; the wire adapter composes above it — see the [trait map](crate::guide::trait_map).*
///
#[cfg(feature = "owned-frame-transport")]
/// Encoded physical owned-frame mechanics implemented by product transports.
///
/// Implementing this core buys selected-wire metadata encoding/decoding,
/// identity rejection, semantic frame validation, and the public owned-frame
/// transport API from [`UWireTransport`]. The required send receives metadata
/// already encoded for the selected profile. Pull receive and listener hooks
/// default to unsupported.
#[async_trait]
pub trait UOwnedTransportCore: Send + Sync {
    /// Sends an owned frame with already encoded metadata bytes.
    async fn send_prepared_owned(&self, frame: PreparedOwnedFrame) -> Result<(), UStatus>;

    /// Receives one matching raw encoded owned frame from cores that support pull receive.
    async fn receive_encoded_owned(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<EncodedOwnedFrame, UStatus> {
        Err(unimplemented())
    }

    /// Registers a raw encoded owned listener after public filter validation.
    async fn register_encoded_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UEncodedOwnedListener>,
    ) -> Result<(), UStatus> {
        Err(unimplemented())
    }

    /// Unregisters a raw encoded owned listener after public filter validation.
    async fn unregister_encoded_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UEncodedOwnedListener>,
    ) -> Result<(), UStatus> {
        Err(unimplemented())
    }
}

#[cfg(feature = "owned-frame-transport")]
#[async_trait]
impl<TCore, W, C> UOwnedTransportImpl for UWireTransport<TCore, W, C>
where
    TCore: UOwnedTransportCore,
    W: UWire + Send + Sync + 'static,
    C: UWireMetadataCodecFor<W> + Clone + Send + Sync + 'static,
{
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.core
            .send_prepared_owned(PreparedOwnedFrame::from_validated::<W, C>(
                frame,
                &self.metadata_codec,
            )?)
            .await
    }

    async fn receive_validated_owned(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        let core_source_filter = selected_wire_core_source_filter_for(source_filter);
        loop {
            let frame = self
                .core
                .receive_encoded_owned(&core_source_filter, sink_filter)
                .await?
                .decode::<W, C>(&self.metadata_codec)?;
            if owned_frame_matches(&frame, source_filter, sink_filter) {
                return Ok(frame);
            }
        }
    }

    async fn register_validated_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        let key = listener_key(
            source_filter,
            sink_filter,
            owned_listener_pointer(&listener),
        );
        let (listener, inserted) =
            self.registered_owned_listener(&key, source_filter, sink_filter, listener);
        let core_source_filter = selected_wire_core_source_filter_for(source_filter);
        let result = self
            .core
            .register_encoded_owned_listener(&core_source_filter, sink_filter, listener)
            .await;
        if result.is_err() && inserted {
            self.owned_listeners
                .lock()
                .expect("wire owned listener registry lock poisoned")
                .remove(&key);
        }
        result
    }

    async fn unregister_validated_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        let key = listener_key(
            source_filter,
            sink_filter,
            owned_listener_pointer(&listener),
        );
        let listener = self.owned_listener_for_unregister(&key, listener);
        let core_source_filter = selected_wire_core_source_filter_for(source_filter);
        let result = self
            .core
            .unregister_encoded_owned_listener(&core_source_filter, sink_filter, listener)
            .await;
        if result.is_ok() {
            self.owned_listeners
                .lock()
                .expect("wire owned listener registry lock poisoned")
                .remove(&key);
        }
        result
    }
}

#[cfg(feature = "owned-frame-transport")]
impl<TCore, W, C> UWireTransport<TCore, W, C>
where
    TCore: UOwnedTransportCore,
    W: UWire + Send + Sync + 'static,
    C: UWireMetadataCodecFor<W> + Clone + Send + Sync + 'static,
{
    fn registered_owned_listener(
        &self,
        key: &WireListenerKey,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> (Arc<dyn UEncodedOwnedListener>, bool) {
        let mut registry = self
            .owned_listeners
            .lock()
            .expect("wire owned listener registry lock poisoned");
        if let Some(existing) = registry.get(key) {
            if let Ok(existing) = existing.clone().downcast::<WireOwnedListener<W, C>>() {
                return (existing, false);
            }
        }

        let wrapped = Arc::new(WireOwnedListener::<W, C> {
            source_filter: source_filter.clone(),
            sink_filter: sink_filter.cloned(),
            listener,
            metadata_codec: self.metadata_codec.clone(),
            _wire: PhantomData,
        });
        registry.insert(key.clone(), wrapped.clone());
        (wrapped, true)
    }

    fn owned_listener_for_unregister(
        &self,
        key: &WireListenerKey,
        fallback: Arc<dyn UOwnedListener>,
    ) -> Arc<dyn UEncodedOwnedListener> {
        self.owned_listeners
            .lock()
            .expect("wire owned listener registry lock poisoned")
            .get(key)
            .and_then(|listener| listener.clone().downcast::<WireOwnedListener<W, C>>().ok())
            .unwrap_or_else(|| {
                Arc::new(WireOwnedListener::<W, C> {
                    source_filter: key.source_filter.clone(),
                    sink_filter: key.sink_filter.clone(),
                    listener: fallback,
                    metadata_codec: self.metadata_codec.clone(),
                    _wire: PhantomData,
                })
            })
    }
}

#[cfg(feature = "owned-frame-transport")]
struct WireOwnedListener<W, C>
where
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: Arc<dyn UOwnedListener>,
    metadata_codec: C,
    _wire: PhantomData<W>,
}

#[cfg(feature = "owned-frame-transport")]
#[async_trait]
impl<W, C> UEncodedOwnedListener for WireOwnedListener<W, C>
where
    W: UWire + Send + Sync + 'static,
    C: UWireMetadataCodecFor<W> + Send + Sync + 'static,
{
    async fn on_receive_encoded_owned(&self, frame: EncodedOwnedFrame) {
        match frame.decode::<W, C>(&self.metadata_codec) {
            Ok(frame)
                if owned_frame_matches(&frame, &self.source_filter, self.sink_filter.as_ref()) =>
            {
                self.listener.on_receive_owned(frame).await;
            }
            Ok(_) => {}
            Err(error) => warn!(%error, "dropping invalid selected-wire owned frame"),
        }
    }
}

#[cfg(feature = "owned-frame-transport")]
fn owned_frame_matches(
    frame: &UOwnedFrame,
    source_filter: &UUri,
    sink_filter: Option<&UUri>,
) -> bool {
    selected_wire_metadata_matches(frame.metadata(), source_filter, sink_filter)
}

#[cfg(feature = "owned-frame-transport")]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct WireListenerKey {
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: usize,
}

#[cfg(feature = "owned-frame-transport")]
fn listener_key(
    source_filter: &UUri,
    sink_filter: Option<&UUri>,
    listener: usize,
) -> WireListenerKey {
    WireListenerKey {
        source_filter: source_filter.clone(),
        sink_filter: sink_filter.cloned(),
        listener,
    }
}

#[cfg(feature = "owned-frame-transport")]
fn owned_listener_pointer(listener: &Arc<dyn UOwnedListener>) -> usize {
    let ptr = Arc::as_ptr(listener);
    let thin_ptr = ptr as *const ();
    thin_ptr as usize
}

#[cfg(feature = "owned-frame-transport")]
fn unimplemented() -> UStatus {
    UStatus::fail_with_code(UCode::Unimplemented, "not implemented")
}

#[cfg(feature = "owned-frame-transport")]
fn invalid_metadata(error: crate::UFrameMetadataError) -> UStatus {
    UStatus::fail_with_code(UCode::InvalidArgument, error.to_string())
}

#[cfg(all(
    test,
    feature = "zero-copy-transport",
    feature = "transport-implementer-api"
))]
mod prepared_tests;

#[cfg(all(test, feature = "owned-frame-transport", feature = "up-core-api"))]
mod tests {
    use crate::test_support::{RawRx, RecordingCore};
    #[cfg(feature = "zero-copy-transport")]
    use std::io::Cursor;
    use std::sync::Mutex as StdMutex;

    use protobuf::well_known_types::wrappers::StringValue;

    use super::*;
    use crate::wire::{UProtocolNativeWire, UWireMetadataCodec};
    #[cfg(feature = "zero-copy-transport")]
    use crate::{
        BorrowPayload, PayloadLoanProvenance, StableContainerPayload, StablePayloadField,
        UVecRxLease,
    };
    #[cfg(feature = "protobuf-support")]
    use crate::{EncodePayload, ProtobufPayload};
    use crate::{PayloadEncoding, UMessageBuilder};
    use test_case::test_case;

    #[cfg(feature = "zero-copy-transport")]
    #[repr(C)]
    #[derive(Debug, Eq, PartialEq, crate::StablePayload, crate::StablePayloadInit)]
    #[stable_payload(type_name = "uprotocol.test.WireStableBytes")]
    struct WireStableBytes {
        bytes: [u8; 4],
    }

    #[cfg(feature = "zero-copy-transport")]
    #[repr(C)]
    #[derive(Debug, Eq, PartialEq, crate::StablePayload)]
    #[stable_payload(type_name = "uprotocol.test.WireBool")]
    struct WireBool {
        value: bool,
    }

    #[cfg(feature = "zero-copy-transport")]
    #[repr(C)]
    #[derive(Debug, Eq, PartialEq, crate::StablePayload)]
    #[stable_payload(type_name = "uprotocol.test.WireNestedInner")]
    struct WireNestedInner {
        value: bool,
    }

    #[cfg(feature = "zero-copy-transport")]
    #[repr(C)]
    #[derive(Debug, Eq, PartialEq, crate::StablePayload)]
    #[stable_payload(type_name = "uprotocol.test.WireNested")]
    struct WireNested {
        inner: WireNestedInner,
        padding: [u8; 3],
        letter: char,
    }

    #[cfg(feature = "zero-copy-transport")]
    #[repr(C)]
    #[derive(Debug, Eq, PartialEq, crate::StablePayload)]
    #[stable_payload(type_name = "uprotocol.test.WireAligned")]
    struct WireAligned {
        value: u32,
    }

    #[cfg(feature = "zero-copy-transport")]
    #[derive(Debug)]
    struct CheckedDefaultCodec;

    #[cfg(feature = "zero-copy-transport")]
    impl crate::PayloadCodecIdentity for CheckedDefaultCodec {
        fn name() -> &'static str {
            "checked-default-test"
        }

        fn encoding() -> PayloadEncoding {
            PayloadEncoding::RAW
        }
    }

    // SAFETY: Both lanes check exact length and the bool bit pattern before the
    // only cast; the unchecked lane is a test double that deliberately keeps
    // the checks.
    #[cfg(feature = "zero-copy-transport")]
    unsafe impl BorrowPayload<WireBool> for CheckedDefaultCodec {
        fn borrow_payload(src: &[u8]) -> Result<&WireBool, UWireError> {
            if src.len() != core::mem::size_of::<WireBool>() || !WireBool::validate_field_bytes(src)
            {
                return Err(UWireError::invalid_payload("invalid checked-default bool"));
            }
            // SAFETY: WireBool has alignment one and its bool byte was checked.
            Ok(unsafe { &*src.as_ptr().cast::<WireBool>() })
        }
    }

    #[cfg(feature = "zero-copy-transport")]
    #[repr(align(8))]
    struct AlignedPayloadStorage([u8; 5]);

    #[cfg(feature = "zero-copy-transport")]
    struct MisalignedRawRx {
        encoded_metadata: Vec<u8>,
        storage: AlignedPayloadStorage,
    }

    #[cfg(feature = "zero-copy-transport")]
    impl MisalignedRawRx {
        fn payload(&self) -> &[u8] {
            &self.storage.0[1..]
        }
    }

    #[cfg(feature = "zero-copy-transport")]
    impl UEncodedRxFrame for MisalignedRawRx {
        type PayloadReader<'a>
            = Cursor<&'a [u8]>
        where
            Self: 'a;
        type PayloadSlices<'a>
            = std::iter::Once<&'a [u8]>
        where
            Self: 'a;

        fn encoded_metadata(&self) -> &[u8] {
            &self.encoded_metadata
        }

        fn payload_len(&self) -> usize {
            self.payload().len()
        }

        fn payload_reader(&self) -> Self::PayloadReader<'_> {
            Cursor::new(self.payload())
        }

        fn payload_slices(&self) -> Self::PayloadSlices<'_> {
            std::iter::once(self.payload())
        }

        fn try_contiguous_payload(&self) -> Option<&[u8]> {
            Some(self.payload())
        }
    }

    #[cfg(feature = "zero-copy-transport")]
    impl UEncodedLoanedRxFrame for MisalignedRawRx {
        fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError> {
            // The payload is borrowed directly from this raw receive object.
            Ok(LoanedPayload::new(
                self.payload(),
                PayloadLoanProvenance::OpaqueTransportLoan,
            ))
        }
    }

    #[cfg(feature = "owned-frame-transport")]
    #[derive(Default)]
    struct RecordingOwnedListener(StdMutex<Vec<UOwnedFrame>>);

    #[cfg(feature = "owned-frame-transport")]
    #[async_trait]
    impl UOwnedListener for RecordingOwnedListener {
        async fn on_receive_owned(&self, frame: UOwnedFrame) {
            self.0.lock().unwrap().push(frame);
        }
    }

    #[cfg(feature = "owned-frame-transport")]
    #[test_case(5, 1, "unknown metadata layout"; "layout")]
    #[test_case(10, 2, "wrong selected wire"; "wire")]
    #[test_case(13, 2, "payload family mismatch"; "family")]
    #[tokio::test]
    async fn owned_receive_and_listener_reject_identity_mismatch(
        offset: usize,
        value: u8,
        message: &str,
    ) {
        let mut raw = raw_frame_for_topic(0x9000, b"must-not-be-exposed");
        *raw.encoded_metadata.get_mut(offset).unwrap() = value;
        let core = RecordingCore::default();
        core.received.lock().unwrap().push_back(raw.clone());
        let transport = core.into_native_prefix_wire_transport(UProtocolNativeWire);
        let topic = crate::test_support::topic(0x9000);
        let error = transport
            .receive_validated_owned(&topic, None)
            .await
            .unwrap_err();
        assert_eq!(error.code(), UCode::InvalidArgument);
        assert!(error
            .message()
            .is_some_and(|detail| detail.contains(message)));
        let listener = Arc::new(RecordingOwnedListener::default());
        transport
            .register_validated_owned_listener(&topic, None, listener.clone())
            .await
            .unwrap();
        let registered = transport
            .core()
            .owned_listeners
            .lock()
            .unwrap()
            .first()
            .unwrap()
            .clone();
        registered
            .on_receive_encoded_owned(EncodedOwnedFrame::new(
                raw.encoded_metadata,
                Some(raw.payload.into()),
            ))
            .await;
        assert!(listener.0.lock().unwrap().is_empty());
    }

    #[cfg(feature = "owned-frame-transport")]
    #[tokio::test]
    async fn sinkless_owned_listener_does_not_duplicate_notifications() {
        use crate::UOwnedTransport;
        let transport = RecordingCore::default().into_protobuf_transport();
        let source = UUri::try_from_parts("source", 0x4210, 1, 0x8001).unwrap();
        let sink = UUri::try_from_parts("sink", 0x4210, 1, 0).unwrap();
        let publishes = Arc::new(RecordingOwnedListener::default());
        let notifications = Arc::new(RecordingOwnedListener::default());
        transport
            .register_owned_listener(&source, None, publishes.clone())
            .await
            .unwrap();
        transport
            .register_owned_listener(&source, Some(&sink), notifications.clone())
            .await
            .unwrap();
        let metadata = UFrameMetadata::notification(source, sink)
            .with_payload_encoding(PayloadEncoding::PROTOBUF)
            .build()
            .unwrap();
        let encoded = UFrameFieldsMetadataCodec
            .encode_frame_metadata(
                UWireMetadataContext::from_wire::<ProtobufWire, UFrameFieldsMetadataCodec>(),
                &metadata,
            )
            .unwrap();
        let listeners = transport.core().owned_listeners.lock().unwrap().clone();
        for listener in listeners {
            listener
                .on_receive_encoded_owned(EncodedOwnedFrame::new(
                    encoded.clone(),
                    Some(Bytes::from_static(b"notify")),
                ))
                .await;
        }
        assert!(
            publishes.0.lock().unwrap().is_empty(),
            "None means no sink, not any sink"
        );
        assert_eq!(notifications.0.lock().unwrap().len(), 1);
    }

    struct CompileCore;

    #[cfg(feature = "owned-frame-transport")]
    #[async_trait]
    impl UOwnedTransportCore for CompileCore {
        async fn send_prepared_owned(&self, _frame: PreparedOwnedFrame) -> Result<(), UStatus> {
            Ok(())
        }
    }

    fn metadata_with_payload() -> UFrameMetadata {
        metadata_with_payload_encoding(PayloadEncoding::RAW)
    }

    fn metadata_with_payload_encoding(payload_encoding: PayloadEncoding) -> UFrameMetadata {
        metadata_with_topic_and_payload_encoding(0x9000, payload_encoding)
    }

    #[cfg(feature = "zero-copy-transport")]
    fn stable_metadata<T: crate::StablePayload>() -> UFrameMetadata {
        let identity = StableContainerPayload::<T>::identity(&stable_profile()).unwrap();
        metadata_with_payload_encoding(identity.encoding())
            .with_native_payload_identity(identity)
            .unwrap()
    }

    #[cfg(feature = "zero-copy-transport")]
    fn stable_profile() -> crate::NativeProfileAgreement {
        use crate::{NativeProfile, NativeProfileMode, NativeProfileTable, StablePayload};
        static AGREEMENT: std::sync::OnceLock<crate::NativeProfileAgreement> =
            std::sync::OnceLock::new();
        AGREEMENT
            .get_or_init(|| {
                let entries = [
                    (0xF101, WireStableBytes::native_representation()),
                    (0xF102, WireBool::native_representation()),
                    (0xF103, WireNested::native_representation()),
                    (0xF104, WireAligned::native_representation()),
                ]
                .map(|(id, representation)| {
                    (PayloadEncoding::from_id(id).unwrap(), representation)
                });
                let profile = NativeProfile::new(
                    "wire-tests",
                    1,
                    NativeProfileMode::Table(NativeProfileTable::new(entries).unwrap()),
                )
                .unwrap();
                crate::test_support::self_agreement(profile)
            })
            .clone()
    }

    #[cfg(feature = "zero-copy-transport")]
    fn stable_rx<R: UEncodedRxFrame>(
        raw: R,
    ) -> UWireRx<R, StableContainerWire, UFrameFieldsMetadataCodec> {
        UWireRx::try_from_encoded_with_native_agreement(
            raw,
            &UFrameFieldsMetadataCodec,
            stable_profile(),
        )
        .unwrap()
    }

    fn metadata_with_topic_and_payload_encoding(
        resource_id: u16,
        payload_encoding: PayloadEncoding,
    ) -> UFrameMetadata {
        let topic = crate::test_support::topic(resource_id);
        let message = UMessageBuilder::publish(topic)
            .build_with_payload(Vec::new(), PayloadEncoding::RAW)
            .expect("message")
            .with_declared_payload_encoding(payload_encoding)
            .expect("carried encoding");
        crate::frame::metadata::try_project_umessage_to_frame_metadata(&message).expect("metadata")
    }

    fn raw_frame_for_topic(resource_id: u16, payload: &[u8]) -> RawRx {
        let metadata = metadata_with_topic_and_payload_encoding(resource_id, PayloadEncoding::RAW);
        RawRx {
            encoded_metadata: encode_metadata::<UProtocolNativeWire>(&metadata),
            payload: payload.to_vec(),
        }
    }

    fn encode_metadata<W>(metadata: &UFrameMetadata) -> Vec<u8>
    where
        W: UWire,
    {
        UFrameFieldsMetadataCodec
            .encode_frame_metadata(
                UWireMetadataContext::from_wire::<W, UFrameFieldsMetadataCodec>(),
                metadata,
            )
            .unwrap()
    }

    #[test]
    fn wire_rx_decodes_metadata_and_delegates_payload() {
        let metadata = metadata_with_payload();
        let encoded_metadata = encode_metadata::<UProtocolNativeWire>(&metadata);
        let raw = RawRx {
            encoded_metadata,
            payload: b"abc".to_vec(),
        };

        let rx =
            UWireRx::<RawRx, UProtocolNativeWire, UFrameFieldsMetadataCodec>::try_from_encoded(
                raw,
                &UFrameFieldsMetadataCodec,
            )
            .unwrap();

        assert_eq!(rx.metadata(), &metadata);
        assert_eq!(rx.payload_len(), 3);
        assert_eq!(rx.try_contiguous_payload(), Some(&b"abc"[..]));
    }

    #[test]
    fn wire_rx_decodes_payload_with_selected_wire() {
        let value = StringValue {
            value: "selected-wire".to_string(),
            special_fields: Default::default(),
        };
        let payload = ProtobufWire::encode_payload_owned(&value).unwrap();
        let metadata = metadata_with_payload_encoding(ProtobufPayload::encoding());
        let encoded_metadata = encode_metadata::<ProtobufWire>(&metadata);
        let raw = RawRx {
            encoded_metadata,
            payload: payload.to_vec(),
        };

        let rx = UWireRx::<RawRx, ProtobufWire, UFrameFieldsMetadataCodec>::try_from_encoded(
            raw,
            &UFrameFieldsMetadataCodec,
        )
        .unwrap();
        let decoded: StringValue = rx.decode_payload(PayloadDecodeLimit::new(1024)).unwrap();

        assert_eq!(decoded.value, "selected-wire");
    }

    #[cfg(feature = "zero-copy-transport")]
    #[test]
    fn selected_stable_wire_safe_borrow_validates_encoding_and_bits() {
        let value = WireBool { value: true };
        let metadata = stable_metadata::<WireBool>();
        let valid = RawRx {
            encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
            payload: vec![1],
        };
        let valid = stable_rx(valid);
        assert_eq!(valid.borrow_payload::<WireBool>().unwrap(), &value);

        let invalid = RawRx {
            encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
            payload: vec![2],
        };
        let invalid = stable_rx(invalid);
        assert!(
            matches!(invalid.borrow_payload::<WireBool>(), Err(UWireError::InvalidPayload(reason)) if reason.contains("not valid"))
        );

        let wrong_metadata = metadata_with_payload_encoding(PayloadEncoding::RAW);
        let wrong = RawRx {
            encoded_metadata: encode_metadata::<StableContainerWire>(&wrong_metadata),
            payload: vec![1],
        };
        let wrong = stable_rx(wrong);
        assert!(matches!(
            wrong.borrow_payload::<WireBool>(),
            Err(UWireError::UnsupportedEncoding { .. })
        ));
    }

    #[cfg(feature = "zero-copy-transport")]
    #[test]
    fn selected_stable_wire_rejects_nested_bool_and_char_bits() {
        let metadata = stable_metadata::<WireNested>();
        let mut nested_bool = vec![0; core::mem::size_of::<WireNested>()];
        let bool_offset = core::mem::offset_of!(WireNested, inner)
            + core::mem::offset_of!(WireNestedInner, value);
        *nested_bool.get_mut(bool_offset).unwrap() = 2;
        let raw = crate::test_support::AlignedRawFrame::new(
            metadata.clone(),
            encode_metadata::<StableContainerWire>(&metadata),
            &nested_bool,
            core::mem::align_of::<WireNested>(),
        );
        let rx = stable_rx(raw);
        assert!(
            matches!(rx.borrow_payload::<WireNested>(), Err(UWireError::InvalidPayload(reason)) if reason.contains("not valid"))
        );

        let mut invalid_char = vec![0; core::mem::size_of::<WireNested>()];
        *invalid_char.get_mut(bool_offset).unwrap() = 1;
        let char_offset = core::mem::offset_of!(WireNested, letter);
        invalid_char
            .get_mut(char_offset..char_offset + core::mem::size_of::<char>())
            .unwrap()
            .copy_from_slice(&0x0011_0000_u32.to_ne_bytes());
        let raw = crate::test_support::AlignedRawFrame::new(
            metadata.clone(),
            encode_metadata::<StableContainerWire>(&metadata),
            &invalid_char,
            core::mem::align_of::<WireNested>(),
        );
        let rx = stable_rx(raw);
        assert!(
            matches!(rx.borrow_payload::<WireNested>(), Err(UWireError::InvalidPayload(reason)) if reason.contains("not valid"))
        );
    }

    #[cfg(feature = "zero-copy-transport")]
    #[test]
    fn selected_stable_wire_rejects_truncated_and_misaligned_payloads() {
        let metadata = stable_metadata::<WireAligned>();
        let truncated = RawRx {
            encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
            payload: vec![0; core::mem::size_of::<WireAligned>() - 1],
        };
        let truncated = stable_rx(truncated);
        assert!(
            matches!(truncated.borrow_payload::<WireAligned>(), Err(UWireError::InvalidPayload(reason)) if reason.contains("length"))
        );

        let misaligned = MisalignedRawRx {
            encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
            storage: AlignedPayloadStorage([0; 5]),
        };
        assert_ne!(
            (misaligned.payload().as_ptr() as usize) % core::mem::align_of::<WireAligned>(),
            0
        );
        let misaligned = stable_rx(misaligned);
        assert!(
            matches!(misaligned.borrow_payload::<WireAligned>(), Err(UWireError::InvalidPayload(reason)) if reason.contains("not aligned"))
        );
    }

    #[cfg(feature = "zero-copy-transport")]
    #[test]
    fn stable_loan_bridge_rejects_malformed_and_wrong_encoding() {
        let metadata = stable_metadata::<WireBool>();
        let malformed = RawRx {
            encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
            payload: vec![2],
        };
        let malformed = stable_rx(malformed);
        assert!(
            matches!(malformed.borrow_stable_payload::<WireBool>(&stable_profile()),
            Err(UWireError::InvalidPayload(reason)) if reason.contains("not valid"))
        );

        let wrong_metadata = metadata_with_payload_encoding(PayloadEncoding::RAW);
        let wrong = RawRx {
            encoded_metadata: encode_metadata::<StableContainerWire>(&wrong_metadata),
            payload: vec![1],
        };
        let wrong = stable_rx(wrong);
        assert!(matches!(
            wrong.borrow_stable_payload::<WireBool>(&stable_profile()),
            Err(UWireError::UnsupportedEncoding { .. })
        ));
    }

    #[cfg(feature = "zero-copy-transport")]
    #[test]
    fn inherited_unchecked_default_remains_checked() {
        // SAFETY: The test codec's unchecked lane is the inherited checked
        // path; the invalid bool pattern below must be rejected, not cast.
        let result = unsafe {
            <CheckedDefaultCodec as BorrowPayload<WireBool>>::borrow_payload_unchecked(&[2])
        };
        assert!(matches!(result, Err(UWireError::InvalidPayload(_))));
    }

    #[cfg(feature = "zero-copy-transport")]
    #[test]
    fn selected_stable_wire_unchecked_lane_accepts_caller_proven_payload() {
        let metadata = stable_metadata::<WireBool>();
        let raw = RawRx {
            encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
            payload: vec![1],
        };
        let rx = stable_rx(raw);

        // SAFETY: This test constructed the single payload byte itself as a
        // valid `bool`, so the caller-proven-validity obligation holds.
        let borrowed = unsafe { rx.borrow_payload_unchecked::<WireBool>() }.unwrap();
        assert!(borrowed.value);
    }

    #[cfg(feature = "zero-copy-transport")]
    #[test_case::test_case(false, None; "checked missing token")]
    #[test_case::test_case(false, Some(1); "checked wrong token")]
    #[test_case::test_case(true, None; "unchecked missing token")]
    #[test_case::test_case(true, Some(1); "unchecked wrong token")]
    fn native_identity_is_checked_in_both_borrow_lanes(unchecked: bool, delta: Option<u32>) {
        let original = stable_metadata::<WireBool>();
        let encoding = *original.payload_encoding().unwrap();
        let mut metadata = original
            .clone()
            .without_payload_encoding()
            .with_payload_encoding(encoding)
            .unwrap();
        if let Some(delta) = delta {
            metadata = metadata
                .with_native_type_token(crate::NativeTypeToken::from_u32(
                    original
                        .native_type_token()
                        .unwrap()
                        .as_u32()
                        .wrapping_add(delta),
                ))
                .unwrap();
        }
        let frame = stable_rx(RawRx {
            encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
            payload: vec![1],
        });
        let result = if unchecked {
            // SAFETY: This producer supplies a valid WireBool byte and alignment.
            // The intentionally wrong metadata is a checked precondition.
            unsafe { frame.borrow_payload_unchecked::<WireBool>() }
        } else {
            frame.borrow_payload::<WireBool>()
        };
        assert!(matches!(
            result,
            Err(UWireError::UnsupportedNativeTypeToken { .. })
        ));
    }

    #[cfg(feature = "zero-copy-transport")]
    #[test_case::test_case(false; "checked missing profile")]
    #[test_case::test_case(true; "unchecked missing profile")]
    fn native_bytes_without_route_context_remain_opaque(unchecked: bool) {
        let metadata = stable_metadata::<WireBool>();
        let frame =
            UWireRx::<RawRx, StableContainerWire, UFrameFieldsMetadataCodec>::try_from_encoded(
                RawRx {
                    encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
                    payload: vec![1],
                },
                &UFrameFieldsMetadataCodec,
            )
            .unwrap();
        assert_eq!(frame.try_contiguous_payload(), Some(&[1][..]));
        let result = if unchecked {
            // SAFETY: Known typed producer, exact WireBool representation and bits.
            unsafe { frame.borrow_payload_unchecked::<WireBool>() }
        } else {
            frame.borrow_payload::<WireBool>()
        };
        assert!(matches!(result, Err(UWireError::NativeProfile(_))));
    }

    #[cfg(feature = "zero-copy-transport")]
    #[test]
    fn received_frame_keeps_its_generation_when_a_route_is_replaced() {
        let metadata = stable_metadata::<WireBool>();
        let frame = stable_rx(RawRx {
            encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
            payload: vec![1],
        });
        let old = stable_profile();
        let next =
            crate::NativeProfile::new("wire-tests", 2, old.profile().mode().clone()).unwrap();
        let next = old.successor(Arc::new(next.clone()), &next).unwrap();
        assert!(frame.borrow_payload::<WireBool>().unwrap().value);
        assert!(matches!(
            frame.borrow_stable_payload::<WireBool>(&next),
            Err(UWireError::NativeProfile(
                crate::NativeProfileError::ProfileMismatch
            ))
        ));
    }

    #[cfg(feature = "owned-frame-transport")]
    #[tokio::test]
    async fn owned_receive_uses_exact_core_filter_for_exact_source_filter() {
        let core = RecordingCore::default();
        core.received
            .lock()
            .unwrap()
            .push_back(raw_frame_for_topic(0x9000, b"owned"));
        let transport = core.into_native_prefix_wire_transport(UProtocolNativeWire);
        let source_filter = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();

        let frame = transport
            .receive_validated_owned(&source_filter, None)
            .await
            .expect("matching owned frame");

        assert_eq!(frame.payload_bytes(), b"owned");
        let filters = transport.core().receive_filters.lock().unwrap();
        assert_eq!(filters.len(), 1);
        let filter = filters.first().expect("one receive filter");
        assert_eq!(filter.0, source_filter);
        assert_eq!(filter.1, None);
    }

    #[cfg(feature = "owned-frame-transport")]
    #[tokio::test]
    async fn owned_receive_preserves_partial_core_source_filter() {
        let core = RecordingCore::default();
        core.received
            .lock()
            .unwrap()
            .push_back(raw_frame_for_topic(0x9000, b"owned"));
        let transport = core.into_native_prefix_wire_transport(UProtocolNativeWire);
        let source_filter = UUri::try_from_parts("vehicle", 0x4210, 0x01, u16::MAX).unwrap();

        let frame = transport
            .receive_validated_owned(&source_filter, None)
            .await
            .expect("matching owned frame");

        assert_eq!(frame.payload_bytes(), b"owned");
        let filters = transport.core().receive_filters.lock().unwrap();
        assert_eq!(filters.len(), 1);
        let filter = filters.first().expect("one receive filter");
        assert_eq!(filter.0, source_filter);
        assert_eq!(filter.1, None);
    }

    #[test]
    fn selected_wire_core_source_filter_preserves_exact_and_partial_patterns() {
        let exact = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let wildcard = UUri::try_from_parts("vehicle", 0x4210, 0x01, u16::MAX).unwrap();

        assert_eq!(selected_wire_core_source_filter_for(&exact), exact);
        assert_eq!(selected_wire_core_source_filter_for(&wildcard), wildcard);
    }

    #[cfg(feature = "owned-frame-transport")]
    #[test_case(0x9000; "exact owned listener filter remains exact")]
    #[test_case(u16::MAX; "wildcard owned listener filter remains broad")]
    #[tokio::test]
    async fn owned_listener_registration_and_removal_use_the_same_core_filter(resource: u16) {
        let transport =
            RecordingCore::default().into_native_prefix_wire_transport(UProtocolNativeWire);
        let source = UUri::try_from_parts("vehicle", 0x4210, 1, resource).unwrap();
        let listener = Arc::new(RecordingOwnedListener::default());
        transport
            .register_validated_owned_listener(&source, None, listener.clone())
            .await
            .unwrap();
        let encoded_listener = transport
            .core()
            .owned_listeners
            .lock()
            .unwrap()
            .first()
            .expect("registered owned listener")
            .clone();
        let raw = raw_frame_for_topic(0x9000, b"owned-after-unregister");
        encoded_listener
            .on_receive_encoded_owned(EncodedOwnedFrame::new(
                raw.encoded_metadata,
                Some(raw.payload.into()),
            ))
            .await;
        transport
            .unregister_validated_owned_listener(&source, None, listener.clone())
            .await
            .unwrap();
        let expected = selected_wire_core_source_filter_for(&source);
        assert_eq!(
            *transport.core().listener_filters.lock().unwrap(),
            vec![(true, expected.clone(), None), (false, expected, None)]
        );
        assert!(transport.core().owned_listeners.lock().unwrap().is_empty());
        drop(encoded_listener);
        drop(transport);
        assert_eq!(
            listener
                .0
                .lock()
                .unwrap()
                .first()
                .expect("delivered owned frame")
                .payload_bytes(),
            b"owned-after-unregister"
        );
    }

    #[cfg(feature = "owned-frame-transport")]
    #[test]
    fn wire_transport_fits_owned_blanket_boundary() {
        fn assert_owned_impl<T: UOwnedTransportImpl>() {}

        type Transport =
            UWireTransport<CompileCore, UProtocolNativeWire, UFrameFieldsMetadataCodec>;
        assert_owned_impl::<Transport>();
    }

    #[test]
    fn with_wire_constructs_adapter() {
        let transport = CompileCore.into_native_prefix_wire_transport(UProtocolNativeWire);
        let _: &UProtocolNativeWire = transport.wire();
        let _: &CompileCore = transport.core();
    }

    #[cfg(feature = "zero-copy-transport")]
    #[test]
    fn existing_public_rx_lease_still_compiles_independently() {
        fn assert_public_rx<T: UZeroCopyRxLease>() {}

        assert_public_rx::<UVecRxLease>();
    }
}
