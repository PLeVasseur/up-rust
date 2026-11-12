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
//! Native RPC state reserved at endpoint creation.

use std::{
    future::Future,
    marker::PhantomData,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, OnceLock, RwLock,
    },
    time::Duration,
};

use crate::{
    AdmissionError, Frame, FrameSink, HeaderLimits, InitializedStablePayload, MessageFields,
    NativeProfileAgreement, PayloadInit, PreparedTransportCore, Registration, RegistrationConfig,
    RouteConfig, RouteDefinition, RxAdmission, RxLease, SendError, SetupError, StablePayload,
    StablePayloadInit, TokenBuffer, TokenError, TxRoute, TxTemplate, UCode, UMessageType,
    UPriority, UUri, ZeroCopyHandler, UUID,
};

use crate::pending_calls::{PendingCalls, PendingGuard, ReserveError};

fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Creation-time limits of a prepared native RPC client.
#[derive(Clone, Copy, Debug)]
pub struct RpcClientConfig {
    /// Maximum simultaneously pending calls.
    pub max_in_flight: usize,
    /// Request TTL and response wait bound.
    pub ttl: Duration,
    /// Reserved token capacity; zero disables token transmission.
    pub max_token_len: usize,
    /// Request priority, at least CS4.
    pub priority: UPriority,
}

/// Fixed-size outcome of one prepared RPC call.
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum NativeCallError {
    /// Every reserved pending entry is occupied.
    #[error("too many RPC calls in flight")]
    TooManyInFlight,
    /// The request's response wait expired.
    #[error("RPC response timed out")]
    TimedOut,
    /// Synchronous request publication failed.
    #[error("RPC send failed: {0}")]
    Send(SendError),
    /// The response did not pass typed admission.
    #[error("RPC response admission failed: {0}")]
    Response(AdmissionError),
    /// A correlated response reported a non-OK status.
    #[error("RPC status: {0:?}")]
    Status(UCode),
    /// The client's response admission closed.
    #[error("RPC client is closed")]
    Closed,
}

/// Counters for one prepared RPC client's response sink.
#[derive(Debug, Default)]
pub struct RpcClientStats {
    /// Malformed, incorrectly addressed or non-response frames.
    pub rejected: AtomicU64,
    /// Late or unknown request IDs.
    pub unmatched: AtomicU64,
}

struct PendingTable<P, F: Frame> {
    calls: PendingCalls<F>,
    admission: RxAdmission<P>,
    source: UUri,
    sink: UUri,
    stats: RpcClientStats,
}

