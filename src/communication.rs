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
//! This module intentionally uses [`UOwnedFrame`] and [`crate::UEncoding`] instead of
//! reintroducing generated transport envelopes.

#[cfg(all(feature = "util", feature = "protobuf-wire"))]
use std::{
    collections::{hash_map::Entry, HashMap},
    ops::Deref,
    sync::RwLock,
};
use std::{error::Error, fmt::Display, sync::Arc};
#[cfg(feature = "util")]
use std::{sync::Mutex, time::Duration};

use async_trait::async_trait;
#[cfg(feature = "util")]
use tokio::{sync::oneshot, time::timeout};

#[cfg(feature = "protobuf-wire")]
use crate::core::usubscription;
#[cfg(all(feature = "util", feature = "protobuf-wire"))]
use crate::core::usubscription::SubscriptionResponseExt;
#[cfg(all(feature = "util", feature = "protobuf-wire"))]
use crate::core::usubscription::{from_proto_uri, to_proto_uri, State, Update};
#[cfg(feature = "protobuf-wire")]
use crate::core::usubscription::{SubscriptionRequest, USubscription, UnsubscribeRequest};
#[cfg(feature = "protobuf-wire")]
use crate::ProtobufPayload;
#[cfg(feature = "util")]
use crate::UFrameBuilder;
use crate::{
    payload::{PayloadFormat, UDeserializer, USerializer},
    LocalUriProvider, UAttributes, UCode, UFrameMetadata, UMessageType, UOwnedFrame,
    UOwnedListener, UOwnedTransport, UPriority, UStatus, UUri, UUID,
};

mod payload;
pub use payload::UPayload;

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
#[cfg_attr(any(test, feature = "test-util"), mockall::automock)]
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
#[cfg_attr(any(test, feature = "test-util"), mockall::automock)]
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
#[cfg_attr(any(test, feature = "test-util"), mockall::automock)]
#[async_trait]
pub trait Subscriber: Send + Sync {
    async fn subscribe(
        &self,
        topic: &UUri,
        listener: Arc<dyn UOwnedListener>,
        subscription_change_handler: Option<Arc<dyn SubscriptionChangeHandler>>,
    ) -> Result<(), RegistrationError>;

    async fn unsubscribe(
        &self,
        topic: &UUri,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), RegistrationError>;
}

/// Handles subscription status updates for a subscribed topic.
#[cfg(feature = "protobuf-wire")]
#[cfg_attr(any(test, feature = "test-util"), mockall::automock)]
pub trait SubscriptionChangeHandler: Send + Sync {
    fn on_subscription_change(&self, topic: UUri, status: usubscription::SubscriptionStatus);
}

/// Handles subscription status updates for a subscribed topic.
///
/// Subscription status payloads are protobuf service DTOs, so no callback method
/// is available unless `protobuf-wire` is enabled.
#[cfg(not(feature = "protobuf-wire"))]
pub trait SubscriptionChangeHandler: Send + Sync {}

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
#[cfg_attr(any(test, feature = "test-util"), mockall::automock)]
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
        RequestFormat: PayloadFormat + Send + Sync,
        ResponseFormat: PayloadFormat + Send + Sync,
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

/// A [`USubscription`] client implemented over the serializer-neutral RPC client.
#[cfg(feature = "protobuf-wire")]
pub struct RpcClientUSubscription {
    rpc_client: Arc<dyn RpcClient>,
}

#[cfg(feature = "protobuf-wire")]
impl RpcClientUSubscription {
    pub fn new(rpc_client: Arc<dyn RpcClient>) -> Self {
        Self { rpc_client }
    }

    fn default_call_options() -> CallOptions {
        CallOptions::for_rpc_request(5_000, None, None, None)
    }

    async fn invoke<Request, Response>(
        &self,
        method_resource_id: u16,
        request: &Request,
    ) -> Result<Response, UStatus>
    where
        Request: USerializer<ProtobufPayload> + Sync,
        Response: for<'payload> UDeserializer<'payload, ProtobufPayload> + Send,
    {
        self.rpc_client
            .invoke_serialized_method::<ProtobufPayload, ProtobufPayload, _, Response>(
                usubscription::usubscription_uri(method_resource_id),
                Self::default_call_options(),
                request,
            )
            .await
            .map_err(UStatus::from)?
            .ok_or_else(|| {
                UStatus::fail_with_code(
                    UCode::DATA_LOSS,
                    "uSubscription method returned no response payload",
                )
            })
    }
}

