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

use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::num::TryFromIntError;
use std::ops::Deref;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;

use crate::{
    payload::{
        BytePayloadCodec, EncodePayload, EncodedPayload, LoanPayload, LoanUninitPayload,
        LoanedInitPayload, PayloadCodec, PayloadFormat, PayloadLayout, USerializer, UWireError,
    },
    zero_copy::{
        verify_tx_buffer_payload_layout, verify_uninit_tx_buffer_payload_layout,
        LoanedUninitByteWriter, UTxBuffer, UUninitTxBuffer, UZeroCopyRxFrame,
    },
    UCode, UFrameMetadata, UOwnedFrame, UStatus, UUri,
};

#[cfg(any(
    feature = "unsafe-stable-payload-tx",
    feature = "expert-unsafe-payloads"
))]
use crate::payload::{StableContainerPayload, StablePayload, UnsafeStablePayloadTxSlot};

#[cfg(any(test, feature = "test-util"))]
use crate::zero_copy::UVecTxBuffer;

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

/// The serialization-neutral owned-buffer transport API.
///
/// Owned transports are the default path for network, brokered, and in-process
/// transports. They accept native frame metadata plus owned payload bytes.
///
/// ```no_run
/// # use async_trait::async_trait;
/// # use up_rust::{UOwnedFrame, UOwnedTransport, UStatus};
/// struct MyTransport;
///
/// #[async_trait]
/// impl UOwnedTransport for MyTransport {
///     async fn send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
///         let metadata = frame.metadata();
///         let payload = frame.payload_bytes();
///         # let _ = (metadata, payload);
///         Ok(())
///     }
/// }
/// ```
#[async_trait]
pub trait UOwnedTransport: Send + Sync {
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
    Rx: UZeroCopyRxFrame + Send + 'static,
{
    /// Handles one received zero-copy frame lease.
    async fn on_receive_zero_copy(&self, frame: Rx);
}

/// The zero-copy transport capability API.
///
/// Implement this trait only when the transport can loan transmit storage or
/// deliver receive leases without hiding transport-owned copies.
///
/// The metadata passed to [`Self::reserve`] is final for that transmit loan.
/// Implementations may encode metadata into native transport headers, side-band
/// properties, or hidden prefixes before returning the loan. Callers must choose
/// payload encoding, routing attributes, and other metadata before reserving so
/// [`UTxBuffer::payload_mut`] remains the only mutable zero-copy surface.
///
/// This is the zero-copy sibling of [`UOwnedTransport`]. Pull receive and
/// listener registration map one-to-one to the owned API, while send is
/// intentionally split into [`Self::reserve`] plus [`Self::send_zero_copy`] so
/// serializers can write directly into the transport loan.
///
/// ```no_run
/// # use async_trait::async_trait;
/// # use up_rust::{zero_copy::{UVecTxBuffer, UZeroCopyTransport}, UFrameMetadata, UOwnedFrame, UStatus, UUri};
/// struct SharedMemoryTransport;
///
/// #[async_trait]
/// impl UZeroCopyTransport for SharedMemoryTransport {
///     type Tx = UVecTxBuffer;
///     type Rx = UOwnedFrame;
///
///     async fn reserve(
///         &self,
///         metadata: UFrameMetadata,
///         payload_len: usize,
///         _alignment: usize,
///     ) -> Result<Self::Tx, UStatus> {
///         Ok(UVecTxBuffer::new(metadata, payload_len))
///     }
///
///     async fn send_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
///         # let _ = buffer;
///         Ok(())
///     }
/// }
/// ```
#[async_trait]
pub trait UZeroCopyTransport: Send + Sync {
    /// Transport-specific transmit loan type returned by [`Self::reserve`].
    type Tx: UTxBuffer + Send;

    /// Transport-specific receive lease type returned by pull receive and
    /// delivered to zero-copy listeners.
    type Rx: UZeroCopyRxFrame + Send + 'static;

    /// Reserves transmit storage for a frame with `metadata` and payload layout.
    ///
    /// `payload_len` is the number of application payload bytes the serializer
    /// will write. `alignment` is the serializer's required payload alignment.
    /// Implementations must either honor the requested alignment or return an
    /// error before handing the loan to the caller. Metadata is immutable after
    /// this call; transports may use it to compute payload layout and native
    /// transport representation before exposing the payload storage.
    async fn reserve(
        &self,
        metadata: UFrameMetadata,
        payload_len: usize,
        alignment: usize,
    ) -> Result<Self::Tx, UStatus>;

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

/// Optional zero-copy capability for transports that can expose uninitialized TX payload storage.
///
/// This is deliberately separate from [`UZeroCopyTransport::reserve`], whose
/// [`UTxBuffer`] result exposes initialized byte slices. Implementations must
/// initialize any transport-owned bytes before returning the uninitialized loan;
/// callers initialize only the visible application payload bytes.
#[async_trait]
pub trait UZeroCopyUninitTransport: UZeroCopyTransport {
    /// Transport-specific uninitialized transmit loan type.
    type UninitTx: UUninitTxBuffer<Initialized = Self::Tx> + Send;

