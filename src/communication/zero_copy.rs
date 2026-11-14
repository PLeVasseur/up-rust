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

//! Prepared stable-native roles with synchronous sends and generic receive workers.
//!
//! Routes, admission, pending calls and workers are created before traffic. Every
//! received typed lease retains its binding storage and explicit agreement. See
//! the [native loan guide](crate::guide::zero_copy) for the binding contracts.

// Only some feature sets call the extension traits here.
#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
// Internal factory is gated; concrete engines remain independently available.
use crate::{
    Frame, HeaderLimits, InitializedStablePayload, LocalUriProvider, MessageFields,
    NativeProfileAgreement, NativeRequestHandler, PayloadInit, PreparedTransportCore, Registration,
    RegistrationConfig, RegistrationStopError, RouteConfig, RouteDefinition, RpcClientConfig,
    RpcServerConfig, RxLease, SendError, SetupError, StableContainerWire, StablePayload,
    StablePayloadInit, TxRoute, UFrameFieldsMetadataCodec, UHasWire, UMessageType, UPriority, UUri,
    UWireTransport, ZeroCopyHandler,
};
use std::{
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};

pub use crate::{PreparedRpcClient as RpcClient, PreparedRpcServer as RpcServer};

/// Stable-container adapter shared by the prepared communication roles.
pub type StableTransport<C> =
    UWireTransport<Arc<C>, StableContainerWire, UFrameFieldsMetadataCodec>;

/// Checked receive lease carrying a particular stable payload type.
pub type StableRx<C, P> = RxLease<P, <C as PreparedTransportCore>::Rx>;

/// Fixed publisher/notifier metadata and route capacities, chosen at creation.
#[derive(Clone, Copy, Debug, Default)]
pub struct SendConfig {
    /// Default message priority.
    pub priority: Option<UPriority>,
    /// Fixed time to live; absent means no expiration.
    pub ttl: Option<Duration>,
    /// Maximum token length and traceparent policy.
    pub route: RouteConfig,
}

/// Typed receive limits reserved when subscribing or starting a listener.
#[derive(Clone, Copy, Debug)]
pub struct ReceiveConfig {
    /// Inbox capacity and bounded overflow policy.
    pub registration: RegistrationConfig,
    /// Token admission policy; the default header limit accepts 2048 bytes.
    pub header: HeaderLimits,
}

#[cfg(test)]
fn agreement<C: PreparedTransportCore>(
    transport: &StableTransport<C>,
) -> Result<NativeProfileAgreement, SetupError> {
    transport
        .native_agreement()
        .cloned()
        .ok_or(SetupError::InvalidConfig("native agreement required"))
}

/// Factory pairing an explicitly agreed prepared core with local URI construction.
#[cfg(test)]
#[allow(dead_code)] // Test-only fixture factory, absent from the public library.
pub(crate) struct Endpoint<C: PreparedTransportCore, U: LocalUriProvider + ?Sized> {
    transport: Arc<StableTransport<C>>,
    uri_provider: Arc<U>,
}

#[cfg(test)]
#[allow(dead_code)] // Test-only fixture factory, absent from the public library.
impl<C: PreparedTransportCore, U: LocalUriProvider + ?Sized> std::fmt::Debug for Endpoint<C, U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint").finish_non_exhaustive()
    }
}

#[cfg(test)]
#[allow(dead_code)] // Test-only fixture factory, absent from the public library.
impl<C: PreparedTransportCore, U: LocalUriProvider + ?Sized> Endpoint<C, U> {
    /// Creates an endpoint; each role subsequently reserves its own fixed route.
    #[must_use]
    pub fn new(transport: Arc<StableTransport<C>>, uri_provider: Arc<U>) -> Self {
        Self {
            transport,
            uri_provider,
        }
    }

    /// Prepares a typed publisher for one local event resource.
    ///
    /// # Errors
    /// Rejects invalid topics, unagreed types or exhausted route capacity.
    pub fn publisher<P: StablePayloadInit>(
        &self,
        resource_id: u16,
        config: SendConfig,
    ) -> Result<Publisher<C, P>, SetupError> {
        Publisher::new(
            &self.transport,
            self.uri_provider.get_resource_uri(resource_id),
            config,
        )
    }

