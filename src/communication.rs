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

//! Native uProtocol communication-layer APIs built on owned frames.
//!
//! This module intentionally uses [`UOwnedFrame`] and [`UEncoding`] instead of
//! reintroducing generated transport envelopes.

use std::{error::Error, fmt::Display, sync::Arc};
#[cfg(feature = "util")]
use std::{sync::Mutex, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
#[cfg(feature = "util")]
use tokio::{sync::oneshot, time::timeout};

use crate::{
    LocalUriProvider, RawBytes, UAttributes, UCode, UDeserializer, UEncoding, UFrameHeader,
    UMessageType, UOwnedFrame, UOwnedListener, UOwnedTransport, UPriority, USerializer, UStatus,
    UUri, UWireError, WireFormat, UUID,
};

/// An error indicating a problem with registering or unregistering a frame listener.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistrationError {
    AlreadyExists,
    MaxListenersExceeded,
    NoSuchListener,
    PushDeliveryMethodNotSupported,
    InvalidFilter(String),
    Unknown(UStatus),
}

impl Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExists => {
                f.write_str("a listener for the given filter criteria already exists")
            }
            Self::MaxListenersExceeded => {
                f.write_str("maximum number of listeners has been reached")
            }
            Self::NoSuchListener => f.write_str("no listener registered for given pattern"),
            Self::PushDeliveryMethodNotSupported => f.write_str(
                "the underlying transport implementation does not support listener registration",
            ),
            Self::InvalidFilter(message) => {
                f.write_fmt(format_args!("invalid filter(s): {message}"))
            }
            Self::Unknown(status) => {
                f.write_fmt(format_args!("error un-/registering listener: {status}"))
            }
        }
    }
}

impl Error for RegistrationError {}

impl From<UStatus> for RegistrationError {
    fn from(value: UStatus) -> Self {
        match value.get_code() {
            UCode::ALREADY_EXISTS => Self::AlreadyExists,
            UCode::NOT_FOUND => Self::NoSuchListener,
            UCode::RESOURCE_EXHAUSTED => Self::MaxListenersExceeded,
            UCode::UNIMPLEMENTED => Self::PushDeliveryMethodNotSupported,
            UCode::INVALID_ARGUMENT => Self::InvalidFilter(value.get_message()),
            UCode::OK
            | UCode::CANCELLED
            | UCode::UNKNOWN
            | UCode::DEADLINE_EXCEEDED
            | UCode::PERMISSION_DENIED
            | UCode::FAILED_PRECONDITION
            | UCode::ABORTED
            | UCode::OUT_OF_RANGE
            | UCode::INTERNAL
            | UCode::UNAVAILABLE
            | UCode::DATA_LOSS
            | UCode::UNAUTHENTICATED => Self::Unknown(value),
        }
    }
}

/// General options for sending a uProtocol communication-layer frame.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CallOptions {
    ttl: Option<u32>,
    message_id: Option<UUID>,
    token: Option<String>,
    priority: Option<UPriority>,
}

impl CallOptions {
    /// Creates options suitable for invoking an RPC method.
    pub fn for_rpc_request(
        ttl: u32,
        message_id: Option<UUID>,
        token: Option<String>,
        priority: Option<UPriority>,
    ) -> Self {
        Self {
            ttl: Some(ttl),
            message_id,
            token,
            priority,
        }
    }

    /// Creates options suitable for sending a notification.
    pub fn for_notification(
        ttl: Option<u32>,
        message_id: Option<UUID>,
        priority: Option<UPriority>,
    ) -> Self {
        Self {
            ttl,
            message_id,
            token: None,
            priority,
        }
    }

    /// Creates options suitable for publishing an event.
    pub fn for_publish(
        ttl: Option<u32>,
        message_id: Option<UUID>,
        priority: Option<UPriority>,
    ) -> Self {
        Self {
            ttl,
            message_id,
            token: None,
            priority,
        }
    }

