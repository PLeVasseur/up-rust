use std::{marker::PhantomData, sync::Arc};

use super::{
    Delivery, Endpoint, Handler, MessageContext, Notifier, PublishOptions, Publisher, Receiver,
    RequestHandler, RpcClient, RpcOptions, RpcServer, ServerOptions, Subscriber, Subscription,
};
use crate::{
    communication::zero_copy::{self as engine, ReceiveConfig, SendConfig, StableTransport},
    AdmissionError, Frame, HeaderLimits, InitializedStablePayload, LocalUriProvider, MessageFields,
    NativeLoanCore, OverflowPolicy, PayloadInit, RegistrationConfig, RouteConfig, RxLease,
    SendError, SetupError, StablePayload, StablePayloadInit, UHasWire, UUri, ZeroCopyHandler,
};

/// Native-only capacities, explicitly reserved during role setup.
#[derive(Clone, Copy, Debug)]
pub struct NativeConfig {
    /// Reserved native TX header token/trace capacities.
    pub route: RouteConfig,
    /// Bounded receive admission and inbox policy.
    pub receive: ReceiveConfig,
}

impl Default for NativeConfig {
    /// The same bounded limits as serialized `DeploymentOptions`: 256 reserved token
    /// bytes in both directions and a 64-entry DropNewest receive queue.
    fn default() -> Self {
        Self {
            route: RouteConfig {
                max_token_len: 256,
                allow_traceparent: false,
            },
            receive: ReceiveConfig {
                registration: RegistrationConfig {
                    capacity: 64,
                    overflow: OverflowPolicy::DropNewest,
                },
                header: HeaderLimits { max_token_len: 256 },
            },
        }
    }
}

/// Requires the actual prepared native core, never an owned-transport fallback.
///
/// A serialized-only transport cannot become native by changing configuration:
///
/// ```compile_fail
/// use std::sync::Arc;
/// use up_rust::communication::typed::{Native, NativeConfig};
/// use up_rust::UOwnedTransport;
/// fn unsupported<T: UOwnedTransport>(transport: Arc<T>, config: NativeConfig) {
///     let _ = Native::new(transport, config);
/// }
/// ```
/// Generic prepared-pool conformance cannot grant physical admission:
///
/// ```compile_fail
/// use std::sync::Arc;
/// use up_rust::{PoolCore, NativeProfileAgreement, StableContainerWire, UFrameFieldsMetadataCodec, UWireTransport};
/// use up_rust::communication::typed::{Native, NativeConfig};
/// fn unsupported(core: Arc<PoolCore>, agreement: NativeProfileAgreement, config: NativeConfig) {
///     let adapter = Arc::new(UWireTransport::with_native_agreement(core, StableContainerWire, UFrameFieldsMetadataCodec, agreement));
///     let _ = Native::new(adapter, config);
/// }
/// ```
pub struct Native<C: NativeLoanCore> {
    transport: Arc<StableTransport<C>>,
    config: NativeConfig,
}

impl<C: NativeLoanCore> Clone for Native<C> {
    fn clone(&self) -> Self {
        Self {
            transport: Arc::clone(&self.transport),
            config: self.config,
        }
    }
}

impl<C: NativeLoanCore> Native<C> {
    /// Admits the configured adapter's independently established native agreement.
    ///
    /// # Errors
    /// Missing agreement is a setup failure. Each role still checks exact payload
    /// membership and reserves binding capacity through the existing engine.
    pub fn new(
        transport: Arc<StableTransport<C>>,
        config: NativeConfig,
    ) -> Result<Self, SetupError> {
        if transport.native_agreement().is_none() {
            return Err(SetupError::InvalidConfig("native agreement required"));
        }
        if config.receive.registration.capacity == 0 {
            return Err(SetupError::InvalidConfig(
                "receive capacity must be positive",
            ));
        }
        Ok(Self { transport, config })
    }
}

impl<P: StablePayload, F: Frame> Delivery<P> for RxLease<P, F> {
    type Error = AdmissionError;

    fn payload(&self) -> Result<&P, Self::Error> {
        RxLease::payload(self)
    }

