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

//! Selected-wire transport adapter core.
//!
//! Product transports implement the small core traits in this module for their
//! physical mechanics. A core accepts metadata bytes already prepared by the
//! selected `W: UWireMetadata`, returns encoded metadata bytes on receive, and
//! validates any physical mirror fields against decoded metadata before public
//! exposure. Product transport modules should not import, match on, or branch
//! by concrete wire families such as `UProtocolNativeWire`; the selected wire is
//! owned by [`UWireTransport<TCore, W>`].

use std::{
    any::Any,
    collections::HashMap,
    io::Read,
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
#[cfg(feature = "owned-frame-transport")]
use bytes::Bytes;
use tracing::warn;

use crate::{
    validate_frame_view_for_transport, LoanedPayload, PayloadCodec, ReadDecodePayload, UCode,
    UFrameMetadata, UFrameView, ULoanedContiguousZeroCopyRxFrame, UStatus, UTxBuffer, UTxLoanSpec,
    UUninitTxBuffer, UUri, UWire, UWireError, UWireMetadata, UZeroCopyListener, UZeroCopyRxLease,
    UZeroCopyTransportImpl, UZeroCopyUninitTransportImpl, ValidatedTxLoanSpec,
};
#[cfg(feature = "owned-frame-transport")]
use crate::{UOwnedFrame, UOwnedListener, UOwnedTransportImpl, ValidatedOwnedFrame};

/// Generic selected-wire transport adapter.
pub struct UWireTransport<TCore, W>
where
    W: UWire,
{
    core: TCore,
    wire: W,
    zero_copy_listeners: Mutex<HashMap<WireListenerKey, Arc<dyn Any + Send + Sync>>>,
    #[cfg(feature = "owned-frame-transport")]
    owned_listeners: Mutex<HashMap<WireListenerKey, Arc<dyn Any + Send + Sync>>>,
}

impl<TCore, W> UWireTransport<TCore, W>
where
    W: UWire,
{
    /// Creates an adapter around a transport core and a selected wire marker.
    #[must_use]
    pub fn new(core: TCore, wire: W) -> Self {
        Self {
            core,
            wire,
            zero_copy_listeners: Mutex::new(HashMap::new()),
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

    /// Consumes the adapter and returns the wrapped core and selected wire.
    #[must_use]
    pub fn into_parts(self) -> (TCore, W) {
        (self.core, self.wire)
    }
}

/// Construction helper for choosing a selected wire on a physical core.
pub trait UWithWire<W>: Sized
where
    W: UWire,
{
    /// Wraps this core in a selected-wire transport adapter.
    #[must_use]
    fn with_wire(self, wire: W) -> UWireTransport<Self, W>;
}

impl<TCore, W> UWithWire<W> for TCore
where
    W: UWire,
{
    fn with_wire(self, wire: W) -> UWireTransport<Self, W> {
        UWireTransport::new(self, wire)
    }
}

/// Exposes the concrete selected wire of an adapter value.
pub trait UHasWire {
    /// Concrete selected wire type.
    type Wire: UWire;

    /// Returns the selected wire marker/value.
    #[must_use]
    fn wire(&self) -> &Self::Wire;
}

impl<TCore, W> UHasWire for UWireTransport<TCore, W>
where
    W: UWire,
{
    type Wire = W;

    fn wire(&self) -> &Self::Wire {
        &self.wire
    }
}

/// Prepared zero-copy transmit request passed from the adapter to a core.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedTxLoanSpec {
    metadata: UFrameMetadata,
    encoded_metadata: Vec<u8>,
    payload_len: usize,
    payload_alignment: usize,
}

impl PreparedTxLoanSpec {
    /// Encodes validated metadata for a selected wire.
    ///
    /// # Errors
    ///
    /// Returns an error if selected-wire metadata encoding fails.
    pub fn from_validated<W>(spec: ValidatedTxLoanSpec) -> Result<Self, UStatus>
    where
        W: UWireMetadata,
    {
        let encoded_metadata = W::encode_frame_metadata(spec.metadata())?;
        Ok(Self {
            metadata: spec.metadata().clone(),
            encoded_metadata,
            payload_len: spec.payload_len(),
            payload_alignment: spec.payload_alignment(),
        })
    }

    /// Creates a prepared loan spec from metadata bytes that are already encoded
    /// for the selected wire associated with `metadata`.
    ///
    /// This is an advanced adapter/core boundary helper for owned-loopback
    /// bridges. Callers must pass `encoded_metadata` produced by the same
    /// selected wire that will decode `metadata` on receive; this constructor
    /// validates the decoded metadata and payload layout but cannot prove that
    /// the opaque metadata bytes were produced by a particular wire.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata, payload presence, or payload alignment is
    /// invalid for a zero-copy transmit loan.
    pub fn from_encoded_parts(
        metadata: UFrameMetadata,
        encoded_metadata: impl Into<Vec<u8>>,
        payload_len: usize,
        payload_alignment: usize,
    ) -> Result<Self, UStatus> {
        let spec = if metadata.payload_encoding().is_some() {
            UTxLoanSpec::payload(metadata, payload_len, payload_alignment)?
        } else {
            if payload_len != 0 {
                return Err(UStatus::fail_with_code(
                    UCode::InvalidArgument,
                    "prepared TX spec without payload encoding cannot carry payload bytes",
                ));
            }
            if payload_alignment != 1 {
                return Err(UStatus::fail_with_code(
                    UCode::InvalidArgument,
                    "prepared TX spec without payload uses alignment 1",
                ));
            }
            UTxLoanSpec::no_payload(metadata)?
        };
        let spec = ValidatedTxLoanSpec::try_from(spec)?;
        Ok(Self {
            metadata: spec.metadata().clone(),
            encoded_metadata: encoded_metadata.into(),
            payload_len: spec.payload_len(),
            payload_alignment: spec.payload_alignment(),
        })
    }

    /// Returns the decoded frame metadata used to prepare this request.
    #[must_use]
    pub fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    /// Returns selected-wire encoded metadata bytes.
    #[must_use]
    pub fn encoded_metadata(&self) -> &[u8] {
        &self.encoded_metadata
    }

    /// Returns the visible application payload length requested from the core.
    #[must_use]
    pub fn payload_len(&self) -> usize {
        self.payload_len
    }

    /// Returns the visible application payload alignment requested from the core.
    #[must_use]
    pub fn payload_alignment(&self) -> usize {
        self.payload_alignment
    }

    /// Returns whether the request carries a payload, including a present empty payload.
    #[must_use]
    pub fn has_payload(&self) -> bool {
        self.metadata.payload_encoding().is_some()
    }

    /// Consumes this request and returns its parts.
    #[must_use]
    pub fn into_parts(self) -> (UFrameMetadata, Vec<u8>, usize, usize) {
        (
            self.metadata,
            self.encoded_metadata,
            self.payload_len,
            self.payload_alignment,
        )
    }
}

/// Raw encoded receive object returned by a transport core.
///
/// This is an implementation-boundary trait. Raw encoded receive objects should
/// not implement public frame or lease traits directly; public receive paths
/// expose [`UWireRx<Rx, W>`] after selected-wire metadata decode and validation.
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

/// Raw encoded receive object that can prove its contiguous payload is loan-backed.
pub trait UEncodedLoanedRxFrame: UEncodedRxFrame {
    /// Returns one contiguous loan-backed application payload view.
    ///
    /// Implementations must not allocate, copy, or coalesce payload bytes to
    /// satisfy this method.
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError>;
}

/// Public zero-copy receive lease after selected-wire metadata validation.
pub struct UWireRx<Rx, W>
where
    W: UWire,
{
    metadata: UFrameMetadata,
    raw: Rx,
    _wire: PhantomData<W>,
}

impl<Rx, W> UWireRx<Rx, W>
where
    Rx: UEncodedRxFrame,
    W: UWireMetadata,
{
    /// Decodes metadata from a raw encoded receive object and validates the public frame view.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata bytes are malformed, selected-wire checks
    /// fail, or the resulting public frame view violates transport invariants.
    pub fn try_from_encoded(raw: Rx) -> Result<Self, UStatus> {
        let metadata = W::decode_frame_metadata(raw.encoded_metadata())?;
        let frame = Self {
            metadata,
            raw,
            _wire: PhantomData,
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
    /// `.with_wire(W)`. Use [`UFrameView::decode_payload_from_reader_as`] only for
    /// low-level codec escape hatches.
    ///
    /// # Errors
    ///
    /// Returns an error if the frame has missing or incompatible payload encoding,
    /// has no payload, or if the selected wire cannot decode the payload bytes.
    pub fn decode_payload<T>(&self) -> Result<T, UWireError>
    where
        W: PayloadCodec + ReadDecodePayload<T>,
    {
        W::verify_encoding(self.metadata.payload_encoding())?;
        if !self.has_payload() {
            return Err(UWireError::MissingPayload);
        }
        W::decode_payload_from_reader(self.payload_reader(), self.payload_len())
    }
}

impl<Rx, W> UFrameView for UWireRx<Rx, W>
where
    Rx: UEncodedRxFrame,
    W: UWireMetadata,
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

impl<Rx, W> UZeroCopyRxLease for UWireRx<Rx, W>
where
    Rx: UEncodedRxFrame,
    W: UWireMetadata,
{
}

impl<Rx, W> ULoanedContiguousZeroCopyRxFrame for UWireRx<Rx, W>
where
    Rx: UEncodedLoanedRxFrame,
    W: UWireMetadata,
{
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError> {
        self.raw.loaned_contiguous_payload()
    }
}

/// Listener used by cores to deliver raw encoded zero-copy receive objects.
#[async_trait]
pub trait UEncodedZeroCopyListener<Rx>: Send + Sync
where
    Rx: UEncodedRxFrame + Send + 'static,
{
    /// Handles one raw encoded receive object.
    async fn on_receive_encoded_zero_copy(&self, frame: Rx);
}

/// Physical zero-copy mechanics implemented by product transports.
#[async_trait]
pub trait UZeroCopyTransportCore: Send + Sync {
    /// Transport-specific transmit loan type.
    type Tx: UTxBuffer + Send;

    /// Transport-specific raw encoded receive type.
    type Rx: UEncodedRxFrame + Send + 'static;

    /// Reserves transmit storage for a request with already encoded metadata bytes.
    async fn loan_prepared_tx(&self, spec: PreparedTxLoanSpec) -> Result<Self::Tx, UStatus>;

    /// Commits a transmit loan prepared by this core.
    async fn send_prepared_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus>;

    /// Receives one matching raw encoded frame from cores that support pull receive.
    async fn receive_encoded_zero_copy(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus> {
        Err(unimplemented())
    }

    /// Registers a raw encoded listener after public filter validation.
    async fn register_encoded_zero_copy_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UEncodedZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        Err(unimplemented())
    }

    /// Unregisters a raw encoded listener after public filter validation.
    async fn unregister_encoded_zero_copy_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UEncodedZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        Err(unimplemented())
    }
}

/// Optional physical uninitialized-loan mechanics implemented by product transports.
#[async_trait]
pub trait UZeroCopyUninitTransportCore: UZeroCopyTransportCore {
    /// Transport-specific uninitialized transmit loan type.
    type UninitTx: UUninitTxBuffer<Initialized = Self::Tx> + Send;

    /// Reserves uninitialized storage for a request with already encoded metadata bytes.
    async fn loan_prepared_uninit_tx(
        &self,
        spec: PreparedTxLoanSpec,
    ) -> Result<Self::UninitTx, UStatus>;
}

#[async_trait]
impl<TCore, W> UZeroCopyTransportImpl for UWireTransport<TCore, W>
where
    TCore: UZeroCopyTransportCore,
    W: UWireMetadata + Send + Sync + 'static,
{
    type Tx = TCore::Tx;
    type Rx = UWireRx<TCore::Rx, W>;

    async fn loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus> {
        self.core
            .loan_prepared_tx(PreparedTxLoanSpec::from_validated::<W>(spec)?)
            .await
    }

    async fn send_validated_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
        self.core.send_prepared_zero_copy(buffer).await
    }

    async fn receive_validated_zero_copy(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus> {
        let core_source_filter = selected_wire_core_source_filter_for(source_filter);
        loop {
            let frame = self
                .core
                .receive_encoded_zero_copy(&core_source_filter, sink_filter)
                .await?;
            let frame = UWireRx::try_from_encoded(frame)?;
            if wire_frame_matches(&frame, source_filter, sink_filter) {
                return Ok(frame);
            }
        }
    }

    async fn register_validated_zero_copy_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        let key = listener_key(
            source_filter,
            sink_filter,
            zero_copy_listener_pointer::<TCore::Rx, W>(&listener),
        );
        let (listener, inserted) =
            self.registered_zero_copy_listener(&key, source_filter, sink_filter, listener);
        let core_source_filter = selected_wire_core_source_filter();
        let result = self
            .core
            .register_encoded_zero_copy_listener(&core_source_filter, None, listener)
            .await;
        if result.is_err() && inserted {
            self.zero_copy_listeners
                .lock()
                .expect("wire zero-copy listener registry lock poisoned")
                .remove(&key);
        }
        result
    }

    async fn unregister_validated_zero_copy_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        let key = listener_key(
            source_filter,
            sink_filter,
            zero_copy_listener_pointer::<TCore::Rx, W>(&listener),
        );
        let listener = self.zero_copy_listener_for_unregister(&key, listener);
        let core_source_filter = selected_wire_core_source_filter();
        let result = self
            .core
            .unregister_encoded_zero_copy_listener(&core_source_filter, None, listener)
            .await;
        if result.is_ok() {
            self.zero_copy_listeners
                .lock()
                .expect("wire zero-copy listener registry lock poisoned")
                .remove(&key);
        }
        result
    }
}

#[async_trait]
impl<TCore, W> UZeroCopyUninitTransportImpl for UWireTransport<TCore, W>
where
    TCore: UZeroCopyUninitTransportCore,
    W: UWireMetadata + Send + Sync + 'static,
{
    type UninitTx = TCore::UninitTx;

    async fn loan_validated_uninit_tx(
        &self,
        spec: ValidatedTxLoanSpec,
    ) -> Result<Self::UninitTx, UStatus> {
        self.core
            .loan_prepared_uninit_tx(PreparedTxLoanSpec::from_validated::<W>(spec)?)
            .await
    }
}

impl<TCore, W> UWireTransport<TCore, W>
where
    TCore: UZeroCopyTransportCore,
    W: UWireMetadata + Send + Sync + 'static,
{
    fn registered_zero_copy_listener(
        &self,
        key: &WireListenerKey,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<UWireRx<TCore::Rx, W>>>,
    ) -> (Arc<dyn UEncodedZeroCopyListener<TCore::Rx>>, bool) {
        let mut registry = self
            .zero_copy_listeners
            .lock()
            .expect("wire zero-copy listener registry lock poisoned");
        if let Some(existing) = registry.get(key) {
            if let Ok(existing) = existing
                .clone()
                .downcast::<WireZeroCopyListener<TCore::Rx, W>>()
            {
                return (existing, false);
            }
        }

        let wrapped = Arc::new(WireZeroCopyListener::<TCore::Rx, W> {
            source_filter: source_filter.clone(),
            sink_filter: sink_filter.cloned(),
            listener,
            _wire: PhantomData,
        });
        registry.insert(key.clone(), wrapped.clone());
        (wrapped, true)
    }

    fn zero_copy_listener_for_unregister(
        &self,
        key: &WireListenerKey,
        fallback: Arc<dyn UZeroCopyListener<UWireRx<TCore::Rx, W>>>,
    ) -> Arc<dyn UEncodedZeroCopyListener<TCore::Rx>> {
        self.zero_copy_listeners
            .lock()
            .expect("wire zero-copy listener registry lock poisoned")
            .get(key)
            .and_then(|listener| {
                listener
                    .clone()
                    .downcast::<WireZeroCopyListener<TCore::Rx, W>>()
                    .ok()
            })
            .unwrap_or_else(|| {
                Arc::new(WireZeroCopyListener::<TCore::Rx, W> {
                    source_filter: key.source_filter.clone(),
                    sink_filter: key.sink_filter.clone(),
                    listener: fallback,
                    _wire: PhantomData,
                })
            })
    }
}

struct WireZeroCopyListener<Rx, W>
where
    Rx: UEncodedRxFrame + Send + 'static,
    W: UWireMetadata,
{
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: Arc<dyn UZeroCopyListener<UWireRx<Rx, W>>>,
    _wire: PhantomData<W>,
}

#[async_trait]
impl<Rx, W> UEncodedZeroCopyListener<Rx> for WireZeroCopyListener<Rx, W>
where
    Rx: UEncodedRxFrame + Send + 'static,
    W: UWireMetadata + Send + Sync + 'static,
{
    async fn on_receive_encoded_zero_copy(&self, frame: Rx) {
        match UWireRx::<Rx, W>::try_from_encoded(frame) {
            Ok(frame)
                if wire_frame_matches(&frame, &self.source_filter, self.sink_filter.as_ref()) =>
            {
                self.listener.on_receive_zero_copy(frame).await;
            }
            Ok(_) => {}
            Err(error) => warn!(%error, "dropping invalid selected-wire zero-copy frame"),
        }
    }
}

fn wire_frame_matches<Rx, W>(
    frame: &UWireRx<Rx, W>,
    source_filter: &UUri,
    sink_filter: Option<&UUri>,
) -> bool
where
    Rx: UEncodedRxFrame,
    W: UWireMetadata,
{
    source_filter.matches(frame.metadata().attributes().source())
        && sink_filter.is_none_or(|filter| {
            frame
                .metadata()
                .attributes()
                .sink()
                .is_some_and(|sink| filter.matches(sink))
        })
}

fn selected_wire_core_source_filter() -> UUri {
    UUri::try_from_parts("*", u32::MAX, u8::MAX, u16::MAX)
        .expect("valid selected-wire core wildcard source filter")
}

fn selected_wire_core_source_filter_for(source_filter: &UUri) -> UUri {
    if source_filter.verify_no_wildcards().is_ok() {
        source_filter.clone()
    } else {
        selected_wire_core_source_filter()
    }
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
    pub fn from_validated<W>(frame: ValidatedOwnedFrame) -> Result<Self, UStatus>
    where
        W: UWireMetadata,
    {
        let (metadata, payload) = frame.into_inner().into_parts();
        let encoded_metadata = W::encode_frame_metadata(&metadata)?;
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
    pub fn decode<W>(self) -> Result<UOwnedFrame, UStatus>
    where
        W: UWireMetadata,
    {
        let metadata = W::decode_frame_metadata(&self.encoded_metadata)?;
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

#[cfg(feature = "owned-frame-transport")]
/// Physical owned-frame mechanics implemented by product transports.
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
impl<TCore, W> UOwnedTransportImpl for UWireTransport<TCore, W>
where
    TCore: UOwnedTransportCore,
    W: UWireMetadata + Send + Sync + 'static,
{
    async fn send_validated_owned(&self, frame: ValidatedOwnedFrame) -> Result<(), UStatus> {
        self.core
            .send_prepared_owned(PreparedOwnedFrame::from_validated::<W>(frame)?)
            .await
    }

    async fn receive_validated_owned(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        let core_source_filter = selected_wire_core_source_filter();
        loop {
            let frame = self
                .core
                .receive_encoded_owned(&core_source_filter, None)
                .await?
                .decode::<W>()?;
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
        let core_source_filter = selected_wire_core_source_filter();
        let result = self
            .core
            .register_encoded_owned_listener(&core_source_filter, None, listener)
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
        let core_source_filter = selected_wire_core_source_filter();
        let result = self
            .core
            .unregister_encoded_owned_listener(&core_source_filter, None, listener)
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
impl<TCore, W> UWireTransport<TCore, W>
where
    TCore: UOwnedTransportCore,
    W: UWireMetadata + Send + Sync + 'static,
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
            if let Ok(existing) = existing.clone().downcast::<WireOwnedListener<W>>() {
                return (existing, false);
            }
        }

        let wrapped = Arc::new(WireOwnedListener::<W> {
            source_filter: source_filter.clone(),
            sink_filter: sink_filter.cloned(),
            listener,
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
            .and_then(|listener| listener.clone().downcast::<WireOwnedListener<W>>().ok())
            .unwrap_or_else(|| {
                Arc::new(WireOwnedListener::<W> {
                    source_filter: key.source_filter.clone(),
                    sink_filter: key.sink_filter.clone(),
                    listener: fallback,
                    _wire: PhantomData,
                })
            })
    }
}

#[cfg(feature = "owned-frame-transport")]
struct WireOwnedListener<W>
where
    W: UWireMetadata,
{
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: Arc<dyn UOwnedListener>,
    _wire: PhantomData<W>,
}

#[cfg(feature = "owned-frame-transport")]
#[async_trait]
impl<W> UEncodedOwnedListener for WireOwnedListener<W>
where
    W: UWireMetadata + Send + Sync + 'static,
{
    async fn on_receive_encoded_owned(&self, frame: EncodedOwnedFrame) {
        match frame.decode::<W>() {
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
    source_filter.matches(frame.metadata().attributes().source())
        && sink_filter.is_none_or(|filter| {
            frame
                .metadata()
                .attributes()
                .sink()
                .is_some_and(|sink| filter.matches(sink))
        })
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct WireListenerKey {
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: usize,
}

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

fn zero_copy_listener_pointer<Rx, W>(listener: &Arc<dyn UZeroCopyListener<UWireRx<Rx, W>>>) -> usize
where
    Rx: UEncodedRxFrame,
    W: UWireMetadata,
{
    let ptr = Arc::as_ptr(listener);
    let thin_ptr = ptr as *const ();
    thin_ptr as usize
}

#[cfg(feature = "owned-frame-transport")]
fn owned_listener_pointer(listener: &Arc<dyn UOwnedListener>) -> usize {
    let ptr = Arc::as_ptr(listener);
    let thin_ptr = ptr as *const ();
    thin_ptr as usize
}

fn unimplemented() -> UStatus {
    UStatus::fail_with_code(UCode::Unimplemented, "not implemented")
}

#[cfg(feature = "owned-frame-transport")]
fn invalid_metadata(error: crate::UFrameMetadataError) -> UStatus {
    UStatus::fail_with_code(UCode::InvalidArgument, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, io::Cursor, sync::Arc, sync::Mutex as StdMutex};

    use protobuf::well_known_types::wrappers::StringValue;

    use super::*;
    use crate::{
        ByteBackedStablePayload, EncodePayload, PayloadEncoding, PayloadLoanProvenance,
        ProtobufWire, StableContainerPayload, StableContainerWireFormat, StablePayload,
        UMessageBuilder, UPayloadFormat, UProtocolNativeWire, UTxPayloadSpec, UVecRxLease,
        UVecTxBuffer, UVecUninitTxBuffer, UZeroCopyTransportExt,
    };

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    struct WireStableBytes {
        bytes: [u8; 4],
    }

    unsafe impl StablePayload for WireStableBytes {
        const TYPE_NAME: &'static str = "uprotocol.test.WireStableBytes";
    }

    unsafe impl ByteBackedStablePayload for WireStableBytes {}

    #[derive(Clone)]
    struct RawRx {
        encoded_metadata: Vec<u8>,
        payload: Vec<u8>,
    }

    impl UEncodedRxFrame for RawRx {
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
            self.payload.len()
        }

        fn payload_reader(&self) -> Self::PayloadReader<'_> {
            Cursor::new(&self.payload)
        }

        fn payload_slices(&self) -> Self::PayloadSlices<'_> {
            std::iter::once(self.payload.as_slice())
        }

        fn try_contiguous_payload(&self) -> Option<&[u8]> {
            Some(&self.payload)
        }
    }

    impl UEncodedLoanedRxFrame for RawRx {
        fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError> {
            // SAFETY: This test raw receive type models a transport-owned
            // contiguous receive loan for the selected-wire wrapper proof.
            Ok(unsafe {
                LoanedPayload::new_unchecked(
                    self.payload.as_slice(),
                    PayloadLoanProvenance::OpaqueTransportLoan,
                )
            })
        }
    }

    #[derive(Default)]
    struct RecordingCore {
        prepared: StdMutex<Vec<PreparedTxLoanSpec>>,
        sent: StdMutex<Vec<UVecTxBuffer>>,
        received: StdMutex<VecDeque<RawRx>>,
        receive_filters: StdMutex<Vec<(UUri, Option<UUri>)>>,
    }

    #[async_trait]
    impl UZeroCopyTransportCore for RecordingCore {
        type Tx = UVecTxBuffer;
        type Rx = RawRx;

        async fn loan_prepared_tx(&self, spec: PreparedTxLoanSpec) -> Result<Self::Tx, UStatus> {
            self.prepared.lock().unwrap().push(spec.clone());
            UVecTxBuffer::with_alignment(
                spec.metadata().clone(),
                spec.payload_len(),
                spec.payload_alignment(),
            )
        }

        async fn send_prepared_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
            self.sent.lock().unwrap().push(buffer);
            Ok(())
        }

        async fn receive_encoded_zero_copy(
            &self,
            source_filter: &UUri,
            sink_filter: Option<&UUri>,
        ) -> Result<Self::Rx, UStatus> {
            self.receive_filters
                .lock()
                .unwrap()
                .push((source_filter.clone(), sink_filter.cloned()));
            self.received.lock().unwrap().pop_front().ok_or_else(|| {
                UStatus::fail_with_code(UCode::NotFound, "no test encoded frame available")
            })
        }
    }

    #[derive(Default)]
    struct RecordingZeroCopyListener {
        frames: StdMutex<Vec<UWireRx<RawRx, UProtocolNativeWire>>>,
    }

    #[async_trait]
    impl UZeroCopyListener<UWireRx<RawRx, UProtocolNativeWire>> for RecordingZeroCopyListener {
        async fn on_receive_zero_copy(&self, frame: UWireRx<RawRx, UProtocolNativeWire>) {
            self.frames.lock().unwrap().push(frame);
        }
    }

    struct CompileCore;

    #[async_trait]
    impl UZeroCopyTransportCore for CompileCore {
        type Tx = UVecTxBuffer;
        type Rx = RawRx;

        async fn loan_prepared_tx(&self, spec: PreparedTxLoanSpec) -> Result<Self::Tx, UStatus> {
            UVecTxBuffer::with_alignment(
                spec.metadata().clone(),
                spec.payload_len(),
                spec.payload_alignment(),
            )
        }

        async fn send_prepared_zero_copy(&self, _buffer: Self::Tx) -> Result<(), UStatus> {
            Ok(())
        }
    }

    #[async_trait]
    impl UZeroCopyUninitTransportCore for CompileCore {
        type UninitTx = UVecUninitTxBuffer;

        async fn loan_prepared_uninit_tx(
            &self,
            spec: PreparedTxLoanSpec,
        ) -> Result<Self::UninitTx, UStatus> {
            UVecUninitTxBuffer::with_alignment(
                spec.metadata().clone(),
                spec.payload_len(),
                spec.payload_alignment(),
            )
        }
    }

    #[cfg(feature = "owned-frame-transport")]
    #[async_trait]
    impl UOwnedTransportCore for CompileCore {
        async fn send_prepared_owned(&self, _frame: PreparedOwnedFrame) -> Result<(), UStatus> {
            Ok(())
        }
    }

    fn metadata_with_payload() -> UFrameMetadata {
        metadata_with_payload_encoding(PayloadEncoding::Standard(UPayloadFormat::Raw))
    }

    fn metadata_with_payload_encoding(payload_encoding: PayloadEncoding) -> UFrameMetadata {
        metadata_with_topic_and_payload_encoding(0x9000, payload_encoding)
    }

    fn metadata_with_topic_and_payload_encoding(
        resource_id: u16,
        payload_encoding: PayloadEncoding,
    ) -> UFrameMetadata {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, resource_id).expect("topic URI");
        let message = UMessageBuilder::publish(topic).build().expect("message");
        UFrameMetadata::new(message.attributes().clone(), Some(payload_encoding)).expect("metadata")
    }

    fn stable_metadata<T: StablePayload>() -> UFrameMetadata {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).expect("topic URI");
        let message = UMessageBuilder::publish(topic).build().expect("message");
        UFrameMetadata::new(
            message.attributes().clone(),
            Some(StableContainerPayload::<T>::encoding()),
        )
        .expect("metadata")
    }

    fn stable_bytes(value: &WireStableBytes) -> Vec<u8> {
        // SAFETY: `WireStableBytes` is `repr(C)` over `[u8; 4]`, has no padding
        // or drop glue, and every byte pattern is valid for the test type.
        unsafe {
            std::slice::from_raw_parts(
                std::ptr::from_ref(value).cast::<u8>(),
                std::mem::size_of::<WireStableBytes>(),
            )
            .to_vec()
        }
    }

    fn raw_frame_for_topic(resource_id: u16, payload: &[u8]) -> RawRx {
        let metadata = metadata_with_topic_and_payload_encoding(
            resource_id,
            PayloadEncoding::Standard(UPayloadFormat::Raw),
        );
        RawRx {
            encoded_metadata: UProtocolNativeWire::encode_frame_metadata(&metadata).unwrap(),
            payload: payload.to_vec(),
        }
    }

    #[test]
    fn wire_rx_decodes_metadata_and_delegates_payload() {
        let metadata = metadata_with_payload();
        let encoded_metadata = UProtocolNativeWire::encode_frame_metadata(&metadata).unwrap();
        let raw = RawRx {
            encoded_metadata,
            payload: b"abc".to_vec(),
        };

        let rx = UWireRx::<RawRx, UProtocolNativeWire>::try_from_encoded(raw).unwrap();

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
        let metadata = metadata_with_payload_encoding(ProtobufWire::payload_encoding());
        let encoded_metadata = ProtobufWire::encode_frame_metadata(&metadata).unwrap();
        let raw = RawRx {
            encoded_metadata,
            payload: payload.to_vec(),
        };

        let rx = UWireRx::<RawRx, ProtobufWire>::try_from_encoded(raw).unwrap();
        let decoded: StringValue = rx.decode_payload().unwrap();

        assert_eq!(decoded.value, "selected-wire");
    }

    #[test]
    fn stable_borrow_accepts_wire_rx_with_loaned_raw_frame() {
        fn assert_loaned_rx<T: ULoanedContiguousZeroCopyRxFrame>() {}
        assert_loaned_rx::<UWireRx<RawRx, UProtocolNativeWire>>();

        let value = WireStableBytes { bytes: *b"wire" };
        let metadata = stable_metadata::<WireStableBytes>();
        let encoded_metadata = UProtocolNativeWire::encode_frame_metadata(&metadata).unwrap();
        let raw = RawRx {
            encoded_metadata,
            payload: stable_bytes(&value),
        };

        let rx = UWireRx::<RawRx, UProtocolNativeWire>::try_from_encoded(raw).unwrap();
        let borrowed = rx.borrow_stable_payload::<WireStableBytes>().unwrap();

        assert_eq!(borrowed, &value);
        assert_eq!(
            rx.payload_loan_provenance().unwrap(),
            PayloadLoanProvenance::OpaqueTransportLoan
        );
    }

    #[tokio::test]
    async fn stable_initialized_tx_helper_sends_through_selected_wire_transport() {
        let transport = RecordingCore::default().with_wire(StableContainerWireFormat);

        transport
            .send_loaned_payload::<WireStableBytes>(
                stable_metadata::<WireStableBytes>(),
                |payload| payload.bytes.copy_from_slice(b"wire"),
            )
            .await
            .expect("send initialized stable payload through selected wire");

        let prepared = transport.core().prepared.lock().unwrap();
        assert_eq!(prepared.len(), 1);
        let prepared_frame = prepared.first().expect("one prepared frame");
        let decoded =
            StableContainerWireFormat::decode_frame_metadata(prepared_frame.encoded_metadata())
                .expect("decode selected-wire metadata");
        assert_eq!(
            decoded.payload_encoding(),
            Some(&StableContainerPayload::<WireStableBytes>::encoding())
        );
        drop(prepared);

        let sent = transport.core().sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let sent_frame = sent.first().expect("one sent frame");
        let frame = UVecRxLease::new(
            sent_frame.metadata().clone(),
            Some(sent_frame.payload().to_vec()),
        )
        .expect("sent stable frame");
        assert_eq!(
            frame.borrow_stable_payload::<WireStableBytes>().unwrap(),
            &WireStableBytes { bytes: *b"wire" }
        );
    }

    #[tokio::test]
    async fn receive_filters_after_selected_wire_decode() {
        let core = RecordingCore::default();
        core.received.lock().unwrap().extend([
            raw_frame_for_topic(0x9001, b"drop"),
            raw_frame_for_topic(0x9000, b"keep"),
        ]);
        let transport = core.with_wire(UProtocolNativeWire);
        let source_filter = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();

        let frame = transport
            .receive_validated_zero_copy(&source_filter, None)
            .await
            .expect("matching decoded frame");

        assert_eq!(frame.try_contiguous_payload(), Some(&b"keep"[..]));
        assert!(transport.core().received.lock().unwrap().is_empty());
        let filters = transport.core().receive_filters.lock().unwrap();
        assert_eq!(filters.len(), 2);
        assert_eq!(filters[0].0, source_filter);
        assert_eq!(filters[0].1, None);
    }

    #[tokio::test]
    async fn receive_uses_wildcard_core_filter_for_wildcard_source_filter() {
        let core = RecordingCore::default();
        core.received
            .lock()
            .unwrap()
            .push_back(raw_frame_for_topic(0x9000, b"keep"));
        let transport = core.with_wire(UProtocolNativeWire);
        let source_filter = UUri::try_from_parts("vehicle", 0x4210, 0x01, u16::MAX).unwrap();

        let frame = transport
            .receive_validated_zero_copy(&source_filter, None)
            .await
            .expect("matching decoded frame");

        assert_eq!(frame.try_contiguous_payload(), Some(&b"keep"[..]));
        let filters = transport.core().receive_filters.lock().unwrap();
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].0, selected_wire_core_source_filter());
        assert_eq!(filters[0].1, None);
    }

    #[test]
    fn selected_wire_core_source_filter_uses_exact_source_when_safe() {
        let exact = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let wildcard = UUri::try_from_parts("vehicle", 0x4210, 0x01, u16::MAX).unwrap();

        assert_eq!(selected_wire_core_source_filter_for(&exact), exact);
        assert_eq!(
            selected_wire_core_source_filter_for(&wildcard),
            selected_wire_core_source_filter()
        );
    }

    #[tokio::test]
    async fn listener_filters_after_selected_wire_decode() {
        let source_filter = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let listener = Arc::new(RecordingZeroCopyListener::default());
        let wire_listener = WireZeroCopyListener::<RawRx, UProtocolNativeWire> {
            source_filter,
            sink_filter: None,
            listener: listener.clone(),
            _wire: PhantomData,
        };

        wire_listener
            .on_receive_encoded_zero_copy(raw_frame_for_topic(0x9001, b"drop"))
            .await;
        wire_listener
            .on_receive_encoded_zero_copy(raw_frame_for_topic(0x9000, b"keep"))
            .await;

        let frames = listener.frames.lock().unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].try_contiguous_payload(), Some(&b"keep"[..]));
    }

    #[test]
    fn wire_transport_fits_zero_copy_blanket_boundaries() {
        fn assert_zero_copy_impl<T: UZeroCopyTransportImpl>() {}
        fn assert_uninit_impl<T: UZeroCopyUninitTransportImpl>() {}

        type Transport = UWireTransport<CompileCore, UProtocolNativeWire>;
        assert_zero_copy_impl::<Transport>();
        assert_uninit_impl::<Transport>();
    }

    #[test]
    fn prepared_tx_spec_carries_metadata_bytes_and_layout() {
        let metadata = metadata_with_payload();
        let spec = ValidatedTxLoanSpec::try_from(
            crate::UTxLoanSpec::new(
                metadata.clone(),
                UTxPayloadSpec::Present {
                    len: 4,
                    alignment: 2,
                },
            )
            .unwrap(),
        )
        .unwrap();

        let prepared = PreparedTxLoanSpec::from_validated::<UProtocolNativeWire>(spec).unwrap();

        assert_eq!(prepared.metadata(), &metadata);
        assert_eq!(prepared.payload_len(), 4);
        assert_eq!(prepared.payload_alignment(), 2);
        assert!(!prepared.encoded_metadata().is_empty());
    }

    #[cfg(feature = "owned-frame-transport")]
    #[test]
    fn wire_transport_fits_owned_blanket_boundary() {
        fn assert_owned_impl<T: UOwnedTransportImpl>() {}

        type Transport = UWireTransport<CompileCore, UProtocolNativeWire>;
        assert_owned_impl::<Transport>();
    }

    #[test]
    fn with_wire_constructs_adapter() {
        let transport = CompileCore.with_wire(UProtocolNativeWire);
        let _: &UProtocolNativeWire = transport.wire();
        let _: &CompileCore = transport.core();
    }

    #[test]
    fn existing_public_rx_lease_still_compiles_independently() {
        fn assert_public_rx<T: UZeroCopyRxLease>() {}

        assert_public_rx::<UVecRxLease>();
    }
}