    /// Gets the frame time-to-live in milliseconds, if configured.
    pub fn ttl(&self) -> Option<u32> {
        self.ttl
    }

    /// Gets the frame identifier to use, if configured.
    pub fn message_id(&self) -> Option<UUID> {
        self.message_id.clone()
    }

    /// Gets the authentication token to include, if configured.
    pub fn token(&self) -> Option<String> {
        self.token.clone()
    }

    /// Gets the frame priority to use, if configured.
    pub fn priority(&self) -> Option<UPriority> {
        self.priority
    }
}

/// Native payload bytes plus their serializer-neutral encoding metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UPayload {
    encoding: UEncoding,
    payload: Bytes,
}

impl UPayload {
    /// Creates a payload from bytes and explicit encoding metadata.
    pub fn new<T: Into<Bytes>>(payload: T, encoding: UEncoding) -> Self {
        Self {
            encoding,
            payload: payload.into(),
        }
    }

    /// Creates a raw byte payload.
    pub fn from_raw<T: Into<Bytes>>(payload: T) -> Self {
        Self::new(payload, RawBytes::encoding())
    }

    /// Serializes a typed payload into its wire format.
    pub fn from_serializable<F, T>(value: &T) -> Result<Self, UWireError>
    where
        F: WireFormat,
        T: USerializer<F>,
    {
        Ok(Self::new(value.serialize_owned()?, F::encoding()))
    }

    /// Deserializes the payload using a selected wire format.
    pub fn deserialize<'a, F, T>(&'a self) -> Result<T, UWireError>
    where
        F: WireFormat,
        T: UDeserializer<'a, F>,
    {
        if self.encoding != F::encoding() {
            return Err(UWireError::UnsupportedEncoding(self.encoding.clone()));
        }
        T::deserialize_from(self.payload_bytes())
    }

    /// Gets the payload encoding metadata.
    pub fn encoding(&self) -> &UEncoding {
        &self.encoding
    }

    /// Gets the payload bytes.
    pub fn payload_bytes(&self) -> &[u8] {
        self.payload.as_ref()
    }

    /// Consumes this payload and returns its bytes.
    pub fn payload(self) -> Bytes {
        self.payload
    }

    /// Consumes this payload and returns encoding metadata and bytes.
    pub fn into_parts(self) -> (UEncoding, Bytes) {
        (self.encoding, self.payload)
    }
}

/// An error indicating a problem with sending a notification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NotificationError {
    InvalidArgument(String),
    NotifyError(UStatus),
}

impl Display for NotificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArgument(message) => f.write_str(message),
            Self::NotifyError(status) => {
                f.write_fmt(format_args!("failed to send notification: {status}"))
            }
        }
    }
}

impl Error for NotificationError {}

/// A client for sending Notification frames to a uEntity.
#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(
        &self,
        resource_id: u16,
        destination: &UUri,
        call_options: CallOptions,
        payload: Option<UPayload>,
    ) -> Result<(), NotificationError>;

    async fn start_listening(
        &self,
        topic: &UUri,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), RegistrationError>;

    async fn stop_listening(
        &self,
        topic: &UUri,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), RegistrationError>;
}

/// An error indicating a problem with publishing an event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PubSubError {
    InvalidArgument(String),
    PublishError(UStatus),
}

impl Display for PubSubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArgument(message) => f.write_str(message),
            Self::PublishError(status) => {
                f.write_fmt(format_args!("failed to publish message: {status}"))
            }
        }
    }
}

impl Error for PubSubError {}

/// A client for publishing frames to topics.
#[async_trait]
pub trait Publisher: Send + Sync {
    async fn publish(
        &self,
        resource_id: u16,
        call_options: CallOptions,
        payload: Option<UPayload>,
    ) -> Result<(), PubSubError>;
}

/// A client for registering handlers for published topic frames.
#[async_trait]
pub trait Subscriber: Send + Sync {
    async fn subscribe(
        &self,
        topic: &UUri,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), RegistrationError>;

    async fn unsubscribe(
        &self,
        topic: &UUri,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), RegistrationError>;
}

