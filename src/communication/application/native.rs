//! Static operation contracts for schema-owned deployment bindings.
//!
//! These traits are deliberately not object safe. They forward to the real
//! native engine, preserving synchronous loan initialization and retained RX
//! ownership. One domain binding associates these handles for reusable apps.
//!
//! Native cores already require Send/Sync core and route ownership, Send RX
//! frames and Send attachment handles. These operation contracts expose Send
//! futures without boxing them or imposing Sync on an attachment. TX remains
//! synchronous; no initializer or TX loan is carried into an async operation.

use super::{CleanupLeg, ManagedRole};
use crate::{
    communication::{typed, zero_copy as engine},
    AdmissionError, Completion, Frame, InitializedStablePayload, MessageFields, NativeCallError,
    NativeLoanCore, PayloadInit, SendError, SetupError, StablePayload, StablePayloadInit, UUri,
};
use std::{convert::Infallible, future::Future, time::Duration};

pub use super::native_binding::{
    Deployment, Notifier, Publisher, Receiver, RpcClient, RpcServer, Subscription,
};
pub use crate::communication::{typed::NativeConfig, zero_copy::ReceiveConfig};

/// A synchronous publisher operating exclusively on middleware loan storage.
///
/// ```compile_fail
/// use up_rust::{StablePayloadInit, communication::application::native::Publish};
/// fn cannot_await_a_loan<P: StablePayloadInit, Sender: Publish<P>>(sender: &Sender) {
///     sender.publish_in_place(|loan| async move { loan }).unwrap();
/// }
/// ```
pub trait Publish<P: StablePayloadInit> {
    /// Initializes and commits before return; the closure cannot await or escape.
    fn publish_in_place<F>(&self, initialize: F) -> Result<(), SendError>
    where
        F: for<'loan> FnOnce(PayloadInit<'loan, P>) -> InitializedStablePayload<'loan, P>;
    /// Advanced bounded header fields, with the same native ownership contract.
    fn publish_in_place_with<F>(
        &self,
        fields: MessageFields<'_>,
        initialize: F,
    ) -> Result<(), SendError>
    where
        F: for<'loan> FnOnce(PayloadInit<'loan, P>) -> InitializedStablePayload<'loan, P>;
}
impl<P: StablePayloadInit, C: NativeLoanCore> Publish<P>
    for typed::Publisher<P, engine::Publisher<C, P>>
{
    fn publish_in_place<F>(&self, initialize: F) -> Result<(), SendError>
    where
        F: for<'loan> FnOnce(PayloadInit<'loan, P>) -> InitializedStablePayload<'loan, P>,
    {
        self.publish_in_place(initialize)
    }
    fn publish_in_place_with<F>(
        &self,
        fields: MessageFields<'_>,
        initialize: F,
    ) -> Result<(), SendError>
    where
        F: for<'loan> FnOnce(PayloadInit<'loan, P>) -> InitializedStablePayload<'loan, P>,
    {
        self.publish_in_place_with(fields, initialize)
    }
}
/// A synchronous native notifier; no by-value copy fallback exists.
pub trait Notify<P: StablePayloadInit> {
    /// Initializes and commits to the setup-bound exact destination.
    fn notify_in_place<F>(&self, initialize: F) -> Result<(), SendError>
    where
        F: for<'loan> FnOnce(PayloadInit<'loan, P>) -> InitializedStablePayload<'loan, P>;
    /// Advanced bounded metadata overrides.
    fn notify_in_place_with<F>(
        &self,
        fields: MessageFields<'_>,
        initialize: F,
    ) -> Result<(), SendError>
    where
        F: for<'loan> FnOnce(PayloadInit<'loan, P>) -> InitializedStablePayload<'loan, P>;
}
impl<P: StablePayloadInit, C: NativeLoanCore> Notify<P>
    for typed::Notifier<P, engine::Notifier<C, P>>
{
    fn notify_in_place<F>(&self, initialize: F) -> Result<(), SendError>
    where
        F: for<'loan> FnOnce(PayloadInit<'loan, P>) -> InitializedStablePayload<'loan, P>,
    {
        self.notify_in_place(initialize)
    }
    fn notify_in_place_with<F>(
        &self,
        fields: MessageFields<'_>,
        initialize: F,
    ) -> Result<(), SendError>
    where
        F: for<'loan> FnOnce(PayloadInit<'loan, P>) -> InitializedStablePayload<'loan, P>,
    {
        self.notify_in_place_with(fields, initialize)
    }
}
/// Response wait retains the actual owner, not a copied or type-erased payload.
///
/// ```compile_fail
/// use up_rust::communication::application::Delivery;
/// fn cannot_escape<Owner: Delivery<u32>>(owner: Owner) -> &'static u32 {
///     owner.payload().unwrap()
/// }
/// ```
pub trait Response<P>: Sized {
    /// Owner exposing checked payload and borrowed semantic metadata.
    type Delivery: super::Delivery<P, Error = AdmissionError> + Send;
    /// Awaiting begins only after native TX commit; drop cancels correlation.
    fn response(self) -> impl Future<Output = Result<Self::Delivery, NativeCallError>> + Send;
}
impl<P: StablePayload, F: Frame> Response<P> for crate::PendingNativeCall<'_, P, F> {
    type Delivery = crate::RxLease<P, F>;
    fn response(self) -> impl Future<Output = Result<Self::Delivery, NativeCallError>> + Send {
        self.response()
    }
}
/// Client lifecycle independent of the request and response domain types, so
/// retained cleanup does not need them spelled out.
pub trait ClientClose {
    /// Closes admission; failed exact detach remains retryable.
    fn close(&self) -> Result<(), SetupError>;
}

