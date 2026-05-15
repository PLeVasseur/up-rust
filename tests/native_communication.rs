#![cfg(feature = "util")]

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

use std::sync::{Arc, Mutex};

#[cfg(feature = "protobuf-wire")]
use protobuf::well_known_types::wrappers::StringValue;
use tokio::sync::oneshot;
#[cfg(feature = "protobuf-wire")]
use up_rust::ProtobufWire;
use up_rust::{
    communication::{
        CallOptions, InMemoryRpcClient, InMemoryRpcServer, Notifier, Publisher, RequestHandler,
        RpcClient, RpcClientExt, RpcServer, ServiceInvocationError, SimpleNotifier,
        SimplePublisher, UPayload,
    },
    local_transport::LocalTransport,
    LocalUriProvider, RawBytes, StaticUriProvider, UAttributes, UDeserializer, UEncoding,
    UMessageBuilder, UMessageType, UOwnedFrame, UOwnedListener, UOwnedTransport, UPriority,
    USerializer, UWireError, WireFormat, UUID,
};

#[derive(Debug, Eq, PartialEq)]
struct Reading(u16);

struct ReadingWire;

impl WireFormat for ReadingWire {
    fn name() -> &'static str {
        "reading"
    }

    fn encoding() -> UEncoding {
        UEncoding::new(
            "reading",
            "application/x-up-rust-test-reading",
            None::<String>,
        )
    }
}

impl USerializer<ReadingWire> for Reading {
    fn encoded_len(&self) -> usize {
        2
    }

    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        let actual = dst.len();
        let out = dst
            .get_mut(..2)
            .ok_or_else(|| UWireError::buffer_too_small(2, actual))?;
        out.copy_from_slice(&self.0.to_be_bytes());
        Ok(2)
    }
}

impl<'a> UDeserializer<'a, ReadingWire> for Reading {
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
        let bytes: [u8; 2] = src
            .try_into()
            .map_err(|_| UWireError::invalid_payload("reading payload must be two bytes"))?;
        Ok(Self(u16::from_be_bytes(bytes)))
    }
}

struct CaptureListener {
    sender: Mutex<Option<oneshot::Sender<UOwnedFrame>>>,
}

impl CaptureListener {
    fn new(sender: oneshot::Sender<UOwnedFrame>) -> Self {
        Self {
            sender: Mutex::new(Some(sender)),
        }
    }
}

#[async_trait::async_trait]
impl UOwnedListener for CaptureListener {
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        if let Some(sender) = self.sender.lock().unwrap().take() {
            let _ = sender.send(frame);
        }
    }
}

struct EchoRequestHandler;

#[async_trait::async_trait]
impl RequestHandler for EchoRequestHandler {
    async fn handle_request(
        &self,
        resource_id: u16,
        attributes: &UAttributes,
        request_payload: Option<UPayload>,
    ) -> Result<Option<UPayload>, ServiceInvocationError> {
        assert_eq!(resource_id, 0x0001);
        assert_eq!(attributes.message_type(), UMessageType::Request);
        assert_eq!(attributes.token(), Some("rpc-token"));
        Ok(request_payload)
    }
}

struct FailingRequestHandler;

#[async_trait::async_trait]
impl RequestHandler for FailingRequestHandler {
    async fn handle_request(
        &self,
        _resource_id: u16,
        _attributes: &UAttributes,
        _request_payload: Option<UPayload>,
    ) -> Result<Option<UPayload>, ServiceInvocationError> {
        Err(ServiceInvocationError::NotFound(
            "missing resource".to_string(),
        ))
    }
}

struct IncrementReadingHandler;

#[async_trait::async_trait]
impl RequestHandler for IncrementReadingHandler {
    async fn handle_request(
        &self,
        _resource_id: u16,
        _attributes: &UAttributes,
        request_payload: Option<UPayload>,
    ) -> Result<Option<UPayload>, ServiceInvocationError> {
        let request_payload = request_payload.ok_or_else(|| {
            ServiceInvocationError::InvalidArgument("missing reading payload".to_string())
        })?;
        let reading: Reading = request_payload
            .deserialize::<ReadingWire, _>()
            .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))?;
        let response = Reading(reading.0 + 1);
        UPayload::from_serializable::<ReadingWire, _>(&response)
            .map(Some)
            .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))
    }
}

#[cfg(feature = "protobuf-wire")]
struct ProtobufGreetingHandler;