    fn context(&self) -> Result<MessageContext<'_>, Self::Error> {
        let h = self.header()?;
        let token = h.token();
        let traceparent = h.traceparent();
        Ok(MessageContext {
            kind: h.kind,
            source: h.source,
            sink: h.sink,
            id: h.id,
            request_id: h.reqid,
            comm_status: h.comm_status,
            payload_encoding: h.payload_encoding,
            native_type_token: h.native_type_token,
            token,
            permission_level: h.permission_level,
            priority: h.priority,
            ttl: h.ttl,
            traceparent,
        })
    }
}

struct NativeHandler<H>(H);

impl<P, F, H> ZeroCopyHandler<P, F> for NativeHandler<H>
where
    P: StablePayload,
    F: Frame,
    H: Handler<P>,
{
    async fn on_receive(&mut self, delivery: RxLease<P, F>) {
        self.0.on_receive(delivery).await;
    }
}

impl<C: NativeLoanCore, U: LocalUriProvider + ?Sized> Endpoint<Native<C>, U> {
    /// Prepares the existing typed publisher's exact identity and physical route.
    ///
    /// # Errors
    /// Unagreed types, invalid addressing and unavailable capacity fail at setup.
    pub fn publisher<P: StablePayloadInit>(
        &self,
        resource_id: u16,
        options: PublishOptions,
    ) -> Result<Publisher<P, engine::Publisher<C, P>>, SetupError> {
        let inner = engine::Publisher::new(
            &self.setup.transport,
            self.uri_provider.get_resource_uri(resource_id),
            SendConfig {
                priority: options.priority,
                ttl: options.ttl,
                route: self.setup.config.route,
            },
        )?;
        Ok(Publisher {
            inner,
            payload: PhantomData,
        })
    }
}

impl<P: StablePayloadInit, C: NativeLoanCore> Publisher<P, engine::Publisher<C, P>> {
    /// Constructs directly in the middleware TX loan and publishes synchronously.
    ///
    /// # Errors
    /// Returns the unchanged native engine's bounded storage/initialization errors.
    pub fn publish_in_place<F>(&self, initialize: F) -> Result<(), SendError>
    where
        F: for<'a> FnOnce(PayloadInit<'a, P>) -> InitializedStablePayload<'a, P>,
    {
        self.publish_in_place_with(MessageFields::default(), initialize)
    }

    /// Uses existing per-message overrides without extending the TX loan lifetime.
    ///
    /// # Errors
    /// Returns native field-capacity, storage, initialization or delivery errors.
    pub fn publish_in_place_with<F>(
        &self,
        fields: MessageFields<'_>,
        initialize: F,
    ) -> Result<(), SendError>
    where
        F: for<'a> FnOnce(PayloadInit<'a, P>) -> InitializedStablePayload<'a, P>,
    {
        self.inner.publish_stable(fields, initialize)
    }
}

