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

use std::{
    any::Any,
    collections::HashMap,
    io::{Cursor, Read},
    mem::MaybeUninit,
    ops::Deref,
    ptr::NonNull,
    sync::{Arc, LazyLock, Mutex},
};

#[cfg(any(test, feature = "test-util"))]
use std::collections::VecDeque;

use async_trait::async_trait;
use tracing::warn;

#[cfg(any(
    feature = "unsafe-stable-payload-tx",
    feature = "expert-unsafe-payloads"
))]
use crate::payload::UnsafeStablePayloadTxSlot;
#[cfg(feature = "selected-wire-transport-adapter")]
use crate::UHasWire;
#[cfg(feature = "owned-frame-transport")]
use crate::UOwnedFrame;
use crate::{
    payload::{
        InitializedStablePayload, LoanPayload, LoanUninitPayload, LoanedInitPayload,
        LoanedUninitPayload, PayloadCodec, ReadDecodePayload, StableContainerPayload,
        StablePayload, StablePayloadInit, UWireError,
    },
    utransport::verify_filter_criteria,
    UCode, UFrameMetadata, UFrameMetadataError, UStatus, UUri,
};
#[cfg(feature = "selected-wire-transport-adapter")]
use crate::{UWireLoan, UWireLoanUninit};

mod zero_copy_transport_sealed {
    pub trait Sealed {}
}

mod zero_copy_uninit_transport_sealed {
    pub trait Sealed {}
}

/// Payload presence and layout requested for a transmit loan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UTxPayloadSpec {
    /// The frame carries no payload and no payload encoding metadata.
    Absent,
    /// The frame carries a payload, including a present empty payload.
    Present {
        len: usize,
        alignment: PayloadAlignment,
    },
}

/// Validated visible application payload alignment in bytes.
///
/// The value is always nonzero and a power of two. Runtime values from config,
/// metadata, or transport boundaries must be constructed with [`Self::new`] or
/// [`TryFrom<usize>`] so those boundaries keep their explicit validation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PayloadAlignment(usize);

impl PayloadAlignment {
    /// Alignment for absent payloads and byte-oriented payloads.
    pub const ONE: Self = Self(1);

    /// Creates a payload alignment after validating the nonzero power-of-two invariant.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is zero or not a power of two.
    pub fn new(value: usize) -> Result<Self, UStatus> {
        if value == 0 {
            return Err(invalid_argument(
                "payload alignment must be greater than zero",
            ));
        }
        if !value.is_power_of_two() {
            return Err(invalid_argument("payload alignment must be a power of two"));
        }
        Ok(Self(value))
    }

    /// Returns the raw alignment value in bytes.
    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

impl TryFrom<usize> for PayloadAlignment {
    type Error = UStatus;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl UTxPayloadSpec {
    /// Creates a present-payload spec from length and alignment.
    ///
    /// # Errors
    ///
    /// Returns an error if `alignment` is zero or not a power of two.
    pub fn present(len: usize, alignment: usize) -> Result<Self, UStatus> {
        let alignment = PayloadAlignment::new(alignment)?;
        Ok(Self::present_with_alignment(len, alignment))
    }

    /// Creates a present-payload spec from length and a validated alignment.
    #[must_use]
    pub fn present_with_alignment(len: usize, alignment: PayloadAlignment) -> Self {
        Self::Present { len, alignment }
    }

    /// Creates a present-empty-payload spec.
    #[must_use]
    pub fn present_empty() -> Self {
        Self::Present {
            len: 0,
            alignment: PayloadAlignment::ONE,
        }
    }

    /// Returns whether this spec represents a present payload.
    #[must_use]
    pub fn is_present(self) -> bool {
        matches!(self, Self::Present { .. })
    }

    /// Returns the visible application payload length.
    #[must_use]
    pub fn len(self) -> usize {
        match self {
            Self::Absent => 0,
            Self::Present { len, .. } => len,
        }
    }

    /// Returns true if the visible application payload length is zero.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Returns the requested visible application payload alignment.
    #[must_use]
    pub fn alignment(self) -> usize {
        self.payload_alignment().as_usize()
    }

    /// Returns the validated payload alignment proof for this spec.
    #[must_use]
    pub fn payload_alignment(self) -> PayloadAlignment {
        match self {
            Self::Absent => PayloadAlignment::ONE,
            Self::Present { alignment, .. } => alignment,
        }
    }
}

/// Validated transport-independent transmit loan specification.
#[derive(Clone, Debug, PartialEq)]
pub struct UTxLoanSpec {
    metadata: UFrameMetadata,
    payload: UTxPayloadSpec,
}

impl UTxLoanSpec {
    /// Creates a validated transmit loan spec.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata is invalid or if payload presence and payload
    /// encoding metadata disagree.
    pub fn new(metadata: UFrameMetadata, payload: UTxPayloadSpec) -> Result<Self, UStatus> {
        let spec = Self::new_unchecked(metadata, payload);
        validate_tx_loan_spec(&spec)?;
        Ok(spec)
    }

    /// Creates a transmit loan spec without validation.
    #[must_use]
    pub fn new_unchecked(metadata: UFrameMetadata, payload: UTxPayloadSpec) -> Self {
        Self { metadata, payload }
    }

    /// Creates a no-payload transmit loan spec.
    ///
    /// # Errors
    ///
    /// Returns an error if the metadata is invalid or still carries payload encoding.
    pub fn no_payload(metadata: UFrameMetadata) -> Result<Self, UStatus> {
        Self::new(metadata, UTxPayloadSpec::Absent)
    }

    /// Creates a present-payload transmit loan spec.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata is invalid, has no payload encoding, or if
    /// the requested payload alignment is invalid.
    pub fn payload(
        metadata: UFrameMetadata,
        payload_len: usize,
        alignment: usize,
    ) -> Result<Self, UStatus> {
        Self::new(metadata, UTxPayloadSpec::present(payload_len, alignment)?)
    }

    /// Creates a present-empty-payload transmit loan spec.
    ///
    /// # Errors
    ///
    /// Returns an error if the metadata is invalid or has no payload encoding.
    pub fn present_empty_payload(metadata: UFrameMetadata) -> Result<Self, UStatus> {
        Self::new(metadata, UTxPayloadSpec::present_empty())
    }

    /// Returns the immutable frame metadata associated with this loan.
    #[must_use]
    pub fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    /// Consumes the spec and returns its metadata.
    #[must_use]
    pub fn into_metadata(self) -> UFrameMetadata {
        self.metadata
    }

    /// Returns the payload presence and layout spec.
    #[must_use]
    pub fn payload_spec(&self) -> UTxPayloadSpec {
        self.payload
    }

    /// Returns whether the transmit frame carries a payload.
    #[must_use]
    pub fn has_payload(&self) -> bool {
        self.payload.is_present()
    }

    /// Returns the visible application payload length.
    #[must_use]
    pub fn payload_len(&self) -> usize {
        self.payload.len()
    }

    /// Returns the requested visible application payload alignment.
    #[must_use]
    pub fn payload_alignment(&self) -> usize {
        self.payload.alignment()
    }

    /// Returns the validated payload alignment proof.
    #[must_use]
    pub fn payload_alignment_proof(&self) -> PayloadAlignment {
        self.payload.payload_alignment()
    }
}

/// Transmit loan spec that has passed the public transport validation boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedTxLoanSpec(UTxLoanSpec);

impl ValidatedTxLoanSpec {
    /// Returns the validated transmit loan spec.
    #[must_use]
    pub fn as_spec(&self) -> &UTxLoanSpec {
        &self.0
    }

    /// Consumes the wrapper and returns the validated transmit loan spec.
    #[must_use]
    pub fn into_inner(self) -> UTxLoanSpec {
        self.0
    }

    /// Returns the immutable frame metadata associated with this loan.
    #[must_use]
    pub fn metadata(&self) -> &UFrameMetadata {
        self.0.metadata()
    }

    /// Consumes the spec and returns its metadata.
    #[must_use]
    pub fn into_metadata(self) -> UFrameMetadata {
        self.0.into_metadata()
    }

    /// Returns the visible application payload length.
    #[must_use]
    pub fn payload_len(&self) -> usize {
        self.0.payload_len()
    }

    /// Returns the requested visible application payload alignment.
    #[must_use]
    pub fn payload_alignment(&self) -> usize {
        self.0.payload_alignment()
    }

    /// Returns the validated payload alignment proof.
    #[must_use]
    pub fn payload_alignment_proof(&self) -> PayloadAlignment {
        self.0.payload_alignment_proof()
    }
}

impl TryFrom<UTxLoanSpec> for ValidatedTxLoanSpec {
    type Error = UStatus;

    fn try_from(value: UTxLoanSpec) -> Result<Self, Self::Error> {
        validate_tx_loan_spec(&value)?;
        Ok(Self(value))
    }
}

impl Deref for ValidatedTxLoanSpec {
    type Target = UTxLoanSpec;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Mutable transmit storage reserved from a zero-copy transport.
pub trait UTxBuffer {
    /// Returns the immutable frame metadata associated with this transmit loan.
    fn metadata(&self) -> &UFrameMetadata;

    /// Returns the current payload bytes in the transmit loan.
    fn payload(&self) -> &[u8];

    /// Returns mutable payload storage for direct serialization into the loan.
    fn payload_mut(&mut self) -> &mut [u8];
}

/// Mutable transmit storage whose application payload bytes are not yet initialized.
pub trait UUninitTxBuffer {
    /// Initialized transmit loan type produced after payload initialization.
    type Initialized: UTxBuffer;

    /// Returns the immutable frame metadata associated with this transmit loan.
    fn metadata(&self) -> &UFrameMetadata;

