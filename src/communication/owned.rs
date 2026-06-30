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
    communication::{
        CallOptions, PubSubError, RegistrationError, ServiceInvocationError, UPayload,
    },
    LocalUriProvider, UCode, UMessage, UMessageBuilder, UOwnedFrame, UOwnedListener,
    UOwnedTransport, UStatus, UUri, UUID,
};
#[cfg(feature = "selected-wire-transport-adapter")]
use crate::{DecodePayload, EncodePayload, PayloadCodec, UHasWire, UWireDecodeOwned, UWireEncode};

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

    /// Creates an RPC client for owned native-frame request/response messages.
    #[must_use]
    pub fn rpc_client(&self) -> RpcClient<T, P> {
        RpcClient::new(self.transport.clone(), self.uri_provider.clone())
    }
}

impl<T, P> Endpoint<T, P>
where
    T: UOwnedTransport + ?Sized + 'static,
    P: LocalUriProvider + ?Sized,
{
    /// Creates an RPC server for owned native-frame request/response messages.
    #[must_use]
    pub fn rpc_server(&self) -> RpcServer<T, P> {
        RpcServer::new(self.transport.clone(), self.uri_provider.clone())
    }
}

fn build_message_with_payload(
    builder: &mut UMessageBuilder,
    payload: Option<UPayload>,
) -> Result<UMessage, crate::UMessageError> {
    match payload {
        Some(payload) => {
            builder.build_with_payload(payload.payload().clone(), payload.payload_format())
        }
        None => builder.build(),
    }
}

fn frame_from_message(message: &UMessage) -> Result<UOwnedFrame, crate::UFrameMetadataError> {
    let metadata = crate::try_project_umessage_to_frame_metadata(message)?;
    UOwnedFrame::new(metadata, message.payload())
}

/// RPC client implemented over an owned native-frame transport.
pub struct RpcClient<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    transport: Arc<T>,
    uri_provider: Arc<P>,
}

impl<T, P> RpcClient<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    /// Creates an RPC client over an owned native-frame transport.
    #[must_use]
    pub fn new(transport: Arc<T>, uri_provider: Arc<P>) -> Self {
        Self {
            transport,
            uri_provider,
        }
    }

    fn build_request_frame(
        &self,
        method: UUri,
        call_options: &CallOptions,
        message_id: UUID,
        payload: Option<UPayload>,
    ) -> Result<UOwnedFrame, ServiceInvocationError> {
        let mut builder = UMessageBuilder::request(
            method,
            self.uri_provider.get_source_uri(),
            call_options.ttl(),
        );
        builder.with_message_id(message_id);
        if let Some(token) = call_options.token() {
            builder.with_token(token.clone());
        }
        if let Some(priority) = call_options.priority() {
            builder.with_priority(priority);
        }
        let message = build_message_with_payload(&mut builder, payload)
            .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))?;
        frame_from_message(&message)
            .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))
    }

    fn response_payload(response: UMessage) -> Result<Option<UPayload>, ServiceInvocationError> {
        match response.commstatus() {
            Some(UCode::Ok) | None => {
                let payload_format = response
                    .payload_format()
                    .unwrap_or(crate::UPayloadFormat::Unspecified);
                Ok(response
                    .payload()
                    .map(|payload| UPayload::new(payload, payload_format)))
            }
            Some(code) => {
                let status = response.extract_protobuf().unwrap_or_else(|_| {
                    UStatus::fail_with_code(code, "failed to invoke service operation")
                });
                Err(ServiceInvocationError::from(status))
            }
        }
    }

    /// Invokes an RPC method using owned native frames.
    pub async fn invoke_method(
        &self,
        method: UUri,
        call_options: CallOptions,
        payload: Option<UPayload>,
    ) -> Result<Option<UPayload>, ServiceInvocationError> {
        let message_id = call_options
            .message_id()
            .map_or_else(UUID::build, Clone::clone);
        let request_frame =
            self.build_request_frame(method.clone(), &call_options, message_id.clone(), payload)?;
        self.transport.send_owned(request_frame).await?;
        let response_frame = self
            .transport
            .receive_owned(&method, Some(&self.uri_provider.get_source_uri()))
            .await?;
        let response = crate::try_project_frame_to_umessage(
            response_frame.metadata().clone(),
            response_frame.payload().cloned(),
        )
        .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))?;
        if response.request_id_unchecked() != &message_id {
            return Err(ServiceInvocationError::Internal(
                "received RPC response for unexpected request ID".to_string(),
            ));
        }
        Self::response_payload(response)
    }
}