/// Native subscription local state; generic lifecycle owns remote reconciliation.
#[doc(hidden)]
pub struct NativeSubscription<C: NativeLoanCore> {
    receiver: engine::Receiver<C>,
    active: Arc<std::sync::atomic::AtomicBool>,
}
impl<C: NativeLoanCore> std::fmt::Debug for NativeSubscription<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeSubscription").finish_non_exhaustive()
    }
}
impl<C: NativeLoanCore> Subscription<NativeSubscription<C>> {
    /// Native decoded-address rejection counters retained with the ticket.
    pub fn stats(&self) -> &engine::ReceiverStats {
        self.inner.receiver.stats()
    }
}
impl<C: NativeLoanCore> super::SubscriptionLocal for NativeSubscription<C> {
    type Error = SetupError;
    fn set_active(&self, active: bool) {
        self.active
            .store(active, std::sync::atomic::Ordering::Release);
    }
    fn close_admission(&self) {
        self.receiver.close_admission();
    }
    fn detach(&mut self) -> Result<(), SetupError> {
        self.receiver.detach_ref()
    }
    async fn drain(&mut self) -> Result<(), crate::RegistrationStopError> {
        self.receiver.drain_ref().await
    }
}
struct Gated<H> {
    handler: NativeHandler<H>,
    active: Arc<std::sync::atomic::AtomicBool>,
}
impl<P: StablePayload, F: Frame, H: Handler<P>> ZeroCopyHandler<P, F> for Gated<H> {
    async fn on_receive(&mut self, lease: RxLease<P, F>) {
        if self.active.load(std::sync::atomic::Ordering::Acquire) {
            self.handler.on_receive(lease).await;
        }
    }
}
impl<C: NativeLoanCore> Subscriber<Native<C>> {
    /// Prepares native receive capability before any remote side effect. Activate
    /// the returned ticket by mutable borrow; cancellation retains remote state.
    ///
    /// # Errors
    /// Refuses invalid topic, missing agreement/type or physical receive capacity.
    pub async fn subscription<P: StablePayload, H: Handler<P>>(
        &self,
        topic: UUri,
        handler: H,
    ) -> Result<Subscription<NativeSubscription<C>>, SetupError> {
        topic
            .verify_no_wildcards()
            .map_err(|_| SetupError::InvalidRoute("exact topic"))?;
        let active = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let receiver = engine::Receiver::start::<P, _>(
            &self.setup.transport,
            topic.clone(),
            None,
            crate::UMessageType::Publish,
            Gated {
                handler: NativeHandler(handler),
                active: active.clone(),
            },
            self.setup.config.receive,
        )
        .await?;
        Ok(Subscription::new(
            NativeSubscription { receiver, active },
            self.service.clone(),
            topic,
            self.source_uri.clone(),
        ))
    }
}

impl<C: NativeLoanCore, U: LocalUriProvider + ?Sized> Endpoint<Native<C>, U> {
    /// Prepares the exact event/destination route before any notification.
    ///
    /// # Errors
    /// Refuses address, representation and physical route/capacity failures.
    pub fn notifier<P: StablePayloadInit>(
        &self,
        resource: u16,
        destination: UUri,
        options: PublishOptions,
    ) -> Result<Notifier<P, engine::Notifier<C, P>>, SetupError> {
        let inner = engine::Notifier::new(
            &self.setup.transport,
            self.uri_provider.get_resource_uri(resource),
            destination,
            SendConfig {
                priority: options.priority,
                ttl: options.ttl,
                route: self.setup.config.route,
            },
        )?;
        Ok(Notifier {
            inner,
            payload: PhantomData,
        })
    }

    /// Attaches a retained native receiver for the exact event and local destination.
    ///
    /// # Errors
    /// Returns address, agreement/type, receive capability or capacity failures.
    pub async fn listen<P: StablePayload, H: Handler<P>>(
        &self,
        topic: UUri,
        handler: H,
    ) -> Result<Receiver<engine::Receiver<C>>, SetupError> {
        topic
            .verify_no_wildcards()
            .map_err(|_| SetupError::InvalidRoute("exact notification topic"))?;
        let inner = engine::Receiver::start::<P, _>(
            &self.setup.transport,
            topic,
            Some(self.uri_provider.get_source_uri()),
            crate::UMessageType::Notification,
            NativeHandler(handler),
            self.setup.config.receive,
        )
        .await?;
        Ok(Receiver { inner })
    }

    /// Reserves request route, exact response attachment and bounded pending slots.
    ///
    /// # Errors
    /// Refuses method/configuration, complete agreement/type or physical capacity.
    pub async fn rpc_client<Req: StablePayloadInit, Resp: StablePayload>(
        &self,
        method: UUri,
        options: RpcOptions,
    ) -> Result<RpcClient<Req, Resp, engine::RpcClient<C, Req, Resp>>, SetupError> {
        let inner = engine::RpcClient::new(
            Arc::clone(self.setup.transport.core()),
            self.setup
                .transport
                .native_agreement()
                .cloned()
                .ok_or(SetupError::InvalidConfig("native agreement required"))?,
            self.uri_provider.get_source_uri(),
            method,
            crate::RpcClientConfig {
                max_in_flight: options.max_in_flight,
                ttl: options.ttl,
                max_token_len: options.max_token_len,
                priority: options.priority,
            },
        )
        .await?;
        Ok(RpcClient {
            inner,
            payload: PhantomData,
        })
    }