/// Bounded synchronous native submission and a separately owned response wait.
///
/// ```compile_fail
/// use up_rust::{StablePayload, StablePayloadInit, communication::application::native::Invoke};
/// fn no_async_request<Req: StablePayloadInit, Resp: StablePayload, Client: Invoke<Req, Resp>>(
///     client: &Client,
/// ) {
///     client.invoke_in_place(|loan| async move { loan }).unwrap();
/// }
/// ```
pub trait Invoke<Req: StablePayloadInit, Resp: StablePayload>: ClientClose {
    /// Retained response owner independent of the pending guard's client borrow.
    type Delivery: super::Delivery<Resp, Error = AdmissionError> + Send;
    /// The pending guard borrows the client; it is not forged into a static owner.
    type Pending<'call>: Response<Resp, Delivery = Self::Delivery> + Send
    where
        Self: 'call;
    /// Reserves correlation, initializes loan storage and commits synchronously.
    fn invoke_in_place<F>(&self, initialize: F) -> Result<Self::Pending<'_>, NativeCallError>
    where
        F: for<'loan> FnOnce(PayloadInit<'loan, Req>) -> InitializedStablePayload<'loan, Req>;
    /// Bounded credential update; overflow preserves the previous credential.
    fn set_token(&self, token: &str) -> Result<(), crate::TokenError>;
    /// Clears future request credentials.
    fn clear_token(&self);
    /// Occupied correlation slots.
    fn in_flight(&self) -> usize;
    /// Native correlation diagnostics, without a payload projection.
    fn stats(&self) -> &crate::RpcClientStats;
}
impl<C: NativeLoanCore, Req: StablePayloadInit, Resp: StablePayload> ClientClose
    for typed::RpcClient<Req, Resp, engine::RpcClient<C, Req, Resp>>
{
    fn close(&self) -> Result<(), SetupError> {
        self.close()
    }
}
impl<C: NativeLoanCore, Req: StablePayloadInit, Resp: StablePayload> Invoke<Req, Resp>
    for typed::RpcClient<Req, Resp, engine::RpcClient<C, Req, Resp>>
{
    type Delivery = crate::RxLease<Resp, C::Rx>;
    type Pending<'call>
        = crate::PendingNativeCall<'call, Resp, C::Rx>
    where
        Self: 'call;
    fn invoke_in_place<F>(&self, initialize: F) -> Result<Self::Pending<'_>, NativeCallError>
    where
        F: for<'loan> FnOnce(PayloadInit<'loan, Req>) -> InitializedStablePayload<'loan, Req>,
    {
        self.invoke_in_place(initialize)
    }
    fn set_token(&self, token: &str) -> Result<(), crate::TokenError> {
        self.set_token(token)
    }
    fn clear_token(&self) {
        self.clear_token();
    }
    fn in_flight(&self) -> usize {
        self.in_flight()
    }
    fn stats(&self) -> &crate::RpcClientStats {
        self.stats()
    }
}
/// Native notification lifecycle; the concrete owner retains all receive loans.
pub trait Receive: Sized + Send {
    /// Prompt local closure independent of remote operations.
    fn close_admission(&self);
    /// Exact first failure and current stop attempt.
    fn stop_progress(&self) -> &crate::StopProgress<SetupError>;
    /// Admission counters on the underlying owner.
    fn stats(&self) -> &engine::ReceiverStats;
    /// Closes admission and attempts exact physical detach before returning.
    /// Implementations must fence new admissions even if the returned future is
    /// never polled. Failed detach and pending drain retain their exact owners.
    /// Same-worker `CalledFromWorker` is nonterminal, never cached: release the
    /// borrow and retry from outside the worker to observe its actual outcome.
    fn stop(&mut self) -> impl Future<Output = Result<(), engine::ReceiverStopError>> + Send;
}
impl<C: NativeLoanCore> Receive for typed::Receiver<engine::Receiver<C>> {
    fn close_admission(&self) {
        self.close_admission();
    }
    fn stop_progress(&self) -> &crate::StopProgress<SetupError> {
        self.stop_progress()
    }
    fn stats(&self) -> &engine::ReceiverStats {
        self.stats()
    }
    fn stop(&mut self) -> impl Future<Output = Result<(), engine::ReceiverStopError>> + Send {
        self.stop_ref()
    }
}
/// Native response-route reservation and server lifecycle.
pub trait Serve: Sized + Send {
    /// Prompt local closure independent of remote operations.
    fn close_admission(&self);
    /// Exact first failure and current stop attempt.
    fn stop_progress(&self) -> &crate::StopProgress<SetupError>;
    /// Reserves a real native response route, not merely a logical address.
    fn prepare_client(&self, reply_to: &UUri) -> Result<(), SetupError>;
    /// Native server resource and send diagnostics.
    fn stats(&self) -> &crate::RpcServerStats;
    /// Closes admission and attempts exact physical detach before returning.
    /// Implementations must fence new admissions even if the returned future is
    /// never polled. Failed detach and pending drain retain their exact owners.
    /// Same-worker `CalledFromWorker` is nonterminal, never cached: release the
    /// borrow and retry from outside the worker to observe its actual outcome.
    fn stop(&mut self) -> impl Future<Output = Result<(), crate::RpcServerStopError>> + Send;
}
impl<C: NativeLoanCore, Resp: StablePayloadInit> Serve
    for typed::RpcServer<engine::RpcServer<C, Resp>>
{
    fn close_admission(&self) {
        self.close_admission();
    }
    fn stop_progress(&self) -> &crate::StopProgress<SetupError> {
        self.stop_progress()
    }
    fn prepare_client(&self, reply_to: &UUri) -> Result<(), SetupError> {
        self.prepare_client(reply_to)
    }
    fn stats(&self) -> &crate::RpcServerStats {
        self.stats()
    }
    fn stop(&mut self) -> impl Future<Output = Result<(), crate::RpcServerStopError>> + Send {
        self.stop_ref()
    }
}
/// Retained native subscription reconciliation contract.
pub trait Subscribe: Send {
    /// Prompt local closure without asserting remote completion.
    fn close_admission(&mut self);
    /// Exact local partial outcomes across cancelled unsubscribe attempts.
    fn stop_progress(&self) -> &crate::StopProgress<SetupError>;
    /// Remote failure history independent of reconciliation.
    fn service_history(&self) -> &crate::FailureHistory<crate::UStatus>;
    /// The remote operation borrows the ticket; cancellation retains uncertainty.
    fn activate(
        &mut self,
    ) -> impl Future<Output = Result<(), typed::SubscriptionActivationError>> + Send;
    /// Closes local admission synchronously before returning the cleanup future.
    fn unsubscribe(
        &mut self,
    ) -> impl Future<Output = Result<(), typed::SubscriptionCloseError<SetupError>>> + Send;
    /// Only independently observed service state may reconcile cancellation.
    fn reconcile(&mut self, observed: crate::communication::SubscriptionStatus);
    /// Current retained lifecycle phase.
    fn phase(&self) -> typed::SubscriptionPhase;
    /// Retained service, physical detach and worker errors.
    fn close_outcome(&self) -> &typed::SubscriptionCloseError<SetupError>;
}
impl<C: NativeLoanCore> Subscribe for typed::Subscription<typed::NativeSubscription<C>> {
    fn close_admission(&mut self) {
        self.close_admission();
    }
    fn stop_progress(&self) -> &crate::StopProgress<SetupError> {
        self.stop_progress()
    }
    fn service_history(&self) -> &crate::FailureHistory<crate::UStatus> {
        self.service_history()
    }
    fn activate(
        &mut self,
    ) -> impl Future<Output = Result<(), typed::SubscriptionActivationError>> + Send {
        self.activate()
    }
    fn unsubscribe(
        &mut self,
    ) -> impl Future<Output = Result<(), typed::SubscriptionCloseError<SetupError>>> + Send {
        self.unsubscribe()
    }
    fn reconcile(&mut self, observed: crate::communication::SubscriptionStatus) {
        self.reconcile(observed);
    }
    fn phase(&self) -> typed::SubscriptionPhase {
        self.phase()
    }
    fn close_outcome(&self) -> &typed::SubscriptionCloseError<SetupError> {
        self.close_outcome()
    }
}

