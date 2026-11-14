//! Advanced statically typed engines and shared delivery/lifecycle contracts.
//! Ordinary applications use `communication::application` through their schema
//! package; native deployment bindings compose the static engines here.
//!
//! Serialized construction borrows a value; native construction initializes a
//! middleware loan synchronously. Both paths deliver an owning checked borrow.
//! No automatic negotiation, native fallback, or selected-wire codec association.
//!
//! Static-codec serialized construction adapts to the application's mapping
//! tables. It has no separate worker, correlation or cleanup implementation.

use crate::{LocalUriProvider, UCode, UFrameMetadata, UPriority, UUri, UUID};
use std::{convert::Infallible, future::Future, marker::PhantomData, sync::Arc, time::Duration};

mod lifecycle;
pub(crate) use lifecycle::retain_worker_outcome;
#[doc(hidden)]
pub use lifecycle::SubscriptionLocal;
pub use lifecycle::{
    RemoteObservations, Subscription, SubscriptionActivationError, SubscriptionCloseError,
    SubscriptionPhase,
};
mod serialized;
pub use super::application::{
    CallError as SerializedCallError, PendingCall as SerializedCall, SerializedError,
    SerializedReceiveStats, SerializedReceiver, SerializedRpcStats, SerializedServerStats,
    SerializedStopError,
};
pub use serialized::Serialized;
#[cfg(feature = "up-l2-zero-copy")]
mod native;
#[cfg(feature = "up-l2-zero-copy")]
pub use native::{Native, NativeConfig, NativeSubscription};

/// Metadata available without exposing the envelope or detaching the payload.
pub struct MessageContext<'a> {
    /// Message kind.
    pub kind: crate::UMessageType,
    /// Sender's exact address.
    pub source: UUri,
    /// Destination, absent for a publication.
    pub sink: Option<UUri>,
    /// Message identity.
    pub id: UUID,
    /// Request correlation, present on responses.
    pub request_id: Option<UUID>,
    /// Correlated response status, if present.
    pub comm_status: Option<UCode>,
    /// Declared payload encoding; never used to infer a Rust type.
    pub payload_encoding: Option<crate::PayloadEncoding>,
    /// Native structural token, checked with the complete native agreement.
    pub native_type_token: Option<crate::NativeTypeToken>,
    /// Optional authorization token, borrowed from the delivery.
    pub token: Option<&'a str>,
    /// Permission level.
    pub permission_level: Option<u32>,
    /// Message priority.
    pub priority: Option<UPriority>,
    /// Message TTL.
    pub ttl: Option<Duration>,
    /// Trace context, borrowed from the delivery.
    pub traceparent: Option<&'a str>,
}

/// A delivery owns all storage needed by its checked typed borrow.
/// A borrowed payload cannot escape that owner:
///
/// ```compile_fail
/// use up_rust::communication::typed::Delivery;
/// fn escape<D: Delivery<u32>>(delivery: D) -> &'static u32 {
///     delivery.payload().unwrap()
/// }
/// ```
/// Borrowed semantic fields are also tied to their delivery:
///
/// ```compile_fail
/// use up_rust::communication::typed::{Delivery, MessageContext};
/// fn escape_context<D: Delivery<u32>>(delivery: D) -> MessageContext<'static> {
///     delivery.context().unwrap()
/// }
/// ```
pub trait Delivery<P>: Send + 'static {
    /// Owned values are infallible; native leases retain admission errors.
    type Error: std::error::Error + Send + Sync + 'static;
    /// Borrows while retaining the storage owner. Native calls perform full checks.
    fn payload(&self) -> Result<&P, Self::Error>;
    /// Reads message context without allocating a payload or projecting an envelope.
    fn context(&self) -> Result<MessageContext<'_>, Self::Error>;
}

/// The same statically dispatched business handler accepts either delivery kind.
pub trait Handler<P>: Send + 'static {
    /// The delivery may be retained across awaits or moved into an application task.
    fn on_receive<D: Delivery<P>>(&mut self, delivery: D) -> impl Future<Output = ()> + Send;
}

/// RPC business decision, independent of response construction and representation.
pub trait RequestHandler<P>: Send + 'static {
    /// Decision data passed to the setup-bound response constructor.
    type Reply: Send;
    /// Processes a retained checked request; no response TX loan exists yet.
    fn handle<D: Delivery<P>>(
        &mut self,
        request: D,
    ) -> impl Future<Output = Result<Self::Reply, UCode>> + Send;
}

/// Per-route publication and notification defaults.
#[derive(Clone, Copy, Debug, Default)]
pub struct PublishOptions {
    /// Priority, or the protocol default.
    pub priority: Option<UPriority>,
    /// Time to live, or no expiry.
    pub ttl: Option<Duration>,
}

/// RPC limits shared by both representations.
#[derive(Clone, Copy, Debug)]
pub struct RpcOptions {
    /// Bound on simultaneous requests.
    pub max_in_flight: usize,
    /// Request TTL and total response deadline, starting at submission.
    pub ttl: Duration,
    /// Reserved credential capacity.
    pub max_token_len: usize,
    /// At least CS4.
    pub priority: UPriority,
}