    /// Runs the same business handler as serialization, followed by a synchronous
    /// response initializer writing directly in the actual middleware loan.
    ///
    /// # Errors
    /// Refuses method/configuration, complete agreement/type or physical capacity.
    pub async fn rpc_server<Req, Resp, H, W>(
        &self,
        resource: u16,
        handler: H,
        write: W,
        options: ServerOptions,
    ) -> Result<RpcServer<engine::RpcServer<C, Resp>>, SetupError>
    where
        Req: StablePayload,
        Resp: StablePayloadInit,
        H: RequestHandler<Req>,
        W: for<'a> FnMut(H::Reply, PayloadInit<'a, Resp>) -> InitializedStablePayload<'a, Resp>
            + Send
            + 'static,
    {
        let inner = engine::RpcServer::start::<Req, _>(
            Arc::clone(self.setup.transport.core()),
            self.setup
                .transport
                .native_agreement()
                .cloned()
                .ok_or(SetupError::InvalidConfig("native agreement required"))?,
            self.uri_provider.get_resource_uri(resource),
            NativeRequest { handler, write },
            crate::RpcServerConfig {
                registration: self.setup.config.receive.registration,
                header: self.setup.config.receive.header,
                max_clients: options.max_clients,
            },
        )
        .await?;
        Ok(RpcServer { inner })
    }
}

impl<P: StablePayloadInit, C: NativeLoanCore> Notifier<P, engine::Notifier<C, P>> {
    /// Initializes and commits in middleware loan storage before returning.
    ///
    /// # Errors
    /// Returns native initialization/storage/publication failure; never falls back.
    pub fn notify_in_place<F>(&self, initialize: F) -> Result<(), SendError>
    where
        F: for<'a> FnOnce(PayloadInit<'a, P>) -> InitializedStablePayload<'a, P>,
    {
        self.notify_in_place_with(MessageFields::default(), initialize)
    }
    /// Sends with bounded per-message fields without extending loan lifetime.
    ///
    /// # Errors
    /// Returns native field/storage/publication failures.
    pub fn notify_in_place_with<F>(
        &self,
        fields: MessageFields<'_>,
        initialize: F,
    ) -> Result<(), SendError>
    where
        F: for<'a> FnOnce(PayloadInit<'a, P>) -> InitializedStablePayload<'a, P>,
    {
        self.inner.notify_stable(fields, initialize)
    }
}
impl<C: NativeLoanCore> Receiver<engine::Receiver<C>> {
    /// Native address rejection diagnostics.
    pub fn stats(&self) -> &engine::ReceiverStats {
        self.inner.stats()
    }
    /// Eagerly closes admission and attempts exact detach, then drains by borrow.
    /// Dropping an unpolled drain retains the owner for external retry.
    ///
    /// # Errors
    /// Preserves native detachment or worker failure.
    pub fn stop(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), engine::ReceiverStopError>> + Send + '_ {
        self.stop_ref()
    }

    /// Borrows the native owner across detach retry and cancellation-safe drain.
    ///
    /// # Errors
    /// Reports exact-detach failure or the sticky terminal worker failure.
    /// Same-worker `CalledFromWorker` retains the join for external retry and
    /// is not a sticky terminal outcome.
    pub fn stop_ref(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), engine::ReceiverStopError>> + Send + '_ {
        self.inner.stop_ref()
    }
    /// Closes admission before any recovery future is polled.
    pub fn close_admission(&self) {
        self.inner.close_admission();
    }
    /// Constant-space exact stop diagnostics across cancellation and retry.
    pub fn stop_progress(&self) -> &crate::StopProgress<SetupError> {
        self.inner.stop_progress()
    }
}
impl<C: NativeLoanCore, Req: StablePayloadInit, Resp: StablePayload>
    RpcClient<Req, Resp, engine::RpcClient<C, Req, Resp>>
{
    /// Arms correlation and synchronously initializes/commits the request loan.
    /// Only response waiting may await, by calling `.response()` on the result.
    ///
    /// # Errors
    /// Returns bounded pending/initialization/send failure without copying fallback.
    pub fn invoke_in_place<F>(
        &self,
        initialize: F,
    ) -> Result<crate::PendingNativeCall<'_, Resp, C::Rx>, crate::NativeCallError>
    where
        F: for<'a> FnOnce(PayloadInit<'a, Req>) -> InitializedStablePayload<'a, Req>,
    {
        self.inner.begin_call(initialize)
    }
    /// Replaces credentials within the reserved native token buffer.
    ///
    /// # Errors
    /// Oversized credentials preserve the old token.
    pub fn set_token(&self, token: &str) -> Result<(), crate::TokenError> {
        self.inner.set_token(token)
    }
    /// Removes credentials from future requests.
    pub fn clear_token(&self) {
        self.inner.clear_token();
    }
    /// Number of occupied pending entries.
    pub fn in_flight(&self) -> usize {
        self.inner.in_flight()
    }
    /// Native response/correlation diagnostics.
    pub fn stats(&self) -> &crate::RpcClientStats {
        self.inner.stats()
    }
    /// Closes correlation and releases exact response attachment.
    ///
    /// # Errors
    /// Failed detach remains available for retry on Drop.
    pub fn close(&self) -> Result<(), SetupError> {
        self.inner.close()
    }
}

