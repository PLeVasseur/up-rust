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
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::{
    UCode, UFrameMetadata, UOwnedFrame, USerializer, UStatus, UTxBuffer, UUri, UWireError,
    UZeroCopyRxFrame, WireFormat,
};

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

/// A factory for URIs representing this uEntity's resources.
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
        let local_uri = UUri {
            authority_name: authority.into(),
            ue_id: entity_id,
            ue_version_major: u32::from(major_version),
            resource_id: 0x0000,
        };
        StaticUriProvider { local_uri }
    }
}

impl LocalUriProvider for StaticUriProvider {
    fn get_authority(&self) -> String {
        self.local_uri.authority_name.clone()
    }

    fn get_resource_uri(&self, resource_id: u16) -> UUri {
        let mut uri = self.local_uri.clone();
        uri.resource_id = u32::from(resource_id);
        uri
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
        let major_version = u8::try_from(source_uri.ue_version_major)?;
        Ok(StaticUriProvider::new(
            &source_uri.authority_name,
            source_uri.ue_id,
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

struct OneShotOwnedListener {
    sender: Mutex<Option<futures_channel::oneshot::Sender<UOwnedFrame>>>,
}

impl OneShotOwnedListener {
    fn new(sender: futures_channel::oneshot::Sender<UOwnedFrame>) -> Self {
        Self {
            sender: Mutex::new(Some(sender)),
        }
    }
}

#[async_trait]
impl UOwnedListener for OneShotOwnedListener {
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        let Ok(mut sender) = self.sender.lock() else {
            return;
        };
        if let Some(sender) = sender.take() {
            let _ = sender.send(frame);
        }
    }
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

    async fn receive_owned(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        // Pull receive is built from the listener API so push-oriented transports
        // do not need a separate queueing implementation.
        let (sender, receiver) = futures_channel::oneshot::channel();
        let listener: Arc<dyn UOwnedListener> = Arc::new(OneShotOwnedListener::new(sender));
        self.register_owned_listener(source_filter, sink_filter, listener.clone())
            .await?;
        let received = receiver.await.map_err(|_| {
            UStatus::fail_with_code(UCode::CANCELLED, "receive listener was cancelled")
        });
        let unregister_result = self
            .unregister_owned_listener(source_filter, sink_filter, listener)
            .await;
        match (received, unregister_result) {
            (Ok(frame), Ok(())) => Ok(frame),
            (Ok(_), Err(err)) => Err(err),
            (Err(err), _) => Err(err),
        }
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
    async fn send_serialized<F, T>(
        &self,
        metadata: UFrameMetadata,
        value: &T,
    ) -> Result<(), UStatus>
    where
        F: WireFormat + Send + Sync,
        T: USerializer<F> + Sync,
    {
        let frame =
            UOwnedFrame::from_serializable::<F, T>(metadata, value).map_err(UStatus::from)?;
        self.send_owned(frame).await
    }
}

impl<T> UOwnedTransportExt for T where T: UOwnedTransport + ?Sized {}

/// A handler for processing zero-copy receive leases.
#[async_trait]
pub trait UZeroCopyListener<Rx>: Send + Sync
where
    Rx: UZeroCopyRxFrame + Send + 'static,
{
    async fn on_receive_zero_copy(&self, frame: Rx);
}

/// The zero-copy transport capability API.
///
/// Implement this trait only when the transport can loan transmit storage or
/// deliver receive leases without hiding transport-owned copies.
///
/// ```no_run
/// # use async_trait::async_trait;
/// # use up_rust::{UFrameMetadata, UOwnedFrame, UStatus, UUri, UVecTxBuffer, UZeroCopyTransport};
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
    type Tx: UTxBuffer + Send;
    type Rx: UZeroCopyRxFrame + Send + 'static;

    async fn reserve(
        &self,
        metadata: UFrameMetadata,
        payload_len: usize,
        alignment: usize,
    ) -> Result<Self::Tx, UStatus>;

    async fn send_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus>;

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

/// Convenience methods for zero-copy transports.
#[async_trait]
pub trait UZeroCopyTransportExt: UZeroCopyTransport {
    async fn send_serialized_zero_copy<F, T>(
        &self,
        metadata: UFrameMetadata,
        value: &T,
    ) -> Result<(), UStatus>
    where
        F: WireFormat + Send + Sync,
        T: USerializer<F> + Sync,
    {
        let payload_len = value.encoded_len();
        let mut buffer = self
            .reserve(
                metadata.with_encoding(F::encoding()),
                payload_len,
                T::ALIGNMENT,
            )
            .await?;
        let written = value
            .serialize_into(buffer.payload_mut())
            .map_err(UStatus::from)?;
        if written != payload_len {
            return Err(UStatus::from(UWireError::invalid_payload(format!(
                "serializer wrote {written} bytes but encoded_len returned {payload_len} bytes"
            ))));
        }
        self.send_zero_copy(buffer).await
    }
}

impl<T> UZeroCopyTransportExt for T where T: UZeroCopyTransport + ?Sized {}

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
    use std::str::FromStr;

    use super::*;

    #[test]
    fn static_uri_provider_get_source() {
        let provider = StaticUriProvider::new("my-vehicle", 0x4210, 0x05);
        let source_uri = provider.get_source_uri();
        assert_eq!(source_uri.authority_name, "my-vehicle");
        assert_eq!(source_uri.ue_id, 0x4210);
        assert_eq!(source_uri.ue_version_major, 0x05);
        assert_eq!(source_uri.resource_id, 0x0000);
    }

    #[test]
    fn verify_filter_criteria_accepts_publish_topic() {
        let source_filter = UUri::from_str("//vehicle1/AA/1/9000").expect("invalid source URI");
        assert!(verify_filter_criteria(&source_filter, None).is_ok());
    }
}
