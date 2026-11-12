//! Modern serialized implementer contracts. Implement every method you need with an
//! ordinary `async fn`; do not put `#[async_trait]` on these impls. The SDK erases the
//! send future inline, so per-send dispatch never allocates and no per-transport
//! future state machine is required. Receive and setup futures still cross the
//! object-safe adapter, which boxes them at its `dyn` boundary. Existing async-trait
//! `UOwnedTransport` implementations stay boxed unless explicitly migrated or
//! overriding the dispatch hook with static work.
use crate::wire_transport::{
    EncodedOwnedFrame, PreparedOwnedFrame, UEncodedOwnedListener, UOwnedTransportCore,
};
use crate::{
    OwnedAttachment, UCode, UOwnedFrame, UOwnedListener, UOwnedTransportImpl, UStatus, UUri,
};
use async_trait::async_trait;
use std::{future::Future, sync::Arc};

/// Modern semantic carriage. Implement any method with a plain `async fn`; every
/// optional method defaults to an explicit refusal.
pub trait StaticOwnedTransport: Send + Sync {
    /// Admits the actual payload identity/family against configured carriage.
    /// Unsupported is the default; payload-agnostic transports explicitly opt in.
    fn admit_payload_contract(
        &self,
        _identity: crate::PayloadIdentity,
        _family: Option<crate::wire::WireIdentity>,
    ) -> Result<(), crate::UWireError> {
        Err(crate::UWireError::invalid_payload(
            "transport has no admitted payload/envelope contract",
        ))
    }

    /// Admits a payload contract for one-way carriage: publish, notify and their
    /// receivers.
    ///
    /// Defaults to [`admit_payload_contract`](Self::admit_payload_contract). A binding
    /// whose one-way carriage assumes a single payload encoding overrides this to
    /// refuse a mismatch at setup rather than on the first send. RPC roles keep the
    /// general hook because they carry payload identity per request.
    fn admit_carriage_payload_contract(
        &self,
        identity: crate::PayloadIdentity,
        family: Option<crate::wire::WireIdentity>,
    ) -> Result<(), crate::UWireError> {
        self.admit_payload_contract(identity, family)
    }

    /// Binding-owned RPC authority, forwarded by [`InlineOwnedTransport`].
    /// `None` keeps the SDK's generic correlation; see `crate::owned_rpc`.
    fn owned_rpc(&self) -> Option<&dyn crate::owned_rpc::OwnedRpc> {
        None
    }

    /// Sends an already validated owned frame.
    fn send_validated_owned(
        &self,
        frame: UOwnedFrame,
    ) -> impl Future<Output = Result<(), UStatus>> + Send + '_;

    /// Receives one matching owned frame from transports that support pull receive.
    fn receive_validated_owned(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> impl Future<Output = Result<UOwnedFrame, UStatus>> + Send {
        std::future::ready(Err(UStatus::fail_with_code(
            UCode::Unimplemented,
            "not implemented",
        )))
    }

    /// Registers an owned listener after public filter validation.
    fn register_validated_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UOwnedListener>,
    ) -> impl Future<Output = Result<(), UStatus>> + Send {
        std::future::ready(Err(UStatus::fail_with_code(
            UCode::Unimplemented,
            "not implemented",
        )))
    }

    /// Attaches a listener with an owned synchronous release capability.
    /// On failure OR cancellation, implementations must leave no registration.
    /// A transport supporting only async legacy detach must refuse this method;
    /// returning a no-op handle would violate resource conservation.
    fn attach_validated_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UOwnedListener>,
    ) -> impl Future<Output = Result<OwnedAttachment, UStatus>> + Send {
        std::future::ready(Err(UStatus::fail_with_code(
            UCode::Unimplemented,
            "scoped owned attachment required",
        )))
    }

    /// Unregisters an owned listener after public filter validation.
    fn unregister_validated_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UOwnedListener>,
    ) -> impl Future<Output = Result<(), UStatus>> + Send {
        std::future::ready(Err(UStatus::fail_with_code(
            UCode::Unimplemented,
            "not implemented",
        )))
    }
}