    /// Returns the visible application payload length.
    fn payload_len(&self) -> usize;

    /// Returns mutable uninitialized application payload storage.
    fn payload_uninit_mut(&mut self) -> &mut [MaybeUninit<u8>];

    /// Converts this uninitialized loan into its initialized TX buffer form.
    ///
    /// # Safety
    ///
    /// The caller must guarantee every visible application payload byte has been
    /// initialized before conversion.
    unsafe fn assume_payload_init(self) -> Self::Initialized;
}

/// Neutral view of frame metadata plus ordered payload bytes.
pub trait UFrameView {
    type PayloadReader<'a>: Read + 'a
    where
        Self: 'a;
    type PayloadSlices<'a>: Iterator<Item = &'a [u8]> + 'a
    where
        Self: 'a;

    /// Returns the native frame metadata.
    fn metadata(&self) -> &UFrameMetadata;

    /// Returns the number of application payload bytes visible through this view.
    fn payload_len(&self) -> usize;

    /// Returns whether this view carries a payload, including a present empty payload.
    fn has_payload(&self) -> bool {
        self.payload_len() > 0 || self.metadata().payload_encoding().is_some()
    }

    /// Returns an ordered reader over the application payload bytes.
    fn payload_reader(&self) -> Self::PayloadReader<'_>;

    /// Returns ordered borrowed payload slices.
    fn payload_slices(&self) -> Self::PayloadSlices<'_>;

    /// Returns a contiguous borrowed payload view when this view can provide one without copying.
    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        None
    }

    /// Decodes this frame view from its ordered payload reader with codec `C`.
    ///
    /// This path works for both contiguous and segmented receive storage without
    /// forcing a coalescing copy before the selected codec sees the bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the frame has no payload, has missing or incompatible
    /// encoding metadata, or if the codec cannot decode the payload bytes.
    fn decode_payload_from_reader_as<C, T>(&self) -> Result<T, UWireError>
    where
        C: PayloadCodec + ReadDecodePayload<T>,
    {
        C::verify_encoding(self.metadata().payload_encoding())?;
        if !self.has_payload() {
            return Err(UWireError::MissingPayload);
        }
        C::decode_payload_from_reader(self.payload_reader(), self.payload_len())
    }
}

#[cfg(feature = "owned-frame-transport")]
impl UFrameView for UOwnedFrame {
    type PayloadReader<'a>
        = Cursor<&'a [u8]>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = std::option::IntoIter<&'a [u8]>
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata {
        self.metadata()
    }

    fn payload_len(&self) -> usize {
        self.payload_bytes().len()
    }

    fn has_payload(&self) -> bool {
        UOwnedFrame::has_payload(self)
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        Cursor::new(self.payload_bytes())
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        self.payload().map(bytes::Bytes::as_ref).into_iter()
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        self.payload().map(bytes::Bytes::as_ref)
    }
}

/// Receive-side zero-copy frame lease returned by a transport.
pub trait UZeroCopyRxLease: UFrameView {}

/// Diagnostic provenance for loan-backed payload storage.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PayloadLoanProvenance {
    /// Payload bytes are backed by a transport loan whose domain is opaque to up-rust.
    OpaqueTransportLoan,
}

/// Immutable payload bytes with explicit transport-loan provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoanedPayload<'a> {
    bytes: &'a [u8],
    provenance: PayloadLoanProvenance,
}

impl<'a> LoanedPayload<'a> {
    /// Creates a loaned payload view from transport-owned storage.
    ///
    /// # Safety
    ///
    /// Callers must guarantee `bytes` stays valid and immutable for `'a`, and
    /// that the storage was not allocated or coalesced solely to manufacture a
    /// zero-copy receive proof.
    #[must_use]
    pub unsafe fn new_unchecked(bytes: &'a [u8], provenance: PayloadLoanProvenance) -> Self {
        Self { bytes, provenance }
    }

    /// Returns diagnostic storage provenance.
    #[must_use]
    pub fn provenance(self) -> PayloadLoanProvenance {
        self.provenance
    }

    /// Returns the payload bytes.
    #[must_use]
    pub fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Returns the payload length in bytes.
    #[must_use]
    pub fn len(self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the payload is empty.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }
}

impl AsRef<[u8]> for LoanedPayload<'_> {
    fn as_ref(&self) -> &[u8] {
        self.bytes
    }
}

impl Deref for LoanedPayload<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.bytes
    }
}

/// Mutable uninitialized payload bytes with explicit transport-loan provenance.
pub struct LoanedPayloadUninitMut<'a> {
    bytes: &'a mut [MaybeUninit<u8>],
    provenance: PayloadLoanProvenance,
}

impl<'a> LoanedPayloadUninitMut<'a> {
    /// Creates a mutable uninitialized loaned payload view from transport-owned storage.
    ///
    /// # Safety
    ///
    /// `bytes` must be a valid mutable uninitialized byte slice for `'a`, with no
    /// other active access path, and must be the exact visible application
    /// payload range for the loan described by `provenance`.
    #[must_use]
    pub unsafe fn new_unchecked(
        bytes: &'a mut [MaybeUninit<u8>],
        provenance: PayloadLoanProvenance,
    ) -> Self {
        Self { bytes, provenance }
    }

    /// Returns diagnostic storage provenance.
    #[must_use]
    pub fn provenance(&self) -> PayloadLoanProvenance {
        self.provenance
    }

    /// Returns the payload length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the payload is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub(crate) fn as_uninit_bytes_mut_internal(&mut self) -> &mut [MaybeUninit<u8>] {
        self.bytes
    }

    pub(crate) fn into_uninit_bytes_mut_internal(self) -> &'a mut [MaybeUninit<u8>] {
        self.bytes
    }
}

/// Receive lease that can expose a contiguous payload from loan-backed storage.
pub trait ULoanedContiguousZeroCopyRxFrame: UZeroCopyRxLease {
    /// Returns one contiguous loan-backed application payload view.
    ///
    /// Implementations must not allocate, copy, or coalesce payload bytes to
    /// satisfy this method.
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError>;

    /// Returns diagnostic provenance for successful loaned payload borrows.
    fn payload_loan_provenance(&self) -> Result<PayloadLoanProvenance, UWireError> {
        Ok(self.loaned_contiguous_payload()?.provenance())
    }

    /// Returns only loan-backed contiguous payload bytes.
    fn try_loaned_contiguous_payload(&self) -> Result<&[u8], UWireError> {
        Ok(self.loaned_contiguous_payload()?.as_bytes())
    }

    /// Borrows one stable-container value from loan-backed contiguous storage.
    fn borrow_stable_payload<T>(&self) -> Result<&T, UWireError>
    where
        T: StablePayload,
    {
        StableContainerPayload::<T>::verify_encoding(self.metadata().payload_encoding())?;
        let payload = self.loaned_contiguous_payload()?;
        StableContainerPayload::<T>::borrow_checked_payload(payload.as_bytes())
    }
}

/// A handler for processing zero-copy receive leases.
#[async_trait]
pub trait UZeroCopyListener<Rx>: Send + Sync
where
    Rx: UZeroCopyRxLease + Send + 'static,
{
    /// Handles one received zero-copy frame lease.
    async fn on_receive_zero_copy(&self, frame: Rx);
}

/// Implementation boundary for transports that can loan zero-copy storage.
#[async_trait]
pub trait UZeroCopyTransportImpl: Send + Sync {
    /// Transport-specific transmit loan type.
    type Tx: UTxBuffer + Send;

    /// Transport-specific receive lease type.
    type Rx: UZeroCopyRxLease + Send + 'static;

    /// Reserves transmit storage for a validated frame loan spec.
    async fn loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus>;

    /// Commits a validated transmit loan.
    async fn send_validated_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus>;

    /// Receives one matching zero-copy frame from transports that support pull receive.
    async fn receive_validated_zero_copy(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus> {
        Err(UStatus::fail_with_code(
            UCode::Unimplemented,
            "not implemented",
        ))
    }

    /// Registers a zero-copy listener after public filter validation.
    async fn register_validated_zero_copy_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        Err(UStatus::fail_with_code(
            UCode::Unimplemented,
            "not implemented",
        ))
    }

    /// Unregisters a zero-copy listener after public filter validation.
    async fn unregister_validated_zero_copy_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        Err(UStatus::fail_with_code(
            UCode::Unimplemented,
            "not implemented",
        ))
    }
}

impl<T> zero_copy_transport_sealed::Sealed for T where T: UZeroCopyTransportImpl + ?Sized {}

/// The zero-copy transport capability API.
#[async_trait]
pub trait UZeroCopyTransport: zero_copy_transport_sealed::Sealed + Send + Sync {
    /// Transport-specific transmit loan type returned by [`Self::loan_tx`].
    type Tx: UTxBuffer + Send;

    /// Transport-specific receive lease type returned by pull receive and listeners.
    type Rx: UZeroCopyRxLease + Send + 'static;

    /// Reserves transmit storage for a validated frame loan spec.
    async fn loan_tx(&self, spec: UTxLoanSpec) -> Result<Self::Tx, UStatus>;

    /// Commits a previously reserved transmit loan.
    async fn send_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus>;