impl<P: StablePayload, F: Frame> FrameSink<F> for PendingTable<P, F> {
    fn push(&self, frame: F) {
        if self.calls.is_closed() {
            self.stats.rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let header = match self.admission.parse(frame.encoded_metadata()) {
            Ok(header)
                if header.kind == UMessageType::Response
                    && header.source == self.source
                    && header.sink.as_ref() == Some(&self.sink) =>
            {
                header
            }
            _ => {
                self.stats.rejected.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let Some(id) = header.reqid.clone() else {
            self.stats.rejected.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let status_only = header.comm_status.is_some_and(|status| status != UCode::Ok);
        let valid = if status_only {
            frame.payload().is_none()
                && header.payload_encoding.is_none()
                && header.native_type_token.is_none()
        } else {
            // A wrong identity or invalid representation must not steal a pending
            // slot. The retained response is checked again on every typed borrow.
            self.admission.check_frame(&frame).is_ok()
        };
        if !valid {
            self.stats.rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if !self.calls.deliver(&id, frame) {
            self.stats.unmatched.fetch_add(1, Ordering::Relaxed);
        }
    }
    fn close(&self) {
        self.calls.close();
    }
}

/// Native RPC client with setup-reserved pending entries, request route and token.
pub struct PreparedRpcClient<C: PreparedTransportCore, Req, Resp> {
    core: Arc<C>,
    request: TxRoute<C, Req>,
    pending: Arc<PendingTable<Resp, C::Rx>>,
    subscription: Mutex<Option<C::Subscription>>,
    token: RwLock<TokenBuffer>,
    ttl: Duration,
}

impl<C: PreparedTransportCore, Req, Resp> std::fmt::Debug for PreparedRpcClient<C, Req, Resp> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedRpcClient")
            .field("in_flight", &self.pending.calls.in_flight())
            .finish_non_exhaustive()
    }
}

impl<C, Req, Resp> PreparedRpcClient<C, Req, Resp>
where
    C: PreparedTransportCore,
    Req: StablePayloadInit,
    Resp: StablePayload,
{
    /// Reserves all per-client state and attaches a direct synchronous response sink.
    ///
    /// # Errors
    /// Rejects invalid capacities/addresses, unagreed types and exhausted tables.
    pub async fn new(
        core: Arc<C>,
        agreement: NativeProfileAgreement,
        reply_to: UUri,
        method: UUri,
        config: RpcClientConfig,
    ) -> Result<Self, SetupError> {
        if config.max_in_flight == 0 || config.ttl.is_zero() {
            return Err(SetupError::InvalidConfig(
                "RPC capacity and TTL must be positive",
            ));
        }
        let admission =
            RxAdmission::<Resp>::new(agreement.clone(), HeaderLimits { max_token_len: 0 })
                .map_err(|_| SetupError::UnknownPayloadType)?;
        let request = TxTemplate::<Req>::new(agreement)?.route(
            Arc::clone(&core),
            RouteDefinition {
                kind: UMessageType::Request,
                source: reply_to.clone(),
                sink: Some(method.clone()),
                priority: Some(config.priority),
                ttl: Some(config.ttl),
                permission_level: None,
            },
            RouteConfig {
                max_token_len: config.max_token_len,
                allow_traceparent: false,
            },
        )?;
        let token = TokenBuffer::new(config.max_token_len)
            .map_err(|_| SetupError::InvalidConfig("RPC token capacity"))?;
        let pending = Arc::new(PendingTable {
            calls: PendingCalls::new(config.max_in_flight).map_err(|_| SetupError::Allocation)?,
            admission,
            source: method.clone(),
            sink: reply_to.clone(),
            stats: RpcClientStats::default(),
        });
        let sink: Arc<dyn FrameSink<C::Rx>> = pending.clone();
        let subscription = core.attach(&method, Some(&reply_to), sink).await?;
        Ok(Self {
            core,
            request,
            pending,
            subscription: Mutex::new(Some(subscription)),
            token: RwLock::new(token),
            ttl: config.ttl,
        })
    }
    /// Replaces future requests' token without allocating; oversize keeps the old value.
    ///
    /// # Errors
    /// Returns TooLong when the reserved capacity is insufficient.
    pub fn set_token(&self, token: &str) -> Result<(), TokenError> {
        self.token
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set(token)
    }
    /// Wipes the stored token and removes its presence from later requests.
    pub fn clear_token(&self) {
        self.token
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
    /// Number of calls currently holding reserved entries.
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.pending.calls.in_flight()
    }
    /// Response rejection and late/unknown-correlation counters.
    #[must_use]
    pub fn stats(&self) -> &RpcClientStats {
        &self.pending.stats
    }

    /// Arms correlation and commits a directly initialized native loan synchronously.
    /// The returned wait owns a pending slot, never a TX loan or token lock.
    /// Dropping it before polling cancels the pending call. Its deadline starts now.
    ///
    /// # Errors
    /// Returns bounded capacity, closed-client or synchronous send failures.
    pub fn begin_call<F>(
        &self,
        fill: F,
    ) -> Result<PendingNativeCall<'_, Resp, C::Rx>, NativeCallError>
    where
        F: for<'a> FnOnce(PayloadInit<'a, Req>) -> InitializedStablePayload<'a, Req>,
    {
        let deadline = tokio::time::Instant::now() + self.ttl;
        let id = UUID::build();
        let pending = self
            .pending
            .calls
            .reserve(&id)
            .map_err(|error| match error {
                ReserveError::Full => NativeCallError::TooManyInFlight,
                ReserveError::Closed => NativeCallError::Closed,
            })?;
        let frame = {
            let token = self
                .token
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            self.request
                .prepare(
                    MessageFields {
                        id: Some(&id),
                        token: token.as_str(),
                        ..MessageFields::default()
                    },
                    fill,
                )
                .map_err(NativeCallError::Send)?
        };
        self.request.publish(frame).map_err(NativeCallError::Send)?;
        Ok(PendingNativeCall {
            pending,
            admission: &self.pending.admission,
            deadline,
        })
    }

    /// Compatibility convenience: submits when polled, then waits for the response.
    ///
    /// # Errors
    /// Returns submission, timeout, remote status or full typed-admission failures.
    pub async fn call<F>(&self, fill: F) -> Result<RxLease<Resp, C::Rx>, NativeCallError>
    where
        F: for<'a> FnOnce(PayloadInit<'a, Req>) -> InitializedStablePayload<'a, Req>,
    {
        self.begin_call(fill)?.response().await
    }

    /// Closes pending admission and synchronously detaches the response sink.
    ///
    /// # Errors
    /// A failed detach is retained for retry on Drop.
    pub fn close(&self) -> Result<(), SetupError> {
        self.pending.calls.close();
        let mut attachment = lock(&self.subscription);
        if let Some(subscription) = &mut *attachment {
            self.core.detach(subscription)?;
        }
        attachment.take();
        Ok(())
    }
}

impl<C: PreparedTransportCore, Req, Resp> Drop for PreparedRpcClient<C, Req, Resp> {
    fn drop(&mut self) {
        self.pending.calls.close();
        if let Some(mut subscription) = lock(&self.subscription).take() {
            let _ = self.core.detach(&mut subscription);
        }
    }
}

/// A committed native request's response wait, borrowing the client's admission.
/// There is no transmit storage in this value or in the future it creates.
#[must_use = "dropping the wait cancels correlation"]
pub struct PendingNativeCall<'a, P, F: Frame> {
    pending: PendingGuard<'a, F>,
    admission: &'a RxAdmission<P>,
    deadline: tokio::time::Instant,
}
impl<P: StablePayload, F: Frame> PendingNativeCall<'_, P, F> {
    /// Waits only for a response; always runs complete checked typed admission.
    ///
    /// # Errors
    /// Returns timeout, closed-client, status or admission failure.
    pub async fn response(self) -> Result<RxLease<P, F>, NativeCallError> {
        if tokio::time::Instant::now() >= self.deadline {
            return Err(NativeCallError::TimedOut);
        }
        let frame = tokio::time::timeout_at(
            self.deadline,
            std::future::poll_fn(|context| self.pending.poll(context)),
        )
        .await
        .map_err(|_| NativeCallError::TimedOut)?
        .map_err(|()| NativeCallError::Closed)?;
        let header = self
            .admission
            .parse(frame.encoded_metadata())
            .map_err(NativeCallError::Response)?;
        if let Some(status) = header.comm_status {
            if status != UCode::Ok {
                return Err(NativeCallError::Status(status));
            }
        }
        self.admission
            .admit(frame)
            .map_err(NativeCallError::Response)
    }
}