/// An error indicating a problem with invoking a service operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceInvocationError {
    AlreadyExists(String),
    DeadlineExceeded,
    FailedPrecondition(String),
    Internal(String),
    InvalidArgument(String),
    NotFound(String),
    PermissionDenied(String),
    ResourceExhausted(String),
    RpcError(UStatus),
    Unauthenticated,
    Unavailable(String),
    Unimplemented(String),
}

impl Display for ServiceInvocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExists(message) => {
                f.write_fmt(format_args!("entity already exists: {message}"))
            }
            Self::DeadlineExceeded => f.write_str("request timed out"),
            Self::FailedPrecondition(message) => {
                f.write_fmt(format_args!("failed precondition: {message}"))
            }
            Self::Internal(message) => f.write_fmt(format_args!("internal error: {message}")),
            Self::InvalidArgument(message) => {
                f.write_fmt(format_args!("invalid argument: {message}"))
            }
            Self::NotFound(message) => f.write_fmt(format_args!("no such entity: {message}")),
            Self::PermissionDenied(message) => {
                f.write_fmt(format_args!("permission denied: {message}"))
            }
            Self::ResourceExhausted(message) => {
                f.write_fmt(format_args!("resource exhausted: {message}"))
            }
            Self::RpcError(status) => f.write_fmt(format_args!("unknown error: {status}")),
            Self::Unauthenticated => f.write_str("unauthenticated"),
            Self::Unavailable(message) => {
                f.write_fmt(format_args!("resource unavailable: {message}"))
            }
            Self::Unimplemented(message) => f.write_fmt(format_args!("unimplemented: {message}")),
        }
    }
}

impl Error for ServiceInvocationError {}

impl From<UStatus> for ServiceInvocationError {
    fn from(value: UStatus) -> Self {
        match value.get_code() {
            UCode::ALREADY_EXISTS => Self::AlreadyExists(value.get_message()),
            UCode::DEADLINE_EXCEEDED => Self::DeadlineExceeded,
            UCode::FAILED_PRECONDITION => Self::FailedPrecondition(value.get_message()),
            UCode::INTERNAL => Self::Internal(value.get_message()),
            UCode::INVALID_ARGUMENT => Self::InvalidArgument(value.get_message()),
            UCode::NOT_FOUND => Self::NotFound(value.get_message()),
            UCode::PERMISSION_DENIED => Self::PermissionDenied(value.get_message()),
            UCode::RESOURCE_EXHAUSTED => Self::ResourceExhausted(value.get_message()),
            UCode::UNAUTHENTICATED => Self::Unauthenticated,
            UCode::UNAVAILABLE => Self::Unavailable(value.get_message()),
            UCode::UNIMPLEMENTED => Self::Unimplemented(value.get_message()),
            UCode::OK
            | UCode::CANCELLED
            | UCode::UNKNOWN
            | UCode::ABORTED
            | UCode::OUT_OF_RANGE
            | UCode::DATA_LOSS => Self::RpcError(value),
        }
    }
}

impl From<ServiceInvocationError> for UStatus {
    fn from(value: ServiceInvocationError) -> Self {
        match value {
            ServiceInvocationError::AlreadyExists(message) => {
                UStatus::fail_with_code(UCode::ALREADY_EXISTS, message)
            }
            ServiceInvocationError::DeadlineExceeded => {
                UStatus::fail_with_code(UCode::DEADLINE_EXCEEDED, "request timed out")
            }
            ServiceInvocationError::FailedPrecondition(message) => {
                UStatus::fail_with_code(UCode::FAILED_PRECONDITION, message)
            }
            ServiceInvocationError::Internal(message) => {
                UStatus::fail_with_code(UCode::INTERNAL, message)
            }
            ServiceInvocationError::InvalidArgument(message) => {
                UStatus::fail_with_code(UCode::INVALID_ARGUMENT, message)
            }
            ServiceInvocationError::NotFound(message) => {
                UStatus::fail_with_code(UCode::NOT_FOUND, message)
            }
            ServiceInvocationError::PermissionDenied(message) => {
                UStatus::fail_with_code(UCode::PERMISSION_DENIED, message)
            }
            ServiceInvocationError::ResourceExhausted(message) => {
                UStatus::fail_with_code(UCode::RESOURCE_EXHAUSTED, message)
            }
            ServiceInvocationError::Unauthenticated => {
                UStatus::fail_with_code(UCode::UNAUTHENTICATED, "client must authenticate")
            }
            ServiceInvocationError::Unavailable(message) => {
                UStatus::fail_with_code(UCode::UNAVAILABLE, message)
            }
            ServiceInvocationError::Unimplemented(message) => {
                UStatus::fail_with_code(UCode::UNIMPLEMENTED, message)
            }
            ServiceInvocationError::RpcError(status) => status,
        }
    }
}

