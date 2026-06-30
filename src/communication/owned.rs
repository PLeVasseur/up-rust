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

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    communication::{CallOptions, PubSubError, UPayload},
    LocalUriProvider, UMessageBuilder, UOwnedFrame, UOwnedTransport,
};
#[cfg(feature = "selected-wire-transport-adapter")]
use crate::{EncodePayload, PayloadCodec, UHasWire, UWireEncode};

/// Front door for owned native-frame communication-layer clients.
pub struct Endpoint<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    transport: Arc<T>,
    uri_provider: Arc<P>,
}

impl<T, P> Endpoint<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    /// Creates an owned native-frame communication endpoint.
    #[must_use]
    pub fn new(transport: Arc<T>, uri_provider: Arc<P>) -> Self {
        Self {
            transport,
            uri_provider,
        }
    }

    /// Creates a publisher for owned native-frame publish messages.
    #[must_use]
    pub fn publisher(&self) -> Publisher<T, P> {
        Publisher::new(self.transport.clone(), self.uri_provider.clone())
    }
}

/// Publisher implemented over an owned native-frame transport.
pub struct Publisher<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    transport: Arc<T>,
    uri_provider: Arc<P>,
}

impl<T, P> Publisher<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    /// Creates a publisher over an owned native-frame transport.
    #[must_use]
    pub fn new(transport: Arc<T>, uri_provider: Arc<P>) -> Self {
        Self {
            transport,
            uri_provider,
        }
    }

    fn build_frame(
        &self,
        resource_id: u16,
        call_options: CallOptions,
        payload: Option<UPayload>,
    ) -> Result<UOwnedFrame, PubSubError> {
        let mut builder = UMessageBuilder::publish(self.uri_provider.get_resource_uri(resource_id));
        builder.with_ttl(call_options.ttl());
        if let Some(message_id) = call_options.message_id() {
            builder.with_message_id(message_id.clone());
        }
        if let Some(priority) = call_options.priority() {
            builder.with_priority(priority);
        }
        let message = match payload {
            Some(payload) => {
                builder.build_with_payload(payload.payload().clone(), payload.payload_format())
            }
            None => builder.build(),
        }
        .map_err(|error| {
            PubSubError::InvalidArgument(format!(
                "failed to create Publish message from parameters: {error}"
            ))
        })?;
        let metadata =
            crate::try_project_umessage_to_frame_metadata(&message).map_err(|error| {
                PubSubError::InvalidArgument(format!(
                    "failed to create owned Publish frame metadata from parameters: {error}"
                ))
            })?;
        UOwnedFrame::new(metadata, message.payload()).map_err(|error| {
            PubSubError::InvalidArgument(format!(
                "failed to create owned Publish frame from parameters: {error}"
            ))
        })
    }

    /// Publishes an owned native-frame message.
    pub async fn publish(
        &self,
        resource_id: u16,
        call_options: CallOptions,
        payload: Option<UPayload>,
    ) -> Result<(), PubSubError> {
        let frame = self.build_frame(resource_id, call_options, payload)?;
        self.transport
            .send_owned(frame)
            .await
            .map_err(Box::from)
            .map_err(PubSubError::PublishError)
    }
}

#[cfg(feature = "selected-wire-transport-adapter")]
impl<T, P> Publisher<T, P>
where
    T: UOwnedTransport + UHasWire + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    /// Publishes a typed payload using the transport endpoint's selected wire.
    pub async fn publish_typed<Payload>(
        &self,
        resource_id: u16,
        call_options: CallOptions,
        payload: &Payload,
    ) -> Result<(), PubSubError>
    where
        T::Wire: UWireEncode<Payload>,
    {
        let payload_bytes = <T::Wire as EncodePayload<Payload>>::encode_payload_owned(payload)
            .map_err(|error| PubSubError::InvalidArgument(error.to_string()))?;
        let payload_format = <T::Wire as PayloadCodec>::payload_encoding()
            .standard_format()
            .ok_or_else(|| {
                PubSubError::InvalidArgument(
                    "selected wire uses a native-only payload encoding that cannot be sent by P73U1 owned publish"
                        .to_string(),
                )
            })?;
        self.publish(
            resource_id,
            call_options,
            Some(UPayload::new(payload_bytes, payload_format)),
        )
        .await
    }
}

