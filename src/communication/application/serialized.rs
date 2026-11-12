use super::*;
use crate::{
    OwnedAttachment, PayloadDecodeLimit, Registration, RegistrationHandler, RegistrationInbox,
    RegistrationSetupError, RegistrationStopError, UMessageType, UOwnedFrame, UOwnedListener,
    UStatus, UWireError,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Serialized setup, encoding and transport errors. Native operations keep their
/// bounded native error types. Serialized sends use bounded inline dispatch;
/// legacy implementations and real encoding/storage retain their actual costs.
#[derive(Debug, thiserror::Error)]
pub enum SerializedError {
    /// Invalid local configuration.
    #[error("invalid setup: {0}")]
    Config(&'static str),
    /// Payload identity/schema/frame failure.
    #[error("payload/frame: {0}")]
    Payload(#[from] UWireError),
    /// Physical carriage or capability refusal.
    #[error("transport: {0}")]
    Transport(UStatus),
    /// Receive worker reservation failed.
    #[error("receive preparation: {0}")]
    Prepare(RegistrationSetupError),
}

pub(super) fn metadata_error(error: crate::UFrameMetadataError) -> UWireError {
    UWireError::invalid_payload(error.to_string())
}

/// Exact route inferred from publisher/notifier setup.
#[doc(hidden)]
pub(super) struct SerializedRoute<P> {
    setup: Deployment,
    encoder: Encoder<P>,
    source: UUri,
    sink: Option<UUri>,
    options: PublishOptions,
}
impl<P> SerializedRoute<P> {
    fn metadata(&self) -> Result<UFrameMetadata, UWireError> {
        let mut builder = if let Some(sink) = &self.sink {
            UFrameMetadata::notification(self.source.clone(), sink.clone())
        } else {
            UFrameMetadata::publish(self.source.clone())
        };
        if let Some(priority) = self.options.priority {
            builder = builder.with_priority(priority);
        }
        if let Some(ttl) = self.options.ttl {
            builder = builder.with_ttl(ttl);
        }
        self.encoder
            .identity()
            .apply_to(builder.build().map_err(metadata_error)?)
    }
    async fn send(&self, value: &P) -> Result<(), SerializedError> {
        let payload = self.encoder.encode(value)?;
        let frame = UOwnedFrame::with_payload(self.metadata()?, payload).map_err(metadata_error)?;
        self.setup
            .transport
            .send_owned(frame)
            .await
            .map_err(SerializedError::Transport)
    }
}

impl Deployment {
    /// Resolves one event and the codec's actual domain mapping before traffic.
    ///
    /// # Errors
    /// Refuses invalid local event metadata; missing codec mapping is a type error.
    pub fn publisher<P>(
        &self,
        resource: u16,
        options: PublishOptions,
        encoder: Encoder<P>,
    ) -> Result<Publisher<P>, SerializedError> {
        encoder.admit_carriage(self.transport.as_ref())?;
        let route = SerializedRoute {
            setup: self.clone(),
            encoder,
            source: self.uri_provider.get_resource_uri(resource),
            sink: None,
            options,
        };
        route.metadata()?;
        Ok(Publisher { inner: route })
    }
    /// Resolves one event and exact destination; no wildcard notification fanout.
    ///
    /// # Errors
    /// Refuses invalid event/destination metadata at setup.
    pub fn notifier<P>(
        &self,
        resource: u16,
        destination: UUri,
        options: PublishOptions,
        encoder: Encoder<P>,
    ) -> Result<Notifier<P>, SerializedError> {
        destination
            .verify_no_wildcards()
            .map_err(|_| SerializedError::Config("exact notification destination required"))?;
        encoder.admit_carriage(self.transport.as_ref())?;
        let route = SerializedRoute {
            setup: self.clone(),
            encoder,
            source: self.uri_provider.get_resource_uri(resource),
            sink: Some(destination),
            options,
        };
        route.metadata()?;
        Ok(Notifier { inner: route })
    }
}
impl<P> Publisher<P> {
    /// Encodes a borrowed domain value with the setup-bound codec.
    ///
    /// # Errors
    /// Reports codec/frame or transport failure.
    pub async fn publish(&self, value: &P) -> Result<(), SerializedError> {
        self.inner.send(value).await
    }
}
impl<P> Notifier<P> {
    /// Sends one encoded notification to the setup-bound exact destination.
    ///
    /// # Errors
    /// Reports codec/frame or transport failure.
    pub async fn notify(&self, value: &P) -> Result<(), SerializedError> {
        self.inner.send(value).await
    }
}

pub(super) fn decode<P>(
    frame: UOwnedFrame,
    decoder: Decoder<P>,
    limit: PayloadDecodeLimit,
) -> Result<Decoded<P>, UWireError> {
    decoder.identity().verify(
        frame.metadata().payload_encoding(),
        frame.metadata().native_type_token(),
    )?;
    let bytes = frame.payload().ok_or(UWireError::MissingPayload)?;
    if bytes.len() > limit.max_payload_bytes() {
        return Err(UWireError::invalid_payload(
            "encoded receive limit exceeded",
        ));
    }
    let payload = decoder.decode(bytes)?;
    let (metadata, _) = frame.into_parts();
    Ok(Decoded { payload, metadata })
}

/// Diagnostics of decoded/address admission; inbox counts remain separate.
#[derive(Default, Debug)]
pub struct SerializedReceiveStats {
    /// Wrong kind/source/sink or inactive service subscription.
    pub filtered: AtomicU64,
    /// Payload identity, length or mapping refusal.
    pub rejected: AtomicU64,
}
struct DecodeWorker<P, H> {
    handler: H,
    source: UUri,
    sink: Option<UUri>,
    kind: UMessageType,
    decoder: Decoder<P>,
    limit: PayloadDecodeLimit,
    stats: Arc<SerializedReceiveStats>,
    active: Arc<AtomicBool>,
}
impl<P, H> RegistrationHandler<UOwnedFrame> for DecodeWorker<P, H>
where
    P: Send + 'static,
    H: Handler<P>,
{
    async fn on_receive(&mut self, frame: UOwnedFrame) {
        let m = frame.metadata();
        if !self.active.load(Ordering::Acquire)
            || m.kind() != self.kind
            || m.source() != &self.source
            || m.sink() != self.sink.as_ref()
        {
            self.stats.filtered.fetch_add(1, Ordering::Relaxed);
            return;
        }
        match decode(frame, self.decoder, self.limit) {
            Ok(value) => self.handler.on_receive(value).await,
            Err(_) => {
                self.stats.rejected.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

pub(super) struct InboxListener(pub(super) Arc<RegistrationInbox<UOwnedFrame>>);
#[async_trait::async_trait]
impl UOwnedListener for InboxListener {
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        let _ = self.0.push(frame);
    }
}

/// All ordered shutdown failures; service, detach and drain are all attempted.
#[derive(Debug, thiserror::Error)]
#[error("shutdown: service={service:?}, detach={detach:?}, worker={worker:?}")]
pub struct SerializedStopError {
    /// Remote service failure.
    pub service: Option<UStatus>,
    /// Physical detach failure.
    pub detach: Option<UStatus>,
    /// Worker drain failure.
    pub worker: Option<RegistrationStopError>,
}
impl SerializedStopError {
    /// Resource completion of this stop failure, matching the native stop
    /// errors' classifiers: service uncertainty, a failed detach or a
    /// nonterminal worker refusal stays retryable; a terminal worker is final.
    pub fn completion(&self) -> crate::Completion {
        if self.service.is_some()
            || self.detach.is_some()
            || self.worker.is_some_and(|error| !error.is_terminal())
        {
            crate::Completion::Pending
        } else if self.worker.is_some() {
            crate::Completion::Failed
        } else {
            crate::Completion::Succeeded
        }
    }
}

/// Owns scoped attachment and bounded decode worker. Drop closes, detaches and
/// aborts; explicit stop drains accepted work. Retained deliveries remain valid.
#[doc(hidden)]
pub struct SerializedReceiver {
    attachment: OwnedAttachment,
    registration: Option<Registration<UOwnedFrame>>,
    stats: Arc<SerializedReceiveStats>,
    active: Arc<AtomicBool>,
    worker_error: Option<RegistrationStopError>,
    progress: crate::StopProgress<UStatus>,
}
impl SerializedReceiver {
    async fn start<P, H>(
        setup: &Deployment,
        decoder: Decoder<P>,
        source: UUri,
        sink: Option<UUri>,
        kind: UMessageType,
        handler: H,
        active: bool,
    ) -> Result<Self, SerializedError>
    where
        P: Send + 'static,
        H: Handler<P>,
    {
        decoder.admit_carriage(setup.transport.as_ref())?;
        source
            .verify_event()
            .map_err(|_| SerializedError::Config("event topic required"))?;
        source
            .verify_no_wildcards()
            .map_err(|_| SerializedError::Config("exact event topic required"))?;
        if let Some(sink) = &sink {
            sink.verify_no_wildcards()
                .map_err(|_| SerializedError::Config("exact destination required"))?;
        }
        let stats = Arc::new(SerializedReceiveStats::default());
        let active = Arc::new(AtomicBool::new(active));
        let registration = Registration::new(
            setup.registration,
            DecodeWorker::<P, H> {
                handler,
                source: source.clone(),
                sink: sink.clone(),
                kind,
                decoder,
                limit: setup.limit,
                stats: stats.clone(),
                active: active.clone(),
            },
            || {},
        )
        .map_err(SerializedError::Prepare)?;
        let attachment = setup
            .transport
            .attach_owned_listener(
                &source,
                sink.as_ref(),
                Arc::new(InboxListener(registration.inbox())),
            )
            .await
            .map_err(SerializedError::Transport)?;
        Ok(Self {
            attachment,
            registration: Some(registration),
            stats,
            active,
            worker_error: None,
            progress: crate::StopProgress::default(),
        })
    }
    pub(super) fn stop(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), SerializedStopError>> + Send + '_ {
        self.close_admission();
        let detach = self.attachment.detach().err();
        self.progress.begin(detach.clone());
        async move {
            let worker = if let Some(registration) = &mut self.registration {
                registration.stop_ref().await.err()
            } else {
                None
            };
            let worker = super::super::typed::retain_worker_outcome(&mut self.worker_error, worker);
            self.progress.finish(worker);
            if detach.is_some() || worker.is_some() {
                Err(SerializedStopError {
                    service: None,
                    detach,
                    worker,
                })
            } else {
                Ok(())
            }
        }
    }
}
impl Drop for SerializedReceiver {
    fn drop(&mut self) {
        if let Some(registration) = &self.registration {
            registration.inbox().close();
        }
        let _ = self.attachment.detach();
    }
}

impl Deployment {
    /// Listens for this endpoint's exact notification destination.
    ///
    /// # Errors
    /// Refuses invalid addressing, missing scoped receive capability or capacity.
    pub async fn listen<P, H>(
        &self,
        topic: UUri,
        handler: H,
        decoder: Decoder<P>,
    ) -> Result<Receiver, SerializedError>
    where
        P: Send + 'static,
        H: Handler<P>,
    {
        let inner = SerializedReceiver::start::<P, H>(
            self,
            decoder,
            topic,
            Some(self.uri_provider.get_source_uri()),
            UMessageType::Notification,
            handler,
            true,
        )
        .await?;
        Ok(Receiver { inner })
    }
}
impl Receiver {
    /// Address and codec rejection counters.
    pub fn stats(&self) -> &SerializedReceiveStats {
        &self.inner.stats
    }
    /// Closes, detaches and drains the receiver.
    ///
    /// # Errors
    /// Preserves physical and worker failures.
    pub fn stop(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), SerializedStopError>> + Send + '_ {
        self.inner.stop()
    }
    /// Close local admission without waiting for remote or worker progress.
    pub fn close_admission(&self) {
        self.inner.close_admission();
    }
    /// Current and first-failure observations, including cancelled attempts.
    pub fn stop_progress(&self) -> &crate::StopProgress<UStatus> {
        &self.inner.progress
    }
}

impl SubscriptionLocal for SerializedReceiver {
    type Error = UStatus;
    fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Release);
    }
    fn close_admission(&self) {
        if let Some(r) = &self.registration {
            r.inbox().close();
        }
    }
    fn detach(&mut self) -> Result<(), UStatus> {
        self.attachment.detach()
    }
    async fn drain(&mut self) -> Result<(), RegistrationStopError> {
        if let Some(r) = &mut self.registration {
            r.stop_ref().await
        } else {
            Ok(())
        }
    }
}
impl Subscriber {
    /// Prepares local reception with business delivery gated off. No remote call
    /// occurs until the returned ticket's borrowed `activate` operation.
    ///
    /// # Errors
    /// Refuses addressing, mapping or receive capability before remote side effects.
    pub async fn subscription<P, H>(
        &self,
        topic: UUri,
        handler: H,
        decoder: Decoder<P>,
    ) -> Result<Subscription, SerializedError>
    where
        P: Send + 'static,
        H: Handler<P>,
    {
        let receiver = SerializedReceiver::start::<P, H>(
            &self.setup,
            decoder,
            topic.clone(),
            None,
            UMessageType::Publish,
            handler,
            false,
        )
        .await?;
        Ok(Subscription::new(
            receiver,
            self.service.clone(),
            topic,
            self.setup.uri_provider.get_source_uri(),
        ))
    }
}
impl super::super::typed::Subscription<SerializedReceiver> {
    /// Address/identity rejection counters retained with this ticket.
    pub fn stats(&self) -> &SerializedReceiveStats {
        &self.inner.stats
    }
}
impl std::fmt::Debug for SerializedReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SerializedReceiver").finish_non_exhaustive()
    }
}