    /// Receives one matching zero-copy frame from transports that support pull receive.
    async fn receive_zero_copy(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus>;

    /// Registers a listener for matching zero-copy receive leases.
    async fn register_zero_copy_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus>;

    /// Unregisters a listener for matching zero-copy receive leases.
    async fn unregister_zero_copy_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus>;
}

#[async_trait]
impl<T> UZeroCopyTransport for T
where
    T: UZeroCopyTransportImpl + ?Sized,
{
    type Tx = T::Tx;
    type Rx = T::Rx;

    async fn loan_tx(&self, spec: UTxLoanSpec) -> Result<Self::Tx, UStatus> {
        UZeroCopyTransportImpl::loan_validated_tx(self, ValidatedTxLoanSpec::try_from(spec)?).await
    }

    async fn send_zero_copy(&self, mut buffer: Self::Tx) -> Result<(), UStatus> {
        validate_tx_buffer_for_transport(&mut buffer)?;
        UZeroCopyTransportImpl::send_validated_zero_copy(self, buffer).await
    }

    async fn receive_zero_copy(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus> {
        verify_zero_copy_filter_criteria(source_filter, sink_filter)?;
        let frame =
            UZeroCopyTransportImpl::receive_validated_zero_copy(self, source_filter, sink_filter)
                .await?;
        validate_frame_view_for_transport(&frame)?;
        Ok(frame)
    }

    async fn register_zero_copy_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        verify_zero_copy_filter_criteria(source_filter, sink_filter)?;
        let key = zero_copy_listener_registration_key(
            self,
            source_filter,
            sink_filter,
            zero_copy_listener_pointer(&listener),
        );
        let (listener, inserted) = registered_zero_copy_listener(&key, listener);
        let result = UZeroCopyTransportImpl::register_validated_zero_copy_listener(
            self,
            source_filter,
            sink_filter,
            listener,
        )
        .await;
        if result.is_err() && inserted {
            ZERO_COPY_LISTENER_REGISTRY
                .lock()
                .expect("zero-copy listener registry lock poisoned")
                .remove(&key);
        }
        result
    }

    async fn unregister_zero_copy_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        verify_zero_copy_filter_criteria(source_filter, sink_filter)?;
        let key = zero_copy_listener_registration_key(
            self,
            source_filter,
            sink_filter,
            zero_copy_listener_pointer(&listener),
        );
        let listener = zero_copy_listener_for_unregister(&key, listener);
        let result = UZeroCopyTransportImpl::unregister_validated_zero_copy_listener(
            self,
            source_filter,
            sink_filter,
            listener,
        )
        .await;
        if result.is_ok() {
            ZERO_COPY_LISTENER_REGISTRY
                .lock()
                .expect("zero-copy listener registry lock poisoned")
                .remove(&key);
        }
        result
    }
}

/// Convenience methods for zero-copy transports with initialized TX storage.
#[async_trait]
pub trait UZeroCopyTransportExt: UZeroCopyTransport {
    /// Initializes a typed payload using the adapter's selected wire and sends it.
    ///
    /// Prefer this selected-wire helper on values produced by explicit selected-wire adapter construction.
    /// Use [`Self::send_loaned_payload_as`] only for low-level codec escape hatches.
    ///
    /// # Errors
    ///
    /// Returns an error if the selected wire does not successfully loan, encode,
    /// or send the payload through the underlying transport.
    #[cfg(feature = "selected-wire-transport-adapter")]
    async fn send_loaned_payload<T>(
        &self,
        metadata: UFrameMetadata,
        init: impl for<'payload> FnOnce(&'payload mut T) + Send,
    ) -> Result<(), UStatus>
    where
        Self: UHasWire,
        Self::Wire: UWireLoan<T>,
        <Self::Wire as UWireLoan<T>>::Codec: Send + Sync,
    {
        self.send_loaned_payload_as::<<Self::Wire as UWireLoan<T>>::Codec, T>(metadata, init)
            .await
    }

    /// Initializes a typed payload directly in a transmit loan and sends it.
    ///
    /// This is the low-level codec-selected form. Product code that already uses
    /// explicit selected-wire adapter construction should prefer [`Self::send_loaned_payload`] so the
    /// selected wire supplies the payload codec.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata validation fails, the transport cannot loan
    /// the requested initialized layout, the codec rejects the loaned storage, or
    /// sending the committed loan fails.
    async fn send_loaned_payload_as<C, T>(
        &self,
        metadata: UFrameMetadata,
        init: impl for<'payload> FnOnce(&'payload mut T) + Send,
    ) -> Result<(), UStatus>
    where
        C: PayloadCodec + LoanPayload<T> + Send + Sync,
    {
        let metadata = UFrameMetadata::new(metadata.into_attributes(), Some(C::payload_encoding()))
            .map_err(frame_metadata_error)?;
        let layout = C::loan_layout().map_err(UStatus::from)?;
        let mut buffer = self
            .loan_tx(UTxLoanSpec::payload(
                metadata,
                layout.len(),
                layout.align(),
            )?)
            .await?;
        verify_tx_buffer_payload_layout(&mut buffer, layout.len(), layout.align())?;
        {
            let payload = C::loan_payload(buffer.payload_mut()).map_err(UStatus::from)?;
            init(payload);
        }
        self.send_zero_copy(buffer).await
    }
}

impl<T> UZeroCopyTransportExt for T where T: UZeroCopyTransport + ?Sized {}

