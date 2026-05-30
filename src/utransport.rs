/********************************************************************************
 * Copyright (c) 2023 Contributors to the Eclipse Foundation
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

use std::any::Any;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::num::TryFromIntError;
use std::ops::Deref;
use std::sync::{Arc, LazyLock, Mutex as StdMutex};

use async_trait::async_trait;
use bytes::Bytes;
use tracing::warn;

use crate::{
    payload::{
        BytePayloadCodec, EncodePayload, EncodedPayload, LoanPayload, LoanUninitPayload,
        LoanedInitPayload, PayloadCodec, PayloadFormat, PayloadLayout, USerializer, UWireError,
    },
    zero_copy::{
        verify_tx_buffer_payload_layout, verify_uninit_tx_buffer_payload_layout,
        LoanedUninitByteWriter, UFrameView, UTxBuffer, UUninitTxBuffer, UZeroCopyRxLease,
    },
    UCode, UFrameMetadata, UOwnedFrame, UStatus, UUri,
};

mod sealed {
    pub trait OwnedTransportSealed {}
    pub trait ZeroCopyTransportSealed {}
    pub trait ZeroCopyUninitTransportSealed {}
}

#[cfg(any(
    feature = "unsafe-stable-payload-tx",
    feature = "expert-unsafe-payloads"
))]
use crate::payload::{StableContainerPayload, StablePayload, UnsafeStablePayloadTxSlot};

#[cfg(any(test, feature = "test-util"))]
use crate::zero_copy::{UVecRxLease, UVecTxBuffer};

/// Verifies that given UUris can be used as source and sink filter UUris.
pub fn verify_filter_criteria(
    source_filter: &UUri,
    sink_filter: Option<&UUri>,
) -> Result<(), UStatus> {
    UUri::check_validity(source_filter).map_err(|err| {
        UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            format!("invalid source filter URI: {err}"),
        )
    })?;
    if let Some(sink_filter_uuri) = sink_filter {
        UUri::check_validity(sink_filter_uuri).map_err(|err| {
            UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!("invalid sink filter URI: {err}"),
            )
        })?;

        if sink_filter_uuri.is_notification_destination()
            && source_filter.is_notification_destination()
        {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "source and sink filters must not both have resource ID 0",
            ));
        }
        if sink_filter_uuri.is_rpc_method()
            && !source_filter.has_wildcard_resource_id()
            && !source_filter.is_notification_destination()
        {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "source filter must either have the wildcard resource ID or resource ID 0, if sink filter matches RPC method resource ID"));
        }
    } else if !source_filter.has_wildcard_resource_id() && !source_filter.is_event() {
        return Err(UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            "source filter must either have the wildcard resource ID or a resource ID from topic range, if sink filter is empty"));
    }
    Ok(())
}

/// Validates frame metadata before transport send or application delivery.
pub fn validate_frame_metadata_for_transport(metadata: &UFrameMetadata) -> Result<(), UStatus> {
    metadata.validate().map_err(|err| {
        UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            format!("invalid frame metadata: {err}"),
        )
    })?;
    if metadata.attributes().is_expired() {
        return Err(UStatus::fail_with_code(
            UCode::DEADLINE_EXCEEDED,
            "message has expired",
        ));
    }
    Ok(())
}

/// Validates frame metadata together with explicit payload presence.
pub fn validate_frame_metadata_for_payload(
    metadata: &UFrameMetadata,
    has_payload: bool,
) -> Result<(), UStatus> {
    validate_frame_metadata_for_transport(metadata)?;
    match (has_payload, metadata.encoding().is_some()) {
        (true, true) | (false, false) => Ok(()),
        (true, false) => Err(UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            "message payload is present but payload encoding is absent",
        )),
        (false, true) => Err(UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            "payload encoding is present but message payload is absent",
        )),
    }
}

/// Validates an owned frame before transport send or application delivery.
pub fn validate_owned_frame_for_transport(frame: &UOwnedFrame) -> Result<(), UStatus> {
    validate_frame_metadata_for_payload(frame.metadata(), frame.has_payload())
}

/// Validates a frame view before transport delivery.
pub fn validate_frame_view_for_transport(
    frame: &(impl UFrameView + ?Sized),
) -> Result<(), UStatus> {
    validate_frame_metadata_for_payload(frame.metadata(), frame.has_payload())
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ListenerRegistrationKey {
    transport: usize,
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: usize,
}

static OWNED_LISTENER_REGISTRY: LazyLock<
    StdMutex<HashMap<ListenerRegistrationKey, Arc<dyn UOwnedListener>>>,
> = LazyLock::new(|| StdMutex::new(HashMap::new()));

static ZERO_COPY_LISTENER_REGISTRY: LazyLock<
    StdMutex<HashMap<ListenerRegistrationKey, Arc<dyn Any + Send + Sync>>>,
> = LazyLock::new(|| StdMutex::new(HashMap::new()));

fn transport_pointer<T: ?Sized>(transport: &T) -> usize {
    let ptr = transport as *const T;
    ptr.cast::<()>() as usize
}

fn owned_listener_pointer(listener: &Arc<dyn UOwnedListener>) -> usize {
    Arc::as_ptr(listener).cast::<()>() as usize
}

fn zero_copy_listener_pointer<Rx>(listener: &Arc<dyn UZeroCopyListener<Rx>>) -> usize
where
    Rx: UZeroCopyRxLease + Send + 'static,
{
    Arc::as_ptr(listener).cast::<()>() as usize
}

fn listener_registration_key<T: ?Sized>(
    transport: &T,
    source_filter: &UUri,
    sink_filter: Option<&UUri>,
    listener: usize,
) -> ListenerRegistrationKey {
    ListenerRegistrationKey {
        transport: transport_pointer(transport),
        source_filter: source_filter.clone(),
        sink_filter: sink_filter.cloned(),
        listener,
    }
}

struct ValidatingOwnedListener {
    listener: Arc<dyn UOwnedListener>,
}

#[async_trait]
impl UOwnedListener for ValidatingOwnedListener {
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        match validate_owned_frame_for_transport(&frame) {
            Ok(()) => self.listener.on_receive_owned(frame).await,
            Err(error) => warn!(%error, "dropping invalid owned frame before listener delivery"),
        }
    }
}

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

fn registered_owned_listener(
    key: &ListenerRegistrationKey,
    listener: Arc<dyn UOwnedListener>,
) -> (Arc<dyn UOwnedListener>, bool) {
    let mut registry = OWNED_LISTENER_REGISTRY
        .lock()
        .expect("owned listener registry lock poisoned");
    if let Some(existing) = registry.get(key) {
        return (existing.clone(), false);
    }

    let validating_listener: Arc<dyn UOwnedListener> =
        Arc::new(ValidatingOwnedListener { listener });
    registry.insert(key.clone(), validating_listener.clone());
    (validating_listener, true)
}

fn registered_zero_copy_listener<Rx>(
    key: &ListenerRegistrationKey,
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
    let erased: Arc<dyn Any + Send + Sync> = validating_listener.clone();
    registry.insert(key.clone(), erased);
    let validating_listener: Arc<dyn UZeroCopyListener<Rx>> = validating_listener;
    (validating_listener, true)
}

fn owned_listener_for_unregister(
    key: &ListenerRegistrationKey,
    fallback: Arc<dyn UOwnedListener>,
) -> Arc<dyn UOwnedListener> {
    OWNED_LISTENER_REGISTRY
        .lock()
        .expect("owned listener registry lock poisoned")
        .get(key)
        .cloned()
        .unwrap_or(fallback)
}

fn zero_copy_listener_for_unregister<Rx>(
    key: &ListenerRegistrationKey,
    fallback: Arc<dyn UZeroCopyListener<Rx>>,
) -> Arc<dyn UZeroCopyListener<Rx>>
where
    Rx: UZeroCopyRxLease + Send + 'static,
{
    ZERO_COPY_LISTENER_REGISTRY
        .lock()
        .expect("zero-copy listener registry lock poisoned")
        .get(key)
        .cloned()
        .and_then(|listener| listener.downcast::<ValidatingZeroCopyListener<Rx>>().ok())
        .map(|listener| listener as Arc<dyn UZeroCopyListener<Rx>>)
        .unwrap_or(fallback)
}

/// Owned frame that has passed transport-boundary validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedOwnedFrame(UOwnedFrame);

impl ValidatedOwnedFrame {
    /// Returns the validated owned frame.
    pub fn as_frame(&self) -> &UOwnedFrame {
        &self.0
    }

    /// Consumes the wrapper and returns the validated owned frame.
    pub fn into_inner(self) -> UOwnedFrame {
        self.0
    }
}

impl TryFrom<UOwnedFrame> for ValidatedOwnedFrame {
    type Error = UStatus;

    fn try_from(value: UOwnedFrame) -> Result<Self, Self::Error> {
        validate_owned_frame_for_transport(&value)?;
        Ok(Self(value))
    }
}

impl Deref for ValidatedOwnedFrame {
    type Target = UOwnedFrame;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Payload presence and layout requested for a transmit loan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UTxPayloadSpec {
    /// The frame carries no payload and no payload encoding metadata.
    Absent,
    /// The frame carries a payload, including a present empty payload.
    Present(PayloadLayout),
}

impl UTxPayloadSpec {
    /// Creates a present-payload spec from length and alignment.
    pub fn present(payload_len: usize, alignment: usize) -> Result<Self, UWireError> {
        PayloadLayout::new(payload_len, alignment).map(Self::Present)
    }

    /// Creates a present-empty-payload spec.
    pub fn present_empty() -> Self {
        Self::Present(PayloadLayout::new(0, 1).expect("alignment 1 is valid"))
    }

    /// Returns whether this spec represents a present payload.
    pub fn is_present(self) -> bool {
        matches!(self, Self::Present(_))
    }

    /// Returns the present payload layout, if any.
    pub fn layout(self) -> Option<PayloadLayout> {
        match self {
            Self::Absent => None,
            Self::Present(layout) => Some(layout),
        }
    }
}

/// Validated transport-independent transmit loan specification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UTxLoanSpec {
    metadata: UFrameMetadata,
    payload: UTxPayloadSpec,
}

impl UTxLoanSpec {
    /// Creates a validated transmit loan spec.
    pub fn new(metadata: UFrameMetadata, payload: UTxPayloadSpec) -> Result<Self, UStatus> {
        validate_frame_metadata_for_payload(&metadata, payload.is_present())?;
        Ok(Self { metadata, payload })
    }

    /// Creates a no-payload transmit loan spec.
    pub fn no_payload(metadata: UFrameMetadata) -> Result<Self, UStatus> {
        Self::new(metadata, UTxPayloadSpec::Absent)
    }

    /// Creates a present-payload transmit loan spec.
    pub fn payload(metadata: UFrameMetadata, layout: PayloadLayout) -> Result<Self, UStatus> {
        Self::new(metadata, UTxPayloadSpec::Present(layout))
    }

    /// Creates a present-empty-payload transmit loan spec.
    pub fn present_empty_payload(metadata: UFrameMetadata) -> Result<Self, UStatus> {
        Self::new(metadata, UTxPayloadSpec::present_empty())
    }

    /// Returns the immutable frame metadata associated with this loan.
    pub fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    /// Consumes the spec and returns its metadata.
    pub fn into_metadata(self) -> UFrameMetadata {
        self.metadata
    }

    /// Returns the payload presence and layout spec.
    pub fn payload_spec(&self) -> UTxPayloadSpec {
        self.payload
    }

    /// Returns whether the transmit frame carries a payload.
    pub fn has_payload(&self) -> bool {
        self.payload.is_present()
    }

    /// Returns the visible application payload length.
    pub fn payload_len(&self) -> usize {
        self.payload.layout().map_or(0, PayloadLayout::len)
    }

    /// Returns the requested visible application payload alignment.
    pub fn payload_alignment(&self) -> usize {
        self.payload.layout().map_or(1, PayloadLayout::align)
    }

    /// Returns the present payload layout, if any.
    pub fn payload_layout(&self) -> Option<PayloadLayout> {
        self.payload.layout()
    }
}

/// Transmit loan spec that has passed the public transport validation boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedTxLoanSpec(UTxLoanSpec);

impl ValidatedTxLoanSpec {
    /// Returns the validated transmit loan spec.
    pub fn as_spec(&self) -> &UTxLoanSpec {
        &self.0
    }

    /// Consumes the wrapper and returns the validated transmit loan spec.
    pub fn into_inner(self) -> UTxLoanSpec {
        self.0
    }

    /// Returns immutable frame metadata associated with this loan.
    pub fn metadata(&self) -> &UFrameMetadata {
        self.0.metadata()
    }

    /// Consumes the spec and returns its metadata.
    pub fn into_metadata(self) -> UFrameMetadata {
        self.0.into_metadata()
    }

    /// Returns whether the transmit frame carries a payload.
    pub fn has_payload(&self) -> bool {
        self.0.has_payload()
    }

    /// Returns the visible application payload length.
    pub fn payload_len(&self) -> usize {
        self.0.payload_len()
    }

    /// Returns the requested visible application payload alignment.
    pub fn payload_alignment(&self) -> usize {
        self.0.payload_alignment()
    }

    /// Returns the payload presence and layout spec.
    pub fn payload_spec(&self) -> UTxPayloadSpec {
        self.0.payload_spec()
    }

    /// Returns the present payload layout, if any.
    pub fn payload_layout(&self) -> Option<PayloadLayout> {
        self.0.payload_layout()
    }
}

impl TryFrom<UTxLoanSpec> for ValidatedTxLoanSpec {
    type Error = UStatus;

    fn try_from(value: UTxLoanSpec) -> Result<Self, Self::Error> {
        validate_frame_metadata_for_payload(value.metadata(), value.has_payload())?;
        Ok(Self(value))
    }
}

impl Deref for ValidatedTxLoanSpec {
    type Target = UTxLoanSpec;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A factory for URIs representing this uEntity's resources.
#[cfg_attr(any(test, feature = "test-util"), mockall::automock)]
pub trait LocalUriProvider: Send + Sync {
    fn get_authority(&self) -> String;
    fn get_resource_uri(&self, resource_id: u16) -> UUri;
    fn get_source_uri(&self) -> UUri;
}

/// A URI provider statically configured with authority, entity ID and version.
pub struct StaticUriProvider {
    local_uri: UUri,
}

impl StaticUriProvider {
    pub fn new(authority: impl Into<String>, entity_id: u32, major_version: u8) -> Self {
        let local_uri = UUri::from_parts_unchecked(
            authority.into(),
            entity_id,
            u32::from(major_version),
            0x0000,
        );
        StaticUriProvider { local_uri }
    }
}

impl LocalUriProvider for StaticUriProvider {
    fn get_authority(&self) -> String {
        self.local_uri.authority_name()
    }

    fn get_resource_uri(&self, resource_id: u16) -> UUri {
        self.local_uri.clone().with_resource_id(resource_id)
    }

    fn get_source_uri(&self) -> UUri {
        self.local_uri.clone()
    }
}

impl TryFrom<UUri> for StaticUriProvider {
    type Error = TryFromIntError;
    fn try_from(value: UUri) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

impl TryFrom<&UUri> for StaticUriProvider {
    type Error = TryFromIntError;
    fn try_from(source_uri: &UUri) -> Result<Self, Self::Error> {
        let major_version = u8::try_from(source_uri.ue_version_major())?;
        Ok(StaticUriProvider::new(
            source_uri.authority_name(),
            source_uri.ue_id(),
            major_version,
        ))
    }
}

/// A handler for processing owned, serialization-neutral uProtocol frames.
#[cfg_attr(any(test, feature = "test-util"), mockall::automock)]
#[async_trait]
pub trait UOwnedListener: Send + Sync {
    async fn on_receive_owned(&self, frame: UOwnedFrame);
}

/// Implementation boundary for serialization-neutral owned-buffer transports.
///
/// Transport authors implement this trait. The public [`UOwnedTransport`]
/// blanket implementation validates frames and filters before delegating here.
#[async_trait]
pub trait UOwnedTransportImpl: Send + Sync {
    async fn send_validated_owned(&self, frame: ValidatedOwnedFrame) -> Result<(), UStatus>;

    /// Receives one matching owned frame from transports that support pull receive.
    async fn receive_validated_owned(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "not implemented",
        ))
    }

    /// Registers an owned listener after public filter validation.
    ///
    /// The public wrapper passes a listener that validates reconstructed frames
    /// before invoking the user listener. Implementations must keep the listener
    /// object identity they receive so the public unregister path can remove the
    /// same validating wrapper later.
    async fn register_validated_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "not implemented",
        ))
    }

    async fn unregister_validated_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "not implemented",
        ))
    }
}

impl<T> sealed::OwnedTransportSealed for T where T: UOwnedTransportImpl + ?Sized {}

/// The serialization-neutral owned-buffer transport API.
///
/// Owned transports are the default path for network, brokered, and in-process
/// transports. They accept native frame metadata plus owned payload bytes.
///
/// ```no_run
/// # use async_trait::async_trait;
/// # use up_rust::{transport::{UOwnedTransportImpl, ValidatedOwnedFrame}, UStatus};
/// struct MyTransport;
///
/// #[async_trait]
/// impl UOwnedTransportImpl for MyTransport {
///     async fn send_validated_owned(&self, frame: ValidatedOwnedFrame) -> Result<(), UStatus> {
///         let metadata = frame.metadata();
///         let payload = frame.payload_bytes();
///         # let _ = (metadata, payload);
///         Ok(())
///     }
/// }
/// ```
#[async_trait]
pub trait UOwnedTransport: sealed::OwnedTransportSealed + Send + Sync {
    async fn send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus>;

    /// Receives one matching owned frame from transports that support pull receive.
    ///
    /// The default implementation returns [`UCode::UNIMPLEMENTED`], matching the
    /// mainline `UTransport::receive` default. Push-oriented transports should
    /// implement listener registration instead of relying on a hidden
    /// listener-backed receive adapter.
    async fn receive_owned(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "not implemented",
        ))
    }

    async fn register_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "not implemented",
        ))
    }

    async fn unregister_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "not implemented",
        ))
    }
}

#[async_trait]
impl<T> UOwnedTransport for T
where
    T: UOwnedTransportImpl + ?Sized,
{
    async fn send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.send_validated_owned(ValidatedOwnedFrame::try_from(frame)?)
            .await
    }

    async fn receive_owned(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        verify_filter_criteria(source_filter, sink_filter)?;
        let frame =
            UOwnedTransportImpl::receive_validated_owned(self, source_filter, sink_filter).await?;
        Ok(ValidatedOwnedFrame::try_from(frame)?.into_inner())
    }

    async fn register_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        verify_filter_criteria(source_filter, sink_filter)?;
        let key = listener_registration_key(
            self,
            source_filter,
            sink_filter,
            owned_listener_pointer(&listener),
        );
        let (listener, inserted) = registered_owned_listener(&key, listener);
        let result = UOwnedTransportImpl::register_validated_owned_listener(
            self,
            source_filter,
            sink_filter,
            listener,
        )
        .await;
        if result.is_err() && inserted {
            OWNED_LISTENER_REGISTRY
                .lock()
                .expect("owned listener registry lock poisoned")
                .remove(&key);
        }
        result
    }

    async fn unregister_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        verify_filter_criteria(source_filter, sink_filter)?;
        let key = listener_registration_key(
            self,
            source_filter,
            sink_filter,
            owned_listener_pointer(&listener),
        );
        let listener = owned_listener_for_unregister(&key, listener);
        let result = UOwnedTransportImpl::unregister_validated_owned_listener(
            self,
            source_filter,
            sink_filter,
            listener,
        )
        .await;
        if result.is_ok() {
            OWNED_LISTENER_REGISTRY
                .lock()
                .expect("owned listener registry lock poisoned")
                .remove(&key);
        }
        result
    }
}

/// Convenience methods for owned transports.
#[async_trait]
pub trait UOwnedTransportExt: UOwnedTransport {
    /// Serializes `value` into owned bytes and sends it as a native frame.
    ///
    /// This helper validates transport metadata, sets `metadata.encoding()` from
    /// the selected [`PayloadFormat`], serializes `value` into an owned buffer,
    /// and calls [`UOwnedTransport::send_owned`]. Use
    /// [`UZeroCopyTransportExt::send_serialized_zero_copy`] instead when the
    /// transport can loan payload storage and the caller wants to avoid building
    /// an intermediate owned payload buffer.
    async fn send_serialized<F, T>(
        &self,
        metadata: UFrameMetadata,
        value: &T,
    ) -> Result<(), UStatus>
    where
        F: PayloadFormat + Send + Sync,
        T: USerializer<F> + Sync,
    {
        self.send_payload_as::<F, T>(metadata, value).await
    }

    /// Encodes `value` with payload codec `C` into owned bytes and sends it.
    async fn send_payload_as<C, T>(
        &self,
        metadata: UFrameMetadata,
        value: &T,
    ) -> Result<(), UStatus>
    where
        C: PayloadCodec + EncodePayload<T> + Send + Sync,
        T: ?Sized + Sync,
    {
        validate_frame_metadata_for_transport(&metadata)?;
        let frame = UOwnedFrame::from_payload_as::<C, T>(metadata, value).map_err(UStatus::from)?;
        self.send_owned(frame).await
    }

    /// Sends bytes that are already encoded for byte-oriented codec `C`.
    async fn send_bytes_as<C, B>(&self, metadata: UFrameMetadata, payload: B) -> Result<(), UStatus>
    where
        C: PayloadCodec + BytePayloadCodec + Send + Sync,
        B: Into<Bytes> + Send,
    {
        validate_frame_metadata_for_transport(&metadata)?;
        let frame =
            UOwnedFrame::from_encoded_payload(metadata, EncodedPayload::<C>::from_bytes(payload));
        self.send_owned(frame).await
    }

    /// Sends bytes that are already encoded and tagged for codec `C`.
    async fn send_encoded_payload<C>(
        &self,
        metadata: UFrameMetadata,
        payload: EncodedPayload<C>,
    ) -> Result<(), UStatus>
    where
        C: PayloadCodec + Send + Sync,
    {
        validate_frame_metadata_for_transport(&metadata)?;
        let frame = UOwnedFrame::from_encoded_payload(metadata, payload);
        self.send_owned(frame).await
    }
}

impl<T> UOwnedTransportExt for T where T: UOwnedTransport + ?Sized {}

/// A handler for processing zero-copy receive leases.
///
/// The listener receives the transport-specific lease type. Any borrowed payload
/// views derived from `frame` must not outlive the callback unless the callback
/// explicitly copies the payload into owned storage.
#[async_trait]
pub trait UZeroCopyListener<Rx>: Send + Sync
where
    Rx: UZeroCopyRxLease + Send + 'static,
{
    /// Handles one received zero-copy frame lease.
    async fn on_receive_zero_copy(&self, frame: Rx);
}

/// Implementation boundary for transports that can loan zero-copy storage.
///
/// Transport authors implement this trait. The public [`UZeroCopyTransport`]
/// blanket implementation validates loan specs, send buffers, receive leases,
/// and filters before delegating here.
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
            UCode::UNIMPLEMENTED,
            "not implemented",
        ))
    }

    /// Registers a zero-copy listener after public filter validation.
    ///
    /// The public wrapper passes a listener that validates reconstructed receive
    /// leases before invoking the user listener. Implementations must keep the
    /// listener object identity they receive so the public unregister path can
    /// remove the same validating wrapper later.
    async fn register_validated_zero_copy_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "not implemented",
        ))
    }

    async fn unregister_validated_zero_copy_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "not implemented",
        ))
    }
}

impl<T> sealed::ZeroCopyTransportSealed for T where T: UZeroCopyTransportImpl + ?Sized {}

/// The zero-copy transport capability API.
///
/// Implement this trait only when the transport can loan transmit storage or
/// deliver receive leases without hiding transport-owned copies.
///
/// The metadata passed to [`Self::loan_tx`] is final for that transmit loan.
/// Implementations may encode metadata into native transport headers, side-band
/// properties, or hidden prefixes before returning the loan. Callers must choose
/// payload encoding, routing attributes, and other metadata before reserving so
/// [`UTxBuffer::payload_mut`] remains the only mutable zero-copy surface.
///
/// This is the zero-copy sibling of [`UOwnedTransport`]. Pull receive and
/// listener registration map one-to-one to the owned API, while send is
/// intentionally split into [`Self::loan_tx`] plus [`Self::send_zero_copy`] so
/// serializers can write directly into the transport loan.
///
/// ```no_run
/// # use async_trait::async_trait;
/// # use up_rust::{zero_copy::{UVecTxBuffer, UVecRxLease, UZeroCopyTransportImpl}, UStatus};
/// struct SharedMemoryTransport;
///
/// #[async_trait]
/// impl UZeroCopyTransportImpl for SharedMemoryTransport {
///     type Tx = UVecTxBuffer;
///     type Rx = UVecRxLease;
///
///     async fn loan_validated_tx(
///         &self,
///         spec: up_rust::zero_copy::ValidatedTxLoanSpec,
///     ) -> Result<Self::Tx, UStatus> {
///         let payload_len = spec.payload_len();
///         Ok(UVecTxBuffer::new(spec.into_metadata(), payload_len))
///     }
///
///     async fn send_validated_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
///         # let _ = buffer;
///         Ok(())
///     }
/// }
/// ```
#[async_trait]
pub trait UZeroCopyTransport: sealed::ZeroCopyTransportSealed + Send + Sync {
    /// Transport-specific transmit loan type returned by [`Self::loan_tx`].
    type Tx: UTxBuffer + Send;

    /// Transport-specific receive lease type returned by pull receive and
    /// delivered to zero-copy listeners.
    type Rx: UZeroCopyRxLease + Send + 'static;

    /// Reserves transmit storage for a validated frame loan spec.
    ///
    /// The spec has already validated payload presence against encoding metadata
    /// and the requested payload alignment. Implementations must either honor the
    /// requested alignment or return an error before handing the loan to the
    /// caller. Metadata is immutable after this call; transports may use it to
    /// compute payload layout and native transport representation before exposing
    /// the payload storage.
    async fn loan_tx(&self, spec: UTxLoanSpec) -> Result<Self::Tx, UStatus>;

    /// Commits a previously reserved transmit loan.
    ///
    /// After this method returns, callers must treat `buffer` as consumed. The
    /// transport may reclaim, publish, or otherwise invalidate the underlying
    /// storage.
    async fn send_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus>;

    /// Receives one matching zero-copy frame from transports that support pull
    /// receive.
    ///
    /// The default implementation returns [`UCode::UNIMPLEMENTED`]. Push-oriented
    /// zero-copy transports should implement listener registration instead of
    /// hiding a temporary listener-backed receive adapter.
    async fn receive_zero_copy(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "not implemented",
        ))
    }

    /// Registers a listener for matching zero-copy receive leases.
    ///
    /// The listener receives the transport-specific [`Self::Rx`] lease type. If
    /// the transport's receive lease is consumed by one subscriber, the
    /// implementation must create independent subscriber state for independent
    /// listener registrations or explicitly copy for secondary listeners.
    async fn register_zero_copy_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "not implemented",
        ))
    }

    /// Unregisters a listener previously registered with
    /// [`Self::register_zero_copy_listener`].
    async fn unregister_zero_copy_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "not implemented",
        ))
    }
}

#[async_trait]
impl<T> UZeroCopyTransport for T
where
    T: UZeroCopyTransportImpl + ?Sized,
{
    type Tx = T::Tx;
    type Rx = T::Rx;

    async fn loan_tx(&self, spec: UTxLoanSpec) -> Result<Self::Tx, UStatus> {
        self.loan_validated_tx(ValidatedTxLoanSpec::try_from(spec)?)
            .await
    }

    async fn send_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
        let has_payload = !buffer.payload().is_empty() || buffer.metadata().encoding().is_some();
        validate_frame_metadata_for_payload(buffer.metadata(), has_payload)?;
        self.send_validated_zero_copy(buffer).await
    }

    async fn receive_zero_copy(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus> {
        verify_filter_criteria(source_filter, sink_filter)?;
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
        verify_filter_criteria(source_filter, sink_filter)?;
        let key = listener_registration_key(
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
        verify_filter_criteria(source_filter, sink_filter)?;
        let key = listener_registration_key(
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

impl<T> sealed::ZeroCopyUninitTransportSealed for T where T: UZeroCopyUninitTransportImpl + ?Sized {}

/// Optional zero-copy capability for transports that can expose uninitialized TX payload storage.
///
/// This is deliberately separate from [`UZeroCopyTransport::loan_tx`], whose
/// [`UTxBuffer`] result exposes initialized byte slices. Implementations must
/// initialize any transport-owned bytes before returning the uninitialized loan;
/// callers initialize only the visible application payload bytes.
#[async_trait]
pub trait UZeroCopyUninitTransport:
    UZeroCopyTransport + sealed::ZeroCopyUninitTransportSealed
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
        self.loan_validated_uninit_tx(ValidatedTxLoanSpec::try_from(spec)?)
            .await
    }
}

fn validate_payload_layout_request(
    payload_len: usize,
    alignment: usize,
) -> Result<PayloadLayout, UStatus> {
    PayloadLayout::new(payload_len, alignment).map_err(UStatus::from)
}

fn bad_reserved_payload_layout(error: UWireError) -> UStatus {
    UStatus::fail_with_code(
        UCode::INTERNAL,
        format!("transport returned a TX loan with invalid payload layout: {error}"),
    )
}

fn verify_reserved_tx_payload_layout(
    buffer: &mut impl UTxBuffer,
    layout: PayloadLayout,
) -> Result<(), UStatus> {
    verify_tx_buffer_payload_layout(buffer, layout.len(), layout.align())
        .map_err(bad_reserved_payload_layout)
}

fn verify_reserved_uninit_payload_layout(
    buffer: &mut impl UUninitTxBuffer,
    layout: PayloadLayout,
) -> Result<(), UStatus> {
    verify_uninit_tx_buffer_payload_layout(buffer, layout.len(), layout.align())
        .map_err(bad_reserved_payload_layout)
}

/// Convenience methods for zero-copy transports.
#[async_trait]
pub trait UZeroCopyTransportExt: UZeroCopyTransport {
    /// Serializes `value` directly into a transport transmit loan and sends it.
    ///
    /// This helper sets `metadata.encoding()` from the selected [`PayloadFormat`],
    /// reserves a loan of `value.encoded_len()` bytes using the serializer's
    /// alignment, writes into [`UTxBuffer::payload_mut`], and commits with
    /// [`UZeroCopyTransport::send_zero_copy`].
    async fn send_serialized_zero_copy<F, T>(
        &self,
        metadata: UFrameMetadata,
        value: &T,
    ) -> Result<(), UStatus>
    where
        F: PayloadFormat + Send + Sync,
        T: USerializer<F> + Sync,
    {
        self.send_encoded_payload_as::<F, T>(metadata, value).await
    }

    /// Encodes `value` directly into a transport transmit loan and sends it.
    ///
    /// This avoids an intermediate owned payload buffer, but it still performs
    /// serialization/copying from `value` into the loan. Use
    /// [`Self::send_loaned_payload_as`] when the payload should be initialized
    /// directly in loaned storage.
    async fn send_encoded_payload_as<C, T>(
        &self,
        metadata: UFrameMetadata,
        value: &T,
    ) -> Result<(), UStatus>
    where
        C: PayloadCodec + EncodePayload<T> + Send + Sync,
        T: ?Sized + Sync,
    {
        validate_frame_metadata_for_transport(&metadata)?;
        let layout = C::payload_layout(value).map_err(UStatus::from)?;
        let spec = UTxLoanSpec::payload(metadata.with_encoding(C::payload_encoding()), layout)?;
        let mut buffer = self.loan_tx(spec).await?;
        verify_reserved_tx_payload_layout(&mut buffer, layout)?;
        C::encode_payload(value, buffer.payload_mut()).map_err(UStatus::from)?;
        self.send_zero_copy(buffer).await
    }

    /// Copies already encoded payload bytes directly into a transport transmit loan.
    async fn send_encoded_payload<C>(
        &self,
        metadata: UFrameMetadata,
        payload: EncodedPayload<C>,
    ) -> Result<(), UStatus>
    where
        C: PayloadCodec + Send + Sync,
    {
        validate_frame_metadata_for_transport(&metadata)?;
        let payload = payload.into_bytes();
        let layout = validate_payload_layout_request(payload.len(), 1)?;
        let spec = UTxLoanSpec::payload(metadata.with_encoding(C::payload_encoding()), layout)?;
        let mut buffer = self.loan_tx(spec).await?;
        verify_reserved_tx_payload_layout(&mut buffer, layout)?;
        buffer
            .loaned_payload_mut()
            .copy_from_slice(payload.as_ref());
        self.send_zero_copy(buffer).await
    }

    /// Initializes a typed payload directly in a transmit loan and sends it.
    async fn send_loaned_payload_as<C, T>(
        &self,
        metadata: UFrameMetadata,
        init: impl for<'payload> FnOnce(&'payload mut T) + Send,
    ) -> Result<(), UStatus>
    where
        C: PayloadCodec + LoanPayload<T> + Send + Sync,
    {
        validate_frame_metadata_for_transport(&metadata)?;
        let layout = C::loan_layout().map_err(UStatus::from)?;
        let spec = UTxLoanSpec::payload(metadata.with_encoding(C::payload_encoding()), layout)?;
        let mut buffer = self.loan_tx(spec).await?;
        verify_reserved_tx_payload_layout(&mut buffer, layout)?;
        {
            let mut loaned_payload = buffer.loaned_payload_mut();
            let payload = C::loan_payload(loaned_payload.as_mut_bytes()).map_err(UStatus::from)?;
            init(payload);
        }
        self.send_zero_copy(buffer).await
    }
}

impl<T> UZeroCopyTransportExt for T where T: UZeroCopyTransport + ?Sized {}

/// Convenience methods for zero-copy transports with uninitialized TX storage.
#[async_trait]
pub trait UZeroCopyUninitTransportExt: UZeroCopyUninitTransport {
    /// Constructs a typed payload directly in uninitialized transmit storage and sends it.
    async fn send_uninit_loaned_payload_as<C, T>(
        &self,
        metadata: UFrameMetadata,
        init: impl for<'payload> FnOnce(
                crate::payload::LoanedUninitPayload<'payload, T>,
            ) -> Result<LoanedInitPayload<'payload, T>, UWireError>
            + Send,
    ) -> Result<(), UStatus>
    where
        C: PayloadCodec + LoanUninitPayload<T> + Send + Sync,
        T: Send,
    {
        validate_frame_metadata_for_transport(&metadata)?;
        let layout = C::loan_uninit_layout().map_err(UStatus::from)?;
        let spec = UTxLoanSpec::payload(metadata.with_encoding(C::payload_encoding()), layout)?;
        let mut buffer = self.loan_uninit_tx(spec).await?;
        verify_reserved_uninit_payload_layout(&mut buffer, layout)?;
        {
            let loaned_payload = buffer.payload_uninit_mut();
            let payload = C::loan_uninit_payload(loaned_payload).map_err(UStatus::from)?;
            let _initialized = init(payload).map_err(UStatus::from)?;
        }
        // SAFETY:
        // - `C::loan_uninit_payload` returned a typed slot whose initialized
        //   marker can only be produced by `LoanedUninitPayload::write` or a
        //   feature-gated unsafe initialization proof.
        // - The closure returned that initialized marker successfully before
        //   the buffer is committed.
        let buffer = unsafe { buffer.assume_payload_init() };
        self.send_zero_copy(buffer).await
    }

    /// Generates already-encoded bytes directly into uninitialized transmit storage.
    async fn send_uninit_loaned_bytes_as<C>(
        &self,
        metadata: UFrameMetadata,
        payload_len: usize,
        alignment: usize,
        write: impl for<'payload> FnOnce(
                LoanedUninitByteWriter<'payload>,
            )
                -> Result<LoanedUninitByteWriter<'payload>, UWireError>
            + Send,
    ) -> Result<(), UStatus>
    where
        C: PayloadCodec + BytePayloadCodec + Send + Sync,
    {
        validate_frame_metadata_for_transport(&metadata)?;
        let layout = validate_payload_layout_request(payload_len, alignment)?;
        let spec = UTxLoanSpec::payload(metadata.with_encoding(C::payload_encoding()), layout)?;
        let mut buffer = self.loan_uninit_tx(spec).await?;
        verify_reserved_uninit_payload_layout(&mut buffer, layout)?;
        {
            let writer = buffer.payload_uninit_mut().into_writer();
            let writer = write(writer).map_err(UStatus::from)?;
            let _initialized = writer.finish().map_err(UStatus::from)?;
        }
        // SAFETY: `LoanedUninitByteWriter::finish` succeeded, which proves the
        // safe writer initialized exactly the full visible payload range.
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
        validate_frame_metadata_for_transport(&metadata)?;
        let layout = PayloadLayout::for_type::<T>();
        let spec = UTxLoanSpec::payload(
            metadata.with_encoding(StableContainerPayload::<T>::encoding()),
            layout,
        )?;
        let mut buffer = self.loan_uninit_tx(spec).await?;
        verify_reserved_uninit_payload_layout(&mut buffer, layout)?;
        {
            let slot = UnsafeStablePayloadTxSlot::new(buffer.payload_uninit_mut())
                .map_err(UStatus::from)?;
            let _initialized = init(slot).map_err(UStatus::from)?;
        }
        // SAFETY:
        // - This method is unsafe; its caller guarantees `init` returns only
        //   after every transported byte contains one valid initialized `T`.
        // - `UnsafeStablePayloadTxSlot::new` checked the loan's exact stable
        //   payload length and alignment before handing the slot to `init`.
        let buffer = unsafe { buffer.assume_payload_init() };
        self.send_zero_copy(buffer).await
    }
}

impl<T> UZeroCopyUninitTransportExt for T where T: UZeroCopyUninitTransport + ?Sized {}

#[cfg(not(tarpaulin_include))]
#[cfg(any(test, feature = "test-util"))]
mockall::mock! {
    pub UOwnedTransport {
        pub async fn do_send_validated_owned(&self, frame: ValidatedOwnedFrame) -> Result<(), UStatus>;
        pub async fn do_receive_owned<'a>(&'a self, source_filter: &'a UUri, sink_filter: Option<&'a UUri>) -> Result<UOwnedFrame, UStatus>;
        pub async fn do_register_owned_listener<'a>(&'a self, source_filter: &'a UUri, sink_filter: Option<&'a UUri>, listener: Arc<dyn UOwnedListener>) -> Result<(), UStatus>;
        pub async fn do_unregister_owned_listener<'a>(&'a self, source_filter: &'a UUri, sink_filter: Option<&'a UUri>, listener: Arc<dyn UOwnedListener>) -> Result<(), UStatus>;
    }
}

#[cfg(not(tarpaulin_include))]
#[cfg(any(test, feature = "test-util"))]
#[async_trait]
impl UOwnedTransportImpl for MockUOwnedTransport {
    async fn send_validated_owned(&self, frame: ValidatedOwnedFrame) -> Result<(), UStatus> {
        self.do_send_validated_owned(frame).await
    }

    async fn receive_validated_owned(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        self.do_receive_owned(source_filter, sink_filter).await
    }

    async fn register_validated_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        self.do_register_owned_listener(source_filter, sink_filter, listener)
            .await
    }

    async fn unregister_validated_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        self.do_unregister_owned_listener(source_filter, sink_filter, listener)
            .await
    }
}

#[cfg(not(tarpaulin_include))]
#[cfg(any(test, feature = "test-util"))]
mockall::mock! {
    pub UZeroCopyTransport {
        pub async fn do_loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<UVecTxBuffer, UStatus>;
        pub async fn do_send_validated_zero_copy(&self, buffer: UVecTxBuffer) -> Result<(), UStatus>;
        pub async fn do_receive_zero_copy<'a>(&'a self, source_filter: &'a UUri, sink_filter: Option<&'a UUri>) -> Result<UVecRxLease, UStatus>;
        pub async fn do_register_zero_copy_listener<'a>(&'a self, source_filter: &'a UUri, sink_filter: Option<&'a UUri>, listener: Arc<dyn UZeroCopyListener<UVecRxLease>>) -> Result<(), UStatus>;
        pub async fn do_unregister_zero_copy_listener<'a>(&'a self, source_filter: &'a UUri, sink_filter: Option<&'a UUri>, listener: Arc<dyn UZeroCopyListener<UVecRxLease>>) -> Result<(), UStatus>;
    }
}

#[cfg(not(tarpaulin_include))]
#[cfg(any(test, feature = "test-util"))]
#[async_trait]
impl UZeroCopyTransportImpl for MockUZeroCopyTransport {
    type Tx = UVecTxBuffer;
    type Rx = UVecRxLease;

    async fn loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus> {
        self.do_loan_validated_tx(spec).await
    }

    async fn send_validated_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
        self.do_send_validated_zero_copy(buffer).await
    }

    async fn receive_validated_zero_copy(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus> {
        self.do_receive_zero_copy(source_filter, sink_filter).await
    }

    async fn register_validated_zero_copy_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        self.do_register_zero_copy_listener(source_filter, sink_filter, listener)
            .await
    }

    async fn unregister_validated_zero_copy_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        self.do_unregister_zero_copy_listener(source_filter, sink_filter, listener)
            .await
    }
}

/// A wrapper type that allows comparing [`UOwnedListener`]s to each other.
#[derive(Clone)]
pub struct ComparableOwnedListener {
    listener: Arc<dyn UOwnedListener>,
}

impl ComparableOwnedListener {
    pub fn new(listener: Arc<dyn UOwnedListener>) -> Self {
        Self { listener }
    }

    pub fn into_inner(&self) -> Arc<dyn UOwnedListener> {
        self.listener.clone()
    }

    fn pointer_address(&self) -> usize {
        let ptr = Arc::as_ptr(&self.listener);
        let thin_ptr = ptr.cast::<()>();
        thin_ptr as usize
    }
}

impl Deref for ComparableOwnedListener {
    type Target = dyn UOwnedListener;

    fn deref(&self) -> &Self::Target {
        &*self.listener
    }
}

impl Hash for ComparableOwnedListener {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.listener).hash(state);
    }
}

impl PartialEq for ComparableOwnedListener {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.listener, &other.listener)
    }
}

impl Eq for ComparableOwnedListener {}

impl Debug for ComparableOwnedListener {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ComparableOwnedListener: {}", self.pointer_address())
    }
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Mutex};

    use crate::{
        payload::RawBytes,
        test_util::{InMemoryOwnedTransport, InMemoryZeroCopyTransport},
        zero_copy::{UVecRxLease, UVecUninitTxBuffer},
    };

    use super::*;

    #[test]
    fn static_uri_provider_get_source() {
        let provider = StaticUriProvider::new("my-vehicle", 0x4210, 0x05);
        let source_uri = provider.get_source_uri();
        assert_eq!(source_uri.authority_name(), "my-vehicle");
        assert_eq!(source_uri.ue_id(), 0x4210);
        assert_eq!(source_uri.ue_version_major(), 0x05);
        assert_eq!(source_uri.resource_id_raw(), 0x0000);
    }

    #[test]
    fn verify_filter_criteria_accepts_publish_topic() {
        let source_filter = UUri::from_str("//vehicle1/AA/1/9000").expect("invalid source URI");
        assert!(verify_filter_criteria(&source_filter, None).is_ok());
    }

    #[tokio::test]
    async fn owned_transport_defaults_are_unimplemented() {
        struct EmptyTransport;

        #[async_trait]
        impl UOwnedTransportImpl for EmptyTransport {
            async fn send_validated_owned(
                &self,
                _frame: ValidatedOwnedFrame,
            ) -> Result<(), UStatus> {
                Ok(())
            }
        }

        let transport = EmptyTransport;
        let listener = Arc::new(MockUOwnedListener::new());
        let source = UUri::any();

        assert!(transport
            .receive_owned(&source, None)
            .await
            .is_err_and(|status| status.get_code() == UCode::UNIMPLEMENTED));
        assert!(transport
            .register_owned_listener(&source, None, listener.clone())
            .await
            .is_err_and(|status| status.get_code() == UCode::UNIMPLEMENTED));
        assert!(transport
            .unregister_owned_listener(&source, None, listener)
            .await
            .is_err_and(|status| status.get_code() == UCode::UNIMPLEMENTED));
    }

    #[tokio::test]
    async fn mock_owned_transport_delegates_send() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let frame = crate::UFrameBuilder::publish(topic)
            .build_with_raw_payload(Vec::<u8>::new())
            .unwrap();
        let expected = frame.clone();
        let mut transport = MockUOwnedTransport::new();
        transport
            .expect_do_send_validated_owned()
            .once()
            .withf(move |actual| actual.as_frame() == &expected)
            .return_const(Ok(()));

        transport.send_owned(frame).await.unwrap();
    }

    #[tokio::test]
    async fn owned_transport_rejects_invalid_frame_before_impl_observes_it() {
        #[derive(Default)]
        struct SpyOwnedTransport {
            send_count: Mutex<usize>,
        }

        #[async_trait]
        impl UOwnedTransportImpl for SpyOwnedTransport {
            async fn send_validated_owned(
                &self,
                _frame: ValidatedOwnedFrame,
            ) -> Result<(), UStatus> {
                *self.send_count.lock().expect("send count lock poisoned") += 1;
                Ok(())
            }
        }

        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let frame = UOwnedFrame::with_payload_unchecked(
            UFrameMetadata::publish_unchecked(topic),
            bytes::Bytes::from_static(b"missing encoding"),
        );
        let transport = SpyOwnedTransport::default();

        let status = transport
            .send_owned(frame)
            .await
            .expect_err("invalid frame must be rejected before implementation callback");

        assert_eq!(status.get_code(), UCode::INVALID_ARGUMENT);
        assert_eq!(*transport.send_count.lock().unwrap(), 0);
    }

    #[derive(Default)]
    struct CountingOwnedListener {
        count: Mutex<usize>,
    }

    impl CountingOwnedListener {
        fn count(&self) -> usize {
            *self
                .count
                .lock()
                .expect("owned listener count lock poisoned")
        }
    }

    #[async_trait]
    impl UOwnedListener for CountingOwnedListener {
        async fn on_receive_owned(&self, _frame: UOwnedFrame) {
            *self
                .count
                .lock()
                .expect("owned listener count lock poisoned") += 1;
        }
    }

    #[derive(Default)]
    struct ListenerSpyOwnedTransport {
        listener: Mutex<Option<Arc<dyn UOwnedListener>>>,
    }

    #[async_trait]
    impl UOwnedTransportImpl for ListenerSpyOwnedTransport {
        async fn send_validated_owned(&self, _frame: ValidatedOwnedFrame) -> Result<(), UStatus> {
            Ok(())
        }

        async fn register_validated_owned_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            listener: Arc<dyn UOwnedListener>,
        ) -> Result<(), UStatus> {
            *self.listener.lock().expect("listener lock poisoned") = Some(listener);
            Ok(())
        }

        async fn unregister_validated_owned_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            listener: Arc<dyn UOwnedListener>,
        ) -> Result<(), UStatus> {
            let mut registered = self.listener.lock().expect("listener lock poisoned");
            match registered.as_ref() {
                Some(existing) if Arc::ptr_eq(existing, &listener) => {
                    *registered = None;
                    Ok(())
                }
                _ => Err(UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    "unregistered listener identity did not match registered listener",
                )),
            }
        }
    }

    #[tokio::test]
    async fn owned_listener_wrapper_rejects_invalid_delivery_and_preserves_unregister_identity() {
        let source = UUri::any();
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let transport = ListenerSpyOwnedTransport::default();
        let listener = Arc::new(CountingOwnedListener::default());

        transport
            .register_owned_listener(&source, None, listener.clone())
            .await
            .unwrap();
        let registered = transport
            .listener
            .lock()
            .expect("listener lock poisoned")
            .as_ref()
            .expect("listener not registered")
            .clone();

        let invalid_frame = UOwnedFrame::with_payload_unchecked(
            UFrameMetadata::publish_unchecked(topic),
            bytes::Bytes::from_static(b"missing encoding"),
        );
        registered.on_receive_owned(invalid_frame).await;

        assert_eq!(listener.count(), 0);
        transport
            .unregister_owned_listener(&source, None, listener)
            .await
            .unwrap();
        assert!(transport.listener.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn owned_transport_ext_sends_payload_as_codec() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let transport = InMemoryOwnedTransport::default();

        transport
            .send_payload_as::<RawBytes, [u8]>(UFrameMetadata::publish_unchecked(topic), b"payload")
            .await
            .unwrap();

        let sent = transport.sent_frames();
        let frame = sent.first().unwrap();
        assert_eq!(frame.metadata().encoding(), Some(&RawBytes::encoding()));
        assert_eq!(frame.payload_bytes(), b"payload");
    }

    #[tokio::test]
    async fn zero_copy_transport_ext_sends_payload_as_codec() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let transport = InMemoryZeroCopyTransport::default();

        transport
            .send_encoded_payload_as::<RawBytes, [u8]>(
                UFrameMetadata::publish_unchecked(topic),
                b"payload",
            )
            .await
            .unwrap();

        let sent = transport.sent_frames();
        let frame = sent.first().unwrap();
        assert_eq!(frame.metadata().encoding(), Some(&RawBytes::encoding()));
        assert_eq!(frame.payload_bytes(), b"payload");
    }

    #[tokio::test]
    async fn zero_copy_transport_ext_sends_encoded_payload() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let transport = InMemoryZeroCopyTransport::default();
        let payload = EncodedPayload::<RawBytes>::from_bytes(bytes::Bytes::from_static(b"payload"));

        UZeroCopyTransportExt::send_encoded_payload(
            &transport,
            UFrameMetadata::publish_unchecked(topic),
            payload,
        )
        .await
        .unwrap();

        let sent = transport.sent_frames();
        let frame = sent.first().unwrap();
        assert_eq!(frame.metadata().encoding(), Some(&RawBytes::encoding()));
        assert_eq!(frame.payload_bytes(), b"payload");
    }

    #[derive(Default)]
    struct OversizedTxLoanTransport {
        send_count: Mutex<usize>,
    }

    #[async_trait]
    impl UZeroCopyTransportImpl for OversizedTxLoanTransport {
        type Tx = UVecTxBuffer;
        type Rx = UVecRxLease;

        async fn loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus> {
            UVecTxBuffer::with_alignment(
                spec.metadata().clone(),
                spec.payload_len() + 1,
                spec.payload_alignment(),
            )
            .map_err(UStatus::from)
        }

        async fn send_validated_zero_copy(&self, _buffer: Self::Tx) -> Result<(), UStatus> {
            *self.send_count.lock().expect("send count lock poisoned") += 1;
            Ok(())
        }
    }

    #[tokio::test]
    async fn zero_copy_ext_rejects_bad_tx_loan_layout_without_send() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let transport = OversizedTxLoanTransport::default();

        let status = transport
            .send_encoded_payload_as::<RawBytes, [u8]>(
                UFrameMetadata::publish_unchecked(topic),
                b"payload",
            )
            .await
            .expect_err("bad transport loan must fail before send");

        assert_eq!(status.get_code(), UCode::INTERNAL);
        assert_eq!(*transport.send_count.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn zero_copy_transport_rejects_invalid_send_buffer_before_impl_observes_it() {
        #[derive(Default)]
        struct SpyZeroCopyTransport {
            send_count: Mutex<usize>,
        }

        #[async_trait]
        impl UZeroCopyTransportImpl for SpyZeroCopyTransport {
            type Tx = UVecTxBuffer;
            type Rx = UVecRxLease;

            async fn loan_validated_tx(
                &self,
                spec: ValidatedTxLoanSpec,
            ) -> Result<Self::Tx, UStatus> {
                let payload_len = spec.payload_len();
                Ok(UVecTxBuffer::new(spec.into_metadata(), payload_len))
            }

            async fn send_validated_zero_copy(&self, _buffer: Self::Tx) -> Result<(), UStatus> {
                *self.send_count.lock().expect("send count lock poisoned") += 1;
                Ok(())
            }
        }

        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let buffer = UVecTxBuffer::new(UFrameMetadata::publish_unchecked(topic), 3);
        let transport = SpyZeroCopyTransport::default();

        let status = transport
            .send_zero_copy(buffer)
            .await
            .expect_err("invalid buffer must be rejected before implementation callback");

        assert_eq!(status.get_code(), UCode::INVALID_ARGUMENT);
        assert_eq!(*transport.send_count.lock().unwrap(), 0);
    }

    #[derive(Default)]
    struct CountingZeroCopyListener {
        count: Mutex<usize>,
    }

    impl CountingZeroCopyListener {
        fn count(&self) -> usize {
            *self
                .count
                .lock()
                .expect("zero-copy listener count lock poisoned")
        }
    }

    #[async_trait]
    impl UZeroCopyListener<UVecRxLease> for CountingZeroCopyListener {
        async fn on_receive_zero_copy(&self, _frame: UVecRxLease) {
            *self
                .count
                .lock()
                .expect("zero-copy listener count lock poisoned") += 1;
        }
    }

    #[derive(Default)]
    struct ListenerSpyZeroCopyTransport {
        listener: Mutex<Option<Arc<dyn UZeroCopyListener<UVecRxLease>>>>,
    }

    #[async_trait]
    impl UZeroCopyTransportImpl for ListenerSpyZeroCopyTransport {
        type Tx = UVecTxBuffer;
        type Rx = UVecRxLease;

        async fn loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus> {
            Ok(UVecTxBuffer::new(spec.into_metadata(), 0))
        }

        async fn send_validated_zero_copy(&self, _buffer: Self::Tx) -> Result<(), UStatus> {
            Ok(())
        }

        async fn register_validated_zero_copy_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
        ) -> Result<(), UStatus> {
            *self.listener.lock().expect("listener lock poisoned") = Some(listener);
            Ok(())
        }

        async fn unregister_validated_zero_copy_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
        ) -> Result<(), UStatus> {
            let mut registered = self.listener.lock().expect("listener lock poisoned");
            match registered.as_ref() {
                Some(existing) if Arc::ptr_eq(existing, &listener) => {
                    *registered = None;
                    Ok(())
                }
                _ => Err(UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    "unregistered listener identity did not match registered listener",
                )),
            }
        }
    }

    #[tokio::test]
    async fn zero_copy_listener_wrapper_rejects_invalid_delivery_and_preserves_unregister_identity()
    {
        let source = UUri::any();
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let transport = ListenerSpyZeroCopyTransport::default();
        let listener = Arc::new(CountingZeroCopyListener::default());

        transport
            .register_zero_copy_listener(&source, None, listener.clone())
            .await
            .unwrap();
        let registered = transport
            .listener
            .lock()
            .expect("listener lock poisoned")
            .as_ref()
            .expect("listener not registered")
            .clone();

        let invalid_frame = UOwnedFrame::with_payload_unchecked(
            UFrameMetadata::publish_unchecked(topic),
            bytes::Bytes::from_static(b"missing encoding"),
        );
        registered
            .on_receive_zero_copy(UVecRxLease::new(invalid_frame))
            .await;

        assert_eq!(listener.count(), 0);
        transport
            .unregister_zero_copy_listener(&source, None, listener)
            .await
            .unwrap();
        assert!(transport.listener.lock().unwrap().is_none());
    }

    #[derive(Default)]
    struct OversizedUninitTxLoanTransport {
        send_count: Mutex<usize>,
    }

    #[async_trait]
    impl UZeroCopyTransportImpl for OversizedUninitTxLoanTransport {
        type Tx = UVecTxBuffer;
        type Rx = UVecRxLease;

        async fn loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus> {
            UVecTxBuffer::with_alignment(
                spec.metadata().clone(),
                spec.payload_len(),
                spec.payload_alignment(),
            )
            .map_err(UStatus::from)
        }

        async fn send_validated_zero_copy(&self, _buffer: Self::Tx) -> Result<(), UStatus> {
            *self.send_count.lock().expect("send count lock poisoned") += 1;
            Ok(())
        }
    }

    #[async_trait]
    impl UZeroCopyUninitTransportImpl for OversizedUninitTxLoanTransport {
        type UninitTx = UVecUninitTxBuffer;

        async fn loan_validated_uninit_tx(
            &self,
            spec: ValidatedTxLoanSpec,
        ) -> Result<Self::UninitTx, UStatus> {
            UVecUninitTxBuffer::with_alignment(
                spec.metadata().clone(),
                spec.payload_len() + 1,
                spec.payload_alignment(),
            )
            .map_err(UStatus::from)
        }
    }

    #[tokio::test]
    async fn zero_copy_uninit_ext_rejects_bad_uninit_loan_layout_without_send() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let transport = OversizedUninitTxLoanTransport::default();

        let status = transport
            .send_uninit_loaned_bytes_as::<RawBytes>(
                UFrameMetadata::publish_unchecked(topic),
                3,
                1,
                |mut writer| {
                    writer.write_all(b"abc")?;
                    Ok(writer)
                },
            )
            .await
            .expect_err("bad uninit transport loan must fail before send");

        assert_eq!(status.get_code(), UCode::INTERNAL);
        assert_eq!(*transport.send_count.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn zero_copy_uninit_ext_rejects_invalid_requested_layout_as_invalid_argument() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let transport = InMemoryZeroCopyTransport::default();

        let status = transport
            .send_uninit_loaned_bytes_as::<RawBytes>(
                UFrameMetadata::publish_unchecked(topic),
                3,
                3,
                |writer| Ok(writer),
            )
            .await
            .expect_err("non-power-of-two requested alignment must be invalid argument");

        assert_eq!(status.get_code(), UCode::INVALID_ARGUMENT);
        assert!(transport.sent_frames().is_empty());
    }

    #[test]
    fn validate_owned_frame_accepts_absent_payload_without_encoding() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let frame = crate::UFrameBuilder::publish(topic).build().unwrap();

        validate_owned_frame_for_transport(&frame).unwrap();
    }

    #[test]
    fn tx_loan_spec_accepts_absent_payload_without_encoding() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let spec = UTxLoanSpec::no_payload(UFrameMetadata::publish_unchecked(topic)).unwrap();

        assert!(!spec.has_payload());
        assert_eq!(spec.payload_len(), 0);
        assert_eq!(spec.payload_alignment(), 1);
    }

    #[test]
    fn tx_loan_spec_accepts_present_empty_payload_with_encoding() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let spec = UTxLoanSpec::present_empty_payload(
            UFrameMetadata::publish_unchecked(topic).with_encoding(RawBytes::encoding()),
        )
        .unwrap();

        assert!(spec.has_payload());
        assert_eq!(spec.payload_len(), 0);
        assert_eq!(spec.payload_alignment(), 1);
    }

    #[test]
    fn tx_loan_spec_accepts_payload_bytes_with_encoding() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let layout = PayloadLayout::new(8, 4).unwrap();
        let spec = UTxLoanSpec::payload(
            UFrameMetadata::publish_unchecked(topic).with_encoding(RawBytes::encoding()),
            layout,
        )
        .unwrap();

        assert!(spec.has_payload());
        assert_eq!(spec.payload_layout(), Some(layout));
    }

    #[test]
    fn tx_loan_spec_rejects_payload_without_encoding() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let layout = PayloadLayout::new(1, 1).unwrap();
        let status =
            UTxLoanSpec::payload(UFrameMetadata::publish_unchecked(topic), layout).unwrap_err();

        assert_eq!(status.get_code(), UCode::INVALID_ARGUMENT);
        assert!(status.get_message().contains("payload encoding is absent"));
    }

    #[test]
    fn tx_loan_spec_rejects_encoding_with_absent_payload() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let status = UTxLoanSpec::no_payload(
            UFrameMetadata::publish_unchecked(topic).with_encoding(RawBytes::encoding()),
        )
        .unwrap_err();

        assert_eq!(status.get_code(), UCode::INVALID_ARGUMENT);
        assert!(status.get_message().contains("payload is absent"));
    }

    #[test]
    fn tx_payload_spec_rejects_invalid_alignment() {
        let error = UTxPayloadSpec::present(1, 3).unwrap_err();

        assert!(matches!(error, UWireError::InvalidPayload(_)));
    }

    #[test]
    fn validate_owned_frame_accepts_present_empty_payload_with_encoding() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let frame = crate::UFrameBuilder::publish(topic)
            .build_with_raw_payload(Vec::<u8>::new())
            .unwrap();

        validate_owned_frame_for_transport(&frame).unwrap();
    }

    #[test]
    fn validate_owned_frame_rejects_payload_without_encoding() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let frame = UOwnedFrame::with_payload_unchecked(
            UFrameMetadata::publish_unchecked(topic),
            Vec::<u8>::new(),
        );
        let status = validate_owned_frame_for_transport(&frame).unwrap_err();

        assert_eq!(status.get_code(), UCode::INVALID_ARGUMENT);
        assert!(status.get_message().contains("payload encoding is absent"));
    }

    #[test]
    fn validate_frame_metadata_rejects_expired_frames() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let attributes = crate::UAttributes::new_unchecked(
            crate::UUID::build_for_timestamp_millis(1),
            topic,
            None,
            crate::UMessageType::Publish,
        )
        .with_ttl(1);
        let metadata = UFrameMetadata::without_payload_encoding_unchecked(attributes);
        let status = validate_frame_metadata_for_transport(&metadata).unwrap_err();

        assert_eq!(status.get_code(), UCode::DEADLINE_EXCEEDED);
    }
}
