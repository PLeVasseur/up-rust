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

//! Zero-copy communication-layer facade.
//!
//! This module exposes the simple L2 operations that can be expressed without
//! hiding zero-copy lifetimes. Stable/no-zero publish is provided as an ordinary
//! front-door API. Receive, listener, RPC server, and request handling remain at
//! the [`UZeroCopyTransport`] layer for now because their non-copying shape is
//! tied to the transport receive lease type. Any future L2 convenience that
//! copies out of a receive lease must use a `copying` name and must not claim
//! no-copy behavior.

use std::sync::Arc;

use crate::{
    communication::{CallOptions, PubSubError},
    LocalUriProvider, StableContainerPayload, StablePayloadInit, UFrameMetadata, UHasWire,
    UMessageBuilder, UWireLoan, UWireLoanUninit, UZeroCopyTransport, UZeroCopyTransportExt,
    UZeroCopyUninitTransport, UZeroCopyUninitTransportExt,
};

/// Front door for zero-copy communication-layer clients.
pub struct Endpoint<T, P>
where
    T: UZeroCopyTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    transport: Arc<T>,
    uri_provider: Arc<P>,
}

impl<T, P> Endpoint<T, P>
where
    T: UZeroCopyTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    /// Creates a zero-copy communication endpoint.
    #[must_use]
    pub fn new(transport: Arc<T>, uri_provider: Arc<P>) -> Self {
        Self {
            transport,
            uri_provider,
        }
    }

    /// Creates a publisher for zero-copy stable payload messages.
    #[must_use]
    pub fn publisher(&self) -> Publisher<T, P> {
        Publisher::new(self.transport.clone(), self.uri_provider.clone())
    }
}

/// Publisher implemented over a selected-wire zero-copy transport.
pub struct Publisher<T, P>
where
    T: UZeroCopyTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    transport: Arc<T>,
    uri_provider: Arc<P>,
}

impl<T, P> Publisher<T, P>
where
    T: UZeroCopyTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    /// Creates a publisher over a zero-copy transport.
    #[must_use]
    pub fn new(transport: Arc<T>, uri_provider: Arc<P>) -> Self {
        Self {
            transport,
            uri_provider,
        }
    }

    fn build_metadata(
        &self,
        resource_id: u16,
        call_options: CallOptions,
    ) -> Result<UFrameMetadata, PubSubError> {
        let mut builder = UMessageBuilder::publish(self.uri_provider.get_resource_uri(resource_id));
        builder.with_ttl(call_options.ttl());
        if let Some(message_id) = call_options.message_id() {
            builder.with_message_id(message_id.clone());
        }
        if let Some(priority) = call_options.priority() {
            builder.with_priority(priority);
        }
        let message = builder.build().map_err(|error| {
            PubSubError::InvalidArgument(format!(
                "failed to create zero-copy Publish message metadata from parameters: {error}"
            ))
        })?;
        crate::try_project_umessage_to_frame_metadata(&message).map_err(|error| {
            PubSubError::InvalidArgument(format!(
                "failed to create zero-copy Publish frame metadata from parameters: {error}"
            ))
        })
    }
}

impl<T, P> Publisher<T, P>
where
    T: UZeroCopyTransport + UHasWire + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    /// Publishes an initialized stable payload using the transport endpoint's selected wire.
    pub async fn publish_stable<Payload>(
        &self,
        resource_id: u16,
        call_options: CallOptions,
        init: impl for<'payload> FnOnce(&'payload mut Payload) + Send,
    ) -> Result<(), PubSubError>
    where
        T::Wire: UWireLoan<Payload>,
        <T::Wire as UWireLoan<Payload>>::Codec: Send + Sync,
    {
        let metadata = self.build_metadata(resource_id, call_options)?;
        self.transport
            .send_loaned_payload::<Payload>(metadata, init)
            .await
            .map_err(Box::from)
            .map_err(PubSubError::PublishError)
    }
}