/// SDK adapter from static send futures to the existing object-safe contract.
/// Construction does not allocate. Only send dispatch is unboxed; setup and
/// receive retain the existing async-trait contracts and their allocations.
pub struct InlineOwnedTransport<T>(T);
impl<T> InlineOwnedTransport<T> {
    /// Wraps a modern implementation; send layout is admitted at code generation.
    pub fn new(inner: T) -> Self {
        Self(inner)
    }
    /// Borrows the concrete implementation for advanced configuration.
    pub fn inner(&self) -> &T {
        &self.0
    }
}
impl<T> std::fmt::Debug for InlineOwnedTransport<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InlineOwnedTransport")
            .finish_non_exhaustive()
    }
}
#[async_trait]
impl<T: StaticOwnedTransport> UOwnedTransportImpl for InlineOwnedTransport<T> {
    fn admit_payload_contract(
        &self,
        identity: crate::PayloadIdentity,
        family: Option<crate::wire::WireIdentity>,
    ) -> Result<(), crate::UWireError> {
        self.0.admit_payload_contract(identity, family)
    }
    fn admit_carriage_payload_contract(
        &self,
        identity: crate::PayloadIdentity,
        family: Option<crate::wire::WireIdentity>,
    ) -> Result<(), crate::UWireError> {
        self.0.admit_carriage_payload_contract(identity, family)
    }
    fn owned_rpc(&self) -> Option<&dyn crate::owned_rpc::OwnedRpc> {
        self.0.owned_rpc()
    }
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.0.send_validated_owned(frame).await
    }
    fn dispatch_owned(&self, frame: UOwnedFrame) -> crate::inline_dispatch::SendFuture<'_> {
        crate::inline_dispatch::SendFuture::new(
            async move { self.0.send_validated_owned(frame).await },
        )
    }
    async fn receive_validated_owned(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        self.0.receive_validated_owned(source, sink).await
    }
    async fn register_validated_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        self.0
            .register_validated_owned_listener(source, sink, listener)
            .await
    }
    async fn attach_validated_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<OwnedAttachment, UStatus> {
        self.0
            .attach_validated_owned_listener(source, sink, listener)
            .await
    }
    async fn unregister_validated_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        self.0
            .unregister_validated_owned_listener(source, sink, listener)
            .await
    }
}

/// Modern encoded carriage. The wire adapter still validates/encodes metadata.
/// Implement any method with a plain `async fn`; optional methods default to refusal.
pub trait StaticOwnedCore: Send + Sync {
    /// Sends an owned frame with already encoded metadata bytes.
    fn send_prepared_owned(
        &self,
        frame: PreparedOwnedFrame,
    ) -> impl Future<Output = Result<(), UStatus>> + Send + '_;

    /// Receives one matching raw encoded owned frame from cores that support pull receive.
    fn receive_encoded_owned(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> impl Future<Output = Result<EncodedOwnedFrame, UStatus>> + Send {
        std::future::ready(Err(unimplemented()))
    }

    /// Registers a raw encoded owned listener after public filter validation.
    fn register_encoded_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UEncodedOwnedListener>,
    ) -> impl Future<Output = Result<(), UStatus>> + Send {
        std::future::ready(Err(unimplemented()))
    }

    /// Attaches with a synchronous exact release handle. Cancellation and failure
    /// must roll back partial registration. The default refuses unsupported cores.
    fn attach_encoded_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UEncodedOwnedListener>,
    ) -> impl Future<Output = Result<crate::OwnedAttachment, UStatus>> + Send {
        std::future::ready(Err(unimplemented()))
    }

    /// Unregisters a raw encoded owned listener after public filter validation.
    fn unregister_encoded_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UEncodedOwnedListener>,
    ) -> impl Future<Output = Result<(), UStatus>> + Send {
        std::future::ready(Err(unimplemented()))
    }
}