/// Convenience methods for zero-copy transports with uninitialized TX storage.
#[async_trait]
pub trait UZeroCopyUninitTransportExt: UZeroCopyUninitTransport {
    /// Constructs a typed payload using the adapter's selected wire and sends it.
    ///
    /// Prefer this selected-wire helper on values produced by explicit selected-wire adapter construction.
    /// Use [`Self::send_uninit_loaned_payload_as`] only for low-level codec
    /// escape hatches.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata validation, loaning, initialization, or send
    /// fails.
    #[cfg(feature = "selected-wire-transport-adapter")]
    async fn send_uninit_loaned_payload<T>(
        &self,
        metadata: UFrameMetadata,
        init: impl for<'payload> FnOnce(
                LoanedUninitPayload<'payload, T>,
            ) -> Result<LoanedInitPayload<'payload, T>, UWireError>
            + Send,
    ) -> Result<(), UStatus>
    where
        Self: UHasWire,
        Self::Wire: UWireLoanUninit<T>,
        <Self::Wire as UWireLoanUninit<T>>::Codec: Send + Sync,
        T: Send,
    {
        self.send_uninit_loaned_payload_as::<<Self::Wire as UWireLoanUninit<T>>::Codec, T>(
            metadata, init,
        )
        .await
    }

    /// Constructs a typed payload directly in uninitialized transmit storage and sends it.
    ///
    /// This is the low-level codec-selected form. Product code that already uses
    /// explicit selected-wire adapter construction should prefer [`Self::send_uninit_loaned_payload`] so the
    /// selected wire supplies the payload codec.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata validation fails, the transport cannot loan
    /// the requested uninitialized layout, the codec rejects the loaned storage,
    /// the initializer fails, or sending the committed loan fails.
    async fn send_uninit_loaned_payload_as<C, T>(
        &self,
        metadata: UFrameMetadata,
        init: impl for<'payload> FnOnce(
                LoanedUninitPayload<'payload, T>,
            ) -> Result<LoanedInitPayload<'payload, T>, UWireError>
            + Send,
    ) -> Result<(), UStatus>
    where
        C: PayloadCodec + LoanUninitPayload<T> + Send + Sync,
        T: Send,
    {
        let metadata = UFrameMetadata::new(metadata.into_attributes(), Some(C::payload_encoding()))
            .map_err(frame_metadata_error)?;
        let layout = C::loan_uninit_layout().map_err(UStatus::from)?;
        let mut buffer = self
            .loan_uninit_tx(UTxLoanSpec::payload(
                metadata,
                layout.len(),
                layout.align(),
            )?)
            .await?;
        verify_uninit_tx_buffer_payload_layout(&mut buffer, layout.len(), layout.align())?;
        {
            let payload = buffer.payload_uninit_mut();
            // SAFETY: `UZeroCopyUninitTransport::loan_uninit_tx` returned this
            // buffer as the transport loan for the validated spec. The public
            // verifier above checked that this visible range matches the request.
            let loaned = unsafe {
                LoanedPayloadUninitMut::new_unchecked(
                    payload,
                    PayloadLoanProvenance::OpaqueTransportLoan,
                )
            };
            let loaned = C::loan_uninit_payload(loaned).map_err(UStatus::from)?;
            let expected = loaned.uninit_ptr();
            let initialized = init(loaned).map_err(UStatus::from)?;
            if initialized.initialized_ptr().cast::<MaybeUninit<T>>() != expected {
                return Err(invalid_argument(
                    "initialized payload proof does not match the TX loan",
                ));
            }
        }
        // SAFETY: the initializer returned a marker tied to the same checked
        // loan slot, proving the visible payload bytes have been initialized.
        let buffer = unsafe { buffer.assume_payload_init() };
        self.send_zero_copy(buffer).await
    }

    /// Initializes a stable-container payload through the adapter's selected wire.
    ///
    /// Prefer this selected-wire helper on values produced by explicit selected-wire adapter construction.
    /// It is available only for selected wires whose uninitialized-loan codec is
    /// `StableContainerPayload<T>`.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata validation, stable initialization, loaning, or
    /// send fails.
    #[cfg(feature = "selected-wire-transport-adapter")]
    async fn send_uninit_stable_payload<T>(
        &self,
        metadata: UFrameMetadata,
        init: impl for<'payload> FnOnce(
                T::Init<'payload>,
            ) -> Result<InitializedStablePayload<T>, UWireError>
            + Send,
    ) -> Result<(), UStatus>
    where
        Self: UHasWire,
        Self::Wire: UWireLoanUninit<T, Codec = StableContainerPayload<T>>,
        T: StablePayloadInit + Send,
    {
        self.send_uninit_stable_payload_as::<T>(metadata, init)
            .await
    }

    /// Initializes a stable-container payload directly in uninitialized transmit storage.
    ///
    /// This is the low-level stable-container form. Product code that already
    /// uses explicit selected-wire adapter construction should prefer [`Self::send_uninit_stable_payload`] so
    /// the selected wire authorizes the stable-container payload family.
    ///
    /// The initializer is generated by `#[derive(StablePayloadInit)]`; it exposes
    /// named typed setters and returns a completion token only after all required
    /// fields and generated padding gaps are initialized.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata validation fails, the transport cannot loan
    /// the requested stable layout, initialization fails, the completion token is
    /// not tied to the loaned slot, or sending the committed loan fails.
    async fn send_uninit_stable_payload_as<T>(
        &self,
        metadata: UFrameMetadata,
        init: impl for<'payload> FnOnce(
                T::Init<'payload>,
            ) -> Result<InitializedStablePayload<T>, UWireError>
            + Send,
    ) -> Result<(), UStatus>
    where
        T: StablePayloadInit + Send,
    {
        let metadata = UFrameMetadata::new(
            metadata.into_attributes(),
            Some(StableContainerPayload::<T>::encoding()),
        )
        .map_err(frame_metadata_error)?;
        let layout_len = std::mem::size_of::<T>();
        let layout_align = std::mem::align_of::<T>();
        let mut buffer = self
            .loan_uninit_tx(UTxLoanSpec::payload(metadata, layout_len, layout_align)?)
            .await?;
        verify_uninit_tx_buffer_payload_layout(&mut buffer, layout_len, layout_align)?;
        {
            let payload = buffer.payload_uninit_mut();
            // SAFETY: same loan/provenance argument as the typed uninit helper;
            // `StablePayloadInit` validates the stable-container layout below.
            let mut loaned = unsafe {
                LoanedPayloadUninitMut::new_unchecked(
                    payload,
                    PayloadLoanProvenance::OpaqueTransportLoan,
                )
            };
            let expected = stable_uninit_payload_ptr::<T>(&mut loaned).map_err(UStatus::from)?;
            let initializer = T::init_from_uninit_payload(loaned).map_err(UStatus::from)?;
            let initialized = init(initializer).map_err(UStatus::from)?;
            if initialized.initialized_ptr() != expected {
                return Err(invalid_argument(
                    "stable payload init proof does not match the TX loan",
                ));
            }
        }
        // SAFETY: the generated stable initializer returned a completion proof
        // for the same loan slot after all fields and padding were initialized.
        let buffer = unsafe { buffer.assume_payload_init() };
        self.send_zero_copy(buffer).await
    }

    /// Expert hatch for sending a stable-container payload whose bytes cannot be
    /// proven byte-backed by the safe API.
    ///
    /// # Safety
    ///
    /// `init` must initialize every transported byte in the slot, including
    /// implicit padding, before returning an initialized marker. Returning an
    /// initialized marker before the full byte range contains one valid `T` is
    /// undefined behavior for receivers that borrow the stable payload.
    #[cfg(any(
        feature = "unsafe-stable-payload-tx",
        feature = "expert-unsafe-payloads"
    ))]
    async unsafe fn send_uninit_stable_payload_unchecked<T>(
        &self,
        metadata: UFrameMetadata,
        init: impl for<'payload> FnOnce(
                UnsafeStablePayloadTxSlot<'payload, T>,
            ) -> Result<LoanedInitPayload<'payload, T>, UWireError>
            + Send,
    ) -> Result<(), UStatus>
    where
        T: StablePayload + Send,
    {
        let metadata = UFrameMetadata::new(
            metadata.into_attributes(),
            Some(StableContainerPayload::<T>::encoding()),
        )
        .map_err(frame_metadata_error)?;
        let layout_len = std::mem::size_of::<T>();
        let layout_align = std::mem::align_of::<T>();
        let mut buffer = self
            .loan_uninit_tx(UTxLoanSpec::payload(metadata, layout_len, layout_align)?)
            .await?;
        verify_uninit_tx_buffer_payload_layout(&mut buffer, layout_len, layout_align)?;
        {
            let payload = buffer.payload_uninit_mut();
            // SAFETY: same loan/provenance argument as the safe uninit helpers;
            // the expert slot validates the stable payload layout below.
            let mut loaned = unsafe {
                LoanedPayloadUninitMut::new_unchecked(
                    payload,
                    PayloadLoanProvenance::OpaqueTransportLoan,
                )
            };
            let expected = stable_uninit_payload_ptr::<T>(&mut loaned).map_err(UStatus::from)?;
            let slot = UnsafeStablePayloadTxSlot::new(loaned).map_err(UStatus::from)?;
            let initialized = init(slot).map_err(UStatus::from)?;
            if initialized.initialized_ptr().cast::<MaybeUninit<T>>() != expected {
                return Err(invalid_argument(
                    "expert stable payload proof does not match the TX loan",
                ));
            }
        }
        // SAFETY: this unsafe method requires the initializer to return only
        // after the exact visible payload loan contains one initialized `T`.
        let buffer = unsafe { buffer.assume_payload_init() };
        self.send_zero_copy(buffer).await
    }
}

impl<T> UZeroCopyUninitTransportExt for T where T: UZeroCopyUninitTransport + ?Sized {}

/// Implementation boundary for transports that can expose uninitialized TX payload storage.
#[async_trait]
pub trait UZeroCopyUninitTransportImpl: UZeroCopyTransportImpl {
    /// Transport-specific uninitialized transmit loan type.
    type UninitTx: UUninitTxBuffer<Initialized = Self::Tx> + Send;

    /// Reserves uninitialized transmit storage for a validated frame loan spec.
    async fn loan_validated_uninit_tx(
        &self,
        spec: ValidatedTxLoanSpec,
    ) -> Result<Self::UninitTx, UStatus>;
}

impl<T> zero_copy_uninit_transport_sealed::Sealed for T where
    T: UZeroCopyUninitTransportImpl + ?Sized
{
}

/// Optional zero-copy capability for transports that can expose uninitialized TX payload storage.
#[async_trait]
pub trait UZeroCopyUninitTransport:
    UZeroCopyTransport + zero_copy_uninit_transport_sealed::Sealed
{
    /// Transport-specific uninitialized transmit loan type.
    type UninitTx: UUninitTxBuffer<Initialized = Self::Tx> + Send;

    /// Reserves uninitialized transmit storage for a validated frame loan spec.
    async fn loan_uninit_tx(&self, spec: UTxLoanSpec) -> Result<Self::UninitTx, UStatus>;
}

#[async_trait]
impl<T> UZeroCopyUninitTransport for T
where
    T: UZeroCopyUninitTransportImpl + ?Sized,
{
    type UninitTx = T::UninitTx;

    async fn loan_uninit_tx(&self, spec: UTxLoanSpec) -> Result<Self::UninitTx, UStatus> {
        UZeroCopyUninitTransportImpl::loan_validated_uninit_tx(
            self,
            ValidatedTxLoanSpec::try_from(spec)?,
        )
        .await
    }
}

/// Verifies the visible transmit payload layout exposed by a zero-copy loan.
///
/// # Errors
///
/// Returns an error if the requested layout is invalid or the exposed payload
/// range does not match it.
pub fn verify_tx_buffer_payload_layout(
    buffer: &mut impl UTxBuffer,
    payload_len: usize,
    alignment: usize,
) -> Result<(), UStatus> {
    validate_payload_layout(payload_len, alignment)?;
    let payload = buffer.payload_mut();
    verify_payload_slice_layout(payload, payload_len, alignment)
}

/// Verifies the visible uninitialized transmit payload layout exposed by a loan.
///
/// # Errors
///
/// Returns an error if the requested layout is invalid or the exposed payload
/// range does not match it.
pub fn verify_uninit_tx_buffer_payload_layout(
    buffer: &mut impl UUninitTxBuffer,
    payload_len: usize,
    alignment: usize,
) -> Result<(), UStatus> {
    validate_payload_layout(payload_len, alignment)?;
    let payload = buffer.payload_uninit_mut();
    if payload.len() != payload_len {
        return Err(internal_zero_copy_error(format!(
            "transport returned uninitialized TX payload length {} but {payload_len} was requested",
            payload.len()
        )));
    }
    if !payload.is_empty() && !(payload.as_ptr() as usize).is_multiple_of(alignment) {
        return Err(internal_zero_copy_error(format!(
            "transport returned uninitialized TX payload alignment {} but {alignment} was requested",
            payload.as_ptr() as usize
        )));
    }
    Ok(())
}

/// Validates a frame view before returning it from a public zero-copy boundary.
///
/// # Errors
///
/// Returns an error if metadata is invalid, payload presence disagrees with
/// payload encoding, or ordered slices do not match `payload_len`.
pub fn validate_frame_view_for_transport(
    frame: &(impl UFrameView + ?Sized),
) -> Result<(), UStatus> {
    validate_metadata(frame.metadata())?;
    validate_payload_presence(
        frame.has_payload(),
        frame.metadata().payload_encoding().is_some(),
        "frame view",
    )?;
    let mut observed = 0_usize;
    for slice in frame.payload_slices() {
        observed = observed
            .checked_add(slice.len())
            .ok_or_else(|| internal_zero_copy_error("frame view payload slices overflow usize"))?;
    }
    if observed != frame.payload_len() {
        return Err(internal_zero_copy_error(format!(
            "frame view payload slices yielded {observed} bytes but payload_len returned {}",
            frame.payload_len()
        )));
    }
    Ok(())
}

