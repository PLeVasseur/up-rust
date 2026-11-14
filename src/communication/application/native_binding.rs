//! Static deployment binding for the same typed resource declarations.
//!
//! A reusable application carries one core association. SDK aliases supply engine
//! spelling; schema authors supply only their actual stable payload declarations
//! and synchronous in-place initializers. No owned-transport adapter is involved.

use super::{
    Event, Handler, Notification, PublishOptions, RequestHandler, Rpc, RpcOptions, ServerOptions,
};
use crate::{
    communication::{typed, zero_copy as engine},
    core::usubscription::USubscription,
    InitializedStablePayload, LocalUriProvider, NativeLoanCore, NativeProfileAgreement,
    PayloadInit, SetupError, StableContainerWire, StablePayload, StablePayloadInit,
    UFrameFieldsMetadataCodec, UUri, UWireTransport,
};
use std::sync::Arc;

/// Storable native publication handle; no schema-local engine matrix.
pub type Publisher<C, P> = typed::Publisher<P, engine::Publisher<C, P>>;
/// Storable native notification handle.
pub type Notifier<C, P> = typed::Notifier<P, engine::Notifier<C, P>>;
/// Retained native subscription ticket.
pub type Subscription<C> = typed::Subscription<typed::NativeSubscription<C>>;
/// Native receiver retaining physical attachments and worker ownership.
pub type Receiver<C> = typed::Receiver<engine::Receiver<C>>;
/// Native client retaining full-key correlation and independent RX ownership.
pub type RpcClient<C, Req, Resp> = typed::RpcClient<Req, Resp, engine::RpcClient<C, Req, Resp>>;
/// Native server with a synchronous response initializer.
pub type RpcServer<C, Resp> = typed::RpcServer<engine::RpcServer<C, Resp>>;

/// One admitted static deployment, reusable by every schema's operations.
pub struct Deployment<C: NativeLoanCore> {
    endpoint: typed::Endpoint<typed::Native<C>, dyn LocalUriProvider>,
    address: Arc<dyn LocalUriProvider>,
}
impl<C: NativeLoanCore> Deployment<C> {
    /// Admits a deployment over `core` with bounded ordinary defaults, the native
    /// equivalent of serialized `Deployment::with_defaults`.
    ///
    /// `agreement` remains the deployment's real native agreement: its profile,
    /// target and generation are checked by every role, and no default is assumed.
    ///
    /// # Errors
    /// Refuses a missing agreement or invalid capacity, like [`Self::new`].
    pub fn with_defaults(
        core: Arc<C>,
        agreement: NativeProfileAgreement,
        address: Arc<dyn LocalUriProvider>,
    ) -> Result<Self, SetupError> {
        Self::configured(core, agreement, address, typed::NativeConfig::default())
    }

    /// Named advanced capacities over the same admission path as `with_defaults`.
    ///
    /// # Errors
    /// Refuses a missing agreement or invalid capacity, like [`Self::new`].
    pub fn configured(
        core: Arc<C>,
        agreement: NativeProfileAgreement,
        address: Arc<dyn LocalUriProvider>,
        config: typed::NativeConfig,
    ) -> Result<Self, SetupError> {
        let transport = Arc::new(UWireTransport::with_native_agreement(
            core,
            StableContainerWire,
            UFrameFieldsMetadataCodec,
            agreement,
        ));
        Self::new(transport, config, address)
    }