#[cfg(feature = "protobuf-wire")]
#[async_trait::async_trait]
impl RequestHandler for ProtobufGreetingHandler {
    async fn handle_request(
        &self,
        _resource_id: u16,
        _attributes: &UAttributes,
        request_payload: Option<UPayload>,
    ) -> Result<Option<UPayload>, ServiceInvocationError> {
        let request_payload = request_payload.ok_or_else(|| {
            ServiceInvocationError::InvalidArgument("missing protobuf payload".to_string())
        })?;
        let request: StringValue = request_payload
            .deserialize::<ProtobufWire, _>()
            .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))?;
        let mut response = StringValue::new();
        response.value = format!("hello, {}", request.value);
        UPayload::from_serializable::<ProtobufWire, _>(&response)
            .map(Some)
            .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))
    }
}

#[tokio::test]
async fn simple_publisher_sends_native_owned_frame() {
    let transport = Arc::new(LocalTransport::default());
    let uri_provider = Arc::new(StaticUriProvider::new("", 0x0005, 0x02));
    let topic = uri_provider.get_resource_uri(0x9001);
    let (sender, receiver) = oneshot::channel();
    transport
        .register_owned_listener(&topic, None, Arc::new(CaptureListener::new(sender)))
        .await
        .unwrap();

    let message_id = UUID::build();
    let publisher = SimplePublisher::new(transport, uri_provider);
    publisher
        .publish(
            0x9001,
            CallOptions::for_publish(Some(5_000), Some(message_id.clone()), Some(UPriority::CS3)),
            Some(UPayload::from_raw(vec![0x01, 0x02, 0x03])),
        )
        .await
        .unwrap();

    let frame = receiver.await.unwrap();
    let attributes = frame.metadata().attributes();
    assert_eq!(attributes.message_type(), UMessageType::Publish);
    assert_eq!(attributes.id(), &message_id);
    assert_eq!(attributes.source(), &topic);
    assert_eq!(attributes.sink(), None);
    assert_eq!(attributes.ttl(), Some(5_000));
    assert_eq!(attributes.priority(), UPriority::CS3);
    assert_eq!(frame.metadata().encoding(), &RawBytes::encoding());
    assert_eq!(frame.payload_bytes(), &[0x01, 0x02, 0x03]);
}

#[tokio::test]
async fn simple_notifier_sends_to_registered_notification_listener() {
    let transport = Arc::new(LocalTransport::default());
    let uri_provider = Arc::new(StaticUriProvider::new("", 0x0005, 0x02));
    let topic = uri_provider.get_resource_uri(0x9002);
    let destination = uri_provider.get_source_uri();
    let (sender, receiver) = oneshot::channel();
    let notifier = SimpleNotifier::new(transport, uri_provider);
    notifier
        .start_listening(&topic, Arc::new(CaptureListener::new(sender)))
        .await
        .unwrap();

    notifier
        .notify(
            0x9002,
            &destination,
            CallOptions::for_notification(None, None, Some(UPriority::CS2)),
            Some(UPayload::from_raw(vec![0x0a])),
        )
        .await
        .unwrap();

    let frame = receiver.await.unwrap();
    let attributes = frame.metadata().attributes();
    assert_eq!(attributes.message_type(), UMessageType::Notification);
    assert_eq!(attributes.source(), &topic);
    assert_eq!(attributes.sink(), Some(&destination));
    assert_eq!(attributes.priority(), UPriority::CS2);
    assert_eq!(frame.payload_bytes(), &[0x0a]);
}

