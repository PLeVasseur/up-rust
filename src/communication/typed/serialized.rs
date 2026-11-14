//! Advanced static-codec construction adapts to the sole mapping-aware engine.
use super::super::application as app;
use super::*;
use crate::{
    DecodePayload, EncodePayload, PayloadCodec, PayloadDecodeLimit, RegistrationConfig,
    UOwnedTransport,
};

/// Compatibility setup for a statically selected schema codec, not an engine.
/// Ordinary applications select declared profiles and store domain-only roles.
pub struct Serialized<T: UOwnedTransport + ?Sized, K> {
    transport: Arc<dyn UOwnedTransport>,
    limit: PayloadDecodeLimit,
    registration: RegistrationConfig,
    max_token_len: usize,
    marker: PhantomData<fn(&T) -> K>,
}
impl<T: UOwnedTransport + ?Sized, K> Clone for Serialized<T, K> {
    fn clone(&self) -> Self {
        Self {
            transport: self.transport.clone(),
            limit: self.limit,
            registration: self.registration,
            max_token_len: self.max_token_len,
            marker: PhantomData,
        }
    }
}
impl<T: UOwnedTransport + ?Sized, K> std::fmt::Debug for Serialized<T, K> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SerializedMappingAdapter")
            .finish_non_exhaustive()
    }
}
impl<T: UOwnedTransport + 'static, K: PayloadCodec> Serialized<T, K> {
    /// Admits static codec identity and limits, without starting a second engine.
    pub fn new(
        transport: Arc<T>,
        limit: PayloadDecodeLimit,
        registration: RegistrationConfig,
    ) -> Result<Self, SerializedError> {
        Self::from_transport(transport, limit, registration)
    }
}
impl<K: PayloadCodec> Serialized<dyn UOwnedTransport, K> {
    /// Supports an already erased owned carriage without a forwarding transport.
    pub fn from_dyn(
        transport: Arc<dyn UOwnedTransport>,
        limit: PayloadDecodeLimit,
        registration: RegistrationConfig,
    ) -> Result<Self, SerializedError> {
        Self::from_transport(transport, limit, registration)
    }
}
impl<T: UOwnedTransport + ?Sized, K: PayloadCodec> Serialized<T, K> {
    fn from_transport(
        transport: Arc<dyn UOwnedTransport>,
        limit: PayloadDecodeLimit,
        registration: RegistrationConfig,
    ) -> Result<Self, SerializedError> {
        let identity = K::payload_identity(None)?;
        if !matches!(identity, crate::PayloadIdentity::Fixed(_)) {
            return Err(SerializedError::Config(
                "serialized setup requires a fixed codec",
            ));
        }
        transport.admit_payload_contract(identity, K::payload_family())?;
        if registration.capacity == 0 {
            return Err(SerializedError::Config("receive capacity must be positive"));
        }
        Ok(Self {
            transport,
            limit,
            registration,
            max_token_len: 2048,
            marker: PhantomData,
        })
    }
    /// Bounds request credential admission; this changes no payload mapping.
    pub fn with_max_token_len(mut self, max_token_len: usize) -> Self {
        self.max_token_len = max_token_len;
        self
    }
    fn deployment(&self, source: UUri) -> Result<app::Deployment, SerializedError> {
        app::Deployment::new(
            self.transport.clone(),
            Arc::new(crate::StaticUriProvider::from(source)),
            self.limit,
            self.registration,
            self.max_token_len,
        )
    }
}