struct NativeRequest<H, W> {
    handler: H,
    write: W,
}
impl<Req, Resp, F, H, W> crate::NativeRequestHandler<Req, Resp, F> for NativeRequest<H, W>
where
    Req: StablePayload,
    Resp: StablePayloadInit,
    F: Frame,
    H: RequestHandler<Req>,
    W: for<'a> FnMut(H::Reply, PayloadInit<'a, Resp>) -> InitializedStablePayload<'a, Resp>
        + Send
        + 'static,
{
    type Reply = H::Reply;
    async fn handle(&mut self, request: RxLease<Req, F>) -> Result<Self::Reply, crate::UCode> {
        self.handler.handle(request).await
    }
    fn write_reply<'a>(
        &mut self,
        reply: Self::Reply,
        initializer: Resp::Initializer<'a>,
    ) -> InitializedStablePayload<'a, Resp> {
        (self.write)(reply, PayloadInit::from_initializer(initializer))
    }
}
impl<C: NativeLoanCore, Resp: StablePayloadInit> RpcServer<engine::RpcServer<C, Resp>> {
    /// Reserves one client in the actual native response-route cache.
    ///
    /// # Errors
    /// Returns address, layout or physical/table capacity refusal.
    pub fn prepare_client(&self, reply_to: &UUri) -> Result<(), SetupError> {
        self.inner.prepare_client(reply_to)
    }
    /// Native reply and resource counters.
    pub fn stats(&self) -> &crate::RpcServerStats {
        self.inner.stats()
    }
    /// Eagerly closes admission and attempts exact detach, then drains by borrow.
    /// Dropping an unpolled drain retains the owner for external retry.
    ///
    /// # Errors
    /// Reports physical detach and/or receive-worker failure.
    pub fn stop(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), crate::RpcServerStopError>> + Send + '_ {
        self.stop_ref()
    }

    /// Borrows the native owner across detach retry and cancellation-safe drain.
    ///
    /// # Errors
    /// Reports exact-detach and sticky terminal worker failures together.
    /// Same-worker `CalledFromWorker` retains the join for external retry and
    /// is not a sticky terminal outcome.
    pub fn stop_ref(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), crate::RpcServerStopError>> + Send + '_ {
        self.inner.stop_ref()
    }
    /// Closes admission before any recovery future is polled.
    pub fn close_admission(&self) {
        self.inner.close_admission();
    }
    /// Constant-space exact stop diagnostics across cancellation and retry.
    pub fn stop_progress(&self) -> &crate::StopProgress<SetupError> {
        self.inner.stop_progress()
    }
}

impl<C: NativeLoanCore> std::fmt::Debug for Native<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Native").finish_non_exhaustive()
    }
}
