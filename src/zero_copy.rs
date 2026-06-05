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
    sync::{Arc, LazyLock, Mutex},
};

#[cfg(any(test, feature = "test-util"))]
use std::collections::VecDeque;

use async_trait::async_trait;
use tracing::warn;

#[cfg(feature = "owned-frame-transport")]
use crate::UOwnedFrame;
use crate::{
    utransport::verify_filter_criteria, UCode, UFrameMetadata, UFrameMetadataError, UStatus, UUri,
};

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
    Present { len: usize, alignment: usize },
}

impl UTxPayloadSpec {
    /// Creates a present-payload spec from length and alignment.
    ///
    /// # Errors
    ///
    /// Returns an error if `alignment` is zero or not a power of two.
    pub fn present(len: usize, alignment: usize) -> Result<Self, UStatus> {
        validate_payload_layout(len, alignment)?;
        Ok(Self::Present { len, alignment })
    }

    /// Creates a present-empty-payload spec.
    #[must_use]
    pub fn present_empty() -> Self {
        Self::Present {
            len: 0,
            alignment: 1,
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
        match self {
            Self::Absent => 1,
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
    if alignment == 0 {
        return Err(invalid_argument(
            "payload alignment must be greater than zero",
        ));
    }
    if !alignment.is_power_of_two() {
        return Err(invalid_argument("payload alignment must be a power of two"));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PayloadEncoding, UMessageBuilder, UPayloadFormat};
    use std::sync::Mutex as StdMutex;

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