/// A client for performing RPCs on service methods.
#[async_trait]
pub trait RpcClient: Send + Sync {
    async fn invoke_method(
        &self,
        method: UUri,
        call_options: CallOptions,
        payload: Option<UPayload>,
    ) -> Result<Option<UPayload>, ServiceInvocationError>;
}

/// Typed convenience methods for serializer-neutral RPC clients.
#[async_trait]
pub trait RpcClientExt: RpcClient {
    async fn invoke_serialized_method<RequestFormat, ResponseFormat, Request, Response>(
        &self,
        method: UUri,
        call_options: CallOptions,
        request: &Request,
    ) -> Result<Option<Response>, ServiceInvocationError>
    where
        RequestFormat: WireFormat + Send + Sync,
        ResponseFormat: WireFormat + Send + Sync,
        Request: USerializer<RequestFormat> + Sync,
        Response: for<'payload> UDeserializer<'payload, ResponseFormat> + Send,
    {
        let request_payload = UPayload::from_serializable::<RequestFormat, _>(request)
            .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))?;
        let Some(response_payload) = self
            .invoke_method(method, call_options, Some(request_payload))
            .await?
        else {
            return Ok(None);
        };
        response_payload
            .deserialize::<ResponseFormat, Response>()
            .map(Some)
            .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))
    }
}

impl<T> RpcClientExt for T where T: RpcClient + ?Sized {}

/// A handler for processing incoming RPC requests.
#[async_trait]
pub trait RequestHandler: Send + Sync {
    async fn handle_request(
        &self,
        resource_id: u16,
        attributes: &UAttributes,
        request_payload: Option<UPayload>,
    ) -> Result<Option<UPayload>, ServiceInvocationError>;
}

/// A server for exposing RPC endpoints.
#[async_trait]
pub trait RpcServer: Send + Sync {
    async fn register_endpoint(
        &self,
        origin_filter: Option<&UUri>,
        resource_id: u16,
        request_handler: Arc<dyn RequestHandler>,
    ) -> Result<(), RegistrationError>;

    async fn unregister_endpoint(
        &self,
        origin_filter: Option<&UUri>,
        resource_id: u16,
        request_handler: Arc<dyn RequestHandler>,
    ) -> Result<(), RegistrationError>;
}

#[cfg(feature = "util")]
struct RpcResponseListener {
    request_id: UUID,
    sender: Mutex<Option<oneshot::Sender<UOwnedFrame>>>,
}

#[cfg(feature = "util")]
impl RpcResponseListener {
    fn new(request_id: UUID, sender: oneshot::Sender<UOwnedFrame>) -> Self {
        Self {
            request_id,
            sender: Mutex::new(Some(sender)),
        }
    }
}

#[cfg(feature = "util")]
#[async_trait]
impl UOwnedListener for RpcResponseListener {
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        let attributes = frame.header().attributes();
        if attributes.message_type() != UMessageType::Response {
            return;
        }
        if attributes.request_id() != Some(&self.request_id) {
            return;
        }
        let Ok(mut sender) = self.sender.lock() else {
            return;
        };
        if let Some(sender) = sender.take() {
            let _ = sender.send(frame);
        }
    }
}