    /// Prepares a typed notifier for one local event and exact destination.
    ///
    /// # Errors
    /// Rejects invalid addresses, unagreed types or exhausted route capacity.
    pub fn notifier<P: StablePayloadInit>(
        &self,
        resource_id: u16,
        destination: UUri,
        config: SendConfig,
    ) -> Result<Notifier<C, P>, SetupError> {
        Notifier::new(
            &self.transport,
            self.uri_provider.get_resource_uri(resource_id),
            destination,
            config,
        )
    }

    /// Starts a typed notification worker for this endpoint's source URI.
    ///
    /// # Errors
    /// Rejects an invalid topic, missing agreement or unavailable registration.
    pub async fn listen<P, H>(
        &self,
        topic: UUri,
        handler: H,
        config: ReceiveConfig,
    ) -> Result<Receiver<C>, SetupError>
    where
        P: StablePayload,
        H: ZeroCopyHandler<P, C::Rx>,
    {
        Receiver::start(
            &self.transport,
            topic,
            Some(self.uri_provider.get_source_uri()),
            UMessageType::Notification,
            handler,
            config,
        )
        .await
    }

    /// Reserves a typed RPC client before its first request.
    ///
    /// # Errors
    /// Returns address, agreement, pending-table or route-capacity failures.
    pub async fn rpc_client<Req: StablePayloadInit, Resp: StablePayload>(
        &self,
        method: UUri,
        config: RpcClientConfig,
    ) -> Result<RpcClient<C, Req, Resp>, SetupError> {
        RpcClient::new(
            Arc::clone(self.transport.core()),
            agreement(&self.transport)?,
            self.uri_provider.get_source_uri(),
            method,
            config,
        )
        .await
    }

    /// Reserves a request worker and response-route cache for one local method.
    ///
    /// # Errors
    /// Rejects invalid method addresses, unagreed types or unavailable capacity.
    pub async fn rpc_server<Req, Resp, H>(
        &self,
        resource_id: u16,
        handler: H,
        config: RpcServerConfig,
    ) -> Result<RpcServer<C, Resp>, SetupError>
    where
        Req: StablePayload,
        Resp: StablePayloadInit,
        H: NativeRequestHandler<Req, Resp, C::Rx>,
    {
        RpcServer::start::<Req, H>(
            Arc::clone(self.transport.core()),
            agreement(&self.transport)?,
            self.uri_provider.get_resource_uri(resource_id),
            handler,
            config,
        )
        .await
    }

    /// Creates a subscription-service facade; each subscription owns its worker.
    #[cfg(all(test, feature = "usubscription"))]
    #[must_use]
    pub fn subscriber(
        &self,
        service: Arc<dyn crate::core::usubscription::USubscription>,
    ) -> Subscriber<C> {
        Subscriber {
            transport: Arc::clone(&self.transport),
            service,
        }
    }
}

/// Publisher holding a fixed, typed stable-native transmit route.
#[derive(Debug)]
pub struct Publisher<C: PreparedTransportCore, P> {
    route: TxRoute<C, P>,
}

impl<C: PreparedTransportCore, P: StablePayloadInit> Publisher<C, P> {
    /// Resolves a topic, payload identity and route capacities before publication.
    ///
    /// # Errors
    /// Rejects invalid topics, unagreed types or exhausted route capacity.
    pub fn new(
        transport: &StableTransport<C>,
        topic: UUri,
        config: SendConfig,
    ) -> Result<Self, SetupError> {
        let route = transport.prepare_stable_route(
            RouteDefinition {
                kind: UMessageType::Publish,
                source: topic,
                sink: None,
                priority: config.priority,
                ttl: config.ttl,
                permission_level: None,
            },
            config.route,
        )?;
        Ok(Self { route })
    }

    /// Uses a route from an already prepared template without another setup allocation.
    ///
    /// # Errors
    /// Rejects a route whose kind is not Publish.
    pub fn from_route(route: TxRoute<C, P>) -> Result<Self, SetupError> {
        if route.definition().kind != UMessageType::Publish {
            return Err(SetupError::InvalidRoute("publisher route kind"));
        }
        Ok(Self { route })
    }