/// Independent native cleanup progress; allocation occurs only when storing an
/// actual cleanup error, never on native TX/RX or synchronous initialization.
#[derive(Debug, Default)]
pub struct NativeCleanup {
    /// Subscription's remote and local result.
    pub subscription: super::CleanupLeg<typed::SubscriptionCloseError<SetupError>>,
    /// Notification receiver result.
    pub notifications: super::CleanupLeg<engine::ReceiverStopError>,
    /// Request worker result.
    pub server: super::CleanupLeg<crate::RpcServerStopError>,
    /// Response correlation result.
    pub client: super::CleanupLeg<SetupError>,
}
impl NativeCleanup {
    /// Resource-complete failures are finite and distinguishable from success.
    pub fn completion(&self) -> crate::Completion {
        super::aggregate_completion(&[
            self.subscription.completion,
            self.notifications.completion,
            self.server.completion,
            self.client.completion,
        ])
    }
    /// Closes all admissions eagerly; no remote or drain await can starve peers.
    /// Partial setup passes None for roles which were never acquired.
    pub fn close<'a, Sub, Recv, Srv, Client>(
        &'a mut self,
        mut subscription: Option<&'a mut Sub>,
        notifications: Option<&'a mut Recv>,
        server: Option<&'a mut Srv>,
        client: Option<&'a Client>,
        deadline: std::time::Duration,
    ) -> impl Future<Output = crate::Completion> + Send + 'a
    where
        Sub: Subscribe,
        Recv: Receive,
        Srv: Serve,
        Client: ClientClose,
    {
        self.subscription.begin();
        self.notifications.begin();
        self.server.begin();
        self.client.begin();
        if let Some(owner) = subscription.as_mut() {
            owner.close_admission();
        }
        if let Some(owner) = &notifications {
            owner.close_admission();
        }
        if let Some(owner) = &server {
            owner.close_admission();
        }
        self.client
            .record(client.map_or(Ok(()), ClientClose::close), |_| {
                crate::Completion::Pending
            });
        let subscription = subscription.map(Subscribe::unsubscribe);
        let notifications = notifications.map(Receive::stop);
        let server = server.map(Serve::stop);
        async move {
            tokio::join!(
                self.subscription.run(
                    deadline,
                    async {
                        match subscription {
                            Some(future) => future.await,
                            None => Ok(()),
                        }
                    },
                    typed::SubscriptionCloseError::completion
                ),
                self.notifications.run(
                    deadline,
                    async {
                        match notifications {
                            Some(future) => future.await,
                            None => Ok(()),
                        }
                    },
                    engine::ReceiverStopError::completion
                ),
                self.server.run(
                    deadline,
                    async {
                        match server {
                            Some(future) => future.await,
                            None => Ok(()),
                        }
                    },
                    crate::RpcServerStopError::completion
                ),
            );
            self.completion()
        }
    }
}