/// A native RPC client implemented directly on an owned-frame transport.
#[cfg(feature = "util")]
pub struct InMemoryRpcClient<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    transport: Arc<T>,
    uri_provider: Arc<P>,
}

#[cfg(feature = "util")]
impl<T, P> InMemoryRpcClient<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    pub fn new(transport: Arc<T>, uri_provider: Arc<P>) -> Self {
        Self {
            transport,
            uri_provider,
        }
    }
}

#[cfg(feature = "util")]
#[async_trait]
impl<T, P> RpcClient for InMemoryRpcClient<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    async fn invoke_method(
        &self,
        method: UUri,
        call_options: CallOptions,
        payload: Option<UPayload>,
    ) -> Result<Option<UPayload>, ServiceInvocationError> {
        method
            .verify_rpc_method()
            .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))?;
        let reply_to = self.uri_provider.get_source_uri();
        reply_to
            .verify_rpc_response()
            .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))?;

        let (request_header, request_id, ttl) =
            rpc_request_header(method.clone(), reply_to.clone(), call_options)?;
        let response_filter = method.clone();
        let (sender, receiver) = oneshot::channel();
        let listener = Arc::new(RpcResponseListener::new(request_id, sender));

        self.transport
            .register_owned_listener(&response_filter, Some(&reply_to), listener.clone())
            .await
            .map_err(ServiceInvocationError::from)?;

        let send_result = self
            .transport
            .send_owned(frame_from_payload(request_header, payload))
            .await;
        if let Err(status) = send_result {
            unregister_response_listener(
                self.transport.as_ref(),
                &response_filter,
                &reply_to,
                listener,
            )
            .await;
            return Err(ServiceInvocationError::from(status));
        }

        let response_result = timeout(Duration::from_millis(u64::from(ttl)), receiver).await;
        unregister_response_listener(
            self.transport.as_ref(),
            &response_filter,
            &reply_to,
            listener,
        )
        .await;

        let response = match response_result {
            Ok(Ok(response)) => response,
            Ok(Err(_closed)) => {
                return Err(ServiceInvocationError::Internal(
                    "RPC response listener closed before receiving a response".to_string(),
                ));
            }
            Err(_elapsed) => return Err(ServiceInvocationError::DeadlineExceeded),
        };

        if let Some(commstatus) = response.header().attributes().commstatus() {
            if commstatus != UCode::OK {
                return Err(ServiceInvocationError::from(UStatus::fail_with_code(
                    commstatus,
                    "RPC response indicated failure",
                )));
            }
        }

        Ok(payload_from_frame(&response))
    }
}

#[cfg(feature = "util")]
async fn unregister_response_listener<T>(
    transport: &T,
    source_filter: &UUri,
    sink_filter: &UUri,
    listener: Arc<dyn UOwnedListener>,
) where
    T: UOwnedTransport + ?Sized,
{
    let _ = transport
        .unregister_owned_listener(source_filter, Some(sink_filter), listener)
        .await;
}

#[cfg(feature = "util")]
struct RpcEndpointListener<T>
where
    T: UOwnedTransport + ?Sized,
{
    transport: Arc<T>,
    method: UUri,
    request_handler: Arc<dyn RequestHandler>,
}

#[cfg(feature = "util")]
impl<T> RpcEndpointListener<T>
where
    T: UOwnedTransport + ?Sized,
{
    fn new(transport: Arc<T>, method: UUri, request_handler: Arc<dyn RequestHandler>) -> Self {
        Self {
            transport,
            method,
            request_handler,
        }
    }

    async fn send_response(&self, request: &UOwnedFrame, payload: Option<UPayload>, status: UCode) {
        let attributes = request.header().attributes();
        let Some(reply_to) = request.header().sink().map(|_| attributes.source().clone()) else {
            return;
        };
        let mut response_attributes = UAttributes::new(
            UUID::build(),
            self.method.clone(),
            Some(reply_to),
            UMessageType::Response,
        )
        .with_priority(UPriority::CS4)
        .with_request_id(attributes.id().clone());
        if status != UCode::OK {
            response_attributes = response_attributes.with_commstatus(status);
        }
        let response = frame_from_payload(
            UFrameHeader::new(response_attributes, UEncoding::default()),
            payload,
        );
        let _ = self.transport.send_owned(response).await;
    }
}