    /// Initializes and publishes in existing storage, without an asynchronous TX loan.
    ///
    /// # Errors
    /// Returns fixed-size override, storage, initialization or delivery failures.
    pub fn publish_stable<F>(
        &self,
        fields: MessageFields<'_>,
        initialize: F,
    ) -> Result<(), SendError>
    where
        F: for<'a> FnOnce(PayloadInit<'a, P>) -> InitializedStablePayload<'a, P>,
    {
        self.route.send(fields, initialize)
    }
}

/// Notifier holding an event source and exact destination in a prepared route.
#[derive(Debug)]
pub struct Notifier<C: PreparedTransportCore, P> {
    route: TxRoute<C, P>,
}

impl<C: PreparedTransportCore, P: StablePayloadInit> Notifier<C, P> {
    /// Resolves the notification route and payload identity once at setup.
    ///
    /// # Errors
    /// Rejects invalid addresses, unagreed types or exhausted route capacity.
    pub fn new(
        transport: &StableTransport<C>,
        topic: UUri,
        destination: UUri,
        config: SendConfig,
    ) -> Result<Self, SetupError> {
        let route = transport.prepare_stable_route(
            RouteDefinition {
                kind: UMessageType::Notification,
                source: topic,
                sink: Some(destination),
                priority: config.priority,
                ttl: config.ttl,
                permission_level: None,
            },
            config.route,
        )?;
        Ok(Self { route })
    }

    /// Initializes and sends a notification synchronously in the route's storage.
    ///
    /// # Errors
    /// Returns fixed-size override, storage, initialization or delivery failures.
    pub fn notify_stable<F>(
        &self,
        fields: MessageFields<'_>,
        initialize: F,
    ) -> Result<(), SendError>
    where
        F: for<'a> FnOnce(PayloadInit<'a, P>) -> InitializedStablePayload<'a, P>,
    {
        self.route.send(fields, initialize)
    }
}

/// Decoded-address rejections at a typed role's receive boundary.
#[derive(Debug, Default)]
pub struct ReceiverStats {
    /// Typed frames whose decoded kind/source/sink do not match this role.
    pub filtered: AtomicU64,
}