#[cfg(feature = "selected-wire-transport-adapter")]
impl<T, P> RpcClient<T, P>
where
    T: UOwnedTransport + UHasWire + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    /// Invokes an RPC method using the transport endpoint's selected wire.
    pub async fn invoke_typed<Request, Response>(
        &self,
        method: UUri,
        call_options: CallOptions,
        request: &Request,
    ) -> Result<Response, ServiceInvocationError>
    where
        T::Wire: UWireEncode<Request> + UWireDecodeOwned<Response>,
    {
        let payload_bytes = <T::Wire as EncodePayload<Request>>::encode_payload_owned(request)
            .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))?;
        let payload_format = <T::Wire as PayloadCodec>::payload_encoding()
            .standard_format()
            .ok_or_else(|| {
                ServiceInvocationError::InvalidArgument(
                    "selected wire uses a native-only payload encoding that cannot be sent by P73U2 owned RPC"
                        .to_string(),
                )
            })?;
        let response = self
            .invoke_method(
                method,
                call_options,
                Some(UPayload::new(payload_bytes, payload_format)),
            )
            .await?
            .ok_or_else(|| ServiceInvocationError::InvalidArgument("No payload".to_string()))?;
        <T::Wire as DecodePayload<'_, Response>>::decode_payload(response.payload())
            .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))
    }
}

#[async_trait]
impl<T, P> crate::communication::RpcClient for RpcClient<T, P>
where
    T: UOwnedTransport + ?Sized + 'static,
    P: LocalUriProvider + ?Sized,
{
    async fn invoke_method(
        &self,
        method: UUri,
        call_options: CallOptions,
        payload: Option<UPayload>,
    ) -> Result<Option<UPayload>, ServiceInvocationError> {
        RpcClient::invoke_method(self, method, call_options, payload).await
    }
}

struct RequestListener<T>
where
    T: UOwnedTransport + ?Sized + 'static,
{
    request_handler: Arc<dyn crate::communication::RequestHandler>,
    transport: Arc<T>,
}

impl<T> RequestListener<T>
where
    T: UOwnedTransport + ?Sized + 'static,
{
    async fn process_request(&self, request_frame: UOwnedFrame) {
        let Ok(request_message) = crate::try_project_frame_to_umessage(
            request_frame.metadata().clone(),
            request_frame.payload().cloned(),
        ) else {
            return;
        };
        if !request_message.is_request() {
            return;
        }
        let resource_id = request_message.sink_unchecked().resource_id();
        let payload_format = request_message
            .payload_format()
            .unwrap_or(crate::UPayloadFormat::Unspecified);
        let request_payload = request_message
            .payload()
            .map(|payload| UPayload::new(payload, payload_format));
        let mut response_builder =
            UMessageBuilder::response_for_request(request_message.attributes());
        let response = match self
            .request_handler
            .handle_request(resource_id, request_message.attributes(), request_payload)
            .await
        {
            Ok(response_payload) => {
                build_message_with_payload(&mut response_builder, response_payload)
            }
            Err(error) => {
                let status = UStatus::from(error);
                response_builder
                    .with_comm_status(status.get_code())
                    .build_with_protobuf_payload(&status)
            }
        };
        if let Ok(response_message) = response {
            if let Ok(response_frame) = frame_from_message(&response_message) {
                let _ = self.transport.send_owned(response_frame).await;
            }
        }
    }
}

#[async_trait]
impl<T> UOwnedListener for RequestListener<T>
where
    T: UOwnedTransport + ?Sized + 'static,
{
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        self.process_request(frame).await;
    }
}

/// RPC server implemented over an owned native-frame transport.
pub struct RpcServer<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    transport: Arc<T>,
    uri_provider: Arc<P>,
    request_listeners: tokio::sync::Mutex<std::collections::HashMap<u16, Arc<dyn UOwnedListener>>>,
}