#[cfg(feature = "util")]
#[async_trait]
impl<T> UOwnedListener for RpcEndpointListener<T>
where
    T: UOwnedTransport + ?Sized,
{
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        let attributes = frame.header().attributes();
        if attributes.message_type() != UMessageType::Request
            || frame.header().sink() != Some(&self.method)
        {
            return;
        }

        let request_payload = payload_from_frame(&frame);
        match self
            .request_handler
            .handle_request(self.method.resource_id(), attributes, request_payload)
            .await
        {
            Ok(response_payload) => {
                self.send_response(&frame, response_payload, UCode::OK)
                    .await
            }
            Err(error) => {
                let status = UStatus::from(error);
                self.send_response(&frame, None, status.get_code()).await;
            }
        }
    }
}

#[cfg(feature = "util")]
struct EndpointRegistration {
    source_filter: UUri,
    method: UUri,
    request_handler: Arc<dyn RequestHandler>,
    listener: Arc<dyn UOwnedListener>,
}

/// A native RPC server implemented directly on an owned-frame transport.
#[cfg(feature = "util")]
pub struct InMemoryRpcServer<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    transport: Arc<T>,
    uri_provider: Arc<P>,
    registrations: Mutex<Vec<EndpointRegistration>>,
}

#[cfg(feature = "util")]
impl<T, P> InMemoryRpcServer<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    pub fn new(transport: Arc<T>, uri_provider: Arc<P>) -> Self {
        Self {
            transport,
            uri_provider,
            registrations: Mutex::new(Vec::new()),
        }
    }

    fn endpoint_filters(
        &self,
        origin_filter: Option<&UUri>,
        resource_id: u16,
    ) -> Result<(UUri, UUri), RegistrationError> {
        let method = self.uri_provider.get_resource_uri(resource_id);
        method
            .verify_rpc_method()
            .map_err(|error| RegistrationError::InvalidFilter(error.to_string()))?;
        let source_filter = origin_filter
            .cloned()
            .unwrap_or_else(|| UUri::any_with_resource_id(0));
        crate::verify_filter_criteria(&source_filter, Some(&method))
            .map_err(RegistrationError::from)?;
        Ok((source_filter, method))
    }
}

#[cfg(feature = "util")]
#[async_trait]
impl<T, P> RpcServer for InMemoryRpcServer<T, P>
where
    T: UOwnedTransport + ?Sized + 'static,
    P: LocalUriProvider + ?Sized,
{
    async fn register_endpoint(
        &self,
        origin_filter: Option<&UUri>,
        resource_id: u16,
        request_handler: Arc<dyn RequestHandler>,
    ) -> Result<(), RegistrationError> {
        let (source_filter, method) = self.endpoint_filters(origin_filter, resource_id)?;
        {
            let registrations = self.registrations.lock().map_err(|_| {
                RegistrationError::Unknown(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    "failed to acquire RPC endpoint registry lock",
                ))
            })?;
            if registrations.iter().any(|registration| {
                registration.source_filter == source_filter
                    && registration.method == method
                    && Arc::ptr_eq(&registration.request_handler, &request_handler)
            }) {
                return Err(RegistrationError::AlreadyExists);
            }
        }

        let listener = Arc::new(RpcEndpointListener::new(
            self.transport.clone(),
            method.clone(),
            request_handler.clone(),
        ));
        self.transport
            .register_owned_listener(&source_filter, Some(&method), listener.clone())
            .await
            .map_err(RegistrationError::from)?;
        let mut registrations = self.registrations.lock().map_err(|_| {
            RegistrationError::Unknown(UStatus::fail_with_code(
                UCode::INTERNAL,
                "failed to acquire RPC endpoint registry lock",
            ))
        })?;
        registrations.push(EndpointRegistration {
            source_filter,
            method,
            request_handler,
            listener,
        });
        Ok(())
    }

    async fn unregister_endpoint(
        &self,
        origin_filter: Option<&UUri>,
        resource_id: u16,
        request_handler: Arc<dyn RequestHandler>,
    ) -> Result<(), RegistrationError> {
        let (source_filter, method) = self.endpoint_filters(origin_filter, resource_id)?;
        let registration = {
            let mut registrations = self.registrations.lock().map_err(|_| {
                RegistrationError::Unknown(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    "failed to acquire RPC endpoint registry lock",
                ))
            })?;
            let Some(index) = registrations.iter().position(|registration| {
                registration.source_filter == source_filter
                    && registration.method == method
                    && Arc::ptr_eq(&registration.request_handler, &request_handler)
            }) else {
                return Err(RegistrationError::NoSuchListener);
            };
            registrations.remove(index)
        };

        self.transport
            .unregister_owned_listener(
                &registration.source_filter,
                Some(&registration.method),
                registration.listener,
            )
            .await
            .map_err(RegistrationError::from)
    }
}