    /// Reserves uninitialized transmit storage for a frame payload.
    async fn reserve_uninit(
        &self,
        metadata: UFrameMetadata,
        payload_len: usize,
        alignment: usize,
    ) -> Result<Self::UninitTx, UStatus>;
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
        let mut buffer = self
            .reserve(
                metadata.with_encoding(C::payload_encoding()),
                layout.len(),
                layout.align(),
            )
            .await?;
        verify_reserved_tx_payload_layout(&mut buffer, layout)?;
        C::encode_payload(value, buffer.payload_mut()).map_err(UStatus::from)?;
        self.send_zero_copy(buffer).await
    }

    /// Compatibility alias for [`Self::send_encoded_payload_as`].
    async fn send_payload_as<C, T>(
        &self,
        metadata: UFrameMetadata,
        value: &T,
    ) -> Result<(), UStatus>
    where
        C: PayloadCodec + EncodePayload<T> + Send + Sync,
        T: ?Sized + Sync,
    {
        self.send_encoded_payload_as::<C, T>(metadata, value).await
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
        let mut buffer = self
            .reserve(
                metadata.with_encoding(C::payload_encoding()),
                layout.len(),
                layout.align(),
            )
            .await?;
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
        let mut buffer = self
            .reserve(
                metadata.with_encoding(C::payload_encoding()),
                layout.len(),
                layout.align(),
            )
            .await?;
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
        let mut buffer = self
            .reserve_uninit(
                metadata.with_encoding(C::payload_encoding()),
                layout.len(),
                layout.align(),
            )
            .await?;
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
        let mut buffer = self
            .reserve_uninit(
                metadata.with_encoding(C::payload_encoding()),
                layout.len(),
                layout.align(),
            )
            .await?;
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
        let mut buffer = self
            .reserve_uninit(
                metadata.with_encoding(StableContainerPayload::<T>::encoding()),
                layout.len(),
                layout.align(),
            )
            .await?;
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
        pub async fn do_send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus>;
        pub async fn do_receive_owned<'a>(&'a self, source_filter: &'a UUri, sink_filter: Option<&'a UUri>) -> Result<UOwnedFrame, UStatus>;
        pub async fn do_register_owned_listener<'a>(&'a self, source_filter: &'a UUri, sink_filter: Option<&'a UUri>, listener: Arc<dyn UOwnedListener>) -> Result<(), UStatus>;
        pub async fn do_unregister_owned_listener<'a>(&'a self, source_filter: &'a UUri, sink_filter: Option<&'a UUri>, listener: Arc<dyn UOwnedListener>) -> Result<(), UStatus>;
    }
}

#[cfg(not(tarpaulin_include))]
#[cfg(any(test, feature = "test-util"))]
#[async_trait]
impl UOwnedTransport for MockUOwnedTransport {
    async fn send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.do_send_owned(frame).await
    }

    async fn receive_owned(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        self.do_receive_owned(source_filter, sink_filter).await
    }

    async fn register_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        self.do_register_owned_listener(source_filter, sink_filter, listener)
            .await
    }

    async fn unregister_owned_listener(
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
        pub async fn do_reserve(&self, metadata: UFrameMetadata, payload_len: usize, alignment: usize) -> Result<UVecTxBuffer, UStatus>;
        pub async fn do_send_zero_copy(&self, buffer: UVecTxBuffer) -> Result<(), UStatus>;
        pub async fn do_receive_zero_copy<'a>(&'a self, source_filter: &'a UUri, sink_filter: Option<&'a UUri>) -> Result<UOwnedFrame, UStatus>;
        pub async fn do_register_zero_copy_listener<'a>(&'a self, source_filter: &'a UUri, sink_filter: Option<&'a UUri>, listener: Arc<dyn UZeroCopyListener<UOwnedFrame>>) -> Result<(), UStatus>;
        pub async fn do_unregister_zero_copy_listener<'a>(&'a self, source_filter: &'a UUri, sink_filter: Option<&'a UUri>, listener: Arc<dyn UZeroCopyListener<UOwnedFrame>>) -> Result<(), UStatus>;
    }
}

#[cfg(not(tarpaulin_include))]
#[cfg(any(test, feature = "test-util"))]
#[async_trait]
impl UZeroCopyTransport for MockUZeroCopyTransport {
    type Tx = UVecTxBuffer;
    type Rx = UOwnedFrame;

    async fn reserve(
        &self,
        metadata: UFrameMetadata,
        payload_len: usize,
        alignment: usize,
    ) -> Result<Self::Tx, UStatus> {
        self.do_reserve(metadata, payload_len, alignment).await
    }

