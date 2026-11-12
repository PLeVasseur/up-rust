use super::serialized::{decode, metadata_error};
use super::*;
use crate::owned_rpc::{self, OwnedRpcCall, OwnedRpcClient};
use crate::pending_calls::{PendingCalls, PendingGuard, ReserveError};
use crate::{
    InboxPushError, OwnedAttachment, PayloadDecodeLimit, PayloadIdentity, Registration,
    RegistrationHandler, RegistrationInbox, UMessageType, UOwnedFrame, UOwnedListener, UStatus,
    UWireError,
};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex, MutexGuard,
};

fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Submission, deadline, remote status and payload errors on serialized RPC.
#[derive(Debug, thiserror::Error)]
pub enum SerializedCallError {
    /// Every reserved slot is occupied.
    #[error("too many RPC calls in flight")]
    TooManyInFlight,
    /// Deadline includes asynchronous physical submission time.
    #[error("RPC timed out")]
    TimedOut,
    /// Response admission has closed.
    #[error("RPC client closed")]
    Closed,
    /// Codec, physical send or binding-owned completion failed.
    #[error("RPC submission: {0}")]
    Submit(#[from] SerializedError),
    /// Correlated peer returned an error status.
    #[error("RPC status: {0:?}")]
    Status(UCode),
    /// Typed decoding failed.
    #[error("RPC response: {0}")]
    Response(UWireError),
}

/// Response address/identity and correlation diagnostics.
#[derive(Debug, Default)]
pub struct SerializedRpcStats {
    /// Incorrect kind, source, sink, payload identity or length.
    pub rejected: AtomicU64,
    /// Unknown, cancelled, duplicate or late request IDs.
    pub unmatched: AtomicU64,
}

/// Exact response admission shared by the generic listener and owned completions.
fn admissible_response(
    frame: &UOwnedFrame,
    method: &UUri,
    reply_to: &UUri,
    identity: PayloadIdentity,
    limit: PayloadDecodeLimit,
) -> bool {
    let m = frame.metadata();
    let status_only = m.comm_status().is_some_and(|s| s != UCode::Ok);
    let valid_payload = if status_only {
        frame.payload().is_none()
            && m.payload_encoding().is_none()
            && m.native_type_token().is_none()
    } else {
        identity
            .verify(m.payload_encoding(), m.native_type_token())
            .is_ok()
            && frame
                .payload()
                .is_some_and(|p| p.len() <= limit.max_payload_bytes())
    };
    m.kind() == UMessageType::Response
        && m.source() == method
        && m.sink() == Some(reply_to)
        && valid_payload
}

struct ResponseSink {
    calls: Arc<PendingCalls<UOwnedFrame>>,
    source: UUri,
    sink: UUri,
    identity: PayloadIdentity,
    limit: PayloadDecodeLimit,
    stats: Arc<SerializedRpcStats>,
}
#[async_trait::async_trait]
impl UOwnedListener for ResponseSink {
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        if self.calls.is_closed() {
            self.stats.rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if !admissible_response(&frame, &self.source, &self.sink, self.identity, self.limit) {
            self.stats.rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let Some(id) = frame.metadata().reqid().cloned() else {
            self.stats.rejected.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if !self.calls.deliver(&id, frame) {
            self.stats.unmatched.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Response correlation selected once at setup and never switched afterwards.
enum Correlation {
    /// SDK bounded table plus the exact response attachment.
    Generic {
        pending: Arc<PendingCalls<UOwnedFrame>>,
        attachment: Mutex<OwnedAttachment>,
    },
    /// The binding's completion authority. No SDK table or listener exists.
    Owned {
        client: Box<dyn OwnedRpcClient>,
        closed: AtomicBool,
    },
}
impl Correlation {
    fn close(&self) -> Result<(), UStatus> {
        match self {
            Self::Generic {
                pending,
                attachment,
            } => {
                pending.close();
                lock(attachment).detach()
            }
            Self::Owned { client, closed } => {
                closed.store(true, Ordering::Release);
                client.close()
            }
        }
    }
    fn in_flight(&self) -> usize {
        match self {
            Self::Generic { pending, .. } => pending.in_flight(),
            Self::Owned { client, .. } => client.in_flight(),
        }
    }
}

/// Serialized client mechanics with distinct request and response schema tables.
#[doc(hidden)]
pub(super) struct SerializedRpcClient<Req, Resp> {
    setup: Deployment,
    request: Encoder<Req>,
    response: Decoder<Resp>,
    method: UUri,
    reply_to: UUri,
    options: RpcOptions,
    correlation: Correlation,
    token: Mutex<(String, bool)>,
    stats: Arc<SerializedRpcStats>,
}
impl<Req, Resp> Drop for SerializedRpcClient<Req, Resp> {
    fn drop(&mut self) {
        let _ = self.correlation.close();
    }
}
impl<Req, Resp> SerializedRpcClient<Req, Resp> {
    fn request_metadata(&self, id: UUID) -> Result<UFrameMetadata, SerializedError> {
        let mut builder =
            UFrameMetadata::request(self.method.clone(), self.reply_to.clone(), self.options.ttl)
                .with_id(id)
                .with_priority(self.options.priority);
        {
            let token = lock(&self.token);
            if token.1 {
                builder = builder.with_token(token.0.as_str());
            }
        }
        self.request
            .identity()
            .apply_to(
                builder
                    .build()
                    .map_err(metadata_error)
                    .map_err(SerializedError::Payload)?,
            )
            .map_err(SerializedError::Payload)
    }
    fn request_frame(
        &self,
        metadata: UFrameMetadata,
        value: &Req,
    ) -> Result<UOwnedFrame, SerializedError> {
        let bytes = self
            .request
            .encode(value)
            .map_err(SerializedError::Payload)?;
        UOwnedFrame::with_payload(metadata, bytes)
            .map_err(metadata_error)
            .map_err(SerializedError::Payload)
    }
}

/// Capacity refusal keeps the generic error; every other refusal is final.
fn prepare_error(status: UStatus) -> SerializedCallError {
    if status.code() == UCode::ResourceExhausted {
        SerializedCallError::TooManyInFlight
    } else {
        SerializedCallError::Submit(SerializedError::Transport(status))
    }
}

/// Binding completion: expiry is a timeout, closure is closure, others are final.
fn completion_error(status: UStatus) -> SerializedCallError {
    let code = status.code();
    if code == UCode::DeadlineExceeded {
        SerializedCallError::TimedOut
    } else if code == UCode::Unavailable {
        SerializedCallError::Closed
    } else {
        SerializedCallError::Submit(SerializedError::Transport(status))
    }
}

impl Deployment {
    /// Installs response correlation before any request can be sent.
    ///
    /// A transport supplying [`crate::owned_rpc::OwnedRpc`] installs only the
    /// binding's client role: no generic table or listener, and no fallback.
    ///
    /// # Errors
    /// Refuses invalid methods/options, allocation or absent scoped receive support.
    pub async fn rpc_client<Req, Resp>(
        &self,
        method: UUri,
        options: RpcOptions,
        request: Encoder<Req>,
        response: Decoder<Resp>,
    ) -> Result<RpcClient<Req, Resp>, SerializedError> {
        request.admit(self.transport.as_ref())?;
        response.admit(self.transport.as_ref())?;
        if options.max_in_flight == 0 || options.ttl.is_zero() {
            return Err(SerializedError::Config(
                "RPC capacity and TTL must be positive",
            ));
        }
        let reply_to = self.uri_provider.get_source_uri();
        UFrameMetadata::request(method.clone(), reply_to.clone(), options.ttl)
            .with_priority(options.priority)
            .build()
            .map_err(metadata_error)?;
        let stats = Arc::new(SerializedRpcStats::default());
        let mut token = String::new();
        token
            .try_reserve_exact(options.max_token_len)
            .map_err(|_| SerializedError::Config("token reservation failed"))?;
        let correlation = if let Some(binding) = self.transport.owned_rpc() {
            let client = binding
                .attach_client(owned_rpc::ClientRoute {
                    method: &method,
                    reply_to: &reply_to,
                    request: request.identity(),
                    request_family: request.family(),
                    response: response.identity(),
                    response_family: response.family(),
                    max_in_flight: options.max_in_flight,
                    ttl: options.ttl,
                })
                .await
                .map_err(SerializedError::Transport)?;
            Correlation::Owned {
                client,
                closed: AtomicBool::new(false),
            }
        } else {
            let pending = Arc::new(
                PendingCalls::new(options.max_in_flight)
                    .map_err(|_| SerializedError::Config("pending allocation failed"))?,
            );
            let listener = Arc::new(ResponseSink {
                calls: pending.clone(),
                source: method.clone(),
                sink: reply_to.clone(),
                identity: response.identity(),
                limit: self.limit,
                stats: stats.clone(),
            });
            let attachment = self
                .transport
                .attach_owned_listener(&method, Some(&reply_to), listener)
                .await
                .map_err(SerializedError::Transport)?;
            Correlation::Generic {
                pending,
                attachment: Mutex::new(attachment),
            }
        };
        Ok(RpcClient {
            inner: SerializedRpcClient {
                setup: self.clone(),
                request,
                response,
                method,
                reply_to,
                options,
                correlation,
                token: Mutex::new((token, false)),
                stats,
            },
        })
    }
}

impl<Req, Resp> RpcClient<Req, Resp> {
    /// Replaces the credential within its setup-reserved capacity.
    ///
    /// # Errors
    /// Oversize credentials leave the previous value unchanged.
    pub fn set_token(&self, value: &str) -> Result<(), SerializedError> {
        if value.len() > self.inner.options.max_token_len {
            return Err(SerializedError::Config("token capacity exceeded"));
        }
        let mut token = lock(&self.inner.token);
        token.0.clear();
        token.0.push_str(value);
        token.1 = true;
        Ok(())
    }
    /// Omits the credential from future requests.
    pub fn clear_token(&self) {
        let mut token = lock(&self.inner.token);
        token.0.clear();
        token.1 = false;
    }
    /// Counts occupied pending slots, including completed waits not yet consumed.
    pub fn in_flight(&self) -> usize {
        self.inner.correlation.in_flight()
    }
    /// Response admission counters.
    pub fn stats(&self) -> &SerializedRpcStats {
        &self.inner.stats
    }
    /// Closes pending admission and releases the exact response attachment, or
    /// the binding-owned client role.
    ///
    /// # Errors
    /// Returns a physical detach failure, retained for retry on Drop.
    pub fn close(&self) -> Result<(), SerializedError> {
        self.inner
            .correlation
            .close()
            .map_err(SerializedError::Transport)
    }
}
impl<Req, Resp: Send + 'static> RpcClient<Req, Resp> {
    /// Encodes and sends after arming correlation; cancellation releases its slot.
    /// The returned handle waits for the response and has the original deadline.
    ///
    /// # Errors
    /// Reports capacity, codec, send, closure or total submission deadline failure.
    pub async fn invoke(
        &self,
        value: &Req,
    ) -> Result<SerializedCall<'_, Resp>, SerializedCallError> {
        let deadline = tokio::time::Instant::now() + self.inner.options.ttl;
        let id = UUID::build();
        let wait = match &self.inner.correlation {
            Correlation::Generic { pending, .. } => {
                let pending = pending.reserve(&id).map_err(|e| match e {
                    ReserveError::Full => SerializedCallError::TooManyInFlight,
                    ReserveError::Closed => SerializedCallError::Closed,
                })?;
                let frame = self
                    .inner
                    .request_frame(self.inner.request_metadata(id)?, value)?;
                tokio::time::timeout_at(deadline, self.inner.setup.transport.send_owned(frame))
                    .await
                    .map_err(|_| SerializedCallError::TimedOut)?
                    .map_err(SerializedError::Transport)?;
                Wait::Generic(pending)
            }
            Correlation::Owned { client, closed } => {
                if closed.load(Ordering::Acquire) {
                    return Err(SerializedCallError::Closed);
                }
                let metadata = self.inner.request_metadata(id.clone())?;
                let mut call = client.prepare(&metadata).map_err(prepare_error)?;
                let frame = self.inner.request_frame(metadata, value)?;
                tokio::time::timeout_at(deadline, call.submit(frame))
                    .await
                    .map_err(|_| SerializedCallError::TimedOut)?
                    .map_err(SerializedError::Transport)?;
                Wait::Owned {
                    call,
                    request: id,
                    expected: Expected {
                        method: &self.inner.method,
                        reply_to: &self.inner.reply_to,
                        identity: self.inner.response.identity(),
                        stats: &self.inner.stats,
                    },
                }
            }
        };
        Ok(SerializedCall {
            wait,
            deadline,
            decoder: self.inner.response,
            limit: self.inner.setup.limit,
        })
    }
}

/// Exact response expectations of one owned call, borrowed from its client.
struct Expected<'a> {
    method: &'a UUri,
    reply_to: &'a UUri,
    identity: PayloadIdentity,
    stats: &'a SerializedRpcStats,
}

/// The completion authority of one call: the SDK table slot or the binding call.
enum Wait<'a> {
    Generic(PendingGuard<'a, UOwnedFrame>),
    Owned {
        call: Box<dyn OwnedRpcCall>,
        request: UUID,
        expected: Expected<'a>,
    },
}
impl Wait<'_> {
    async fn frame(
        &mut self,
        deadline: tokio::time::Instant,
        limit: PayloadDecodeLimit,
    ) -> Result<UOwnedFrame, SerializedCallError> {
        match self {
            Self::Generic(pending) => {
                tokio::time::timeout_at(deadline, std::future::poll_fn(|cx| pending.poll(cx)))
                    .await
                    .map_err(|_| SerializedCallError::TimedOut)?
                    .map_err(|()| SerializedCallError::Closed)
            }
            Self::Owned {
                call,
                request,
                expected,
            } => {
                let frame = tokio::time::timeout_at(
                    deadline,
                    std::future::poll_fn(|cx| call.poll_response(cx)),
                )
                .await
                .map_err(|_| SerializedCallError::TimedOut)?
                .map_err(completion_error)?;
                // Defense in depth: the binding validated already; the SDK still
                // admits only its exact correlated response.
                if !admissible_response(
                    &frame,
                    expected.method,
                    expected.reply_to,
                    expected.identity,
                    limit,
                ) || frame.metadata().reqid() != Some(&*request)
                {
                    expected.stats.rejected.fetch_add(1, Ordering::Relaxed);
                    return Err(SerializedCallError::Response(UWireError::invalid_payload(
                        "binding completion is not the exact correlated response",
                    )));
                }
                Ok(frame)
            }
        }
    }
}

/// A pending serialized response; cancellation frees correlation even if unpolled.
#[must_use = "dropping the wait cancels correlation"]
pub struct SerializedCall<'a, P> {
    wait: Wait<'a>,
    deadline: tokio::time::Instant,
    decoder: Decoder<P>,
    limit: PayloadDecodeLimit,
}
impl<P: Send + 'static> SerializedCall<'_, P> {
    /// Waits within the original deadline, then checks the codec and owns the value.
    ///
    /// # Errors
    /// Reports deadline, closure, remote status or schema decoding failure.
    pub async fn response(self) -> Result<Decoded<P>, SerializedCallError> {
        let SerializedCall {
            mut wait,
            deadline,
            decoder,
            limit,
        } = self;
        if tokio::time::Instant::now() >= deadline {
            return Err(SerializedCallError::TimedOut);
        }
        let frame = wait.frame(deadline, limit).await?;
        if let Some(status) = frame.metadata().comm_status() {
            if status != UCode::Ok {
                return Err(SerializedCallError::Status(status));
            }
        }
        decode(frame, decoder, limit).map_err(SerializedCallError::Response)
    }
}

/// Server admission and reply resource outcomes.
#[derive(Debug, Default)]
pub struct SerializedServerStats {
    /// Incorrect kind, destination, credentials length or schema.
    pub rejected: AtomicU64,
    /// Responses sent, including status-only errors.
    pub responded: AtomicU64,
    /// New client destinations beyond capacity.
    pub refused: AtomicU64,
    /// Response mapping or transport failures.
    pub send_failed: AtomicU64,
}
struct Clients {
    addresses: Mutex<Vec<UUri>>,
    capacity: usize,
}
impl Clients {
    fn admit(&self, address: &UUri) -> Result<(), SerializedError> {
        address
            .verify_rpc_response()
            .map_err(|_| SerializedError::Config("RPC reply destination required"))?;
        address
            .verify_no_wildcards()
            .map_err(|_| SerializedError::Config("exact reply destination required"))?;
        let mut addresses = lock(&self.addresses);
        if addresses.contains(address) {
            return Ok(());
        }
        if addresses.len() == self.capacity {
            return Err(SerializedError::Config("reply client capacity exceeded"));
        }
        addresses.push(address.clone());
        Ok(())
    }
}

/// One admitted server item, selected by the setup-time correlation owner.
enum ServerItem {
    /// Delivered by the generic exact listener; replies use `send_owned`.
    Generic(UOwnedFrame),
    /// Delivered by the binding together with its affine reply permission.
    Owned(owned_rpc::OwnedRequest),
}

/// Generic exact listener feeding the bounded server inbox.
struct RequestListener(Arc<RegistrationInbox<ServerItem>>);
#[async_trait::async_trait]
impl UOwnedListener for RequestListener {
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        let _ = self.0.push(ServerItem::Generic(frame));
    }
}