/// Server limits; receive inbox/header limits belong to endpoint setup.
#[derive(Clone, Copy, Debug)]
pub struct ServerOptions {
    /// Maximum distinct response destinations retained by this server.
    pub max_clients: usize,
}

/// One endpoint selects representation, binding/envelope and local URI identity.
pub struct Endpoint<S, U: LocalUriProvider + ?Sized> {
    setup: S,
    uri_provider: Arc<U>,
}
impl<S, U: LocalUriProvider + ?Sized> Endpoint<S, U> {
    /// Uses admitted representation setup and local semantic URI construction.
    pub fn new(setup: S, uri_provider: Arc<U>) -> Self {
        Self {
            setup,
            uri_provider,
        }
    }
}
impl<S: Clone, U: LocalUriProvider + ?Sized> Endpoint<S, U> {
    /// Retains the real uSubscription service for all remote subscription changes.
    pub fn subscriber(
        &self,
        service: Arc<dyn crate::core::usubscription::USubscription>,
    ) -> Subscriber<S> {
        Subscriber {
            setup: self.setup.clone(),
            service,
            source_uri: self.uri_provider.get_source_uri(),
        }
    }
}

/// Typed publisher; concrete engine type is inferred from setup.
#[cfg(feature = "up-l2-zero-copy")]
pub struct Publisher<P, R> {
    inner: R,
    payload: PhantomData<fn() -> P>,
}
/// Typed notifier with one exact destination.
#[cfg(feature = "up-l2-zero-copy")]
pub struct Notifier<P, R> {
    inner: R,
    payload: PhantomData<fn() -> P>,
}
/// Subscription-service coordination over the configured representation.
pub struct Subscriber<S> {
    setup: S,
    service: Arc<dyn crate::core::usubscription::USubscription>,
    source_uri: UUri,
}
/// Notification receiver owning local delivery and its worker.
#[must_use = "dropping the receiver closes and detaches delivery"]
#[cfg(feature = "up-l2-zero-copy")]
pub struct Receiver<R> {
    inner: R,
}
/// Typed RPC client with bounded correlation and cancellation state.
#[cfg(feature = "up-l2-zero-copy")]
pub struct RpcClient<Req, Resp, R> {
    inner: R,
    payload: PhantomData<fn(Req) -> Resp>,
}
/// Typed RPC server owning admission, worker and response-route resources.
#[must_use = "dropping the server closes and detaches requests"]
#[cfg(feature = "up-l2-zero-copy")]
pub struct RpcServer<R> {
    inner: R,
}

/// Decoded owning delivery; its context retains the validated original metadata.
pub struct Decoded<P> {
    pub(crate) payload: P,
    pub(crate) metadata: UFrameMetadata,
}
impl<P: Send + 'static> Delivery<P> for Decoded<P> {
    type Error = Infallible;
    fn payload(&self) -> Result<&P, Infallible> {
        Ok(&self.payload)
    }
    fn context(&self) -> Result<MessageContext<'_>, Infallible> {
        let m = &self.metadata;
        Ok(MessageContext {
            kind: m.kind(),
            source: m.source().clone(),
            sink: m.sink().cloned(),
            id: m.id().clone(),
            request_id: m.reqid().cloned(),
            comm_status: m.comm_status(),
            payload_encoding: m.payload_encoding().copied(),
            native_type_token: m.native_type_token(),
            token: m.token(),
            permission_level: m.permission_level(),
            priority: m.priority(),
            ttl: m.ttl(),
            traceparent: m.traceparent(),
        })
    }
}

impl<S, U: LocalUriProvider + ?Sized> std::fmt::Debug for Endpoint<S, U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint").finish_non_exhaustive()
    }
}
#[cfg(feature = "up-l2-zero-copy")]
impl<P, R> std::fmt::Debug for Publisher<P, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Publisher").finish_non_exhaustive()
    }
}
#[cfg(feature = "up-l2-zero-copy")]
impl<P, R> std::fmt::Debug for Notifier<P, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Notifier").finish_non_exhaustive()
    }
}
impl<S> std::fmt::Debug for Subscriber<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscriber").finish_non_exhaustive()
    }
}
#[cfg(feature = "up-l2-zero-copy")]
impl<R> std::fmt::Debug for Receiver<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Receiver").finish_non_exhaustive()
    }
}
#[cfg(feature = "up-l2-zero-copy")]
impl<Req, Resp, R> std::fmt::Debug for RpcClient<Req, Resp, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcClient").finish_non_exhaustive()
    }
}
#[cfg(feature = "up-l2-zero-copy")]
impl<R> std::fmt::Debug for RpcServer<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcServer").finish_non_exhaustive()
    }
}
impl<P> std::fmt::Debug for Decoded<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decoded").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for MessageContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageContext")
            .field("kind", &self.kind)
            .field("source", &self.source)
            .field("sink", &self.sink)
            .field("id", &self.id)
            .field("token_len", &self.token.map(str::len))
            .finish_non_exhaustive()
    }
}
