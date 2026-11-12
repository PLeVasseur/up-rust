//! Storable application roles; schema mappings and carriage belong to setup.
//!
//! Schema packages bind named operations to `Deployment`. Applications store
//! domain-only handles, usually through one [`role_set!`](crate::role_set), which
//! owns partial setup and bounded, retryable cleanup for any mix of roles. Native
//! deployments use the static binding contract rather than the owned carriage.
//!
//! Start with `examples/hello_publisher.rs` and the
//! [application guide](crate::guide::applications).
#![cfg_attr(
    feature = "up-l2-zero-copy",
    doc = "\nSee [`native`](crate::communication::application::native) for that contract."
)]

use crate::{LocalUriProvider, UCode, UFrameMetadata, UUri, UUID};
use std::sync::Arc;

use super::typed::SubscriptionLocal;
pub use super::typed::{
    Decoded, Delivery, Handler, MessageContext, PublishOptions, RemoteObservations, RequestHandler,
    RpcOptions, ServerOptions, SubscriptionActivationError, SubscriptionCloseError,
    SubscriptionPhase,
};
mod recovery;
pub use recovery::{acquire, CleanupLeg, ManagedRole, RoleSetError, RoleSlot, SerializedCleanup};
#[doc(hidden)]
pub use recovery::{aggregate_completion, RoleSetProgress};
mod mapping;
pub use mapping::{Decoder, Encoder};
mod operations;
pub use operations::{
    BoundDeployment, DecodeProfile, DeploymentOptions, EncodeProfile, Event, MappingProfile,
    Notification, OperationOptions, ProtobufProfile, Rpc,
};
#[cfg(feature = "up-l2-zero-copy")]
mod native_binding;
mod serialized;
#[doc(hidden)]
pub use serialized::SerializedReceiver;
pub use serialized::{SerializedError, SerializedReceiveStats, SerializedStopError};
mod rpc;
pub use rpc::{
    SerializedCall as PendingCall, SerializedCallError as CallError, SerializedRpcStats,
    SerializedServerStats,
};
#[cfg(feature = "up-l2-zero-copy")]
pub mod native;

/// Deployment-owned carriage, independent envelope admission and resource limits.
#[derive(Clone)]
pub struct Deployment {
    transport: Arc<dyn crate::UOwnedTransport>,
    uri_provider: Arc<dyn LocalUriProvider>,
    limit: crate::PayloadDecodeLimit,
    registration: crate::RegistrationConfig,
    max_token_len: usize,
}
impl Deployment {
    /// Constructs carriage setup. Schema factories separately admit every mapping.
    ///
    /// # Errors
    /// Rejects zero receive capacity before worker or remote effects.
    pub fn new(
        transport: Arc<dyn crate::UOwnedTransport>,
        uri_provider: Arc<dyn LocalUriProvider>,
        limit: crate::PayloadDecodeLimit,
        registration: crate::RegistrationConfig,
        max_token_len: usize,
    ) -> Result<Self, SerializedError> {
        if registration.capacity == 0 {
            return Err(SerializedError::Config("receive capacity must be positive"));
        }
        Ok(Self {
            transport,
            uri_provider,
            limit,
            registration,
            max_token_len,
        })
    }
    /// Advanced carriage access retains the transport's validation boundary.
    pub fn transport(&self) -> &Arc<dyn crate::UOwnedTransport> {
        &self.transport
    }
    /// Deployment's semantic local addressing.
    pub fn uri_provider(&self) -> &dyn LocalUriProvider {
        self.uri_provider.as_ref()
    }
    /// Constructs a reusable subscription service coordinator.
    pub fn subscriber(
        &self,
        service: Arc<dyn crate::core::usubscription::USubscription>,
    ) -> Subscriber {
        Subscriber {
            setup: self.clone(),
            service,
        }
    }
}

/// Publisher storing a schema encoder, route and owned carriage, not a codec type.
pub struct Publisher<P> {
    inner: serialized::SerializedRoute<P>,
}
/// Notifier with an exact destination and schema encoder.
pub struct Notifier<P> {
    inner: serialized::SerializedRoute<P>,
}
/// Deployment's remote subscription coordination.
pub struct Subscriber {
    setup: Deployment,
    service: Arc<dyn crate::core::usubscription::USubscription>,
}
/// Notification attachment and bounded worker owner.
#[must_use = "retain the receiver until explicit shutdown"]
pub struct Receiver {
    inner: SerializedReceiver,
}
/// Retained local and remote reconciliation ticket; activation borrows this owner.
pub type Subscription = super::typed::Subscription<SerializedReceiver>;
/// Domain-only client. Pending responses borrow this client's bounded table.
pub struct RpcClient<Req, Resp> {
    inner: rpc::SerializedRpcClient<Req, Resp>,
}
/// Server owner independent of its handler and request/response mapping types.
#[must_use = "retain the server until explicit shutdown"]
pub struct RpcServer {
    inner: rpc::SerializedRpcServer,
}

impl std::fmt::Debug for Deployment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Deployment").finish_non_exhaustive()
    }
}
impl<P> std::fmt::Debug for Publisher<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Publisher").finish_non_exhaustive()
    }
}
impl<P> std::fmt::Debug for Notifier<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Notifier").finish_non_exhaustive()
    }
}
impl std::fmt::Debug for Subscriber {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Subscriber").finish_non_exhaustive()
    }
}
impl std::fmt::Debug for Receiver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Receiver").finish_non_exhaustive()
    }
}
impl<Req, Resp> std::fmt::Debug for RpcClient<Req, Resp> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("RpcClient").finish_non_exhaustive()
    }
}
impl std::fmt::Debug for RpcServer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("RpcServer").finish_non_exhaustive()
    }
}