/// Validates a transmit buffer before committing it through a public zero-copy boundary.
///
/// # Errors
///
/// Returns an error if metadata is invalid or initialized payload bytes disagree
/// with payload encoding metadata.
pub fn validate_tx_buffer_for_transport(
    buffer: &mut (impl UTxBuffer + ?Sized),
) -> Result<(), UStatus> {
    validate_metadata(buffer.metadata())?;
    if !buffer.payload().is_empty() && buffer.metadata().payload_encoding().is_none() {
        return Err(invalid_argument(
            "TX buffer carries payload bytes without payload encoding",
        ));
    }
    Ok(())
}

/// Owned buffer useful for tests, examples, and adapters that emulate a transmit loan.
#[derive(Clone, Debug, PartialEq)]
pub struct UVecTxBuffer {
    metadata: UFrameMetadata,
    storage: Vec<u8>,
    payload_offset: usize,
    payload_len: usize,
}

/// Owned uninitialized buffer useful for tests and examples.
#[derive(Clone, Debug)]
pub struct UVecUninitTxBuffer {
    metadata: UFrameMetadata,
    storage: Vec<MaybeUninit<u8>>,
    payload_offset: usize,
    payload_len: usize,
}

/// In-memory receive lease for tests and examples that need receive-lease shape.
#[derive(Clone, Debug, PartialEq)]
pub struct UVecRxLease {
    metadata: UFrameMetadata,
    payload: Option<Vec<u8>>,
}

impl UVecTxBuffer {
    /// Creates an owned transmit buffer with `payload_len` zero-initialized bytes.
    #[must_use]
    pub fn new(metadata: UFrameMetadata, payload_len: usize) -> Self {
        Self {
            metadata,
            storage: vec![0_u8; payload_len],
            payload_offset: 0,
            payload_len,
        }
    }

    /// Creates an owned transmit buffer whose visible payload starts at `alignment`.
    ///
    /// # Errors
    ///
    /// Returns an error if the requested alignment is invalid or allocation size overflows.
    pub fn with_alignment(
        metadata: UFrameMetadata,
        payload_len: usize,
        alignment: usize,
    ) -> Result<Self, UStatus> {
        validate_payload_layout(payload_len, alignment)?;
        if payload_len == 0 {
            return Ok(Self::new(metadata, payload_len));
        }
        let extra = alignment.saturating_sub(1);
        let storage_len = payload_len.checked_add(extra).ok_or_else(|| {
            invalid_argument("payload length plus alignment padding overflows usize")
        })?;
        let storage = vec![0_u8; storage_len];
        let payload_offset = aligned_offset(storage.as_ptr() as usize, alignment);
        Ok(Self {
            metadata,
            storage,
            payload_offset,
            payload_len,
        })
    }

    /// Converts the buffer into an in-memory receive lease, consuming the emulated loan.
    #[must_use]
    pub fn into_rx_lease(self) -> UVecRxLease {
        let payload = if self.metadata.payload_encoding().is_some() {
            Some(
                self.storage
                    .get(self.payload_range())
                    .expect("UVecTxBuffer payload range must be in bounds")
                    .to_vec(),
            )
        } else {
            None
        };
        UVecRxLease::new_unchecked(self.metadata, payload)
    }

    fn payload_range(&self) -> std::ops::Range<usize> {
        let end = self
            .payload_offset
            .checked_add(self.payload_len)
            .expect("UVecTxBuffer payload range overflow");
        self.payload_offset..end
    }
}

impl UTxBuffer for UVecTxBuffer {
    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload(&self) -> &[u8] {
        self.storage
            .get(self.payload_range())
            .expect("UVecTxBuffer payload range must be in bounds")
    }

    fn payload_mut(&mut self) -> &mut [u8] {
        let range = self.payload_range();
        self.storage
            .get_mut(range)
            .expect("UVecTxBuffer payload range must be in bounds")
    }
}

impl UVecUninitTxBuffer {
    /// Creates an owned uninitialized transmit buffer.
    #[must_use]
    pub fn new(metadata: UFrameMetadata, payload_len: usize) -> Self {
        Self {
            metadata,
            storage: vec![MaybeUninit::uninit(); payload_len],
            payload_offset: 0,
            payload_len,
        }
    }

    /// Creates an owned uninitialized transmit buffer whose visible payload starts at `alignment`.
    ///
    /// # Errors
    ///
    /// Returns an error if the requested alignment is invalid or allocation size overflows.
    pub fn with_alignment(
        metadata: UFrameMetadata,
        payload_len: usize,
        alignment: usize,
    ) -> Result<Self, UStatus> {
        validate_payload_layout(payload_len, alignment)?;
        if payload_len == 0 {
            return Ok(Self::new(metadata, payload_len));
        }
        let extra = alignment.saturating_sub(1);
        let storage_len = payload_len.checked_add(extra).ok_or_else(|| {
            invalid_argument("payload length plus alignment padding overflows usize")
        })?;
        let storage = vec![MaybeUninit::uninit(); storage_len];
        let payload_offset = aligned_offset(storage.as_ptr() as usize, alignment);
        Ok(Self {
            metadata,
            storage,
            payload_offset,
            payload_len,
        })
    }

    fn payload_range(&self) -> std::ops::Range<usize> {
        let end = self
            .payload_offset
            .checked_add(self.payload_len)
            .expect("UVecUninitTxBuffer payload range overflow");
        self.payload_offset..end
    }
}

impl UUninitTxBuffer for UVecUninitTxBuffer {
    type Initialized = UVecTxBuffer;

    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload_len(&self) -> usize {
        self.payload_len
    }

    fn payload_uninit_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        let range = self.payload_range();
        self.storage
            .get_mut(range)
            .expect("UVecUninitTxBuffer payload range must be in bounds")
    }

    unsafe fn assume_payload_init(self) -> Self::Initialized {
        let Self {
            metadata,
            mut storage,
            payload_offset,
            payload_len,
        } = self;
        for slot in storage
            .get_mut(..payload_offset)
            .expect("UVecUninitTxBuffer prefix range must be in bounds")
        {
            slot.write(0);
        }
        let payload_end = payload_offset
            .checked_add(payload_len)
            .expect("UVecUninitTxBuffer payload range overflow");
        for slot in storage
            .get_mut(payload_end..)
            .expect("UVecUninitTxBuffer suffix range must be in bounds")
        {
            slot.write(0);
        }
        let len = storage.len();
        let capacity = storage.capacity();
        let ptr = storage.as_mut_ptr().cast::<u8>();
        std::mem::forget(storage);
        // SAFETY: `MaybeUninit<u8>` has the same layout as `u8`, prefix/suffix
        // bytes were initialized above, and the caller guarantees visible
        // payload bytes are initialized before conversion.
        let storage = unsafe { Vec::from_raw_parts(ptr, len, capacity) };
        UVecTxBuffer {
            metadata,
            storage,
            payload_offset,
            payload_len,
        }
    }
}

impl UVecRxLease {
    /// Creates an in-memory receive lease after validation.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata or payload presence is invalid.
    pub fn new(metadata: UFrameMetadata, payload: Option<Vec<u8>>) -> Result<Self, UStatus> {
        let frame = Self::new_unchecked(metadata, payload);
        validate_frame_view_for_transport(&frame)?;
        Ok(frame)
    }

    /// Creates an in-memory receive lease without validation.
    #[must_use]
    pub fn new_unchecked(metadata: UFrameMetadata, payload: Option<Vec<u8>>) -> Self {
        Self { metadata, payload }
    }

    /// Consumes the lease and returns metadata plus optional payload bytes.
    #[must_use]
    pub fn into_parts(self) -> (UFrameMetadata, Option<Vec<u8>>) {
        (self.metadata, self.payload)
    }

    fn payload_bytes(&self) -> &[u8] {
        self.payload.as_deref().unwrap_or_default()
    }
}

impl UFrameView for UVecRxLease {
    type PayloadReader<'a>
        = Cursor<&'a [u8]>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = std::option::IntoIter<&'a [u8]>
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload_len(&self) -> usize {
        self.payload_bytes().len()
    }

    fn has_payload(&self) -> bool {
        self.payload.is_some()
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        Cursor::new(self.payload_bytes())
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        self.payload.as_deref().into_iter()
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        self.payload.as_deref()
    }
}

impl UZeroCopyRxLease for UVecRxLease {}

impl ULoanedContiguousZeroCopyRxFrame for UVecRxLease {
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError> {
        let payload = self.payload.as_deref().ok_or(UWireError::MissingPayload)?;
        // SAFETY: `UVecRxLease` is the local vector-backed test receive lease
        // selected in `USR-04B` preflight as the positive fake loan proof.
        Ok(unsafe {
            LoanedPayload::new_unchecked(payload, PayloadLoanProvenance::OpaqueTransportLoan)
        })
    }
}

#[cfg(any(test, feature = "test-util"))]
#[derive(Default)]
struct InMemoryState {
    sent: Vec<UVecRxLease>,
    queue: VecDeque<UVecRxLease>,
    listeners: Vec<Arc<dyn UZeroCopyListener<UVecRxLease>>>,
}

/// In-memory zero-copy transport for tests and examples.
#[cfg(any(test, feature = "test-util"))]
#[derive(Clone, Default)]
pub struct InMemoryZeroCopyTransport {
    state: Arc<Mutex<InMemoryState>>,
}

#[cfg(any(test, feature = "test-util"))]
impl InMemoryZeroCopyTransport {
    /// Returns frames sent through [`UZeroCopyTransport::send_zero_copy`].
    #[must_use]
    pub fn sent_frames(&self) -> Vec<UVecRxLease> {
        self.state
            .lock()
            .expect("zero-copy state lock poisoned")
            .sent
            .clone()
    }