#[cfg(any(test, feature = "typed-communication"))]
struct FilteredHandler<H> {
    source: UUri,
    sink: Option<UUri>,
    kind: UMessageType,
    stats: Arc<ReceiverStats>,
    handler: H,
}
#[cfg(any(test, feature = "typed-communication"))]
impl<P: StablePayload, F: Frame, H: ZeroCopyHandler<P, F>> ZeroCopyHandler<P, F>
    for FilteredHandler<H>
{
    async fn on_receive(&mut self, lease: RxLease<P, F>) {
        let matches = lease.header().is_ok_and(|header| {
            header.kind == self.kind
                && self.source.matches(&header.source)
                && match (&self.sink, &header.sink) {
                    (None, None) => true,
                    (Some(filter), Some(sink)) => filter.matches(sink),
                    _ => false,
                }
        });
        if matches {
            self.handler.on_receive(lease).await;
        } else {
            self.stats
                .filtered
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// Failure while closing physical delivery or joining a receive worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ReceiverStopError {
    /// The binding could not detach this registration.
    #[error("receiver detachment failed: {0}")]
    Detach(SetupError),
    /// The registration worker could not finish draining.
    #[error("receiver drain failed: {0}")]
    Worker(RegistrationStopError),
}

impl ReceiverStopError {
    /// Recovery classification for a failed receiver stop.
    ///
    /// A terminal worker failure is [`Completion::Failed`](crate::Completion::Failed).
    /// Detach failures and same-worker refusals stay
    /// [`Completion::Pending`](crate::Completion::Pending): the exact owners are retained
    /// and the stop can be retried. Pass it directly to cleanup tracking, for example
    /// `progress.record(result, ReceiverStopError::completion)`.
    ///
    /// ```
    /// use up_rust::communication::zero_copy::ReceiverStopError;
    /// use up_rust::{Completion, RegistrationStopError, SetupError};
    /// let panicked = ReceiverStopError::Worker(RegistrationStopError::HandlerPanicked);
    /// assert_eq!(panicked.completion(), Completion::Failed);
    /// let refused = ReceiverStopError::Worker(RegistrationStopError::CalledFromWorker);
    /// assert_eq!(refused.completion(), Completion::Pending);
    /// assert_eq!(ReceiverStopError::Detach(SetupError::Worker).completion(), Completion::Pending);
    /// ```
    pub fn completion(&self) -> crate::Completion {
        match self {
            Self::Worker(error) if error.is_terminal() => crate::Completion::Failed,
            Self::Detach(_) | Self::Worker(_) => crate::Completion::Pending,
        }
    }
}

/// One physical subscription and one generic admission/handler worker.
///
/// Stop closes admission, detaches physical delivery and drains accepted frames.
/// Drop instead closes/detaches and aborts the worker. Retained application frames
/// keep their independent storage owners under either operation.
pub struct Receiver<C: PreparedTransportCore> {
    core: Arc<C>,
    subscription: Option<C::Subscription>,
    registration: Option<Registration<C::Rx>>,
    worker_outcome: Option<Result<(), RegistrationStopError>>,
    progress: crate::StopProgress<SetupError>,
    stats: Arc<ReceiverStats>,
}
impl<C: PreparedTransportCore> std::fmt::Debug for Receiver<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Receiver").finish_non_exhaustive()
    }
}
impl<C: PreparedTransportCore> Receiver<C> {
    #[cfg(any(test, feature = "typed-communication"))]
    pub(crate) async fn start<P: StablePayload, H: ZeroCopyHandler<P, C::Rx>>(
        transport: &StableTransport<C>,
        source: UUri,
        sink: Option<UUri>,
        kind: UMessageType,
        handler: H,
        config: ReceiveConfig,
    ) -> Result<Self, SetupError> {
        source
            .verify_event()
            .map_err(|_| SetupError::InvalidRoute("event source"))?;
        if let Some(sink) = &sink {
            sink.verify_no_wildcards()
                .map_err(|_| SetupError::InvalidRoute("notification destination"))?;
        }
        let admission = transport
            .prepare_stable_receiver::<P>(config.header)
            .map_err(|_| SetupError::UnknownPayloadType)?;
        let stats = Arc::new(ReceiverStats::default());
        let worker = FilteredHandler {
            source: source.clone(),
            sink: sink.clone(),
            kind,
            stats: Arc::clone(&stats),
            handler,
        };
        let registration = admission
            .register(config.registration, worker, || {})
            .map_err(|_| SetupError::Worker)?;
        let core = Arc::clone(transport.core());
        let subscription = core
            .attach(&source, sink.as_ref(), registration.inbox())
            .await?;
        Ok(Self {
            core,
            subscription: Some(subscription),
            registration: Some(registration),
            worker_outcome: None,
            progress: crate::StopProgress::default(),
            stats,
        })
    }

    /// Closes local admission synchronously, without detaching or draining.
    pub fn close_admission(&self) {
        if let Some(r) = &self.registration {
            r.inbox().close();
        }
    }
    pub(crate) fn detach_ref(&mut self) -> Result<(), SetupError> {
        if let Some(subscription) = &mut self.subscription {
            self.core.detach(subscription)?;
        }
        self.subscription.take();
        Ok(())
    }
    pub(crate) async fn drain_ref(&mut self) -> Result<(), RegistrationStopError> {
        if let Some(outcome) = self.worker_outcome {
            return outcome;
        }
        let outcome = if let Some(registration) = &mut self.registration {
            registration.stop_ref().await
        } else {
            Ok(())
        };
        if outcome == Err(RegistrationStopError::CalledFromWorker) {
            return outcome;
        }
        self.worker_outcome = Some(outcome);
        self.registration.take();
        outcome
    }

    /// Exact first failures and current attempt, retained across cancellation.
    pub fn stop_progress(&self) -> &crate::StopProgress<SetupError> {
        &self.progress
    }
    /// Decoded-address rejection counters for this role.
    #[must_use]
    pub fn stats(&self) -> &ReceiverStats {
        &self.stats
    }