/// Setup limits for a prepared native RPC server.
#[derive(Clone, Copy, Debug)]
pub struct RpcServerConfig {
    /// Request inbox capacity and overflow policy.
    pub registration: RegistrationConfig,
    /// Admission limits for request tokens.
    pub header: HeaderLimits,
    /// Maximum distinct clients retained in the response-route cache.
    pub max_clients: usize,
}

/// Application logic with an asynchronous decision followed by in-place reply writing.
pub trait NativeRequestHandler<Req: StablePayload, Resp: StablePayloadInit, F: Frame>:
    Send + 'static
{
    /// State passed from asynchronous handling to synchronous reply initialization.
    type Reply: Send;
    /// Handles the retained request without borrowing a response TX loan.
    fn handle(
        &mut self,
        request: RxLease<Req, F>,
    ) -> impl Future<Output = Result<Self::Reply, UCode>> + Send;
    /// Initializes the response after the async operation has finished.
    fn write_reply<'a>(
        &mut self,
        reply: Self::Reply,
        initializer: Resp::Initializer<'a>,
    ) -> InitializedStablePayload<'a, Resp>;
}

/// Counters for server resource refusals and replies.
#[derive(Debug, Default)]
pub struct RpcServerStats {
    /// Typed frames rejected because their kind or decoded destination is wrong.
    pub rejected: AtomicU64,
    /// Responses published, including status-only replies.
    pub responded: AtomicU64,
    /// Requests beyond the reserved client-route capacity.
    pub refused: AtomicU64,
    /// Responses that could not be prepared or sent.
    pub send_failed: AtomicU64,
}

struct ServerState<C: PreparedTransportCore, Resp> {
    core: Arc<C>,
    template: TxTemplate<Resp>,
    method: UUri,
    routes: Box<[OnceLock<TxRoute<C, Resp>>]>,
    prepare: Mutex<()>,
    stats: RpcServerStats,
}