    /// Injects a frame into the receive queue and registered zero-copy listeners.
    pub async fn inject(&self, frame: UVecRxLease) {
        self.enqueue_and_dispatch(frame).await;
    }

    async fn enqueue_and_dispatch(&self, frame: UVecRxLease) {
        let listeners = {
            let mut state = self.state.lock().expect("zero-copy state lock poisoned");
            state.queue.push_back(frame.clone());
            state.listeners.clone()
        };
        for listener in listeners {
            listener.on_receive_zero_copy(frame.clone()).await;
        }
    }
}

#[cfg(any(test, feature = "test-util"))]
#[async_trait]
impl UZeroCopyTransportImpl for InMemoryZeroCopyTransport {
    type Tx = UVecTxBuffer;
    type Rx = UVecRxLease;

    async fn loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus> {
        UVecTxBuffer::with_alignment(
            spec.metadata().clone(),
            spec.payload_len(),
            spec.payload_alignment(),
        )
    }

    async fn send_validated_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
        let frame = buffer.into_rx_lease();
        {
            self.state
                .lock()
                .expect("zero-copy state lock poisoned")
                .sent
                .push(frame.clone());
        }
        self.enqueue_and_dispatch(frame).await;
        Ok(())
    }

    async fn receive_validated_zero_copy(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus> {
        self.state
            .lock()
            .expect("zero-copy state lock poisoned")
            .queue
            .pop_front()
            .ok_or_else(|| UStatus::fail_with_code(UCode::NotFound, "no frame available"))
    }

    async fn register_validated_zero_copy_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        self.state
            .lock()
            .expect("zero-copy state lock poisoned")
            .listeners
            .push(listener);
        Ok(())
    }

    async fn unregister_validated_zero_copy_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        let mut state = self.state.lock().expect("zero-copy state lock poisoned");
        let Some(index) = state
            .listeners
            .iter()
            .position(|existing| Arc::ptr_eq(existing, &listener))
        else {
            return Err(UStatus::fail_with_code(
                UCode::NotFound,
                "no such zero-copy listener registered for filters",
            ));
        };
        state.listeners.remove(index);
        Ok(())
    }
}

#[cfg(any(test, feature = "test-util"))]
#[async_trait]
impl UZeroCopyUninitTransportImpl for InMemoryZeroCopyTransport {
    type UninitTx = UVecUninitTxBuffer;

    async fn loan_validated_uninit_tx(
        &self,
        spec: ValidatedTxLoanSpec,
    ) -> Result<Self::UninitTx, UStatus> {
        UVecUninitTxBuffer::with_alignment(
            spec.metadata().clone(),
            spec.payload_len(),
            spec.payload_alignment(),
        )
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ZeroCopyListenerRegistrationKey {
    transport: usize,
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: usize,
}

static ZERO_COPY_LISTENER_REGISTRY: LazyLock<
    Mutex<HashMap<ZeroCopyListenerRegistrationKey, Arc<dyn Any + Send + Sync>>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));

struct ValidatingZeroCopyListener<Rx>
where
    Rx: UZeroCopyRxLease + Send + 'static,
{
    listener: Arc<dyn UZeroCopyListener<Rx>>,
}

#[async_trait]
impl<Rx> UZeroCopyListener<Rx> for ValidatingZeroCopyListener<Rx>
where
    Rx: UZeroCopyRxLease + Send + 'static,
{
    async fn on_receive_zero_copy(&self, frame: Rx) {
        match validate_frame_view_for_transport(&frame) {
            Ok(()) => self.listener.on_receive_zero_copy(frame).await,
            Err(error) => {
                warn!(%error, "dropping invalid zero-copy frame before listener delivery")
            }
        }
    }
}

fn registered_zero_copy_listener<Rx>(
    key: &ZeroCopyListenerRegistrationKey,
    listener: Arc<dyn UZeroCopyListener<Rx>>,
) -> (Arc<dyn UZeroCopyListener<Rx>>, bool)
where
    Rx: UZeroCopyRxLease + Send + 'static,
{
    let mut registry = ZERO_COPY_LISTENER_REGISTRY
        .lock()
        .expect("zero-copy listener registry lock poisoned");
    if let Some(existing) = registry.get(key) {
        if let Ok(existing) = existing
            .clone()
            .downcast::<ValidatingZeroCopyListener<Rx>>()
        {
            let existing: Arc<dyn UZeroCopyListener<Rx>> = existing;
            return (existing, false);
        }
    }

    let validating_listener = Arc::new(ValidatingZeroCopyListener { listener });
    registry.insert(key.clone(), validating_listener.clone());
    let validating_listener: Arc<dyn UZeroCopyListener<Rx>> = validating_listener;
    (validating_listener, true)
}

fn zero_copy_listener_for_unregister<Rx>(
    key: &ZeroCopyListenerRegistrationKey,
    fallback: Arc<dyn UZeroCopyListener<Rx>>,
) -> Arc<dyn UZeroCopyListener<Rx>>
where
    Rx: UZeroCopyRxLease + Send + 'static,
{
    ZERO_COPY_LISTENER_REGISTRY
        .lock()
        .expect("zero-copy listener registry lock poisoned")
        .get(key)
        .and_then(|listener| {
            listener
                .clone()
                .downcast::<ValidatingZeroCopyListener<Rx>>()
                .ok()
        })
        .map_or(fallback, |listener| {
            listener as Arc<dyn UZeroCopyListener<Rx>>
        })
}

fn zero_copy_transport_pointer<T: ?Sized>(transport: &T) -> usize {
    let ptr = transport as *const T;
    let thin_ptr = ptr as *const ();
    thin_ptr as usize
}

fn zero_copy_listener_pointer<Rx>(listener: &Arc<dyn UZeroCopyListener<Rx>>) -> usize
where
    Rx: UZeroCopyRxLease + Send + 'static,
{
    let ptr = Arc::as_ptr(listener);
    let thin_ptr = ptr as *const ();
    thin_ptr as usize
}

fn zero_copy_listener_registration_key<T: ?Sized>(
    transport: &T,
    source_filter: &UUri,
    sink_filter: Option<&UUri>,
    listener: usize,
) -> ZeroCopyListenerRegistrationKey {
    ZeroCopyListenerRegistrationKey {
        transport: zero_copy_transport_pointer(transport),
        source_filter: source_filter.clone(),
        sink_filter: sink_filter.cloned(),
        listener,
    }
}

fn verify_zero_copy_filter_criteria(
    source_filter: &UUri,
    sink_filter: Option<&UUri>,
) -> Result<(), UStatus> {
    verify_filter_criteria(source_filter, sink_filter).map_err(|status| *status)
}

fn validate_tx_loan_spec(spec: &UTxLoanSpec) -> Result<(), UStatus> {
    validate_metadata(spec.metadata())?;
    validate_payload_presence(
        spec.has_payload(),
        spec.metadata().payload_encoding().is_some(),
        "TX loan spec",
    )?;
    validate_payload_layout(spec.payload_len(), spec.payload_alignment())
}

fn validate_metadata(metadata: &UFrameMetadata) -> Result<(), UStatus> {
    metadata.validate().map_err(frame_metadata_error)
}

fn frame_metadata_error(error: UFrameMetadataError) -> UStatus {
    invalid_argument(format!("invalid frame metadata: {error}"))
}

fn validate_payload_presence(
    has_payload: bool,
    has_encoding: bool,
    context: &str,
) -> Result<(), UStatus> {
    match (has_payload, has_encoding) {
        (true, true) | (false, false) => Ok(()),
        (true, false) => Err(invalid_argument(format!(
            "{context} carries payload bytes without payload encoding"
        ))),
        (false, true) => Err(invalid_argument(format!(
            "{context} carries payload encoding without payload bytes"
        ))),
    }
}

fn validate_payload_layout(payload_len: usize, alignment: usize) -> Result<(), UStatus> {
    PayloadAlignment::new(alignment)?;
    let _ = payload_len;
    Ok(())
}

fn verify_payload_slice_layout(
    payload: &[u8],
    payload_len: usize,
    alignment: usize,
) -> Result<(), UStatus> {
    if payload.len() != payload_len {
        return Err(internal_zero_copy_error(format!(
            "transport returned TX payload length {} but {payload_len} was requested",
            payload.len()
        )));
    }
    if !payload.is_empty() && !(payload.as_ptr() as usize).is_multiple_of(alignment) {
        return Err(internal_zero_copy_error(format!(
            "transport returned TX payload alignment {} but {alignment} was requested",
            payload.as_ptr() as usize
        )));
    }
    Ok(())
}

fn aligned_offset(address: usize, alignment: usize) -> usize {
    if alignment == 1 {
        0
    } else {
        (alignment - (address % alignment)) % alignment
    }
}

fn invalid_argument(message: impl Into<String>) -> UStatus {
    UStatus::fail_with_code(UCode::InvalidArgument, message.into())
}

fn internal_zero_copy_error(message: impl Into<String>) -> UStatus {
    UStatus::fail_with_code(UCode::Internal, message.into())
}