    /// Closes admission and attempts exact detach before returning the drain future.
    ///
    /// Failed detach retains the original attachment for retry. Cancelling the
    /// returned future retains the pending worker; a completed worker failure is
    /// sticky across every retry. Independently retained RX leases remain valid.
    ///
    /// # Errors
    /// Reports detach first, then the retained worker failure once detach succeeds.
    /// Same-worker drain returns nonterminal `CalledFromWorker`; the join remains
    /// available for external retry and is never recorded as a completed outcome.
    pub fn stop_ref(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), ReceiverStopError>> + Send + '_ {
        self.close_admission();
        let detached = self.detach_ref();
        self.progress.begin(detached.err());
        async move {
            let drained = self.drain_ref().await;
            self.progress.finish(drained.err());
            detached.map_err(ReceiverStopError::Detach)?;
            drained.map_err(ReceiverStopError::Worker)
        }
    }

    /// Borrowed ordinary shutdown, eager even when its returned future is dropped.
    ///
    /// Cancelling the returned future retains the receiver, failed attachment
    /// and pending worker for retry, just like [`Self::stop_ref`].
    ///
    /// # Errors
    /// Reports physical detachment or worker termination failure.
    pub fn stop(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), ReceiverStopError>> + Send + '_ {
        self.stop_ref()
    }
}
impl<C: PreparedTransportCore> Drop for Receiver<C> {
    fn drop(&mut self) {
        if let Some(registration) = &self.registration {
            registration.inbox().close();
        }
        if let Some(subscription) = &mut self.subscription {
            let _ = self.core.detach(subscription);
        }
    }
}

// Test-only service-subscription engine, like `Endpoint`: nothing outside this
// module's tests can construct it. Applications subscribe through the typed and
// application roles (`typed::NativeSubscription`), whose ticket prepares local
// state before remote activation and is retained for reconciliation.

/// Subscription setup/teardown outcome, including service rollback failures.
#[cfg(all(test, feature = "usubscription"))]
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum SubscriptionError {
    /// The subscription service refused or failed the setup request.
    #[error("subscription service failed: {0:?}")]
    Service(crate::UCode),
    /// Local setup failed after the service accepted; rollback was attempted.
    #[error("local subscription setup failed: {cause}; rollback: {rollback:?}")]
    Setup {
        /// Original local setup failure.
        cause: SetupError,
        /// A failed service rollback, if any.
        rollback: Option<crate::UCode>,
    },
    /// Unsubscription always attempts both service removal and local drain.
    #[error("unsubscribe failed; service: {service:?}; receiver: {receiver:?}")]
    Stop {
        /// A failed service removal, if any.
        service: Option<crate::UCode>,
        /// A failed local detach/drain, if any.
        receiver: Option<ReceiverStopError>,
    },
}

/// Subscriber coordinating setup-time service calls with local typed registrations.
#[cfg(all(test, feature = "usubscription"))]
pub(crate) struct Subscriber<C: PreparedTransportCore> {
    transport: Arc<StableTransport<C>>,
    service: Arc<dyn crate::core::usubscription::USubscription>,
}
#[cfg(all(test, feature = "usubscription"))]
impl<C: PreparedTransportCore> std::fmt::Debug for Subscriber<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscriber").finish_non_exhaustive()
    }
}
#[cfg(all(test, feature = "usubscription"))]
impl<C: PreparedTransportCore> Subscriber<C> {
    /// Contacts the service, then reserves local admission and one generic worker.
    ///
    /// # Errors
    /// Reports service or local setup failure. Local failure attempts a service
    /// rollback and preserves both outcomes if that rollback also fails.
    pub async fn subscribe<P, H>(
        &self,
        topic: UUri,
        handler: H,
        config: ReceiveConfig,
    ) -> Result<Subscription<C>, SubscriptionError>
    where
        P: StablePayload,
        H: ZeroCopyHandler<P, C::Rx>,
    {
        topic.verify_event().map_err(|_| SubscriptionError::Setup {
            cause: SetupError::InvalidRoute("event topic"),
            rollback: None,
        })?;
        topic
            .verify_no_wildcards()
            .map_err(|_| SubscriptionError::Setup {
                cause: SetupError::InvalidRoute("exact event topic"),
                rollback: None,
            })?;
        if config.registration.capacity == 0 {
            return Err(SubscriptionError::Setup {
                cause: SetupError::InvalidConfig("receive capacity must be positive"),
                rollback: None,
            });
        }
        // Refuse absent agreement/type capability before changing remote state.
        self.transport
            .prepare_stable_receiver::<P>(config.header)
            .map_err(|_| SubscriptionError::Setup {
                cause: SetupError::UnknownPayloadType,
                rollback: None,
            })?;
        let status = self
            .service
            .subscribe(&topic, None, None)
            .await
            .map_err(|error| SubscriptionError::Service(error.code()))?;
        if !matches!(
            status,
            super::SubscriptionStatus::Subscribed | super::SubscriptionStatus::SubscribePending
        ) {
            return Err(SubscriptionError::Service(crate::UCode::FailedPrecondition));
        }
        match Receiver::start(
            &self.transport,
            topic.clone(),
            None,
            UMessageType::Publish,
            handler,
            config,
        )
        .await
        {
            Ok(receiver) => Ok(Subscription {
                receiver: Some(receiver),
                topic,
                service: Arc::clone(&self.service),
            }),
            Err(cause) => {
                let rollback = self
                    .service
                    .unsubscribe(&topic)
                    .await
                    .err()
                    .map(|error| error.code());
                Err(SubscriptionError::Setup { cause, rollback })
            }
        }
    }
}