fn no_cleanup(leg: &mut CleanupLeg<Infallible>) -> std::future::Ready<Completion> {
    leg.record(Ok(()), |never| match *never {});
    std::future::ready(leg.completion)
}

impl<P: StablePayloadInit, C: NativeLoanCore> ManagedRole
    for typed::Publisher<P, engine::Publisher<C, P>>
{
    type CloseError = Infallible;
    fn fence(&mut self) {}
    fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<Infallible>,
        _deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a {
        no_cleanup(leg)
    }
}
impl<P: StablePayloadInit, C: NativeLoanCore> ManagedRole
    for typed::Notifier<P, engine::Notifier<C, P>>
{
    type CloseError = Infallible;
    fn fence(&mut self) {}
    fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<Infallible>,
        _deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a {
        no_cleanup(leg)
    }
}
impl<C: NativeLoanCore, Req: StablePayloadInit, Resp: StablePayload> ManagedRole
    for typed::RpcClient<Req, Resp, engine::RpcClient<C, Req, Resp>>
{
    type CloseError = SetupError;
    fn fence(&mut self) {
        // `close` closes admission and detaches synchronously.
    }
    fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<SetupError>,
        _deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a {
        leg.record(self.close(), |_| Completion::Pending);
        std::future::ready(leg.completion)
    }
}
impl<C: NativeLoanCore> ManagedRole for typed::Receiver<engine::Receiver<C>> {
    type CloseError = engine::ReceiverStopError;
    fn fence(&mut self) {
        self.close_admission();
    }
    fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<engine::ReceiverStopError>,
        deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a {
        let stop = self.stop_ref();
        async move {
            leg.run(deadline, stop, engine::ReceiverStopError::completion)
                .await;
            leg.completion
        }
    }
}
impl<C: NativeLoanCore, Resp: StablePayloadInit> ManagedRole
    for typed::RpcServer<engine::RpcServer<C, Resp>>
{
    type CloseError = crate::RpcServerStopError;
    fn fence(&mut self) {
        self.close_admission();
    }
    fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<crate::RpcServerStopError>,
        deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a {
        let stop = self.stop_ref();
        async move {
            leg.run(deadline, stop, crate::RpcServerStopError::completion)
                .await;
            leg.completion
        }
    }
}
impl<C: NativeLoanCore> ManagedRole for typed::Subscription<typed::NativeSubscription<C>> {
    type CloseError = typed::SubscriptionCloseError<SetupError>;
    fn fence(&mut self) {
        self.close_admission();
    }
    fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<typed::SubscriptionCloseError<SetupError>>,
        deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a {
        let close = self.unsubscribe();
        async move {
            leg.run(deadline, close, typed::SubscriptionCloseError::completion)
                .await;
            leg.completion
        }
    }
}