fn stable_uninit_payload_ptr<T>(
    payload: &mut LoanedPayloadUninitMut<'_>,
) -> Result<NonNull<MaybeUninit<T>>, UWireError>
where
    T: StablePayload,
{
    let bytes = payload.as_uninit_bytes_mut_internal();
    StableContainerPayload::<T>::check_uninit_layout(bytes)?;
    NonNull::new(bytes.as_mut_ptr().cast::<MaybeUninit<T>>())
        .ok_or_else(|| UWireError::invalid_payload("stable payload slot pointer is null"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        payload::StablePayloadInitSlot, ByteBackedStablePayload, PayloadEncoding,
        StablePayloadVariant, UMessageBuilder, UPayloadFormat,
    };
    use std::sync::Mutex as StdMutex;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    struct StableBytes {
        bytes: [u8; 4],
    }

    unsafe impl StablePayload for StableBytes {
        const TYPE_NAME: &'static str = "uprotocol.test.StableBytes";
    }

    unsafe impl ByteBackedStablePayload for StableBytes {}

    struct StableBytesInit<'a> {
        slot: StablePayloadInitSlot<'a, StableBytes>,
        written: bool,
    }

    impl StableBytesInit<'_> {
        fn bytes_from_array(mut self, bytes: &[u8; 4]) -> Self {
            // SAFETY: `StableBytes` is `repr(C)` over exactly one `[u8; 4]` field
            // at offset zero, and this setter is the only write to that field.
            unsafe { self.slot.write_bytes(0, bytes) };
            self.written = true;
            self
        }

        fn finish(self) -> Result<InitializedStablePayload<StableBytes>, UWireError> {
            if !self.written {
                return Err(UWireError::invalid_payload(
                    "StableBytes.bytes was not initialized",
                ));
            }
            // SAFETY: the only field spans the full payload and has been written.
            Ok(unsafe { self.slot.assume_init() })
        }
    }

    // SAFETY: `StableBytesInit::finish` is available only after construction via
    // this test builder path and writes the complete `[u8; 4]` payload field.
    unsafe impl StablePayloadInit for StableBytes {
        type Init<'a> = StableBytesInit<'a>;

        fn init_from_uninit_bytes<'a>(
            payload: &'a mut [MaybeUninit<u8>],
        ) -> Result<Self::Init<'a>, UWireError> {
            Ok(StableBytesInit {
                slot: StablePayloadInitSlot::from_uninit_bytes(payload)?,
                written: false,
            })
        }

        fn __init_from_slot<'a>(
            slot: StablePayloadInitSlot<'a, Self>,
        ) -> Result<Self::Init<'a>, UWireError> {
            Ok(StableBytesInit {
                slot,
                written: false,
            })
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct OtherStableBytes {
        bytes: [u8; 4],
    }

    unsafe impl StablePayload for OtherStableBytes {
        const TYPE_NAME: &'static str = "uprotocol.test.OtherStableBytes";
    }

    #[repr(C, align(4))]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct AlignedStableBytes {
        bytes: [u8; 4],
    }

    unsafe impl StablePayload for AlignedStableBytes {
        const TYPE_NAME: &'static str = "uprotocol.test.AlignedStableBytes";
    }

    fn topic() -> UUri {
        UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).expect("failed to create topic")
    }

    fn wildcard_topic_filter() -> UUri {
        UUri::try_from_parts("vehicle", 0x4210, 0x01, 0xffff).expect("failed to create filter")
    }

    fn metadata_without_encoding() -> UFrameMetadata {
        let message = UMessageBuilder::publish(topic()).build().expect("message");
        UFrameMetadata::new(message.attributes().clone(), None).expect("metadata")
    }

    fn metadata_with_encoding() -> UFrameMetadata {
        let message = UMessageBuilder::publish(topic()).build().expect("message");
        UFrameMetadata::new(
            message.attributes().clone(),
            Some(PayloadEncoding::Standard(UPayloadFormat::Raw)),
        )
        .expect("metadata")
    }

    fn stable_metadata<T: StablePayload>() -> UFrameMetadata {
        let message = UMessageBuilder::publish(topic()).build().expect("message");
        UFrameMetadata::new(
            message.attributes().clone(),
            Some(StableContainerPayload::<T>::encoding()),
        )
        .expect("metadata")
    }

    fn stable_bytes(value: &StableBytes) -> Vec<u8> {
        // SAFETY: `StableBytes` is `repr(C)` over `[u8; 4]`, has no padding or
        // drop glue, and every byte pattern is valid for the test type.
        unsafe {
            std::slice::from_raw_parts(
                std::ptr::from_ref(value).cast::<u8>(),
                std::mem::size_of::<StableBytes>(),
            )
            .to_vec()
        }
    }

    fn stable_encoding_with<T: StablePayload>(
        type_name: &str,
        variant: StablePayloadVariant,
        size: usize,
        alignment: usize,
    ) -> PayloadEncoding {
        PayloadEncoding::custom(
            StableContainerPayload::<T>::ENCODING_ID,
            format!(
                "application/vnd.uprotocol.stable-container;type=\"{type_name}\";variant={variant};size={size};align={alignment}"
            ),
        )
        .expect("stable encoding")
    }

    #[test]
    fn tx_loan_no_payload_rejects_encoding() {
        let error = UTxLoanSpec::no_payload(metadata_with_encoding()).unwrap_err();
        assert_eq!(error.get_code(), UCode::InvalidArgument);
    }

    #[test]
    fn tx_loan_payload_rejects_missing_encoding() {
        let error = UTxLoanSpec::payload(metadata_without_encoding(), 8, 1).unwrap_err();
        assert_eq!(error.get_code(), UCode::InvalidArgument);
    }

    #[test]
    fn present_empty_payload_preserves_encoding() {
        let spec = UTxLoanSpec::present_empty_payload(metadata_with_encoding()).unwrap();
        assert!(spec.has_payload());
        assert_eq!(spec.payload_len(), 0);
        assert_eq!(spec.payload_alignment(), 1);
        assert_eq!(
            spec.metadata().payload_encoding(),
            metadata_with_encoding().payload_encoding()
        );
    }

    #[test]
    fn payload_alignment_must_be_power_of_two() {
        let error = UTxPayloadSpec::present(16, 3).unwrap_err();
        assert_eq!(error.get_code(), UCode::InvalidArgument);
    }

    #[test]
    fn payload_alignment_proof_rejects_zero() {
        let error = PayloadAlignment::new(0).unwrap_err();
        assert_eq!(error.get_code(), UCode::InvalidArgument);
    }

    #[test]
    fn payload_alignment_proof_rejects_non_power_of_two() {
        let error = PayloadAlignment::new(6).unwrap_err();
        assert_eq!(error.get_code(), UCode::InvalidArgument);
    }

    #[test]
    fn payload_alignment_proof_round_trips_raw_value() {
        let alignment = PayloadAlignment::new(8).unwrap();
        let spec = UTxLoanSpec::new(
            metadata_with_encoding(),
            UTxPayloadSpec::present_with_alignment(16, alignment),
        )
        .unwrap();
        let validated = ValidatedTxLoanSpec::try_from(spec).unwrap();

        assert_eq!(alignment.as_usize(), 8);
        assert_eq!(validated.payload_alignment(), 8);
        assert_eq!(validated.payload_alignment_proof(), alignment);
    }

    struct CountingTransport {
        loan_calls: StdMutex<usize>,
        send_calls: StdMutex<usize>,
        receive: StdMutex<Option<UVecRxLease>>,
    }

    #[async_trait]
    impl UZeroCopyTransportImpl for CountingTransport {
        type Tx = UVecTxBuffer;
        type Rx = UVecRxLease;

        async fn loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus> {
            *self.loan_calls.lock().expect("loan lock") += 1;
            UVecTxBuffer::with_alignment(
                spec.metadata().clone(),
                spec.payload_len(),
                spec.payload_alignment(),
            )
        }

        async fn send_validated_zero_copy(&self, _buffer: Self::Tx) -> Result<(), UStatus> {
            *self.send_calls.lock().expect("send lock") += 1;
            Ok(())
        }

        async fn receive_validated_zero_copy(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
        ) -> Result<Self::Rx, UStatus> {
            self.receive
                .lock()
                .expect("receive lock")
                .take()
                .ok_or_else(|| UStatus::fail_with_code(UCode::NotFound, "none"))
        }
    }

    #[tokio::test]
    async fn invalid_metadata_rejected_before_loan_implementation() {
        let transport = CountingTransport {
            loan_calls: StdMutex::new(0),
            send_calls: StdMutex::new(0),
            receive: StdMutex::new(None),
        };
        let invalid = UTxLoanSpec::new_unchecked(metadata_with_encoding(), UTxPayloadSpec::Absent);

        let error = transport.loan_tx(invalid).await.unwrap_err();

        assert_eq!(error.get_code(), UCode::InvalidArgument);
        assert_eq!(*transport.loan_calls.lock().expect("loan lock"), 0);
    }

    #[tokio::test]
    async fn invalid_tx_buffer_rejected_before_send_implementation() {
        let transport = CountingTransport {
            loan_calls: StdMutex::new(0),
            send_calls: StdMutex::new(0),
            receive: StdMutex::new(None),
        };
        let buffer = UVecTxBuffer::new(metadata_without_encoding(), 4);

        let error = transport.send_zero_copy(buffer).await.unwrap_err();

        assert_eq!(error.get_code(), UCode::InvalidArgument);
        assert_eq!(*transport.send_calls.lock().expect("send lock"), 0);
    }

    #[tokio::test]
    async fn invalid_receive_lease_rejected_before_return() {
        let transport = CountingTransport {
            loan_calls: StdMutex::new(0),
            send_calls: StdMutex::new(0),
            receive: StdMutex::new(Some(UVecRxLease::new_unchecked(
                metadata_with_encoding(),
                None,
            ))),
        };

        let error = transport
            .receive_zero_copy(&wildcard_topic_filter(), None)
            .await
            .unwrap_err();

        assert_eq!(error.get_code(), UCode::InvalidArgument);
    }

    struct SegmentedFrame {
        metadata: UFrameMetadata,
        first: Vec<u8>,
        second: Vec<u8>,
    }

    impl UFrameView for SegmentedFrame {
        type PayloadReader<'a>
            = Cursor<Vec<u8>>
        where
            Self: 'a;
        type PayloadSlices<'a>
            = std::vec::IntoIter<&'a [u8]>
        where
            Self: 'a;

        fn metadata(&self) -> &UFrameMetadata {
            &self.metadata
        }

        fn payload_len(&self) -> usize {
            self.first.len() + self.second.len()
        }

        fn payload_reader(&self) -> Self::PayloadReader<'_> {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&self.first);
            bytes.extend_from_slice(&self.second);
            Cursor::new(bytes)
        }

        fn payload_slices(&self) -> Self::PayloadSlices<'_> {
            vec![self.first.as_slice(), self.second.as_slice()].into_iter()
        }
    }

    #[test]
    fn frame_view_reader_and_slices_preserve_order() {
        let frame = SegmentedFrame {
            metadata: metadata_with_encoding(),
            first: b"abc".to_vec(),
            second: b"def".to_vec(),
        };
        let mut from_reader = Vec::new();
        frame
            .payload_reader()
            .read_to_end(&mut from_reader)
            .expect("reader");
        let from_slices: Vec<u8> = frame.payload_slices().flatten().copied().collect();

        assert_eq!(from_reader, b"abcdef");
        assert_eq!(from_slices, b"abcdef");
        validate_frame_view_for_transport(&frame).unwrap();
    }

    #[test]
    fn stable_borrow_accepts_loan_backed_contiguous_payload() {
        let value = StableBytes { bytes: *b"loan" };
        let frame = UVecRxLease::new(stable_metadata::<StableBytes>(), Some(stable_bytes(&value)))
            .expect("stable frame");

        let borrowed = frame.borrow_stable_payload::<StableBytes>().unwrap();

        assert_eq!(borrowed, &value);
        assert_eq!(
            frame.payload_loan_provenance().unwrap(),
            PayloadLoanProvenance::OpaqueTransportLoan
        );
    }

    #[test]
    fn stable_borrow_rejects_wrong_type_metadata() {
        let value = StableBytes { bytes: *b"loan" };
        let frame = UVecRxLease::new(
            stable_metadata::<OtherStableBytes>(),
            Some(stable_bytes(&value)),
        )
        .expect("stable frame");

        let error = frame.borrow_stable_payload::<StableBytes>().unwrap_err();

        assert!(matches!(error, UWireError::InvalidPayload(_)));
    }

    #[test]
    fn stable_borrow_rejects_wrong_size_metadata() {
        let value = StableBytes { bytes: *b"loan" };
        let message = UMessageBuilder::publish(topic()).build().expect("message");
        let metadata = UFrameMetadata::new(
            message.attributes().clone(),
            Some(stable_encoding_with::<StableBytes>(
                StableBytes::TYPE_NAME,
                StablePayloadVariant::FixedSize,
                std::mem::size_of::<StableBytes>() + 1,
                std::mem::align_of::<StableBytes>(),
            )),
        )
        .expect("metadata");
        let frame = UVecRxLease::new(metadata, Some(stable_bytes(&value))).expect("stable frame");

        let error = frame.borrow_stable_payload::<StableBytes>().unwrap_err();

        assert!(matches!(error, UWireError::InvalidPayload(_)));
    }

    #[test]
    fn stable_borrow_rejects_insufficient_advertised_alignment() {
        let message = UMessageBuilder::publish(topic()).build().expect("message");
        let metadata = UFrameMetadata::new(
            message.attributes().clone(),
            Some(stable_encoding_with::<AlignedStableBytes>(
                AlignedStableBytes::TYPE_NAME,
                StablePayloadVariant::FixedSize,
                std::mem::size_of::<AlignedStableBytes>(),
                1,
            )),
        )
        .expect("metadata");
        let frame = UVecRxLease::new(metadata, Some(vec![0_u8; 4])).expect("stable frame");

        let error = frame
            .borrow_stable_payload::<AlignedStableBytes>()
            .unwrap_err();

        assert!(matches!(error, UWireError::InvalidPayload(_)));
    }

    #[test]
    fn stable_borrow_rejects_payload_length_mismatch() {
        let frame = UVecRxLease::new(stable_metadata::<StableBytes>(), Some(vec![1, 2, 3]))
            .expect("stable frame");

        let error = frame.borrow_stable_payload::<StableBytes>().unwrap_err();

        assert!(
            matches!(error, UWireError::InvalidPayload(message) if message.contains("payload length"))
        );
    }

    #[test]
    fn stable_borrow_rejects_absent_payload() {
        let frame = UVecRxLease::new_unchecked(stable_metadata::<StableBytes>(), None);

        let error = frame.borrow_stable_payload::<StableBytes>().unwrap_err();

        assert_eq!(error, UWireError::MissingPayload);
    }

    #[test]
    fn segmented_frame_is_not_loan_backed_proof() {
        let frame = SegmentedFrame {
            metadata: stable_metadata::<StableBytes>(),
            first: vec![1, 2],
            second: vec![3, 4],
        };

        assert_eq!(frame.try_contiguous_payload(), None);
        validate_frame_view_for_transport(&frame).unwrap();
    }

    #[tokio::test]
    async fn in_memory_zero_copy_transport_round_trips_payload() {
        let transport = InMemoryZeroCopyTransport::default();
        let spec = UTxLoanSpec::payload(metadata_with_encoding(), 4, 1).unwrap();
        let mut buffer = transport.loan_tx(spec).await.unwrap();
        buffer.payload_mut().copy_from_slice(b"test");

        transport.send_zero_copy(buffer).await.unwrap();
        assert_eq!(transport.sent_frames().len(), 1);
        let received = transport
            .receive_zero_copy(&wildcard_topic_filter(), None)
            .await
            .unwrap();

        assert_eq!(received.try_contiguous_payload(), Some(b"test".as_slice()));

        transport
            .inject(UVecRxLease::new(metadata_with_encoding(), Some(b"next".to_vec())).unwrap())
            .await;
        let injected = transport
            .receive_zero_copy(&wildcard_topic_filter(), None)
            .await
            .unwrap();
        assert_eq!(injected.try_contiguous_payload(), Some(b"next".as_slice()));
    }

    #[tokio::test]
    async fn stable_initialized_tx_helper_sends_stable_payload() {
        let transport = InMemoryZeroCopyTransport::default();

        transport
            .send_loaned_payload_as::<StableContainerPayload<StableBytes>, StableBytes>(
                stable_metadata::<StableBytes>(),
                |payload| payload.bytes.copy_from_slice(b"init"),
            )
            .await
            .expect("send initialized stable payload");

        let sent = transport.sent_frames();
        assert_eq!(sent.len(), 1);
        let sent = sent.first().expect("one sent frame");
        assert_eq!(
            sent.metadata().payload_encoding(),
            Some(&StableContainerPayload::<StableBytes>::encoding())
        );
        assert_eq!(
            sent.borrow_stable_payload::<StableBytes>().unwrap(),
            &StableBytes { bytes: *b"init" }
        );
    }

    #[tokio::test]
    async fn stable_uninit_tx_helper_sends_byte_backed_payload() {
        let transport = InMemoryZeroCopyTransport::default();

        transport
            .send_uninit_loaned_payload_as::<StableContainerPayload<StableBytes>, StableBytes>(
                stable_metadata::<StableBytes>(),
                |slot| Ok(slot.write(StableBytes { bytes: *b"noze" })),
            )
            .await
            .expect("send uninit stable payload");

        let sent = transport.sent_frames();
        assert_eq!(sent.len(), 1);
        let sent = sent.first().expect("one sent frame");
        assert_eq!(
            sent.borrow_stable_payload::<StableBytes>().unwrap(),
            &StableBytes { bytes: *b"noze" }
        );
    }

    #[tokio::test]
    async fn stable_uninit_tx_helper_uses_stable_payload_init_builder() {
        let transport = InMemoryZeroCopyTransport::default();

        transport
            .send_uninit_stable_payload_as::<StableBytes>(
                stable_metadata::<StableBytes>(),
                |init| init.bytes_from_array(b"zcpy").finish(),
            )
            .await
            .expect("send stable init payload");

        let sent = transport.sent_frames();
        assert_eq!(sent.len(), 1);
        let sent = sent.first().expect("one sent frame");
        assert_eq!(
            sent.borrow_stable_payload::<StableBytes>().unwrap(),
            &StableBytes { bytes: *b"zcpy" }
        );
    }

    #[tokio::test]
    async fn stable_uninit_tx_rejects_detached_init_proof() {
        let transport = InMemoryZeroCopyTransport::default();

        let error = transport
            .send_uninit_stable_payload_as::<StableBytes>(
                stable_metadata::<StableBytes>(),
                |_init| {
                    let mut detached =
                        vec![MaybeUninit::<u8>::uninit(); std::mem::size_of::<StableBytes>()];
                    StableBytes::init_from_uninit_bytes(&mut detached)?
                        .bytes_from_array(b"bad!")
                        .finish()
                },
            )
            .await
            .expect_err("detached init proof must not commit loan");

        assert_eq!(error.get_code(), UCode::InvalidArgument);
        assert!(transport.sent_frames().is_empty());
    }

    #[cfg(feature = "owned-frame-transport")]
    #[test]
    fn owned_frame_implements_frame_view_when_feature_enabled() {
        let frame = UOwnedFrame::with_payload(
            metadata_with_encoding(),
            bytes::Bytes::from_static(b"owned"),
        )
        .expect("owned frame");

        assert_eq!(UFrameView::payload_len(&frame), 5);
        assert_eq!(frame.try_contiguous_payload(), Some(b"owned".as_slice()));
    }
}