impl<T, K, U> Endpoint<Serialized<T, K>, U>
where
    T: UOwnedTransport + ?Sized,
    K: PayloadCodec,
    U: LocalUriProvider + ?Sized + 'static,
{
    fn deployment(&self) -> Result<app::Deployment, SerializedError> {
        app::Deployment::new(
            self.setup.transport.clone(),
            Arc::new(Provider(self.uri_provider.clone())),
            self.setup.limit,
            self.setup.registration,
            self.setup.max_token_len,
        )
    }
    /// Constructs the same publisher as declaration-based setup.
    pub fn publisher<P>(
        &self,
        resource: u16,
        options: PublishOptions,
    ) -> Result<app::LegacyPublisher<P>, SerializedError>
    where
        K: EncodePayload<P>,
    {
        self.deployment()?
            .publisher(resource, options, app::Encoder::from_schema_mapping::<K>()?)
    }
    /// Constructs the same notification route as declaration-based setup.
    pub fn notifier<P>(
        &self,
        resource: u16,
        destination: UUri,
        options: PublishOptions,
    ) -> Result<app::Notifier<P>, SerializedError>
    where
        K: EncodePayload<P>,
    {
        self.deployment()?.notifier(
            resource,
            destination,
            options,
            app::Encoder::from_schema_mapping::<K>()?,
        )
    }
    /// Attaches the shared retained receiver; `stop` now borrows its owner.
    pub async fn listen<P, H>(
        &self,
        topic: UUri,
        handler: H,
    ) -> Result<app::Receiver, SerializedError>
    where
        P: Send + 'static,
        H: Handler<P>,
        K: for<'bytes> DecodePayload<'bytes, P>,
    {
        self.deployment()?
            .listen(topic, handler, app::Decoder::from_schema_mapping::<K>()?)
            .await
    }
    /// Reserves correlation through the shared mapping-aware client.
    pub async fn rpc_client<Req, Resp>(
        &self,
        method: UUri,
        options: RpcOptions,
    ) -> Result<app::RpcClient<Req, Resp>, SerializedError>
    where
        K: EncodePayload<Req> + for<'bytes> DecodePayload<'bytes, Resp>,
    {
        self.deployment()?
            .rpc_client(
                method,
                options,
                app::Encoder::from_schema_mapping::<K>()?,
                app::Decoder::from_schema_mapping::<K>()?,
            )
            .await
    }
    /// Uses the shared server and sticky terminal cleanup outcomes.
    pub async fn rpc_server<Req, Resp, H, W>(
        &self,
        resource: u16,
        handler: H,
        write: W,
        options: ServerOptions,
    ) -> Result<app::RpcServer, SerializedError>
    where
        Req: Send + 'static,
        Resp: Send + 'static,
        H: RequestHandler<Req>,
        W: FnMut(H::Reply) -> Resp + Send + 'static,
        K: for<'bytes> DecodePayload<'bytes, Req> + EncodePayload<Resp>,
    {
        self.deployment()?
            .rpc_server(
                resource,
                handler,
                write,
                options,
                app::Decoder::from_schema_mapping::<K>()?,
                app::Encoder::from_schema_mapping::<K>()?,
            )
            .await
    }
}

struct Provider<U: LocalUriProvider + ?Sized>(Arc<U>);
impl<U: LocalUriProvider + ?Sized> LocalUriProvider for Provider<U> {
    fn get_authority(&self) -> String {
        self.0.get_authority()
    }
    fn get_source_uri(&self) -> UUri {
        self.0.get_source_uri()
    }
    fn get_resource_uri(&self, resource: u16) -> UUri {
        self.0.get_resource_uri(resource)
    }
}
impl<T: UOwnedTransport + ?Sized, K: PayloadCodec> Subscriber<Serialized<T, K>> {
    /// Returns the shared subscription ticket, including reconciliation and retry.
    pub async fn subscription<P, H>(
        &self,
        topic: UUri,
        handler: H,
    ) -> Result<app::Subscription, SerializedError>
    where
        P: Send + 'static,
        H: Handler<P>,
        K: for<'bytes> DecodePayload<'bytes, P>,
    {
        self.setup
            .deployment(self.source_uri.clone())?
            .subscriber(self.service.clone())
            .subscription(topic, handler, app::Decoder::from_schema_mapping::<K>()?)
            .await
    }
}