    /// Uses an actual native agreement and independently configured peer address.
    pub fn new(
        transport: Arc<engine::StableTransport<C>>,
        config: typed::NativeConfig,
        address: Arc<dyn LocalUriProvider>,
    ) -> Result<Self, SetupError> {
        Ok(Self {
            endpoint: typed::Endpoint::new(typed::Native::new(transport, config)?, address.clone()),
            address,
        })
    }
    /// Semantic addressing; never derives native schema identity from a resource.
    pub fn address(&self) -> &dyn LocalUriProvider {
        self.address.as_ref()
    }
}
impl<C: NativeLoanCore> std::fmt::Debug for Deployment<C> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeDeployment")
            .finish_non_exhaustive()
    }
}
impl<P: StablePayloadInit> Event<P> {
    /// Reserves the native publication route using the existing native engine.
    pub fn native_publisher<C: NativeLoanCore>(
        &self,
        sender: &Deployment<C>,
        options: PublishOptions,
    ) -> Result<Publisher<C, P>, SetupError> {
        sender.endpoint.publisher(self.resource(), options)
    }
}
impl<P: StablePayload> Event<P> {
    /// Prepares a borrowed-cleanup subscription; activation remains explicit.
    pub async fn native_subscription<C: NativeLoanCore, H: Handler<P>>(
        &self,
        receiver: &Deployment<C>,
        source: &dyn LocalUriProvider,
        service: Arc<dyn USubscription>,
        handler: H,
    ) -> Result<Subscription<C>, SetupError> {
        receiver
            .endpoint
            .subscriber(service)
            .subscription::<P, H>(source.get_resource_uri(self.resource()), handler)
            .await
    }
}
impl<P: StablePayloadInit> Notification<P> {
    /// Retains synchronous middleware-loan initialization, not serialized staging.
    pub fn native_notifier<C: NativeLoanCore>(
        &self,
        sender: &Deployment<C>,
        destination: UUri,
        options: PublishOptions,
    ) -> Result<Notifier<C, P>, SetupError> {
        sender
            .endpoint
            .notifier(self.resource(), destination, options)
    }
}
impl<P: StablePayload> Notification<P> {
    /// Attaches the native receive owner for the declared resource.
    pub async fn native_receiver<C: NativeLoanCore, H: Handler<P>>(
        &self,
        receiver: &Deployment<C>,
        source: &dyn LocalUriProvider,
        handler: H,
    ) -> Result<Receiver<C>, SetupError> {
        receiver
            .endpoint
            .listen::<P, H>(source.get_resource_uri(self.resource()), handler)
            .await
    }
}
impl<Req: StablePayloadInit, Resp: StablePayload> Rpc<Req, Resp> {
    /// Prepares the native client before any synchronous request submission.
    pub async fn native_client<C: NativeLoanCore>(
        &self,
        client: &Deployment<C>,
        server: &dyn LocalUriProvider,
        options: RpcOptions,
    ) -> Result<RpcClient<C, Req, Resp>, SetupError> {
        client
            .endpoint
            .rpc_client(server.get_resource_uri(self.resource()), options)
            .await
    }
}
impl<Req: StablePayload, Resp: StablePayloadInit> Rpc<Req, Resp> {
    /// The schema's explicit response initializer writes in real TX loan storage.
    pub async fn native_server<C, H, W>(
        &self,
        server: &Deployment<C>,
        handler: H,
        write: W,
        options: ServerOptions,
    ) -> Result<RpcServer<C, Resp>, SetupError>
    where
        C: NativeLoanCore,
        H: RequestHandler<Req>,
        W: for<'loan> FnMut(
                H::Reply,
                PayloadInit<'loan, Resp>,
            ) -> InitializedStablePayload<'loan, Resp>
            + Send
            + 'static,
    {
        server
            .endpoint
            .rpc_server::<Req, Resp, H, W>(self.resource(), handler, write, options)
            .await
    }
}