#[tokio::test]
async fn in_memory_rpc_round_trips_native_payload() {
    let transport = Arc::new(LocalTransport::default());
    let uri_provider = Arc::new(StaticUriProvider::new("", 0x0005, 0x02));
    let server = InMemoryRpcServer::new(transport.clone(), uri_provider.clone());
    let client = InMemoryRpcClient::new(transport, uri_provider.clone());
    let handler = Arc::new(EchoRequestHandler);

    server
        .register_endpoint(None, 0x0001, handler.clone())
        .await
        .unwrap();

    let response = client
        .invoke_method(
            uri_provider.get_resource_uri(0x0001),
            CallOptions::for_rpc_request(
                5_000,
                Some(UUID::build()),
                Some("rpc-token".to_string()),
                Some(UPriority::CS5),
            ),
            Some(UPayload::from_raw(vec![0x55, 0xaa])),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(response.encoding(), &RawBytes::encoding());
    assert_eq!(response.payload_bytes(), &[0x55, 0xaa]);

    server
        .unregister_endpoint(None, 0x0001, handler)
        .await
        .unwrap();
}

#[tokio::test]
async fn rpc_server_response_preserves_request_metadata() {
    let transport = Arc::new(LocalTransport::default());
    let uri_provider = Arc::new(StaticUriProvider::new("", 0x0005, 0x02));
    let server = InMemoryRpcServer::new(transport.clone(), uri_provider.clone());
    let handler = Arc::new(EchoRequestHandler);

    server
        .register_endpoint(None, 0x0001, handler.clone())
        .await
        .unwrap();

    let method = uri_provider.get_resource_uri(0x0001);
    let reply_to = uri_provider.get_source_uri();
    let (sender, receiver) = oneshot::channel();
    transport
        .register_owned_listener(
            &method,
            Some(&reply_to),
            Arc::new(CaptureListener::new(sender)),
        )
        .await
        .unwrap();

    let request_id = UUID::build();
    let request = UMessageBuilder::request(method.clone(), reply_to.clone(), 7_000)
        .with_message_id(request_id.clone())
        .with_priority(UPriority::CS5)
        .with_token("rpc-token")
        .build_with_raw_payload(vec![0x12, 0x34])
        .unwrap();
    transport.send_owned(request).await.unwrap();

    let response = receiver.await.unwrap();
    let attributes = response.metadata().attributes();
    assert_eq!(attributes.message_type(), UMessageType::Response);
    assert_eq!(attributes.request_id(), Some(&request_id));
    assert_eq!(attributes.source(), &method);
    assert_eq!(attributes.sink(), Some(&reply_to));
    assert_eq!(attributes.priority(), UPriority::CS5);
    assert_eq!(attributes.ttl(), Some(7_000));
    assert_eq!(attributes.commstatus(), None);
    assert_eq!(response.payload_bytes(), &[0x12, 0x34]);

    server
        .unregister_endpoint(None, 0x0001, handler)
        .await
        .unwrap();
}

#[tokio::test]
async fn in_memory_rpc_maps_handler_errors_to_response_status() {
    let transport = Arc::new(LocalTransport::default());
    let uri_provider = Arc::new(StaticUriProvider::new("", 0x0005, 0x02));
    let server = InMemoryRpcServer::new(transport.clone(), uri_provider.clone());
    let client = InMemoryRpcClient::new(transport, uri_provider.clone());
    let handler = Arc::new(FailingRequestHandler);

    server
        .register_endpoint(None, 0x0001, handler.clone())
        .await
        .unwrap();

    let result = client
        .invoke_method(
            uri_provider.get_resource_uri(0x0001),
            CallOptions::for_rpc_request(5_000, None, None, None),
            None,
        )
        .await;

    assert!(matches!(result, Err(ServiceInvocationError::NotFound(_))));
}

#[tokio::test]
async fn rpc_client_ext_invokes_typed_wire_format() {
    let transport = Arc::new(LocalTransport::default());
    let uri_provider = Arc::new(StaticUriProvider::new("", 0x0005, 0x02));
    let server = InMemoryRpcServer::new(transport.clone(), uri_provider.clone());
    let client = InMemoryRpcClient::new(transport, uri_provider.clone());

    server
        .register_endpoint(None, 0x0002, Arc::new(IncrementReadingHandler))
        .await
        .unwrap();

    let response: Option<Reading> = client
        .invoke_serialized_method::<ReadingWire, ReadingWire, _, Reading>(
            uri_provider.get_resource_uri(0x0002),
            CallOptions::for_rpc_request(5_000, None, None, None),
            &Reading(41),
        )
        .await
        .unwrap();

    assert_eq!(response, Some(Reading(42)));
}

#[cfg(feature = "protobuf-wire")]
#[tokio::test]
async fn rpc_client_ext_invokes_typed_protobuf_payload() {
    let transport = Arc::new(LocalTransport::default());
    let uri_provider = Arc::new(StaticUriProvider::new("", 0x0005, 0x02));
    let server = InMemoryRpcServer::new(transport.clone(), uri_provider.clone());
    let client = InMemoryRpcClient::new(transport, uri_provider.clone());

    server
        .register_endpoint(None, 0x0003, Arc::new(ProtobufGreetingHandler))
        .await
        .unwrap();

    let mut request = StringValue::new();
    request.value = "protobuf rpc".to_string();

    let response: Option<StringValue> = client
        .invoke_serialized_method::<ProtobufWire, ProtobufWire, _, StringValue>(
            uri_provider.get_resource_uri(0x0003),
            CallOptions::for_rpc_request(5_000, None, None, None),
            &request,
        )
        .await
        .unwrap();

    assert_eq!(response.unwrap().value, "hello, protobuf rpc");
}