/// A [`Notifier`] implemented directly on an owned-frame transport.
pub struct SimpleNotifier<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    transport: Arc<T>,
    uri_provider: Arc<P>,
}

impl<T, P> SimpleNotifier<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    pub fn new(transport: Arc<T>, uri_provider: Arc<P>) -> Self {
        Self {
            transport,
            uri_provider,
        }
    }
}

#[async_trait]
impl<T, P> Notifier for SimpleNotifier<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    async fn notify(
        &self,
        resource_id: u16,
        destination: &UUri,
        call_options: CallOptions,
        payload: Option<UPayload>,
    ) -> Result<(), NotificationError> {
        destination
            .verify_rpc_response()
            .map_err(|error| NotificationError::InvalidArgument(error.to_string()))?;
        let source = self.uri_provider.get_resource_uri(resource_id);
        source
            .verify_no_wildcards()
            .map_err(|error| NotificationError::InvalidArgument(error.to_string()))?;
        if source.is_notification_destination() {
            return Err(NotificationError::InvalidArgument(
                "notification origin resource ID must not be 0".to_string(),
            ));
        }

        let header = header_with_options(
            source,
            Some(destination.to_owned()),
            UMessageType::Notification,
            UPriority::CS1,
            call_options,
        );
        self.transport
            .send_owned(frame_from_payload(header, payload))
            .await
            .map_err(NotificationError::NotifyError)
    }

    async fn start_listening(
        &self,
        topic: &UUri,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), RegistrationError> {
        topic
            .verify_no_wildcards()
            .map_err(|error| RegistrationError::InvalidFilter(error.to_string()))?;
        self.transport
            .register_owned_listener(topic, Some(&self.uri_provider.get_source_uri()), listener)
            .await
            .map_err(RegistrationError::from)
    }

    async fn stop_listening(
        &self,
        topic: &UUri,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), RegistrationError> {
        topic
            .verify_no_wildcards()
            .map_err(|error| RegistrationError::InvalidFilter(error.to_string()))?;
        self.transport
            .unregister_owned_listener(topic, Some(&self.uri_provider.get_source_uri()), listener)
            .await
            .map_err(RegistrationError::from)
    }
}

/// A [`Publisher`] implemented directly on an owned-frame transport.
pub struct SimplePublisher<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    transport: Arc<T>,
    uri_provider: Arc<P>,
}

impl<T, P> SimplePublisher<T, P>
where
    T: UOwnedTransport + ?Sized,
    P: LocalUriProvider + ?Sized,
{
    pub fn new(transport: Arc<T>, uri_provider: Arc<P>) -> Self {
        Self {
            transport,
            uri_provider,
        }
    }
}