/// Generates concrete stored role names for one selected native deployment.
/// List only the needed roles; repeat any role. No other endpoint is constructed.
/// Paths resolve in the invoking module, wherever the macro is called.
/// Advanced options remain available on the original operation primitives.
#[macro_export]
macro_rules! native_roles {
    ($vis:vis mod $name:ident { core: $core:ty; $($roles:tt)* }) => {
        #[doc = "Selected native role names and one-sided factories."]
        #[allow(dead_code)]
        $vis mod $name {
            #[allow(unused_imports)]
            use super::*;
            #[doc = "One local native endpoint with its real agreement and address."]
            pub type Deployment = $crate::communication::application::native::Deployment<$core>;
            $crate::__native_roles! { $core; $($roles)* }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __native_roles {
    ($core:ty;) => {};
    ($core:ty; client $handle:ident, $factory:ident, $delivery:ident: $operation:path => $request:ty, $response:ty; $($rest:tt)*) => {
        #[doc = "Retained response owner with the selected binding supplied by the SDK."]
        pub type $delivery = <$handle as $crate::communication::application::native::Invoke<$request, $response>>::Delivery;
        $crate::__native_roles! { $core; client $handle, $factory: $operation => $request, $response; $($rest)* }
    };
    ($core:ty; publisher $handle:ident, $factory:ident: $operation:path => $payload:ty; $($rest:tt)*) => {
        #[doc = "Concrete native publisher; initialization stays synchronous and in-place."]
        pub type $handle = $crate::communication::application::native::Publisher<$core, $payload>;
        #[doc = "Constructs only this publisher with standard publication options."]
        pub fn $factory(local: &Deployment) -> ::core::result::Result<$handle, $crate::SetupError> {
            $operation.native_publisher(local, $crate::communication::application::PublishOptions::default())
        }
        $crate::__native_roles! { $core; $($rest)* }
    };
    ($core:ty; subscription $handle:ident, $factory:ident: $operation:path => $payload:ty; $($rest:tt)*) => {
        #[doc = "Concrete native subscription retaining its local and remote cleanup owner."]
        pub type $handle = $crate::communication::application::native::Subscription<$core>;
        #[doc = "Prepares only a local subscription; source is address metadata, not a peer endpoint."]
        pub async fn $factory<H: $crate::communication::application::Handler<$payload>>(
            local: &Deployment,
            source: &dyn $crate::LocalUriProvider,
            service: ::std::sync::Arc<dyn $crate::core::usubscription::USubscription>,
            handler: H,
        ) -> ::core::result::Result<$handle, $crate::SetupError> {
            $operation.native_subscription(local, source, service, handler).await
        }
        $crate::__native_roles! { $core; $($rest)* }
    };
    ($core:ty; notifier $handle:ident, $factory:ident: $operation:path => $payload:ty; $($rest:tt)*) => {
        #[doc = "Concrete exact-destination native notifier."]
        pub type $handle = $crate::communication::application::native::Notifier<$core, $payload>;
        #[doc = "Constructs only this notifier; destination is routing data."]
        pub fn $factory(local: &Deployment, destination: $crate::UUri) -> ::core::result::Result<$handle, $crate::SetupError> {
            $operation.native_notifier(local, destination, $crate::communication::application::PublishOptions::default())
        }
        $crate::__native_roles! { $core; $($rest)* }
    };
    ($core:ty; receiver $handle:ident, $factory:ident: $operation:path => $payload:ty; $($rest:tt)*) => {
        #[doc = "Concrete native receiver with retained attachment and worker ownership."]
        pub type $handle = $crate::communication::application::native::Receiver<$core>;
        #[doc = "Attaches only this local receiver; no notifier is constructed."]
        pub async fn $factory<H: $crate::communication::application::Handler<$payload>>(
            local: &Deployment, source: &dyn $crate::LocalUriProvider, handler: H,
        ) -> ::core::result::Result<$handle, $crate::SetupError> {
            $operation.native_receiver(local, source, handler).await
        }
        $crate::__native_roles! { $core; $($rest)* }
    };
    ($core:ty; client $handle:ident, $factory:ident: $operation:path => $request:ty, $response:ty; $($rest:tt)*) => {
        #[doc = "Concrete native RPC client with retained responses and full-key correlation."]
        pub type $handle = $crate::communication::application::native::RpcClient<$core, $request, $response>;
        #[doc = "Prepares only this client using bounded defaults; the server is an address."]
        pub async fn $factory(local: &Deployment, server: &dyn $crate::LocalUriProvider) -> ::core::result::Result<$handle, $crate::SetupError> {
            $operation.native_client(local, server, $crate::communication::application::OperationOptions::default().rpc).await
        }
        $crate::__native_roles! { $core; $($rest)* }
    };
    ($core:ty; server $handle:ident, $factory:ident: $operation:path => $request:ty, $response:ty;
        reply $decision:ty => $write:expr; $($rest:tt)*) => {
        #[doc = "Concrete native RPC server retaining its synchronous response initializer."]
        pub type $handle = $crate::communication::application::native::RpcServer<$core, $response>;
        #[doc = "Prepares only this server; reply writing runs inside its actual middleware loan."]
        pub async fn $factory<H: $crate::communication::application::RequestHandler<$request, Reply = $decision>>(
            local: &Deployment, handler: H,
        ) -> ::core::result::Result<$handle, $crate::SetupError> {
            $operation.native_server(local, handler, $write, $crate::communication::application::OperationOptions::default().server).await
        }
        $crate::__native_roles! { $core; $($rest)* }
    };
}