#[async_trait]
impl<T, P> crate::communication::Publisher for Publisher<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    async fn publish(
        &self,
        resource_id: u16,
        call_options: CallOptions,
        payload: Option<UPayload>,
    ) -> Result<(), PubSubError> {
        Publisher::publish(self, resource_id, call_options, payload).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{
        StaticUriProvider, UCode, UOwnedTransportImpl, UPayloadFormat, UStatus, ValidatedOwnedFrame,
    };

    struct RecordingOwnedTransport {
        sent: Mutex<Vec<UOwnedFrame>>,
    }

    impl RecordingOwnedTransport {
        fn new() -> Self {
            Self {
                sent: Mutex::new(Vec::new()),
            }
        }

        fn sent_frames(&self) -> Vec<UOwnedFrame> {
            self.sent.lock().expect("sent lock poisoned").clone()
        }
    }

    #[async_trait]
    impl UOwnedTransportImpl for RecordingOwnedTransport {
        async fn send_validated_owned(&self, frame: ValidatedOwnedFrame) -> Result<(), UStatus> {
            self.sent
                .lock()
                .expect("sent lock poisoned")
                .push(frame.into_inner());
            Ok(())
        }
    }

    fn uri_provider() -> Arc<StaticUriProvider> {
        Arc::new(StaticUriProvider::new("", 0x0005, 0x02).expect("uri provider"))
    }

    #[tokio::test]
    async fn publish_sends_owned_frame() {
        let transport = Arc::new(RecordingOwnedTransport::new());
        let publisher = Publisher::new(transport.clone(), uri_provider());

        publisher
            .publish(
                0x9A00,
                CallOptions::for_publish(None, None, None),
                Some(UPayload::new("hello", UPayloadFormat::Text)),
            )
            .await
            .expect("publish succeeds");

        let frames = transport.sent_frames();
        assert_eq!(frames.len(), 1);
        assert_eq!(
            frames[0].payload(),
            Some(&bytes::Bytes::from_static(b"hello"))
        );
        assert_eq!(
            frames[0].metadata().attributes().payload_format(),
            Some(UPayloadFormat::Text)
        );
    }

    #[tokio::test]
    async fn publish_rejects_invalid_topic() {
        let transport = Arc::new(RecordingOwnedTransport::new());
        let publisher = Publisher::new(transport.clone(), uri_provider());

        let result = publisher
            .publish(0x1000, CallOptions::for_publish(None, None, None), None)
            .await;

        assert!(matches!(result, Err(PubSubError::InvalidArgument(_))));
        assert!(transport.sent_frames().is_empty());
    }

    struct FailingOwnedTransport;

    #[async_trait]
    impl UOwnedTransportImpl for FailingOwnedTransport {
        async fn send_validated_owned(&self, _frame: ValidatedOwnedFrame) -> Result<(), UStatus> {
            Err(UStatus::fail_with_code(
                UCode::Unavailable,
                "transport unavailable",
            ))
        }
    }

    #[tokio::test]
    async fn publish_maps_transport_error() {
        let publisher = Publisher::new(Arc::new(FailingOwnedTransport), uri_provider());

        let result = publisher
            .publish(0x9A00, CallOptions::for_publish(None, None, None), None)
            .await;

        assert!(matches!(result, Err(PubSubError::PublishError(_))));
    }
}

#[cfg(all(test, feature = "protobuf-support"))]
mod selected_wire_tests {
    use std::sync::Mutex;

    use protobuf::well_known_types::wrappers::StringValue;

    use super::*;
    use crate::{
        NativePrefixProtobufMetadataCodec, ProtobufWire, UHasWire, UOwnedTransportCore, UStatus,
        UWithNativePrefixProtobufMetadata,
    };

    #[derive(Clone, Default)]
    struct RecordingOwnedCore {
        sent: Arc<Mutex<Vec<crate::PreparedOwnedFrame>>>,
    }

    impl RecordingOwnedCore {
        fn sent_payloads(&self) -> Vec<Option<bytes::Bytes>> {
            self.sent
                .lock()
                .expect("sent lock poisoned")
                .iter()
                .map(|frame| frame.payload().cloned())
                .collect()
        }
    }

    #[async_trait]
    impl UOwnedTransportCore for RecordingOwnedCore {
        async fn send_prepared_owned(
            &self,
            frame: crate::PreparedOwnedFrame,
        ) -> Result<(), UStatus> {
            self.sent.lock().expect("sent lock poisoned").push(frame);
            Ok(())
        }
    }

    fn uri_provider() -> Arc<crate::StaticUriProvider> {
        Arc::new(crate::StaticUriProvider::new("", 0x0005, 0x02).expect("uri provider"))
    }

    #[tokio::test]
    async fn publish_typed_uses_selected_wire() {
        let core = RecordingOwnedCore::default();
        let transport = Arc::new(
            core.clone()
                .with_native_prefix_protobuf_metadata(ProtobufWire),
        );
        assert_eq!(transport.wire(), &ProtobufWire);
        let publisher = Publisher::new(transport, uri_provider());
        let payload = StringValue {
            value: "typed".to_string(),
            ..Default::default()
        };

        publisher
            .publish_typed(0x9A00, CallOptions::for_publish(None, None, None), &payload)
            .await
            .expect("typed publish succeeds");

        let payloads = core.sent_payloads();
        assert_eq!(payloads.len(), 1);
        assert!(payloads[0]
            .as_ref()
            .is_some_and(|payload| !payload.is_empty()));
        let _: NativePrefixProtobufMetadataCodec = NativePrefixProtobufMetadataCodec;
    }
}