impl<T, P> Publisher<T, P>
where
    T: UZeroCopyUninitTransport + UHasWire + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    /// Publishes a stable payload by initializing it directly in uninitialized transport storage.
    pub async fn publish_uninit_stable<Payload>(
        &self,
        resource_id: u16,
        call_options: CallOptions,
        init: impl for<'payload> FnOnce(
                <Payload as StablePayloadInit>::Init<'payload>,
            ) -> Result<
                crate::InitializedStablePayload<Payload>,
                crate::UWireError,
            > + Send,
    ) -> Result<(), PubSubError>
    where
        T::Wire: UWireLoanUninit<Payload, Codec = StableContainerPayload<Payload>>,
        Payload: StablePayloadInit + Send,
    {
        let metadata = self.build_metadata(resource_id, call_options)?;
        self.transport
            .send_uninit_stable_payload::<Payload>(metadata, init)
            .await
            .map_err(Box::from)
            .map_err(PubSubError::PublishError)
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::{
        payload::StablePayloadInitSlot, ByteBackedStablePayload, InMemoryZeroCopyTransport,
        StableContainerWireFormat, StablePayload, StaticUriProvider, UCode, UFrameView,
        ULoanedContiguousZeroCopyRxFrame, UStatus, UVecRxLease, UVecTxBuffer, UVecUninitTxBuffer,
        UZeroCopyTransportImpl, UZeroCopyUninitTransportImpl, ValidatedTxLoanSpec,
    };

    struct SelectedStableTransport {
        inner: InMemoryZeroCopyTransport,
        wire: StableContainerWireFormat,
    }

    impl SelectedStableTransport {
        fn new() -> Self {
            Self {
                inner: InMemoryZeroCopyTransport::default(),
                wire: StableContainerWireFormat,
            }
        }

        fn sent_frames(&self) -> Vec<UVecRxLease> {
            self.inner.sent_frames()
        }
    }

    impl UHasWire for SelectedStableTransport {
        type Wire = StableContainerWireFormat;

        fn wire(&self) -> &Self::Wire {
            &self.wire
        }
    }

    #[async_trait]
    impl UZeroCopyTransportImpl for SelectedStableTransport {
        type Tx = UVecTxBuffer;
        type Rx = UVecRxLease;

        async fn loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus> {
            UZeroCopyTransportImpl::loan_validated_tx(&self.inner, spec).await
        }

        async fn send_validated_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
            UZeroCopyTransportImpl::send_validated_zero_copy(&self.inner, buffer).await
        }
    }

    #[async_trait]
    impl UZeroCopyUninitTransportImpl for SelectedStableTransport {
        type UninitTx = UVecUninitTxBuffer;

        async fn loan_validated_uninit_tx(
            &self,
            spec: ValidatedTxLoanSpec,
        ) -> Result<Self::UninitTx, UStatus> {
            UZeroCopyUninitTransportImpl::loan_validated_uninit_tx(&self.inner, spec).await
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    struct StableBytes {
        bytes: [u8; 4],
    }

    unsafe impl StablePayload for StableBytes {
        const TYPE_NAME: &'static str = "uprotocol.communication.zero_copy.StableBytes";
    }

    unsafe impl ByteBackedStablePayload for StableBytes {}

    struct StableBytesInit<'a> {
        slot: StablePayloadInitSlot<'a, StableBytes>,
        written: bool,
    }

    impl StableBytesInit<'_> {
        fn bytes_from_array(mut self, bytes: &[u8; 4]) -> Self {
            // SAFETY: `StableBytes` is `repr(C)` over exactly one `[u8; 4]` field at offset zero.
            unsafe { self.slot.write_bytes(0, bytes) };
            self.written = true;
            self
        }

        fn finish(self) -> Result<crate::InitializedStablePayload<StableBytes>, crate::UWireError> {
            if !self.written {
                return Err(crate::UWireError::invalid_payload(
                    "StableBytes.bytes was not initialized",
                ));
            }
            // SAFETY: the only field spans the full payload and has been written.
            Ok(unsafe { self.slot.assume_init() })
        }
    }

    unsafe impl StablePayloadInit for StableBytes {
        type Init<'a> = StableBytesInit<'a>;

        fn init_from_uninit_bytes<'a>(
            payload: &'a mut [std::mem::MaybeUninit<u8>],
        ) -> Result<Self::Init<'a>, crate::UWireError> {
            Ok(StableBytesInit {
                slot: StablePayloadInitSlot::from_uninit_bytes(payload)?,
                written: false,
            })
        }

        fn __init_from_slot<'a>(
            slot: StablePayloadInitSlot<'a, Self>,
        ) -> Result<Self::Init<'a>, crate::UWireError> {
            Ok(StableBytesInit {
                slot,
                written: false,
            })
        }
    }

    fn uri_provider() -> Arc<StaticUriProvider> {
        Arc::new(StaticUriProvider::new("", 0x0005, 0x02).expect("uri provider"))
    }

    #[tokio::test]
    async fn publish_stable_sends_initialized_selected_wire_payload() {
        let transport = Arc::new(SelectedStableTransport::new());
        let publisher = Publisher::new(transport.clone(), uri_provider());

        publisher
            .publish_stable::<StableBytes>(
                0x9A00,
                CallOptions::for_publish(None, None, None),
                |payload| payload.bytes.copy_from_slice(b"init"),
            )
            .await
            .expect("stable publish succeeds");

        let frames = transport.sent_frames();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].payload_len(), std::mem::size_of::<StableBytes>());
        assert_eq!(
            frames[0].borrow_stable_payload::<StableBytes>().unwrap(),
            &StableBytes { bytes: *b"init" }
        );
    }

    #[tokio::test]
    async fn publish_uninit_stable_sends_no_zero_selected_wire_payload() {
        let transport = Arc::new(SelectedStableTransport::new());
        let publisher = Endpoint::new(transport.clone(), uri_provider()).publisher();

        publisher
            .publish_uninit_stable::<StableBytes>(
                0x9A00,
                CallOptions::for_publish(None, None, None),
                |init| init.bytes_from_array(b"zero").finish(),
            )
            .await
            .expect("uninit stable publish succeeds");

        let frames = transport.sent_frames();
        assert_eq!(frames.len(), 1);
        assert_eq!(
            frames[0].borrow_stable_payload::<StableBytes>().unwrap(),
            &StableBytes { bytes: *b"zero" }
        );
    }

    #[tokio::test]
    async fn publish_stable_rejects_invalid_topic() {
        let transport = Arc::new(SelectedStableTransport::new());
        let publisher = Publisher::new(transport.clone(), uri_provider());

        let result = publisher
            .publish_stable::<StableBytes>(
                0x1000,
                CallOptions::for_publish(None, None, None),
                |payload| payload.bytes.copy_from_slice(b"drop"),
            )
            .await;

        assert!(matches!(result, Err(PubSubError::InvalidArgument(_))));
        assert!(transport.sent_frames().is_empty());
    }

    struct FailingStableTransport {
        wire: StableContainerWireFormat,
    }

    impl UHasWire for FailingStableTransport {
        type Wire = StableContainerWireFormat;

        fn wire(&self) -> &Self::Wire {
            &self.wire
        }
    }

    #[async_trait]
    impl UZeroCopyTransportImpl for FailingStableTransport {
        type Tx = UVecTxBuffer;
        type Rx = UVecRxLease;

        async fn loan_validated_tx(&self, _spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus> {
            Err(UStatus::fail_with_code(
                UCode::Unavailable,
                "transport unavailable",
            ))
        }

        async fn send_validated_zero_copy(&self, _buffer: Self::Tx) -> Result<(), UStatus> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn publish_stable_maps_transport_error() {
        let publisher = Publisher::new(
            Arc::new(FailingStableTransport {
                wire: StableContainerWireFormat,
            }),
            uri_provider(),
        );

        let result = publisher
            .publish_stable::<StableBytes>(
                0x9A00,
                CallOptions::for_publish(None, None, None),
                |payload| payload.bytes.copy_from_slice(b"fail"),
            )
            .await;

        assert!(matches!(result, Err(PubSubError::PublishError(_))));
    }
}