impl<C: PreparedTransportCore, Resp: StablePayloadInit> ServerState<C, Resp> {
    fn route(&self, reply_to: &UUri) -> Result<&TxRoute<C, Resp>, SetupError> {
        if let Some(route) = self
            .routes
            .iter()
            .filter_map(OnceLock::get)
            .find(|route| route.definition().sink.as_ref() == Some(reply_to))
        {
            return Ok(route);
        }
        let _prepare = lock(&self.prepare);
        if let Some(route) = self
            .routes
            .iter()
            .filter_map(OnceLock::get)
            .find(|route| route.definition().sink.as_ref() == Some(reply_to))
        {
            return Ok(route);
        }
        let slot = self
            .routes
            .iter()
            .find(|slot| slot.get().is_none())
            .ok_or(SetupError::TableFull)?;
        let route = self.template.route(
            Arc::clone(&self.core),
            RouteDefinition {
                kind: UMessageType::Response,
                source: self.method.clone(),
                sink: Some(reply_to.clone()),
                priority: Some(UPriority::CS4),
                ttl: None,
                permission_level: None,
            },
            RouteConfig {
                max_token_len: 0,
                allow_traceparent: false,
            },
        )?;
        slot.set(route).map_err(|_| SetupError::TableFull)?;
        Ok(slot.get().expect("initialized reply route"))
    }
}