impl<T, P> RpcServer<T, P>
where
    T: UOwnedTransport + ?Sized + 'static,
    P: LocalUriProvider + ?Sized,
{
    /// Creates an RPC server over an owned native-frame transport.
    #[must_use]
    pub fn new(transport: Arc<T>, uri_provider: Arc<P>) -> Self {
        Self {
            transport,
            uri_provider,
            request_listeners: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn validate_sink_filter(filter: &UUri) -> Result<(), RegistrationError> {
        if !filter.is_rpc_method() {
            return Err(RegistrationError::InvalidFilter(
                "RPC endpoint's resource ID must be in range [0x0001, 0x7FFF]".to_string(),
            ));
        }
        Ok(())
    }

    fn validate_origin_filter(filter: Option<&UUri>) -> Result<(), RegistrationError> {
        if let Some(uri) = filter {
            if !uri.is_rpc_response() {
                return Err(RegistrationError::InvalidFilter(
                    "origin filter's resource ID must be 0".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Registers an owned RPC endpoint.
    pub async fn register_endpoint(
        &self,
        origin_filter: Option<&UUri>,
        resource_id: u16,
        request_handler: Arc<dyn crate::communication::RequestHandler>,
    ) -> Result<(), RegistrationError> {
        Self::validate_origin_filter(origin_filter)?;
        let sink_filter = self.uri_provider.get_resource_uri(resource_id);
        Self::validate_sink_filter(&sink_filter)?;
        let mut listener_map = self.request_listeners.lock().await;
        if listener_map.contains_key(&resource_id) {
            return Err(RegistrationError::MaxListenersExceeded);
        }
        let listener: Arc<dyn UOwnedListener> = Arc::new(RequestListener {
            request_handler,
            transport: self.transport.clone(),
        });
        self.transport
            .register_owned_listener(
                origin_filter.unwrap_or(&UUri::any_with_resource_id(
                    crate::uri::RESOURCE_ID_RESPONSE,
                )),
                Some(&sink_filter),
                listener.clone(),
            )
            .await
            .map_err(RegistrationError::from)?;
        listener_map.insert(resource_id, listener);
        Ok(())
    }

    /// Unregisters an owned RPC endpoint.
    pub async fn unregister_endpoint(
        &self,
        origin_filter: Option<&UUri>,
        resource_id: u16,
        _request_handler: Arc<dyn crate::communication::RequestHandler>,
    ) -> Result<(), RegistrationError> {
        Self::validate_origin_filter(origin_filter)?;
        let sink_filter = self.uri_provider.get_resource_uri(resource_id);
        Self::validate_sink_filter(&sink_filter)?;
        let mut listener_map = self.request_listeners.lock().await;
        let Some(listener) = listener_map.remove(&resource_id) else {
            return Err(RegistrationError::NoSuchListener);
        };
        self.transport
            .unregister_owned_listener(
                origin_filter.unwrap_or(&UUri::any_with_resource_id(
                    crate::uri::RESOURCE_ID_RESPONSE,
                )),
                Some(&sink_filter),
                listener,
            )
            .await
            .map_err(RegistrationError::from)
    }
}

#[async_trait]
impl<T, P> crate::communication::RpcServer for RpcServer<T, P>
where
    T: UOwnedTransport + ?Sized + 'static,
    P: LocalUriProvider + ?Sized,
{
    async fn register_endpoint(
        &self,
        origin_filter: Option<&UUri>,
        resource_id: u16,
        request_handler: Arc<dyn crate::communication::RequestHandler>,
    ) -> Result<(), RegistrationError> {
        RpcServer::register_endpoint(self, origin_filter, resource_id, request_handler).await
    }

    async fn unregister_endpoint(
        &self,
        origin_filter: Option<&UUri>,
        resource_id: u16,
        request_handler: Arc<dyn crate::communication::RequestHandler>,
    ) -> Result<(), RegistrationError> {
        RpcServer::unregister_endpoint(self, origin_filter, resource_id, request_handler).await
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
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;
    use crate::{
        StaticUriProvider, UAttributes, UCode, UOwnedTransportImpl, UPayloadFormat, UStatus,
        ValidatedOwnedFrame,
    };

    struct RecordingOwnedTransport {
        sent: Mutex<Vec<UOwnedFrame>>,
        received: Mutex<VecDeque<UOwnedFrame>>,
        listener: Mutex<Option<Arc<dyn UOwnedListener>>>,
    }

    impl RecordingOwnedTransport {
        fn new() -> Self {
            Self {
                sent: Mutex::new(Vec::new()),
                received: Mutex::new(VecDeque::new()),
                listener: Mutex::new(None),
            }
        }

        fn sent_frames(&self) -> Vec<UOwnedFrame> {
            self.sent.lock().expect("sent lock poisoned").clone()
        }

        fn push_received(&self, frame: UOwnedFrame) {
            self.received
                .lock()
                .expect("received lock poisoned")
                .push_back(frame);
        }

        fn registered_listener(&self) -> Arc<dyn UOwnedListener> {
            self.listener
                .lock()
                .expect("listener lock poisoned")
                .as_ref()
                .expect("listener registered")
                .clone()
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

        async fn receive_validated_owned(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
        ) -> Result<UOwnedFrame, UStatus> {
            self.received
                .lock()
                .expect("received lock poisoned")
                .pop_front()
                .ok_or_else(|| UStatus::fail_with_code(UCode::NotFound, "no queued frame"))
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
            _listener: Arc<dyn UOwnedListener>,
        ) -> Result<(), UStatus> {
            *self.listener.lock().expect("listener lock poisoned") = None;
            Ok(())
        }
    }

    fn uri_provider() -> Arc<StaticUriProvider> {
        Arc::new(StaticUriProvider::new("", 0x0005, 0x02).expect("uri provider"))
    }

    fn method_uri() -> UUri {
        uri_provider().get_resource_uri(0x1000)
    }

    fn message_frame(message: &UMessage) -> UOwnedFrame {
        frame_from_message(message).expect("frame")
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

    #[tokio::test]
    async fn rpc_client_sends_request_and_returns_response_payload() {
        let transport = Arc::new(RecordingOwnedTransport::new());
        let client = RpcClient::new(transport.clone(), uri_provider());
        let request_id = UUID::build();
        let request =
            UMessageBuilder::request(method_uri(), uri_provider().get_source_uri(), 5_000)
                .with_message_id(request_id.clone())
                .build_with_payload("request", UPayloadFormat::Text)
                .expect("request");
        let response = UMessageBuilder::response_for_request(request.attributes())
            .build_with_payload("response", UPayloadFormat::Text)
            .expect("response");
        transport.push_received(message_frame(&response));

        let result = client
            .invoke_method(
                method_uri(),
                CallOptions::for_rpc_request(5_000, Some(request_id), None, None),
                Some(UPayload::new("request", UPayloadFormat::Text)),
            )
            .await
            .expect("rpc succeeds")
            .expect("response payload");

        assert_eq!(result.payload(), &bytes::Bytes::from_static(b"response"));
        let frames = transport.sent_frames();
        assert_eq!(frames.len(), 1);
        let sent = crate::try_project_frame_to_umessage(
            frames[0].metadata().clone(),
            frames[0].payload().cloned(),
        )
        .expect("sent request");
        assert!(sent.is_request());
        assert_eq!(sent.payload(), Some(bytes::Bytes::from_static(b"request")));
    }

    struct EchoHandler;

    #[async_trait]
    impl crate::communication::RequestHandler for EchoHandler {
        async fn handle_request(
            &self,
            resource_id: u16,
            message_attributes: &UAttributes,
            request_payload: Option<UPayload>,
        ) -> Result<Option<UPayload>, ServiceInvocationError> {
            assert_eq!(resource_id, 0x1000);
            assert!(message_attributes.is_request());
            Ok(request_payload)
        }
    }

    #[tokio::test]
    async fn rpc_server_registers_listener_and_sends_response() {
        let transport = Arc::new(RecordingOwnedTransport::new());
        let server = RpcServer::new(transport.clone(), uri_provider());
        server
            .register_endpoint(None, 0x1000, Arc::new(EchoHandler))
            .await
            .expect("endpoint registered");
        let request =
            UMessageBuilder::request(method_uri(), uri_provider().get_source_uri(), 5_000)
                .build_with_payload("server request", UPayloadFormat::Text)
                .expect("request");

        transport
            .registered_listener()
            .on_receive_owned(message_frame(&request))
            .await;

        let frames = transport.sent_frames();
        assert_eq!(frames.len(), 1);
        let response = crate::try_project_frame_to_umessage(
            frames[0].metadata().clone(),
            frames[0].payload().cloned(),
        )
        .expect("response");
        assert!(response.is_response());
        assert_eq!(
            response.payload(),
            Some(bytes::Bytes::from_static(b"server request"))
        );
        assert_eq!(response.request_id_unchecked(), request.id());
    }
}

#[cfg(all(test, feature = "protobuf-support"))]
mod selected_wire_tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use protobuf::well_known_types::wrappers::StringValue;

    use super::*;
    use crate::{
        NativePrefixProtobufMetadataCodec, ProtobufWire, UHasWire, UOwnedTransportCore,
        UOwnedTransportImpl, UStatus, UWithNativePrefixProtobufMetadata, ValidatedOwnedFrame,
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

    struct DirectSelectedOwnedTransport {
        wire: ProtobufWire,
        sent: Mutex<Vec<UOwnedFrame>>,
        received: Mutex<VecDeque<UOwnedFrame>>,
    }

    impl DirectSelectedOwnedTransport {
        fn new() -> Self {
            Self {
                wire: ProtobufWire,
                sent: Mutex::new(Vec::new()),
                received: Mutex::new(VecDeque::new()),
            }
        }

        fn sent_frames(&self) -> Vec<UOwnedFrame> {
            self.sent.lock().expect("sent lock poisoned").clone()
        }

        fn push_received(&self, frame: UOwnedFrame) {
            self.received
                .lock()
                .expect("received lock poisoned")
                .push_back(frame);
        }
    }

    impl UHasWire for DirectSelectedOwnedTransport {
        type Wire = ProtobufWire;

        fn wire(&self) -> &Self::Wire {
            &self.wire
        }
    }

    #[async_trait]
    impl UOwnedTransportImpl for DirectSelectedOwnedTransport {
        async fn send_validated_owned(&self, frame: ValidatedOwnedFrame) -> Result<(), UStatus> {
            self.sent
                .lock()
                .expect("sent lock poisoned")
                .push(frame.into_inner());
            Ok(())
        }

        async fn receive_validated_owned(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
        ) -> Result<UOwnedFrame, UStatus> {
            self.received
                .lock()
                .expect("received lock poisoned")
                .pop_front()
                .ok_or_else(|| crate::UStatus::fail_with_code(crate::UCode::NotFound, "no frame"))
        }
    }

    #[tokio::test]
    async fn invoke_typed_uses_selected_wire() {
        let transport = Arc::new(DirectSelectedOwnedTransport::new());
        assert_eq!(transport.wire(), &ProtobufWire);
        let client = RpcClient::new(transport.clone(), uri_provider());
        let method = uri_provider().get_resource_uri(0x1000);
        let request_id = crate::UUID::build();
        let request =
            UMessageBuilder::request(method.clone(), uri_provider().get_source_uri(), 5_000)
                .with_message_id(request_id.clone())
                .build()
                .expect("request");
        let response_value = StringValue {
            value: "typed response".to_string(),
            ..Default::default()
        };
        let response_bytes =
            <ProtobufWire as EncodePayload<StringValue>>::encode_payload_owned(&response_value)
                .expect("encoded response");
        let response_format = <ProtobufWire as PayloadCodec>::payload_encoding()
            .standard_format()
            .expect("standard format");
        let response = UMessageBuilder::response_for_request(request.attributes())
            .build_with_payload(response_bytes, response_format)
            .expect("response");
        transport.push_received(frame_from_message(&response).expect("response frame"));
        let request_value = StringValue {
            value: "typed request".to_string(),
            ..Default::default()
        };

        let result: StringValue = client
            .invoke_typed(
                method,
                CallOptions::for_rpc_request(5_000, Some(request_id), None, None),
                &request_value,
            )
            .await
            .expect("typed rpc succeeds");

        assert_eq!(result.value, "typed response");
        let frames = transport.sent_frames();
        assert_eq!(frames.len(), 1);
        assert!(frames[0]
            .payload()
            .is_some_and(|payload| !payload.is_empty()));
    }
}