#[async_trait]
impl<T, P> Publisher for SimplePublisher<T, P>
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
        let topic = self.uri_provider.get_resource_uri(resource_id);
        topic
            .verify_event()
            .map_err(|error| PubSubError::InvalidArgument(error.to_string()))?;
        let header = header_with_options(
            topic,
            None,
            UMessageType::Publish,
            UPriority::CS1,
            call_options,
        );
        self.transport
            .send_owned(frame_from_payload(header, payload))
            .await
            .map_err(PubSubError::PublishError)
    }
}

/// A direct transport-backed [`Subscriber`].
pub struct SimpleSubscriber<T>
where
    T: UOwnedTransport + ?Sized,
{
    transport: Arc<T>,
}

impl<T> SimpleSubscriber<T>
where
    T: UOwnedTransport + ?Sized,
{
    pub fn new(transport: Arc<T>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl<T> Subscriber for SimpleSubscriber<T>
where
    T: UOwnedTransport + ?Sized,
{
    async fn subscribe(
        &self,
        topic: &UUri,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), RegistrationError> {
        topic
            .verify_event()
            .map_err(|error| RegistrationError::InvalidFilter(error.to_string()))?;
        self.transport
            .register_owned_listener(topic, None, listener)
            .await
            .map_err(RegistrationError::from)
    }

    async fn unsubscribe(
        &self,
        topic: &UUri,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), RegistrationError> {
        topic
            .verify_event()
            .map_err(|error| RegistrationError::InvalidFilter(error.to_string()))?;
        self.transport
            .unregister_owned_listener(topic, None, listener)
            .await
            .map_err(RegistrationError::from)
    }
}

fn header_with_options(
    source: UUri,
    sink: Option<UUri>,
    message_type: UMessageType,
    default_priority: UPriority,
    options: CallOptions,
) -> UFrameHeader {
    let id = options.message_id.unwrap_or_else(UUID::build);
    let priority = options.priority.unwrap_or(default_priority);
    let mut attributes = UAttributes::new(id, source, sink, message_type).with_priority(priority);
    if let Some(ttl) = options.ttl {
        attributes = attributes.with_ttl(ttl);
    }
    if let Some(token) = options.token {
        attributes = attributes.with_token(token);
    }
    UFrameHeader::new(attributes, UEncoding::default())
}

#[cfg(feature = "util")]
fn rpc_request_header(
    method: UUri,
    reply_to: UUri,
    options: CallOptions,
) -> Result<(UFrameHeader, UUID, u32), ServiceInvocationError> {
    let ttl = options.ttl.ok_or_else(|| {
        ServiceInvocationError::InvalidArgument("RPC request TTL is required".to_string())
    })?;
    if ttl == 0 {
        return Err(ServiceInvocationError::InvalidArgument(
            "RPC request TTL must be greater than 0".to_string(),
        ));
    }
    let id = options.message_id.unwrap_or_else(UUID::build);
    let priority = options.priority.unwrap_or(UPriority::CS4);
    let mut attributes =
        UAttributes::new(id.clone(), reply_to, Some(method), UMessageType::Request)
            .with_priority(priority)
            .with_ttl(ttl);
    if let Some(token) = options.token {
        attributes = attributes.with_token(token);
    }
    Ok((UFrameHeader::new(attributes, UEncoding::default()), id, ttl))
}

#[cfg(feature = "util")]
fn payload_from_frame(frame: &UOwnedFrame) -> Option<UPayload> {
    if frame.payload_bytes().is_empty() {
        None
    } else {
        Some(UPayload::new(
            frame.payload().clone(),
            frame.header().encoding().clone(),
        ))
    }
}

fn frame_from_payload(header: UFrameHeader, payload: Option<UPayload>) -> UOwnedFrame {
    if let Some(payload) = payload {
        let (encoding, bytes) = payload.into_parts();
        UOwnedFrame::new(header.with_encoding(encoding), bytes)
    } else {
        UOwnedFrame::new(header, Bytes::new())
    }
}