#[cfg(feature = "protobuf-wire")]
#[async_trait]
impl USubscription for RpcClientUSubscription {
    async fn subscribe(
        &self,
        subscription_request: SubscriptionRequest,
    ) -> Result<usubscription::SubscriptionResponse, UStatus> {
        self.invoke(usubscription::RESOURCE_ID_SUBSCRIBE, &subscription_request)
            .await
    }

    async fn fetch_subscriptions(
        &self,
        fetch_subscriptions_request: usubscription::FetchSubscriptionsRequest,
    ) -> Result<usubscription::FetchSubscriptionsResponse, UStatus> {
        self.invoke(
            usubscription::RESOURCE_ID_FETCH_SUBSCRIPTIONS,
            &fetch_subscriptions_request,
        )
        .await
    }

    async fn unsubscribe(&self, unsubscribe_request: UnsubscribeRequest) -> Result<(), UStatus> {
        self.invoke::<_, usubscription::UnsubscribeResponse>(
            usubscription::RESOURCE_ID_UNSUBSCRIBE,
            &unsubscribe_request,
        )
        .await
        .map(|_| ())
    }

    async fn register_for_notifications(
        &self,
        notifications_register_request: usubscription::NotificationsRequest,
    ) -> Result<(), UStatus> {
        self.invoke::<_, usubscription::NotificationsResponse>(
            usubscription::RESOURCE_ID_REGISTER_FOR_NOTIFICATIONS,
            &notifications_register_request,
        )
        .await
        .map(|_| ())
    }

    async fn unregister_for_notifications(
        &self,
        notifications_unregister_request: usubscription::NotificationsRequest,
    ) -> Result<(), UStatus> {
        self.invoke::<_, usubscription::NotificationsResponse>(
            usubscription::RESOURCE_ID_UNREGISTER_FOR_NOTIFICATIONS,
            &notifications_unregister_request,
        )
        .await
        .map(|_| ())
    }

    async fn fetch_subscribers(
        &self,
        fetch_subscribers_request: usubscription::FetchSubscribersRequest,
    ) -> Result<usubscription::FetchSubscribersResponse, UStatus> {
        self.invoke(
            usubscription::RESOURCE_ID_FETCH_SUBSCRIBERS,
            &fetch_subscribers_request,
        )
        .await
    }

    async fn reset(
        &self,
        reset_request: usubscription::ResetRequest,
    ) -> Result<usubscription::ResetResponse, UStatus> {
        self.invoke(usubscription::RESOURCE_ID_RESET, &reset_request)
            .await
    }
}

/// A handler for processing incoming RPC requests.
#[cfg_attr(any(test, feature = "test-util"), mockall::automock)]
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

#[cfg(not(tarpaulin_include))]
#[cfg(any(test, feature = "test-util"))]
mockall::mock! {
    pub RpcServer {
        pub async fn do_register_endpoint<'a>(&'a self, origin_filter: Option<&'a UUri>, resource_id: u16, request_handler: Arc<dyn RequestHandler>) -> Result<(), RegistrationError>;
        pub async fn do_unregister_endpoint<'a>(&'a self, origin_filter: Option<&'a UUri>, resource_id: u16, request_handler: Arc<dyn RequestHandler>) -> Result<(), RegistrationError>;
    }
}

#[cfg(not(tarpaulin_include))]
#[cfg(any(test, feature = "test-util"))]
#[async_trait]
impl RpcServer for MockRpcServer {
    async fn register_endpoint(
        &self,
        origin_filter: Option<&UUri>,
        resource_id: u16,
        request_handler: Arc<dyn RequestHandler>,
    ) -> Result<(), RegistrationError> {
        self.do_register_endpoint(origin_filter, resource_id, request_handler)
            .await
    }

    async fn unregister_endpoint(
        &self,
        origin_filter: Option<&UUri>,
        resource_id: u16,
        request_handler: Arc<dyn RequestHandler>,
    ) -> Result<(), RegistrationError> {
        self.do_unregister_endpoint(origin_filter, resource_id, request_handler)
            .await
    }
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
        let attributes = frame.metadata().attributes();
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

        let (request_metadata, request_id, ttl) =
            rpc_request_metadata(method.clone(), reply_to.clone(), call_options)?;
        let response_filter = method.clone();
        let (sender, receiver) = oneshot::channel();
        let listener = Arc::new(RpcResponseListener::new(request_id, sender));