/// SDK adapter from static send futures to the existing object-safe contract.
/// Construction does not allocate. Only send dispatch is unboxed; setup and
/// receive retain the existing async-trait contracts and their allocations.
pub struct InlineOwnedCore<T>(T);
impl<T> InlineOwnedCore<T> {
    /// Wraps a modern implementation; send layout is admitted at code generation.
    pub fn new(inner: T) -> Self {
        Self(inner)
    }
    /// Borrows the concrete implementation for advanced configuration.
    pub fn inner(&self) -> &T {
        &self.0
    }
}
impl<T> std::fmt::Debug for InlineOwnedCore<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InlineOwnedCore")
            .finish_non_exhaustive()
    }
}
#[async_trait]
impl<T: StaticOwnedCore> UOwnedTransportCore for InlineOwnedCore<T> {
    async fn send_prepared_owned(&self, frame: PreparedOwnedFrame) -> Result<(), UStatus> {
        self.0.send_prepared_owned(frame).await
    }
    fn dispatch_prepared_owned(
        &self,
        frame: PreparedOwnedFrame,
    ) -> crate::inline_dispatch::CoreSendFuture<'_> {
        crate::inline_dispatch::CoreSendFuture::new(async move {
            self.0.send_prepared_owned(frame).await
        })
    }
    async fn receive_encoded_owned(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
    ) -> Result<EncodedOwnedFrame, UStatus> {
        self.0.receive_encoded_owned(source, sink).await
    }
    async fn register_encoded_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UEncodedOwnedListener>,
    ) -> Result<(), UStatus> {
        self.0
            .register_encoded_owned_listener(source, sink, listener)
            .await
    }
    async fn attach_encoded_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UEncodedOwnedListener>,
    ) -> Result<OwnedAttachment, UStatus> {
        self.0
            .attach_encoded_owned_listener(source, sink, listener)
            .await
    }
    async fn unregister_encoded_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UEncodedOwnedListener>,
    ) -> Result<(), UStatus> {
        self.0
            .unregister_encoded_owned_listener(source, sink, listener)
            .await
    }
}

/// Bounded in-process owned-frame mailbox with setup-reserved queue storage.
///
/// Wrap in [`InlineOwnedTransport`] for a public dyn handle. Successful sends
/// move a validated frame into the queue without future or queue allocation.
/// This is send-only carriage with explicit synchronous consumption, not a
/// listener bus or a middleware-native transport. Full queues refuse immediately;
/// there is no hidden unbounded buffering, retry task, or automatic fallback.
pub struct OwnedMailbox {
    frames: std::sync::Mutex<std::collections::VecDeque<UOwnedFrame>>,
    capacity: usize,
}

impl std::fmt::Debug for OwnedMailbox {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OwnedMailbox")
            .field("capacity", &self.capacity)
            .finish_non_exhaustive()
    }
}

impl OwnedMailbox {
    /// Reserves all queue slots before publication of the transport.
    ///
    /// # Errors
    /// Refuses zero capacity and allocation/size overflow during reservation.
    pub fn new(capacity: usize) -> Result<Self, UStatus> {
        if capacity == 0 {
            return Err(UStatus::fail_with_code(
                UCode::InvalidArgument,
                "mailbox capacity must be positive",
            ));
        }
        let mut frames = std::collections::VecDeque::new();
        frames.try_reserve_exact(capacity).map_err(|_| {
            UStatus::fail_with_code(UCode::ResourceExhausted, "mailbox queue reservation failed")
        })?;
        Ok(Self {
            frames: std::sync::Mutex::new(frames),
            capacity,
        })
    }

    /// Removes one queued frame, without cloning its payload or allocating.
    ///
    /// # Errors
    /// Reports a poisoned queue; `None` means the queue is currently empty.
    pub fn try_take(&self) -> Result<Option<UOwnedFrame>, UStatus> {
        let mut frames = self
            .frames
            .lock()
            .map_err(|_| UStatus::fail_with_code(UCode::Internal, "mailbox lock poisoned"))?;
        Ok(frames.pop_front())
    }
}

impl StaticOwnedTransport for OwnedMailbox {
    fn admit_payload_contract(
        &self,
        identity: crate::PayloadIdentity,
        _: Option<crate::wire::WireIdentity>,
    ) -> Result<(), crate::UWireError> {
        if matches!(identity, crate::PayloadIdentity::Fixed(_)) {
            Ok(())
        } else {
            Err(crate::UWireError::invalid_payload(
                "mailbox has no native storage contract",
            ))
        }
    }

    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        let mut frames = self
            .frames
            .lock()
            .map_err(|_| UStatus::fail_with_code(UCode::Internal, "mailbox lock poisoned"))?;
        if frames.len() == self.capacity {
            return Err(UStatus::fail_with_code(
                UCode::ResourceExhausted,
                "mailbox full",
            ));
        }
        frames.push_back(frame);
        Ok(())
    }
}

fn unimplemented() -> UStatus {
    UStatus::fail_with_code(UCode::Unimplemented, "not implemented")
}
