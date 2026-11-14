use crate::communication::SubscriptionStatus;
use crate::{core::usubscription::USubscription, RegistrationStopError, UStatus, UUri};
use std::{future::Future, sync::Arc, time::Duration};

pub(crate) fn retain_worker_outcome(
    terminal: &mut Option<RegistrationStopError>,
    current: Option<RegistrationStopError>,
) -> Option<RegistrationStopError> {
    match current {
        Some(RegistrationStopError::CalledFromWorker) => {
            (*terminal).or(Some(RegistrationStopError::CalledFromWorker))
        }
        Some(
            error @ (RegistrationStopError::Cancelled | RegistrationStopError::HandlerPanicked),
        ) => {
            if terminal.is_none() {
                *terminal = Some(error);
            }
            *terminal
        }
        None => *terminal,
    }
}

/// Observable subscription state retained when activation/cleanup is cancelled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubscriptionPhase {
    /// Local receive capability exists, gated off; no remote request has begun.
    Prepared,
    /// A service request may have completed remotely; explicit reconciliation needed.
    RemoteUnknown,
    /// Remote admission succeeded and local application delivery is active.
    Active,
    /// Local admission closed; cleanup remains retryable.
    Closing,
    /// Remote removal and local detach/drain completed.
    Closed,
}
/// Activation cannot silently repeat an RPC whose remote completion is unknown.
#[derive(Debug, thiserror::Error)]
pub enum SubscriptionActivationError {
    /// Independently observe service state before another activation attempt.
    #[error("subscription needs remote-state reconciliation")]
    ReconciliationRequired,
    /// A closing/closed subscription cannot reactivate its closed inbox.
    #[error("subscription is closing or closed")]
    Closed,
    /// Service error leaves remote completion unknown.
    #[error("subscription service: {0}")]
    Service(UStatus),
    /// Service returned a state other than subscribed/pending.
    #[error("subscription service did not admit the topic")]
    Refused,
}
/// All failed close outcomes, retained on the ticket for inspection and retry.
#[derive(Clone, Debug, thiserror::Error)]
#[error("subscription close: service={service:?}, detach={detach:?}, worker={worker:?}")]
pub struct SubscriptionCloseError<E: std::fmt::Debug> {
    /// Remote service failure.
    pub service: Option<UStatus>,
    /// Physical exact-detach failure.
    pub detach: Option<E>,
    /// Sticky terminal failure or current nonterminal `CalledFromWorker` refusal.
    pub worker: Option<RegistrationStopError>,
}
impl<E: std::fmt::Debug> SubscriptionCloseError<E> {
    /// Resource completion of this close failure, matching the stop errors'
    /// classifiers: a remote failure, failed detach or nonterminal worker refusal
    /// stays retryable; a terminal worker failure is final.
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

/// Independent observations of the remote subscription service, retained on the
/// ticket across timeouts and cancellation for inspection and later retry.
#[derive(Clone, Debug, Default)]
pub struct RemoteObservations {
    /// Observation attempts started, including cancelled ones.
    pub attempts: u64,
    /// True from an attempt's start until it completes; stays true if cancelled.
    pub pending: bool,
    /// Attempts that exceeded their deadline. A timeout is never evidence.
    pub timeouts: u64,
    /// The latest state the service reported; cleared when a new attempt starts.
    pub observed: Option<SubscriptionStatus>,
    /// Service errors, first and latest retained, never treated as a removal.
    pub failures: crate::FailureHistory<UStatus>,
}

/// SDK lifecycle mechanics; applications use the owning `Subscription` ticket.
#[doc(hidden)]
pub trait SubscriptionLocal {
    /// Binding-specific detach error.
    type Error: Clone + std::fmt::Debug;
    /// Sets the gate according to independently observed remote admission.
    fn set_active(&self, active: bool);
    /// Synchronously closes inbox admission.
    fn close_admission(&self);
    /// Detaches the exact handle; failed handles remain in this object.
    fn detach(&mut self) -> Result<(), Self::Error>;
    /// Borrows the worker owner so cancellation does not discard cleanup state.
    fn drain(&mut self) -> impl Future<Output = Result<(), RegistrationStopError>> + Send;
}

/// Local capability and remote lifecycle ticket. Activation and cleanup borrow
/// this value, so cancelling either future retains state for reconciliation.
/// Drop can release local resources; it never claims to undo an unknown remote RPC.
#[must_use = "retain the ticket and explicitly unsubscribe for remote cleanup"]
pub struct Subscription<R: SubscriptionLocal> {
    pub(crate) inner: R,
    service: Arc<dyn USubscription>,
    topic: UUri,
    subscriber: UUri,
    observations: RemoteObservations,
    phase: SubscriptionPhase,
    remote_removed: bool,
    worker_error: Option<RegistrationStopError>,
    progress: crate::StopProgress<R::Error>,
    service_history: crate::FailureHistory<UStatus>,
    last: SubscriptionCloseError<R::Error>,
}
impl<R: SubscriptionLocal> std::fmt::Debug for Subscription<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscription")
            .field("phase", &self.phase)
            .field("topic", &self.topic)
            .finish_non_exhaustive()
    }
}
impl<R: SubscriptionLocal> Subscription<R> {
    pub(crate) fn new(
        inner: R,
        service: Arc<dyn USubscription>,
        topic: UUri,
        subscriber: UUri,
    ) -> Self {
        Self {
            inner,
            service,
            topic,
            subscriber,
            observations: RemoteObservations::default(),
            phase: SubscriptionPhase::Prepared,
            remote_removed: true,
            worker_error: None,
            progress: crate::StopProgress::default(),
            service_history: crate::FailureHistory::default(),
            last: SubscriptionCloseError {
                service: None,
                detach: None,
                worker: None,
            },
        }
    }
    /// Current state, including an unknown remote completion after cancellation.
    pub fn phase(&self) -> SubscriptionPhase {
        self.phase
    }
    /// Latest cleanup outcomes. A terminal drain failure remains visible on retry.
    pub fn close_outcome(&self) -> &SubscriptionCloseError<R::Error> {
        &self.last
    }
    /// First and current exact local cleanup observations.
    pub fn stop_progress(&self) -> &crate::StopProgress<R::Error> {
        &self.progress
    }
    /// Remote errors remain inspectable after independent reconciliation.
    pub fn service_history(&self) -> &crate::FailureHistory<UStatus> {
        &self.service_history
    }
    /// Close local admission promptly; does not assert remote removal.
    pub fn close_admission(&mut self) {
        self.inner.close_admission();
        self.phase = SubscriptionPhase::Closing;
    }
    /// Finite failure is distinct from unresolved ownership and success.
    pub fn completion(&self) -> crate::Completion {
        if !self.remote_removed {
            crate::Completion::Pending
        } else {
            self.progress.completion()
        }
    }
    /// Starts a real service operation while retaining its ticket in the caller.
    ///
    /// # Errors
    /// Refuses unknown/closing state and preserves service failures as unknown.
    pub async fn activate(&mut self) -> Result<(), SubscriptionActivationError> {
        match self.phase {
            SubscriptionPhase::Active => return Ok(()),
            SubscriptionPhase::RemoteUnknown => {
                return Err(SubscriptionActivationError::ReconciliationRequired)
            }
            SubscriptionPhase::Closing | SubscriptionPhase::Closed => {
                return Err(SubscriptionActivationError::Closed)
            }
            SubscriptionPhase::Prepared => {}
        }
        self.phase = SubscriptionPhase::RemoteUnknown;
        self.remote_removed = false;
        let status = self
            .service
            .subscribe(&self.topic, None, None)
            .await
            .map_err(SubscriptionActivationError::Service)?;
        match status {
            SubscriptionStatus::Subscribed | SubscriptionStatus::SubscribePending => {
                self.phase = SubscriptionPhase::Active;
                self.inner.set_active(true);
                Ok(())
            }
            SubscriptionStatus::Unsubscribed => {
                self.phase = SubscriptionPhase::Prepared;
                self.remote_removed = true;
                Err(SubscriptionActivationError::Refused)
            }
            SubscriptionStatus::UnsubscribePending => Err(SubscriptionActivationError::Refused),
        }
    }
    /// Asks the real subscription service whether this subscriber is listed for the
    /// topic and [`reconcile`](Self::reconcile)s on a definite answer.
    ///
    /// A timeout, service error or cancelled attempt changes nothing on the ticket
    /// except [`remote_observations`](Self::remote_observations): the remote state
    /// stays unknown until the service actually answers.
    pub async fn observe_remote(&mut self, deadline: Duration) -> Option<SubscriptionStatus> {
        self.observations.attempts = self.observations.attempts.saturating_add(1);
        self.observations.pending = true;
        self.observations.observed = None;
        let outcome =
            tokio::time::timeout(deadline, self.service.fetch_subscribers(&self.topic)).await;
        self.observations.pending = false;
        match outcome {
            Err(_) => {
                self.observations.timeouts = self.observations.timeouts.saturating_add(1);
                None
            }
            Ok(Err(error)) => {
                let attempt = self.observations.attempts;
                self.observations.failures.observe(attempt, Some(error));
                None
            }
            Ok(Ok(subscribers)) => {
                let status = if subscribers.contains(&self.subscriber) {
                    SubscriptionStatus::Subscribed
                } else {
                    SubscriptionStatus::Unsubscribed
                };
                self.reconcile(status.clone());
                let attempt = self.observations.attempts;
                self.observations.failures.observe(attempt, None);
                self.observations.observed = Some(status.clone());
                Some(status)
            }
        }
    }
    /// Remote observation history retained on this ticket.
    pub fn remote_observations(&self) -> &RemoteObservations {
        &self.observations
    }
    /// Records state independently observed through the real subscription service.
    /// A timeout/cancelled future is NOT evidence of remote rollback. This never
    /// reopens a locally closing inbox, even if the remote state is subscribed.
    pub fn reconcile(&mut self, observed: SubscriptionStatus) {
        self.remote_removed = matches!(observed, SubscriptionStatus::Unsubscribed);
        if self.remote_removed {
            self.last.service = None;
        }
        if matches!(
            self.phase,
            SubscriptionPhase::Closing | SubscriptionPhase::Closed
        ) {
            return;
        }
        self.inner.set_active(matches!(
            observed,
            SubscriptionStatus::Subscribed | SubscriptionStatus::SubscribePending
        ));
        self.phase = match observed {
            SubscriptionStatus::Unsubscribed => SubscriptionPhase::Prepared,
            SubscriptionStatus::Subscribed | SubscriptionStatus::SubscribePending => {
                SubscriptionPhase::Active
            }
            SubscriptionStatus::UnsubscribePending => SubscriptionPhase::RemoteUnknown,
        };
    }
    /// Closes local admission and attempts exact detach synchronously, before the
    /// returned future can await remote cleanup. Cancellation leaves this ticket.
    ///
    /// # Errors
    /// Reports service, detach and worker outcomes together; retry uses
    /// retained failed handles and does not repeat a known successful remote removal.
    pub fn unsubscribe(
        &mut self,
    ) -> impl Future<Output = Result<(), SubscriptionCloseError<R::Error>>> + '_ {
        self.inner.close_admission();
        self.phase = SubscriptionPhase::Closing;
        self.last.detach = self.inner.detach().err();
        self.progress.begin(self.last.detach.clone());
        async move {
            if !self.remote_removed {
                self.last.service = self.service.unsubscribe(&self.topic).await.err();
                self.service_history
                    .observe(self.progress.attempt, self.last.service.clone());
                if self.last.service.is_none() {
                    self.remote_removed = true;
                }
            }
            self.last.worker =
                retain_worker_outcome(&mut self.worker_error, self.inner.drain().await.err());
            self.progress.finish(self.last.worker);
            if self.completion() != crate::Completion::Pending {
                self.phase = SubscriptionPhase::Closed;
            }
            if self.last.service.is_none()
                && self.last.detach.is_none()
                && self.last.worker.is_none()
            {
                self.phase = SubscriptionPhase::Closed;
                Ok(())
            } else {
                Err(self.last.clone())
            }
        }
    }
}