        self.transport
            .register_owned_listener(&response_filter, Some(&reply_to), listener.clone())
            .await
            .map_err(ServiceInvocationError::from)?;

        let send_result = self
            .transport
            .send_owned(frame_from_payload(request_metadata, payload))
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

        if let Some(commstatus) = response.metadata().attributes().commstatus() {
            if commstatus != UCode::OK {
                return Err(ServiceInvocationError::from(response_error_status(
                    &response, commstatus,
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

    async fn send_response(
        &self,
        request: &UOwnedFrame,
        payload: Option<UPayload>,
        status: UStatus,
    ) {
        let attributes = request.metadata().attributes();
        let mut builder = UFrameBuilder::response_for_request(attributes);
        let status_code = status.get_code();
        if status_code != UCode::OK {
            builder = builder.with_comm_status(status_code);
        }
        let Ok(response_metadata) = builder.build_metadata() else {
            return;
        };
        let payload = if status_code == UCode::OK {
            payload
        } else {
            payload.or_else(|| {
                let message = status.get_message();
                (!message.is_empty()).then(|| UPayload::from_raw(message.into_bytes()))
            })
        };
        let response = frame_from_payload(response_metadata, payload);
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
        let attributes = frame.metadata().attributes();
        if attributes.message_type() != UMessageType::Request
            || frame.metadata().sink() != Some(&self.method)
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
                self.send_response(&frame, response_payload, UStatus::ok())
                    .await
            }
            Err(error) => {
                let status = UStatus::from(error);
                self.send_response(&frame, None, status).await;
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
        crate::transport::verify_filter_criteria(&source_filter, Some(&method))
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

        let metadata = metadata_with_options(
            source,
            Some(destination.to_owned()),
            UMessageType::Notification,
            UPriority::CS1,
            call_options,
        )
        .map_err(|error| NotificationError::InvalidArgument(error.to_string()))?;
        self.transport
            .send_owned(frame_from_payload(metadata, payload))
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
        let metadata = metadata_with_options(
            topic,
            None,
            UMessageType::Publish,
            UPriority::CS1,
            call_options,
        )
        .map_err(|error| PubSubError::InvalidArgument(error.to_string()))?;
        self.transport
            .send_owned(frame_from_payload(metadata, payload))
            .await
            .map_err(PubSubError::PublishError)
    }
}

#[cfg(all(feature = "util", feature = "protobuf-wire"))]
#[derive(Clone)]
struct ComparableSubscriptionChangeHandler {
    inner: Arc<dyn SubscriptionChangeHandler>,
}

#[cfg(all(feature = "util", feature = "protobuf-wire"))]
impl ComparableSubscriptionChangeHandler {
    fn new(handler: Arc<dyn SubscriptionChangeHandler>) -> Self {
        Self { inner: handler }
    }
}

#[cfg(all(feature = "util", feature = "protobuf-wire"))]
impl Deref for ComparableSubscriptionChangeHandler {
    type Target = dyn SubscriptionChangeHandler;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

#[cfg(all(feature = "util", feature = "protobuf-wire"))]
impl PartialEq for ComparableSubscriptionChangeHandler {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

#[cfg(all(feature = "util", feature = "protobuf-wire"))]
impl Eq for ComparableSubscriptionChangeHandler {}

#[cfg(all(feature = "util", feature = "protobuf-wire"))]
#[derive(Default)]
struct SubscriptionChangeListener {
    subscription_change_handlers: RwLock<HashMap<UUri, ComparableSubscriptionChangeHandler>>,
}

#[cfg(all(feature = "util", feature = "protobuf-wire"))]
impl SubscriptionChangeListener {
    fn add_handler(
        &self,
        topic: UUri,
        subscription_change_handler: Arc<dyn SubscriptionChangeHandler>,
    ) -> Result<(), RegistrationError> {
        let mut handlers = self.subscription_change_handlers.write().map_err(|_| {
            RegistrationError::Unknown(UStatus::fail_with_code(
                UCode::INTERNAL,
                "failed to acquire write lock for handler map",
            ))
        })?;
        let handler_to_add = ComparableSubscriptionChangeHandler::new(subscription_change_handler);
        match handlers.entry(topic) {
            Entry::Vacant(entry) => {
                entry.insert(handler_to_add);
                Ok(())
            }
            Entry::Occupied(entry) if entry.get() == &handler_to_add => Ok(()),
            Entry::Occupied(_) => Err(RegistrationError::AlreadyExists),
        }
    }

    fn remove_handler(&self, topic: &UUri) -> Result<(), RegistrationError> {
        let mut handlers = self.subscription_change_handlers.write().map_err(|_| {
            RegistrationError::Unknown(UStatus::fail_with_code(
                UCode::INTERNAL,
                "failed to acquire write lock for handler map",
            ))
        })?;
        handlers.remove(topic);
        Ok(())
    }

    fn clear(&self) -> Result<(), RegistrationError> {
        let mut handlers = self.subscription_change_handlers.write().map_err(|_| {
            RegistrationError::Unknown(UStatus::fail_with_code(
                UCode::INTERNAL,
                "failed to acquire write lock for handler map",
            ))
        })?;
        handlers.clear();
        Ok(())
    }

    #[cfg(test)]
    fn has_handler(&self, topic: &UUri) -> bool {
        self.subscription_change_handlers
            .read()
            .is_ok_and(|handlers| handlers.contains_key(topic))
    }
}

#[cfg(all(feature = "util", feature = "protobuf-wire"))]
#[async_trait]
impl UOwnedListener for SubscriptionChangeListener {
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        if frame.metadata().attributes().message_type() != UMessageType::Notification {
            return;
        }
        let Some(payload) = payload_from_frame(&frame) else {
            return;
        };
        let Ok(subscription_update) = payload.deserialize::<ProtobufPayload, Update>() else {
            return;
        };
        let Some(topic) = subscription_update.topic.as_ref().map(from_proto_uri) else {
            return;
        };
        let Some(status) = subscription_update.status.as_ref() else {
            return;
        };

        let Ok(handlers) = self.subscription_change_handlers.read() else {
            return;
        };
        if let Some(handler) = handlers.get(&topic) {
            handler.on_subscription_change(topic, status.to_owned());
        }
    }
}

/// A [`Subscriber`] that mirrors main's uSubscription-backed behavior.
#[cfg(all(feature = "util", feature = "protobuf-wire"))]
pub struct InMemorySubscriber<T, S, N>
where
    T: UOwnedTransport + ?Sized,
    S: USubscription + ?Sized,
    N: Notifier + ?Sized,
{
    transport: Arc<T>,
    usubscription: Arc<S>,
    notifier: Arc<N>,
    subscription_change_listener: Arc<SubscriptionChangeListener>,
}

#[cfg(all(feature = "util", feature = "protobuf-wire"))]
impl<T, P> InMemorySubscriber<T, RpcClientUSubscription, SimpleNotifier<T, P>>
where
    T: UOwnedTransport + ?Sized + 'static,
    P: LocalUriProvider + ?Sized + 'static,
{
    pub async fn new(transport: Arc<T>, uri_provider: Arc<P>) -> Result<Self, RegistrationError> {
        let rpc_client = Arc::new(InMemoryRpcClient::new(
            transport.clone(),
            uri_provider.clone(),
        ));
        let usubscription = Arc::new(RpcClientUSubscription::new(rpc_client));
        let notifier = Arc::new(SimpleNotifier::new(transport.clone(), uri_provider));
        Self::for_clients(transport, usubscription, notifier).await
    }
}

#[cfg(all(feature = "util", feature = "protobuf-wire"))]
impl<T, S, N> InMemorySubscriber<T, S, N>
where
    T: UOwnedTransport + ?Sized,
    S: USubscription + ?Sized,
    N: Notifier + ?Sized,
{
    pub async fn for_clients(
        transport: Arc<T>,
        usubscription: Arc<S>,
        notifier: Arc<N>,
    ) -> Result<Self, RegistrationError> {
        let subscription_change_listener = Arc::new(SubscriptionChangeListener::default());
        notifier
            .start_listening(
                &usubscription::usubscription_uri(usubscription::RESOURCE_ID_SUBSCRIPTION_CHANGE),
                subscription_change_listener.clone(),
            )
            .await?;
        Ok(Self {
            transport,
            usubscription,
            notifier,
            subscription_change_listener,
        })
    }

    pub async fn stop(&self) -> Result<(), RegistrationError> {
        self.notifier
            .stop_listening(
                &usubscription::usubscription_uri(usubscription::RESOURCE_ID_SUBSCRIPTION_CHANGE),
                self.subscription_change_listener.clone(),
            )
            .await
            .and_then(|_| self.subscription_change_listener.clear())
    }

    async fn invoke_subscribe(
        &self,
        topic: &UUri,
        subscription_change_handler: Option<Arc<dyn SubscriptionChangeHandler>>,
    ) -> Result<State, RegistrationError> {
        let subscription_request = SubscriptionRequest {
            topic: Some(to_proto_uri(topic)).into(),
            ..Default::default()
        };
        match self.usubscription.subscribe(subscription_request).await {
            Ok(response) if response.is_state(State::SUBSCRIBED) => {
                if let Some(handler) = subscription_change_handler {
                    self.subscription_change_listener
                        .add_handler(topic.to_owned(), handler)?;
                }
                Ok(State::SUBSCRIBED)
            }
            Ok(response) if response.is_state(State::SUBSCRIBE_PENDING) => {
                if let Some(handler) = subscription_change_handler {
                    self.subscription_change_listener
                        .add_handler(topic.to_owned(), handler)?;
                }
                Ok(State::SUBSCRIBE_PENDING)
            }
            Ok(response) => Err(RegistrationError::Unknown(UStatus::fail_with_code(
                UCode::FAILED_PRECONDITION,
                response.status.as_ref().map_or_else(
                    || "unknown subscription state".to_string(),
                    |status| status.message.clone(),
                ),
            ))),
            Err(_) => Err(RegistrationError::Unknown(UStatus::fail_with_code(
                UCode::INTERNAL,
                "failed to invoke USubscription service",
            ))),
        }
    }

    async fn invoke_unsubscribe(&self, topic: &UUri) -> Result<(), RegistrationError> {
        let request = UnsubscribeRequest {
            topic: Some(to_proto_uri(topic)).into(),
            ..Default::default()
        };
        self.usubscription
            .unsubscribe(request)
            .await
            .map(|_| {
                let _ = self.subscription_change_listener.remove_handler(topic);
            })
            .map_err(|_| {
                RegistrationError::Unknown(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    "failed to invoke USubscription service",
                ))
            })
    }

    #[cfg(test)]
    fn has_subscription_change_handler(&self, topic: &UUri) -> bool {
        self.subscription_change_listener.has_handler(topic)
    }
}

#[cfg(all(feature = "util", feature = "protobuf-wire"))]
#[async_trait]
impl<T, S, N> Subscriber for InMemorySubscriber<T, S, N>
where
    T: UOwnedTransport + ?Sized,
    S: USubscription + ?Sized,
    N: Notifier + ?Sized,
{
    async fn subscribe(
        &self,
        topic: &UUri,
        listener: Arc<dyn UOwnedListener>,
        subscription_change_handler: Option<Arc<dyn SubscriptionChangeHandler>>,
    ) -> Result<(), RegistrationError> {
        self.invoke_subscribe(topic, subscription_change_handler)
            .await?;
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
        self.invoke_unsubscribe(topic).await?;
        self.transport
            .unregister_owned_listener(topic, None, listener)
            .await
            .map_err(RegistrationError::from)
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
        _subscription_change_handler: Option<Arc<dyn SubscriptionChangeHandler>>,
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

fn metadata_with_options(
    source: UUri,
    sink: Option<UUri>,
    message_type: UMessageType,
    default_priority: UPriority,
    options: CallOptions,
) -> Result<UFrameMetadata, crate::UAttributesError> {
    let id = options.message_id.unwrap_or_else(UUID::build);
    let priority = options.priority.unwrap_or(default_priority);
    let mut attributes = UAttributes::new(id, source, sink, message_type).with_priority(priority);
    if let Some(ttl) = options.ttl {
        attributes = attributes.with_ttl(ttl);
    }
    if let Some(token) = options.token {
        attributes = attributes.with_token(token);
    }
    let metadata = UFrameMetadata::without_payload_encoding(attributes);
    metadata.validate()?;
    Ok(metadata)
}

#[cfg(feature = "util")]
fn rpc_request_metadata(
    method: UUri,
    reply_to: UUri,
    options: CallOptions,
) -> Result<(UFrameMetadata, UUID, u32), ServiceInvocationError> {
    let ttl = options.ttl.ok_or_else(|| {
        ServiceInvocationError::InvalidArgument("RPC request TTL is required".to_string())
    })?;
    let id = options.message_id.unwrap_or_else(UUID::build);
    let priority = options.priority.unwrap_or(UPriority::CS4);
    let mut builder = UFrameBuilder::request(method, reply_to, ttl)
        .with_message_id(id.clone())
        .with_priority(priority);
    if let Some(token) = options.token {
        builder = builder.with_token(token);
    }
    let metadata = builder
        .build_metadata()
        .map_err(|error| ServiceInvocationError::InvalidArgument(error.to_string()))?;
    Ok((metadata, id, ttl))
}

#[cfg(feature = "util")]
fn payload_from_frame(frame: &UOwnedFrame) -> Option<UPayload> {
    frame.payload().map(|payload| {
        let encoding = frame
            .metadata()
            .encoding()
            .expect("validated payload frame must carry encoding")
            .clone();
        UPayload::new(payload.clone(), encoding)
    })
}

#[cfg(feature = "util")]
fn response_error_status(response: &UOwnedFrame, commstatus: UCode) -> UStatus {
    let message = payload_from_frame(response)
        .map(|payload| String::from_utf8_lossy(payload.payload_bytes()).into_owned())
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| "RPC response indicated failure".to_string());
    UStatus::fail_with_code(commstatus, message)
}

fn frame_from_payload(metadata: UFrameMetadata, payload: Option<UPayload>) -> UOwnedFrame {
    if let Some(payload) = payload {
        let (encoding, bytes) = payload.into_parts();
        UOwnedFrame::new(metadata.with_encoding(encoding), bytes)
    } else {
        UOwnedFrame::without_payload(metadata)
    }
}

#[cfg(all(test, feature = "util", feature = "protobuf-wire"))]
mod tests {
    use super::*;
    use crate::usubscription::MockUSubscription;
    use crate::{MockUOwnedListener, MockUOwnedTransport};

    fn subscription_topic() -> UUri {
        UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap()
    }

    fn succeeding_notifier() -> Arc<MockNotifier> {
        let mut notifier = MockNotifier::new();
        notifier
            .expect_start_listening()
            .once()
            .return_const(Ok(()));
        Arc::new(notifier)
    }

    #[tokio::test]
    async fn in_memory_subscriber_invokes_usubscription_before_registering_listener() {
        let topic = subscription_topic();
        let mut usubscription = MockUSubscription::new();
        let expected_topic = to_proto_uri(&topic);
        usubscription
            .expect_subscribe()
            .once()
            .withf(move |request| request.topic.as_ref() == Some(&expected_topic))
            .returning(|request| {
                Ok(usubscription::SubscriptionResponse {
                    status: Some(usubscription::subscription_status(State::SUBSCRIBED, "")).into(),
                    topic: request.topic,
                    ..Default::default()
                })
            });

        let mut transport = MockUOwnedTransport::new();
        transport
            .expect_do_register_owned_listener()
            .once()
            .return_const(Ok(()));

        let subscriber = InMemorySubscriber::for_clients(
            Arc::new(transport),
            Arc::new(usubscription),
            succeeding_notifier(),
        )
        .await
        .unwrap();
        let listener = Arc::new(MockUOwnedListener::new());
        let handler = Arc::new(MockSubscriptionChangeHandler::new());

        subscriber
            .subscribe(&topic, listener, Some(handler))
            .await
            .unwrap();

        assert!(subscriber.has_subscription_change_handler(&topic));
    }

    #[tokio::test]
    async fn subscription_change_listener_dispatches_protobuf_update() {
        let topic = subscription_topic();
        let status = usubscription::SubscriptionStatus {
            state: protobuf::EnumOrUnknown::from(State::SUBSCRIBED),
            message: "ready".to_string(),
            ..Default::default()
        };
        let update = Update {
            topic: Some(to_proto_uri(&topic)).into(),
            status: Some(status.clone()).into(),
            ..Default::default()
        };
        let payload = UPayload::from_serializable::<ProtobufPayload, _>(&update).unwrap();
        let origin =
            usubscription::usubscription_uri(usubscription::RESOURCE_ID_SUBSCRIPTION_CHANGE);
        let destination = UUri::try_from_parts("client", 0x8000, 0x01, 0x0000).unwrap();
        let frame = frame_from_payload(
            UFrameMetadata::try_notification(origin, destination).unwrap(),
            Some(payload),
        );

        let mut handler = MockSubscriptionChangeHandler::new();
        let expected_topic = topic.clone();
        handler
            .expect_on_subscription_change()
            .once()
            .withf(move |actual_topic, actual_status| {
                actual_topic == &expected_topic && actual_status == &status
            })
            .return_const(());

        let listener = SubscriptionChangeListener::default();
        listener
            .add_handler(topic, Arc::new(handler))
            .expect("handler should register");
        listener.on_receive_owned(frame).await;
    }
}