/// Binding-owned admission into the same bounded server inbox.
struct RequestSink(Arc<RegistrationInbox<ServerItem>>);
impl owned_rpc::OwnedRequestSink for RequestSink {
    fn push(&self, request: owned_rpc::OwnedRequest) -> Result<(), owned_rpc::RequestRefused> {
        self.0
            .push(ServerItem::Owned(request))
            .map_err(|error| match error {
                InboxPushError::Closed => owned_rpc::RequestRefused::Closed,
                InboxPushError::Full | InboxPushError::DeadlineExceeded => {
                    owned_rpc::RequestRefused::Full
                }
            })
    }
}

struct ServerWorker<Req, Resp, H, W> {
    setup: Deployment,
    request: Decoder<Req>,
    response: Encoder<Resp>,
    method: UUri,
    handler: H,
    write: W,
    clients: Arc<Clients>,
    stats: Arc<SerializedServerStats>,
}
impl<Req, Resp, H, W> RegistrationHandler<ServerItem> for ServerWorker<Req, Resp, H, W>
where
    Req: Send + 'static,
    Resp: Send + 'static,
    H: RequestHandler<Req>,
    W: FnMut(H::Reply) -> Resp + Send + 'static,
{
    async fn on_receive(&mut self, item: ServerItem) {
        // An owned reply dropped on any rejection retires the binding's request.
        let (frame, reply) = match item {
            ServerItem::Generic(frame) => (frame, None),
            ServerItem::Owned(request) => {
                let (frame, reply) = request.into_parts();
                (frame, Some(reply))
            }
        };
        let m = frame.metadata();
        if m.kind() != UMessageType::Request
            || m.sink() != Some(&self.method)
            || m.token()
                .is_some_and(|t| t.len() > self.setup.max_token_len)
        {
            self.stats.rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let source = m.source().clone();
        let id = m.id().clone();
        let priority = m.priority();
        let request = match decode(frame, self.request, self.setup.limit) {
            Ok(value) => value,
            Err(_) => {
                self.stats.rejected.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        if self.clients.admit(&source).is_err() {
            self.stats.refused.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let outcome = self.handler.handle(request).await;
        let mut builder = UFrameMetadata::response(self.method.clone(), source, id);
        if let Some(priority) = priority {
            builder = builder.with_priority(priority);
        }
        let frame: Result<UOwnedFrame, UWireError> = (|| match outcome {
            Ok(reply) => {
                let response = (self.write)(reply);
                let bytes = self.response.encode(&response)?;
                let metadata = self
                    .response
                    .identity()
                    .apply_to(builder.build().map_err(metadata_error)?)?;
                UOwnedFrame::with_payload(metadata, bytes).map_err(metadata_error)
            }
            Err(status) => UOwnedFrame::without_payload(
                builder
                    .with_comm_status(status)
                    .build()
                    .map_err(metadata_error)?,
            )
            .map_err(metadata_error),
        })();
        let sent = match frame {
            Ok(frame) => match reply {
                Some(reply) => reply.respond(frame).await.is_ok(),
                None => self.setup.transport.send_owned(frame).await.is_ok(),
            },
            Err(_) => false,
        };
        if sent {
            self.stats.responded.fetch_add(1, Ordering::Relaxed);
        } else {
            self.stats.send_failed.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Serialized server mechanics; one worker, exact attachment and bounded clients.
#[doc(hidden)]
pub(super) struct SerializedRpcServer {
    registration: Option<Registration<ServerItem>>,
    attachment: OwnedAttachment,
    clients: Arc<Clients>,
    stats: Arc<SerializedServerStats>,
    worker_error: Option<crate::RegistrationStopError>,
    progress: crate::StopProgress<crate::UStatus>,
}
impl Drop for SerializedRpcServer {
    fn drop(&mut self) {
        if let Some(registration) = &self.registration {
            registration.inbox().close();
        }
        let _ = self.attachment.detach();
    }
}
impl Deployment {
    /// Registers a method with a representation-independent business handler and
    /// a value constructor for the codec. Native setup instead takes an initializer.
    ///
    /// A transport supplying [`crate::owned_rpc::OwnedRpc`] installs only the
    /// binding's server role; each request carries its affine reply permission.
    ///
    /// # Errors
    /// Refuses method/capacity errors and absent scoped receive capability.
    pub async fn rpc_server<Req, Resp, H, W>(
        &self,
        resource: u16,
        handler: H,
        write: W,
        options: ServerOptions,
        request: Decoder<Req>,
        response: Encoder<Resp>,
    ) -> Result<RpcServer, SerializedError>
    where
        Req: Send + 'static,
        Resp: Send + 'static,
        H: RequestHandler<Req>,
        W: FnMut(H::Reply) -> Resp + Send + 'static,
    {
        request.admit(self.transport.as_ref())?;
        response.admit(self.transport.as_ref())?;
        if options.max_clients == 0 {
            return Err(SerializedError::Config(
                "server client capacity must be positive",
            ));
        }
        let method = self.uri_provider.get_resource_uri(resource);
        method
            .verify_rpc_method()
            .map_err(|_| SerializedError::Config("RPC method required"))?;
        method
            .verify_no_wildcards()
            .map_err(|_| SerializedError::Config("exact method required"))?;
        let mut addresses = Vec::new();
        addresses
            .try_reserve_exact(options.max_clients)
            .map_err(|_| SerializedError::Config("reply client reservation failed"))?;
        let clients = Arc::new(Clients {
            addresses: Mutex::new(addresses),
            capacity: options.max_clients,
        });
        let stats = Arc::new(SerializedServerStats::default());
        let route = owned_rpc::ServerRoute {
            method: &method,
            request: request.identity(),
            request_family: request.family(),
            response: response.identity(),
            response_family: response.family(),
            max_clients: options.max_clients,
            blocks_when_full: matches!(
                self.registration.overflow,
                crate::OverflowPolicy::Block { .. }
            ),
        };
        let registration = Registration::new(
            self.registration,
            ServerWorker {
                setup: self.clone(),
                request,
                response,
                method: method.clone(),
                handler,
                write,
                clients: clients.clone(),
                stats: stats.clone(),
            },
            || {},
        )
        .map_err(SerializedError::Prepare)?;
        let attachment = if let Some(binding) = self.transport.owned_rpc() {
            binding
                .attach_server(route, Arc::new(RequestSink(registration.inbox())))
                .await
                .map_err(SerializedError::Transport)?
        } else {
            self.transport
                .attach_owned_listener(
                    &UUri::any_with_resource_id(0),
                    Some(&method),
                    Arc::new(RequestListener(registration.inbox())),
                )
                .await
                .map_err(SerializedError::Transport)?
        };
        Ok(RpcServer {
            inner: SerializedRpcServer {
                registration: Some(registration),
                attachment,
                clients,
                stats,
                worker_error: None,
                progress: crate::StopProgress::default(),
            },
        })
    }
}
impl RpcServer {
    /// Reserves a known response destination in the same bounded client set.
    ///
    /// # Errors
    /// Returns invalid address or client capacity refusal.
    pub fn prepare_client(&self, reply_to: &UUri) -> Result<(), SerializedError> {
        self.inner.clients.admit(reply_to)
    }
    /// Admission, resource and reply counters.
    pub fn stats(&self) -> &SerializedServerStats {
        &self.inner.stats
    }
    /// Closes admission, detaches, then drains accepted requests and replies.
    ///
    /// # Errors
    /// Preserves both detach and worker errors.
    pub fn stop(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), SerializedStopError>> + Send + '_ {
        self.close_admission();
        let detach = self.inner.attachment.detach().err();
        self.inner.progress.begin(detach.clone());
        async move {
            let worker = if let Some(registration) = &mut self.inner.registration {
                registration.stop_ref().await.err()
            } else {
                None
            };
            let worker =
                super::super::typed::retain_worker_outcome(&mut self.inner.worker_error, worker);
            self.inner.progress.finish(worker);
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
    /// Prompt local admission closure, independent of drain scheduling.
    pub fn close_admission(&self) {
        if let Some(registration) = &self.inner.registration {
            registration.inbox().close();
        }
    }
    /// Retained exact detach and worker observations.
    pub fn stop_progress(&self) -> &crate::StopProgress<crate::UStatus> {
        &self.inner.progress
    }
}

impl<Req, Resp> std::fmt::Debug for SerializedRpcClient<Req, Resp> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SerializedRpcClient")
            .finish_non_exhaustive()
    }
}
impl<P> std::fmt::Debug for SerializedCall<'_, P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SerializedCall").finish_non_exhaustive()
    }
}
impl std::fmt::Debug for SerializedRpcServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SerializedRpcServer")
            .finish_non_exhaustive()
    }
}