/// Service subscription with an owned local receive registration.
///
/// Call unsubscribe to remove the service subscription as well as drain the local
/// worker. Drop can only close local delivery; it cannot perform asynchronous
/// service cleanup. Cancellation has the same local-close behavior.
#[cfg(all(test, feature = "usubscription"))]
pub(crate) struct Subscription<C: PreparedTransportCore> {
    receiver: Option<Receiver<C>>,
    topic: UUri,
    service: Arc<dyn crate::core::usubscription::USubscription>,
}
#[cfg(all(test, feature = "usubscription"))]
impl<C: PreparedTransportCore> std::fmt::Debug for Subscription<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscription")
            .field("topic", &self.topic)
            .finish_non_exhaustive()
    }
}
#[cfg(all(test, feature = "usubscription"))]
impl<C: PreparedTransportCore> Subscription<C> {
    /// Removes the service subscription and drains local delivery even on failure.
    ///
    /// # Errors
    /// Retains both service and local shutdown outcomes.
    pub async fn unsubscribe(mut self) -> Result<(), SubscriptionError> {
        let service = self
            .service
            .unsubscribe(&self.topic)
            .await
            .err()
            .map(|error| error.code());
        let receiver = if let Some(mut receiver) = self.receiver.take() {
            receiver.stop().await.err()
        } else {
            None
        };
        if service.is_some() || receiver.is_some() {
            Err(SubscriptionError::Stop { service, receiver })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        NativeProfile, OverflowPolicy, PayloadEncoding, PayloadLoanProvenance, PoolCore,
        PoolCoreConfig, PoolFrame, StaticUriProvider,
    };
    use std::sync::Mutex;

    #[repr(C)]
    #[derive(Debug, crate::StablePayload, crate::StablePayloadInit)]
    #[stable_payload(type_name = "uprotocol.test.ZeroCopyRolePayload")]
    struct Value {
        bytes: [u8; 4],
    }
    #[repr(C)]
    #[derive(Debug, crate::StablePayload, crate::StablePayloadInit)]
    #[stable_payload(type_name = "uprotocol.test.ZeroCopyRoleReply")]
    struct Reply {
        value: u64,
    }

    type RoleEndpoint = Endpoint<PoolCore, StaticUriProvider>;
    fn setup() -> (Arc<PoolCore>, RoleEndpoint, RoleEndpoint) {
        let agreement = crate::test_support::self_agreement(
            NativeProfile::new(
                "native-roles",
                1,
                PayloadEncoding::private_use(0xF201),
                [
                    Value::native_representation(),
                    Reply::native_representation(),
                ],
            )
            .unwrap(),
        );
        let core = Arc::new(
            PoolCore::new(PoolCoreConfig {
                slots: 8,
                slot_bytes: 4096,
                alignment: 8,
                max_routes: 8,
                max_registrations: 4,
            })
            .unwrap(),
        );
        let transport = Arc::new(UWireTransport::with_native_agreement(
            Arc::clone(&core),
            StableContainerWire,
            UFrameFieldsMetadataCodec,
            agreement,
        ));
        let sender = Endpoint::new(
            Arc::clone(&transport),
            Arc::new(StaticUriProvider::new("vehicle", 0x0001_0001, 1).unwrap()),
        );
        let receiver = Endpoint::new(
            transport,
            Arc::new(StaticUriProvider::new("vehicle", 0x0001_0002, 1).unwrap()),
        );
        (core, sender, receiver)
    }
    fn config() -> ReceiveConfig {
        ReceiveConfig {
            registration: RegistrationConfig {
                capacity: 4,
                overflow: OverflowPolicy::DropNewest,
            },
            header: HeaderLimits::default(),
        }
    }
    struct Capture(Arc<Mutex<Vec<StableRx<PoolCore, Value>>>>);
    impl ZeroCopyHandler<Value, PoolFrame> for Capture {
        async fn on_receive(&mut self, frame: StableRx<PoolCore, Value>) {
            tokio::task::yield_now().await;
            self.0.lock().unwrap().push(frame);
        }
    }
    fn fill(init: PayloadInit<'_, Value>) -> InitializedStablePayload<'_, Value> {
        init.into_initializer()
            .bytes_from_slice(b"role")
            .unwrap()
            .finish()
    }
    fn assert_value(frame: &StableRx<PoolCore, Value>) {
        assert_eq!(frame.payload().unwrap().bytes, *b"role");
        assert_eq!(
            frame.provenance(),
            Some(PayloadLoanProvenance::OwnedReceiveLease)
        );
    }

    #[tokio::test]
    async fn notifier_roundtrip_retains_value_metadata_and_storage_after_stop() {
        let (core, sender, receiver) = setup();
        let frames = Arc::new(Mutex::new(Vec::new()));
        let topic = sender.uri_provider.get_resource_uri(0x8002);
        let destination = receiver.uri_provider.get_source_uri();
        let mut listener = receiver
            .listen(topic.clone(), Capture(Arc::clone(&frames)), config())
            .await
            .unwrap();
        let notifier = sender
            .notifier::<Value>(0x8002, destination.clone(), SendConfig::default())
            .unwrap();
        let id = crate::UUID::build();
        notifier
            .notify_stable(
                MessageFields {
                    id: Some(&id),
                    ..MessageFields::default()
                },
                fill,
            )
            .unwrap();
        listener.stop().await.unwrap();
        drop((notifier, sender, receiver, core));
        let frames = frames.lock().unwrap();
        assert_eq!(frames.len(), 1);
        let frame = frames.first().unwrap();
        assert_value(frame);
        let header = frame.header().unwrap();
        assert_eq!(header.id, id);
        assert_eq!(header.source, topic);
        assert_eq!(header.sink, Some(destination));
        assert_eq!(header.kind, UMessageType::Notification);
    }

    #[cfg(feature = "usubscription")]
    #[tokio::test]
    async fn subscriber_roundtrip_preserves_values_and_service_lifecycle() {
        let (core, sender, receiver) = setup();
        let frames = Arc::new(Mutex::new(Vec::new()));
        let mut service = crate::core::usubscription::MockUSubscription::new();
        service
            .expect_subscribe()
            .times(1)
            .returning(|_, _, _| Ok(super::super::SubscriptionStatus::Subscribed));
        service.expect_unsubscribe().times(1).returning(|_| Ok(()));
        let subscriber = receiver.subscriber(Arc::new(service));
        let topic = sender.uri_provider.get_resource_uri(0x8001);
        let subscription = subscriber
            .subscribe(topic.clone(), Capture(Arc::clone(&frames)), config())
            .await
            .unwrap();
        let publisher = sender
            .publisher::<Value>(
                0x8001,
                SendConfig {
                    ttl: Some(Duration::from_millis(250)),
                    ..SendConfig::default()
                },
            )
            .unwrap();
        publisher
            .publish_stable(MessageFields::default(), fill)
            .unwrap();
        subscription.unsubscribe().await.unwrap();
        let mut frames = frames.lock().unwrap();
        assert_eq!(frames.len(), 1);
        let frame = frames.first().unwrap();
        assert_value(frame);
        let header = frame.header().unwrap();
        assert_eq!(header.source, topic);
        assert_eq!(header.sink, None);
        assert_eq!(header.ttl, Some(Duration::from_millis(250)));
        assert_eq!(header.kind, UMessageType::Publish);
        frames.clear();
        assert_eq!(core.free_slots(), 8);
    }

    #[cfg(feature = "usubscription")]
    #[tokio::test]
    async fn local_setup_failure_rolls_back_service_subscription() {
        let (_, sender, receiver) = setup();
        let mut service = crate::core::usubscription::MockUSubscription::new();
        service
            .expect_subscribe()
            .times(1)
            .returning(|_, _, _| Ok(super::super::SubscriptionStatus::Subscribed));
        service.expect_unsubscribe().times(1).returning(|_| {
            Err(crate::UStatus::fail_with_code(
                crate::UCode::Unavailable,
                "rollback unavailable",
            ))
        });
        let subscriber = receiver.subscriber(Arc::new(service));
        let mut config = config();
        // Zero is rejected before remote mutation; overflow exercises post-call setup rollback.
        config.registration.capacity = usize::MAX;
        let result = subscriber
            .subscribe(
                sender.uri_provider.get_resource_uri(0x8001),
                Capture(Arc::new(Mutex::new(Vec::new()))),
                config,
            )
            .await;
        assert!(matches!(
            result,
            Err(SubscriptionError::Setup {
                cause: SetupError::Worker,
                rollback: Some(crate::UCode::Unavailable)
            })
        ));
    }

    struct Responder;
    impl NativeRequestHandler<Value, Reply, PoolFrame> for Responder {
        type Reply = u64;
        async fn handle(
            &mut self,
            request: StableRx<PoolCore, Value>,
        ) -> Result<u64, crate::UCode> {
            assert_value(&request);
            assert_eq!(request.header().unwrap().token(), Some("credential"));
            tokio::task::yield_now().await;
            Ok(42)
        }
        fn write_reply<'a>(
            &mut self,
            value: u64,
            init: <Reply as StablePayloadInit>::Initializer<'a>,
        ) -> InitializedStablePayload<'a, Reply> {
            init.value(value).finish()
        }
    }
    #[tokio::test]
    async fn rpc_roles_use_distinct_types_and_correlated_retained_reply() {
        let (core, sender, receiver) = setup();
        let mut server = receiver
            .rpc_server::<Value, Reply, _>(
                0x42,
                Responder,
                RpcServerConfig {
                    registration: config().registration,
                    header: HeaderLimits::default(),
                    max_clients: 1,
                },
            )
            .await
            .unwrap();
        let client = sender
            .rpc_client::<Value, Reply>(
                receiver.uri_provider.get_resource_uri(0x42),
                RpcClientConfig {
                    max_in_flight: 1,
                    ttl: Duration::from_secs(1),
                    max_token_len: 2048,
                    priority: UPriority::CS4,
                },
            )
            .await
            .unwrap();
        client.set_token("credential").unwrap();
        let reply = client.call(fill).await.unwrap();
        assert_eq!(reply.payload().unwrap().value, 42);
        assert!(reply.header().unwrap().reqid.is_some());
        assert_eq!(
            reply.provenance(),
            Some(PayloadLoanProvenance::OwnedReceiveLease)
        );
        drop(client);
        server.stop().await.unwrap();
        drop((sender, receiver, core));
        assert_eq!(reply.payload().unwrap().value, 42);
    }

    #[test_case::test_case(0; "response resource")]
    #[test_case::test_case(0x42; "RPC method resource")]
    #[test_case::test_case(0xFFFF; "wildcard resource")]
    #[tokio::test]
    async fn invalid_event_resources_reject_before_route_or_listener(resource: u16) {
        let (core, sender, receiver) = setup();
        assert!(matches!(
            sender.publisher::<Value>(resource, SendConfig::default()),
            Err(SetupError::InvalidRoute(_))
        ));
        assert!(matches!(
            receiver
                .listen(
                    sender.uri_provider.get_resource_uri(resource),
                    Capture(Arc::new(Mutex::new(Vec::new()))),
                    config()
                )
                .await,
            Err(SetupError::InvalidRoute(_))
        ));
        assert_eq!(core.free_slots(), 8);
    }
}