    async fn send_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
        self.do_send_zero_copy(buffer).await
    }

    async fn receive_zero_copy(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus> {
        self.do_receive_zero_copy(source_filter, sink_filter).await
    }

    async fn register_zero_copy_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        self.do_register_zero_copy_listener(source_filter, sink_filter, listener)
            .await
    }

    async fn unregister_zero_copy_listener(
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
        let thin_ptr = ptr as *const ();
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
        zero_copy::UVecUninitTxBuffer,
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
        impl UOwnedTransport for EmptyTransport {
            async fn send_owned(&self, _frame: UOwnedFrame) -> Result<(), UStatus> {
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
            .expect_do_send_owned()
            .once()
            .withf(move |actual| actual == &expected)
            .return_const(Ok(()));

        transport.send_owned(frame).await.unwrap();
    }

    #[tokio::test]
    async fn owned_transport_ext_sends_payload_as_codec() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let transport = InMemoryOwnedTransport::default();

        transport
            .send_payload_as::<RawBytes, [u8]>(UFrameMetadata::publish(topic), b"payload")
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
            .send_payload_as::<RawBytes, [u8]>(UFrameMetadata::publish(topic), b"payload")
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
            UFrameMetadata::publish(topic),
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
    impl UZeroCopyTransport for OversizedTxLoanTransport {
        type Tx = UVecTxBuffer;
        type Rx = UOwnedFrame;

        async fn reserve(
            &self,
            metadata: UFrameMetadata,
            payload_len: usize,
            alignment: usize,
        ) -> Result<Self::Tx, UStatus> {
            UVecTxBuffer::with_alignment(metadata, payload_len + 1, alignment)
                .map_err(UStatus::from)
        }

        async fn send_zero_copy(&self, _buffer: Self::Tx) -> Result<(), UStatus> {
            *self.send_count.lock().expect("send count lock poisoned") += 1;
            Ok(())
        }
    }

    #[tokio::test]
    async fn zero_copy_ext_rejects_bad_tx_loan_layout_without_send() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let transport = OversizedTxLoanTransport::default();

        let status = transport
            .send_payload_as::<RawBytes, [u8]>(UFrameMetadata::publish(topic), b"payload")
            .await
            .expect_err("bad transport loan must fail before send");

        assert_eq!(status.get_code(), UCode::INTERNAL);
        assert_eq!(*transport.send_count.lock().unwrap(), 0);
    }

    #[derive(Default)]
    struct OversizedUninitTxLoanTransport {
        send_count: Mutex<usize>,
    }

    #[async_trait]
    impl UZeroCopyTransport for OversizedUninitTxLoanTransport {
        type Tx = UVecTxBuffer;
        type Rx = UOwnedFrame;

        async fn reserve(
            &self,
            metadata: UFrameMetadata,
            payload_len: usize,
            alignment: usize,
        ) -> Result<Self::Tx, UStatus> {
            UVecTxBuffer::with_alignment(metadata, payload_len, alignment).map_err(UStatus::from)
        }

        async fn send_zero_copy(&self, _buffer: Self::Tx) -> Result<(), UStatus> {
            *self.send_count.lock().expect("send count lock poisoned") += 1;
            Ok(())
        }
    }

    #[async_trait]
    impl UZeroCopyUninitTransport for OversizedUninitTxLoanTransport {
        type UninitTx = UVecUninitTxBuffer;

        async fn reserve_uninit(
            &self,
            metadata: UFrameMetadata,
            payload_len: usize,
            alignment: usize,
        ) -> Result<Self::UninitTx, UStatus> {
            UVecUninitTxBuffer::with_alignment(metadata, payload_len + 1, alignment)
                .map_err(UStatus::from)
        }
    }

    #[tokio::test]
    async fn zero_copy_uninit_ext_rejects_bad_uninit_loan_layout_without_send() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let transport = OversizedUninitTxLoanTransport::default();

        let status = transport
            .send_uninit_loaned_bytes_as::<RawBytes>(
                UFrameMetadata::publish(topic),
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
                UFrameMetadata::publish(topic),
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
        let frame = UOwnedFrame::new(UFrameMetadata::publish(topic), Vec::<u8>::new());
        let status = validate_owned_frame_for_transport(&frame).unwrap_err();

        assert_eq!(status.get_code(), UCode::INVALID_ARGUMENT);
        assert!(status.get_message().contains("payload encoding is absent"));
    }

    #[test]
    fn validate_frame_metadata_rejects_expired_frames() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let attributes = crate::UAttributes::new(
            crate::UUID::build_for_timestamp_millis(1),
            topic,
            None,
            crate::UMessageType::Publish,
        )
        .with_ttl(1);
        let metadata = UFrameMetadata::without_payload_encoding(attributes);
        let status = validate_frame_metadata_for_transport(&metadata).unwrap_err();

        assert_eq!(status.get_code(), UCode::DEADLINE_EXCEEDED);
    }
}