struct ServerWorker<C: PreparedTransportCore, Req, Resp, H> {
    state: Arc<ServerState<C, Resp>>,
    handler: H,
    request: PhantomData<fn() -> Req>,
}
impl<C, Req, Resp, H> ZeroCopyHandler<Req, C::Rx> for ServerWorker<C, Req, Resp, H>
where
    C: PreparedTransportCore,
    Req: StablePayload,
    Resp: StablePayloadInit,
    H: NativeRequestHandler<Req, Resp, C::Rx>,
{
    async fn on_receive(&mut self, request: RxLease<Req, C::Rx>) {
        let header = match request.header() {
            Ok(header)
                if header.kind == UMessageType::Request
                    && header.sink.as_ref() == Some(&self.state.method) =>
            {
                header
            }
            _ => {
                self.state.stats.rejected.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let reply_to = header.source.clone();
        let id = header.id.clone();
        let priority = header.priority;
        let route = match self.state.route(&reply_to) {
            Ok(route) => route,
            Err(SetupError::TableFull) => {
                self.state.stats.refused.fetch_add(1, Ordering::Relaxed);
                return;
            }
            Err(_) => {
                self.state.stats.send_failed.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        // Cache preparation ends before user code and returns a stable OnceLock
        // reference. Neither a cache lock nor a TX loan spans this await.
        let outcome = self.handler.handle(request).await;
        let fields = MessageFields {
            reqid: Some(&id),
            priority,
            ..MessageFields::default()
        };
        let result = match outcome {
            Ok(reply) => route.send(fields, |initializer| {
                self.handler
                    .write_reply(reply, initializer.into_initializer())
            }),
            Err(status) => route.send_status(fields, status),
        };
        if result.is_ok() {
            self.state.stats.responded.fetch_add(1, Ordering::Relaxed);
        } else {
            self.state.stats.send_failed.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Every failure from ordered server shutdown.
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
#[error("RPC server shutdown failed: detach={detach:?}, worker={worker:?}")]
pub struct RpcServerStopError {
    /// Physical detach failure, if any.
    pub detach: Option<SetupError>,
    /// Receive drain failure, if any.
    pub worker: Option<crate::RegistrationStopError>,
}

impl RpcServerStopError {
    /// Recovery classification for a failed server shutdown.
    ///
    /// A remaining detach failure or a retryable drain refusal is
    /// [`Completion::Pending`](crate::Completion::Pending); otherwise the shutdown failed
    /// terminally (see [`RegistrationStopError::is_terminal`](crate::RegistrationStopError::is_terminal)).
    ///
    /// ```
    /// use up_rust::{Completion, RegistrationStopError, RpcServerStopError};
    /// let refused = RpcServerStopError {
    ///     detach: None,
    ///     worker: Some(RegistrationStopError::CalledFromWorker),
    /// };
    /// assert_eq!(refused.completion(), Completion::Pending);
    /// let panicked = RpcServerStopError {
    ///     detach: None,
    ///     worker: Some(RegistrationStopError::HandlerPanicked),
    /// };
    /// assert_eq!(panicked.completion(), Completion::Failed);
    /// ```
    pub fn completion(&self) -> crate::Completion {
        if self.detach.is_some() || self.worker.is_some_and(|error| !error.is_terminal()) {
            crate::Completion::Pending
        } else {
            crate::Completion::Failed
        }
    }
}

/// Native RPC server with one generic request worker and a fixed reply-route cache.
pub struct PreparedRpcServer<C: PreparedTransportCore, Resp> {
    registration: Option<Registration<C::Rx>>,
    subscription: Option<C::Subscription>,
    worker_outcome: Option<Result<(), crate::RegistrationStopError>>,
    progress: crate::StopProgress<SetupError>,
    state: Arc<ServerState<C, Resp>>,
}
impl<C: PreparedTransportCore, Resp> std::fmt::Debug for PreparedRpcServer<C, Resp> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedRpcServer")
            .field("method", &self.state.method)
            .finish_non_exhaustive()
    }
}
impl<C: PreparedTransportCore, Resp: StablePayloadInit> PreparedRpcServer<C, Resp> {
    /// Reserves all server state and starts one admission/handler worker.
    ///
    /// # Errors
    /// Rejects invalid limits/addresses, unagreed types or exhausted binding tables.
    pub async fn start<Req, H>(
        core: Arc<C>,
        agreement: NativeProfileAgreement,
        method: UUri,
        handler: H,
        config: RpcServerConfig,
    ) -> Result<Self, SetupError>
    where
        Req: StablePayload,
        H: NativeRequestHandler<Req, Resp, C::Rx>,
    {
        if config.max_clients == 0 {
            return Err(SetupError::InvalidConfig(
                "server client capacity must be positive",
            ));
        }
        method
            .verify_rpc_method()
            .map_err(|_| SetupError::InvalidRoute("RPC method"))?;
        let admission = RxAdmission::<Req>::new(agreement.clone(), config.header)
            .map_err(|_| SetupError::UnknownPayloadType)?;
        let template = TxTemplate::<Resp>::new(agreement)?;
        let mut routes = Vec::new();
        routes
            .try_reserve_exact(config.max_clients)
            .map_err(|_| SetupError::Allocation)?;
        routes.resize_with(config.max_clients, OnceLock::new);
        let state = Arc::new(ServerState {
            core: Arc::clone(&core),
            template,
            method: method.clone(),
            routes: routes.into_boxed_slice(),
            prepare: Mutex::new(()),
            stats: RpcServerStats::default(),
        });
        let worker = ServerWorker {
            state: Arc::clone(&state),
            handler,
            request: PhantomData::<fn() -> Req>,
        };
        let registration = admission
            .register(config.registration, worker, || {})
            .map_err(|_| SetupError::Worker)?;
        let inbox: Arc<dyn FrameSink<C::Rx>> = registration.inbox();
        let subscription = core.attach(&UUri::any(), Some(&method), inbox).await?;
        Ok(Self {
            registration: Some(registration),
            subscription: Some(subscription),
            worker_outcome: None,
            progress: crate::StopProgress::default(),
            state,
        })
    }
    /// Prepares a known client using the same reserved route cache as first contact.
    ///
    /// # Errors
    /// Returns a typed capacity/address failure without allocating a new cache entry.
    pub fn prepare_client(&self, reply_to: &UUri) -> Result<(), SetupError> {
        self.state.route(reply_to).map(|_| ())
    }
    /// Prompt local closure independent of physical detach or drain.
    pub fn close_admission(&self) {
        if let Some(registration) = &self.registration {
            registration.inbox().close();
        }
    }
    /// Exact first failures and current attempt, retained across cancellation.
    pub fn stop_progress(&self) -> &crate::StopProgress<SetupError> {
        &self.progress
    }
    /// Server response/resource counters.
    #[must_use]
    pub fn stats(&self) -> &RpcServerStats {
        &self.state.stats
    }
    /// Closes admission and attempts exact detach before returning the drain future.
    ///
    /// Failed detach retains its exact attachment. Cancellation retains a pending
    /// worker join; completed worker outcomes remain sticky on subsequent calls.
    /// Retained request/response RX owners are independent of this server.
    ///
    /// # Errors
    /// Preserves both detach and worker failures. Retry does not restart a worker.
    /// Same-worker drain returns nonterminal `CalledFromWorker`, retaining the
    /// registration and join for external retry without caching that refusal.
    pub fn stop_ref(&mut self) -> impl Future<Output = Result<(), RpcServerStopError>> + Send + '_ {
        if let Some(registration) = &self.registration {
            registration.inbox().close();
        }
        let detach = self
            .subscription
            .as_mut()
            .and_then(|subscription| self.state.core.detach(subscription).err());
        if detach.is_none() {
            self.subscription.take();
        }
        self.progress.begin(detach);
        async move {
            let outcome = if let Some(outcome) = self.worker_outcome {
                outcome
            } else {
                let outcome = if let Some(registration) = self.registration.as_mut() {
                    registration.stop_ref().await
                } else {
                    Ok(())
                };
                if outcome != Err(crate::RegistrationStopError::CalledFromWorker) {
                    self.worker_outcome = Some(outcome);
                    self.registration.take();
                }
                outcome
            };
            let worker = outcome.err();
            self.progress.finish(worker);
            if detach.is_some() || worker.is_some() {
                Err(RpcServerStopError { detach, worker })
            } else {
                Ok(())
            }
        }
    }

    /// Borrowed ordinary shutdown, eager even when its returned future is dropped.
    ///
    /// # Errors
    /// Preserves detach and worker failures without consuming the retry owner.
    pub fn stop(&mut self) -> impl Future<Output = Result<(), RpcServerStopError>> + Send + '_ {
        self.stop_ref()
    }
}
impl<C: PreparedTransportCore, Resp> Drop for PreparedRpcServer<C, Resp> {
    fn drop(&mut self) {
        if let Some(registration) = &self.registration {
            registration.inbox().close();
        }
        if let Some(mut subscription) = self.subscription.take() {
            let _ = self.state.core.detach(&mut subscription);
        }
    }
}

impl<P, F: Frame> std::fmt::Debug for PendingNativeCall<'_, P, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingNativeCall").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        NativeProfile, OverflowPolicy, PayloadEncoding, PoolCore, PoolCoreConfig, PoolFrame,
        RegistrationInbox,
    };
    use std::task::{Context, Waker};

    #[repr(C)]
    #[derive(Debug, crate::StablePayload, crate::StablePayloadInit)]
    #[stable_payload(type_name = "uprotocol.test.PreparedRequest")]
    struct Request {
        value: u32,
    }
    #[repr(C)]
    #[derive(Debug, crate::StablePayload, crate::StablePayloadInit)]
    #[stable_payload(type_name = "uprotocol.test.PreparedReply")]
    struct Reply {
        value: u64,
    }

    struct Handler {
        status: bool,
    }
    impl NativeRequestHandler<Request, Reply, PoolFrame> for Handler {
        type Reply = u64;
        async fn handle(&mut self, request: RxLease<Request, PoolFrame>) -> Result<u64, UCode> {
            let value = u64::from(request.payload().unwrap().value);
            tokio::task::yield_now().await;
            if self.status {
                Err(UCode::NotFound)
            } else {
                Ok(value + 1)
            }
        }
        fn write_reply<'a>(
            &mut self,
            value: u64,
            initializer: <Reply as StablePayloadInit>::Initializer<'a>,
        ) -> InitializedStablePayload<'a, Reply> {
            initializer.value(value).finish()
        }
    }

    fn setup() -> (Arc<PoolCore>, NativeProfileAgreement, UUri, UUri) {
        let core = Arc::new(
            PoolCore::new(PoolCoreConfig {
                slots: 16,
                slot_bytes: 4096,
                alignment: 8,
                max_routes: 16,
                max_registrations: 8,
            })
            .unwrap(),
        );

        let profile = NativeProfile::new(
            "prepared-rpc",
            1,
            PayloadEncoding::private_use(0xF101),
            [
                Request::native_representation(),
                Reply::native_representation(),
            ],
        )
        .unwrap();
        let agreement = crate::test_support::self_agreement(profile);
        (
            core,
            agreement,
            UUri::try_from_wire_parts("service", 0x0001_0001, 1, 0x42).unwrap(),
            UUri::try_from_wire_parts("client", 0x0001_0002, 1, 0).unwrap(),
        )
    }

    fn client_config() -> RpcClientConfig {
        RpcClientConfig {
            max_in_flight: 2,
            ttl: Duration::from_secs(1),
            max_token_len: 2048,
            priority: UPriority::CS4,
        }
    }
    fn server_config(clients: usize) -> RpcServerConfig {
        RpcServerConfig {
            registration: RegistrationConfig {
                capacity: 4,
                overflow: OverflowPolicy::Block {
                    deadline: Duration::from_millis(100),
                },
            },
            header: HeaderLimits::default(),
            max_clients: clients,
        }
    }

    #[tokio::test]
    async fn request_and_different_reply_types_round_trip_on_first_contact() {
        let (core, agreement, method, reply_to) = setup();
        let mut server = PreparedRpcServer::<_, Reply>::start::<Request, _>(
            Arc::clone(&core),
            agreement.clone(),
            method.clone(),
            Handler { status: false },
            server_config(1),
        )
        .await
        .unwrap();
        let client = PreparedRpcClient::<_, Request, Reply>::new(
            Arc::clone(&core),
            agreement,
            reply_to.clone(),
            method,
            client_config(),
        )
        .await
        .unwrap();
        client.set_token("credential").unwrap();
        let reply = client
            .call(|init| init.into_initializer().value(41).finish())
            .await
            .unwrap();
        assert_eq!(reply.payload().unwrap().value, 42);
        assert!(reply.header().unwrap().reqid.is_some());
        assert_eq!(client.in_flight(), 0);
        assert_eq!(server.stats().responded.load(Ordering::Relaxed), 1);
        assert!(server.prepare_client(&reply_to).is_ok());
        let other = UUri::try_from_wire_parts("other", 0x0001_0003, 1, 0).unwrap();
        assert_eq!(server.prepare_client(&other), Err(SetupError::TableFull));
        drop(client);
        server.stop().await.unwrap();
        assert_eq!(reply.payload().unwrap().value, 42);
        drop(reply);
        assert_eq!(core.free_slots(), 16);
    }

    #[tokio::test]
    async fn status_only_response_is_not_mistaken_for_an_empty_typed_payload() {
        let (core, agreement, method, reply_to) = setup();
        let mut server = PreparedRpcServer::<_, Reply>::start::<Request, _>(
            Arc::clone(&core),
            agreement.clone(),
            method.clone(),
            Handler { status: true },
            server_config(1),
        )
        .await
        .unwrap();
        let client = PreparedRpcClient::<_, Request, Reply>::new(
            Arc::clone(&core),
            agreement,
            reply_to,
            method,
            client_config(),
        )
        .await
        .unwrap();
        assert!(matches!(
            client
                .call(|init| init.into_initializer().value(1).finish())
                .await,
            Err(NativeCallError::Status(UCode::NotFound))
        ));
        assert_eq!(client.in_flight(), 0);
        drop(client);
        server.stop().await.unwrap();
        assert_eq!(core.free_slots(), 16);
    }

    #[tokio::test]
    async fn pending_limit_cancellation_and_late_response_release_storage() {
        let (core, agreement, method, reply_to) = setup();
        let admission =
            RxAdmission::<Request>::new(agreement.clone(), HeaderLimits::default()).unwrap();
        let template = TxTemplate::<Reply>::new(agreement.clone()).unwrap();
        let reply_route = template
            .route(
                Arc::clone(&core),
                RouteDefinition {
                    kind: UMessageType::Response,
                    source: method.clone(),
                    sink: Some(reply_to.clone()),
                    priority: Some(UPriority::CS4),
                    ttl: None,
                    permission_level: None,
                },
                RouteConfig {
                    max_token_len: 0,
                    allow_traceparent: false,
                },
            )
            .unwrap();
        let wrong_route = TxTemplate::<Request>::new(agreement.clone())
            .unwrap()
            .route(
                Arc::clone(&core),
                RouteDefinition {
                    kind: UMessageType::Response,
                    source: method.clone(),
                    sink: Some(reply_to.clone()),
                    priority: Some(UPriority::CS4),
                    ttl: None,
                    permission_level: None,
                },
                RouteConfig {
                    max_token_len: 0,
                    allow_traceparent: false,
                },
            )
            .unwrap();
        let requests = Arc::new(
            RegistrationInbox::new(RegistrationConfig {
                capacity: 1,
                overflow: OverflowPolicy::DropNewest,
            })
            .unwrap(),
        );
        let mut subscription = core
            .attach(&reply_to, Some(&method), requests.clone())
            .await
            .unwrap();
        let mut config = client_config();
        config.max_in_flight = 1;
        let client = PreparedRpcClient::<_, Request, Reply>::new(
            Arc::clone(&core),
            agreement,
            reply_to,
            method,
            config,
        )
        .await
        .unwrap();
        let mut call = Box::pin(client.call(|init| init.into_initializer().value(7).finish()));
        let mut context = Context::from_waker(Waker::noop());
        assert!(call.as_mut().poll(&mut context).is_pending());
        assert_eq!(client.in_flight(), 1);
        assert!(matches!(
            client
                .call(|init| init.into_initializer().value(8).finish())
                .await,
            Err(NativeCallError::TooManyInFlight)
        ));
        let request = admission.admit(requests.recv().await.unwrap()).unwrap();
        let id = request.header().unwrap().id.clone();
        wrong_route
            .send(
                MessageFields {
                    reqid: Some(&id),
                    ..MessageFields::default()
                },
                |init| init.into_initializer().value(9).finish(),
            )
            .unwrap();
        assert_eq!(client.stats().rejected.load(Ordering::Relaxed), 1);
        assert_eq!(client.in_flight(), 1);
        drop(call);
        assert_eq!(client.in_flight(), 0);
        reply_route
            .send(
                MessageFields {
                    reqid: Some(&id),
                    ..MessageFields::default()
                },
                |init| init.into_initializer().value(8).finish(),
            )
            .unwrap();
        assert_eq!(client.stats().unmatched.load(Ordering::Relaxed), 1);
        drop(request);
        core.detach(&mut subscription).unwrap();
        assert_eq!(core.free_slots(), 16);
    }

    #[tokio::test]
    async fn timeout_releases_pending_entry() {
        let (core, agreement, method, reply_to) = setup();
        let mut config = client_config();
        config.ttl = Duration::from_millis(1);
        let client = PreparedRpcClient::<_, Request, Reply>::new(
            Arc::clone(&core),
            agreement,
            reply_to,
            method,
            config,
        )
        .await
        .unwrap();
        assert!(matches!(
            client
                .call(|init| init.into_initializer().value(1).finish())
                .await,
            Err(NativeCallError::TimedOut)
        ));
        assert_eq!(client.in_flight(), 0);
        assert_eq!(core.free_slots(), 16);
    }

    #[tokio::test]
    async fn decoded_destination_is_checked_before_cache_or_handler() {
        let (core, agreement, method, reply_to) = setup();
        let other_method = UUri::try_from_wire_parts("other", 0x0001_0003, 1, 0x42).unwrap();
        let requests = Arc::new(
            RegistrationInbox::new(RegistrationConfig {
                capacity: 1,
                overflow: OverflowPolicy::DropNewest,
            })
            .unwrap(),
        );
        let mut subscription = core
            .attach(&reply_to, Some(&other_method), requests.clone())
            .await
            .unwrap();
        let route = TxTemplate::<Request>::new(agreement.clone())
            .unwrap()
            .route(
                Arc::clone(&core),
                RouteDefinition {
                    kind: UMessageType::Request,
                    source: reply_to,
                    sink: Some(other_method),
                    priority: Some(UPriority::CS4),
                    ttl: Some(Duration::from_secs(1)),
                    permission_level: None,
                },
                RouteConfig::default(),
            )
            .unwrap();
        let mut server = PreparedRpcServer::<_, Reply>::start::<Request, _>(
            Arc::clone(&core),
            agreement,
            method,
            Handler { status: false },
            server_config(1),
        )
        .await
        .unwrap();
        let state = Arc::clone(&server.state);
        route
            .send(MessageFields::default(), |init| {
                init.into_initializer().value(41).finish()
            })
            .unwrap();
        // Model a broad physical delivery hint: the validated encoded destination,
        // not the physical subscription that happened to deliver it, is authority.
        server
            .registration
            .as_ref()
            .unwrap()
            .inbox()
            .push(requests.recv().await.unwrap())
            .unwrap();
        server.stop().await.unwrap();
        assert_eq!(state.stats.rejected.load(Ordering::Relaxed), 1);
        assert_eq!(state.stats.responded.load(Ordering::Relaxed), 0);
        assert!(state.routes.iter().all(|slot| slot.get().is_none()));
        core.detach(&mut subscription).unwrap();
        assert_eq!(core.free_slots(), 16);
    }
}
