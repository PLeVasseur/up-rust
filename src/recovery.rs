//! Bounded ownership-side shutdown diagnostics, independent of payload engines.

use crate::RegistrationStopError;

/// A resource outcome is not the same as an operation succeeding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Completion {
    /// An attachment, remote state, or worker is still unresolved.
    Pending,
    /// All resources completed without a current terminal error.
    Succeeded,
    /// All resources completed, but a terminal worker failed.
    Failed,
}

/// Exact first failure and its owning attempt number.
#[derive(Clone, Debug)]
pub struct Failure<E> {
    /// Monotonically increasing attempt identifier within this owner.
    pub attempt: u64,
    /// Original typed error, not a reconstructed message.
    pub error: E,
}

/// Constant-space first-failure plus latest-outcome retention.
///
/// Every observed failure increments `failures`. Later non-first failures may
/// leave `latest` on a subsequent attempt; the count explicitly reports that
/// compaction. Export diagnostics before retry if every distinct failure is needed.
#[derive(Clone, Debug)]
pub struct FailureHistory<E> {
    /// First observed failure never overwritten by retry or reconciliation.
    pub first: Option<Failure<E>>,
    /// Most recent observation (success is `None`).
    pub latest: Option<E>,
    /// Total observed failures, including those no longer stored individually.
    pub failures: u64,
}
impl<E> Default for FailureHistory<E> {
    fn default() -> Self {
        Self {
            first: None,
            latest: None,
            failures: 0,
        }
    }
}
impl<E: Clone> FailureHistory<E> {
    /// Records an observation synchronously, before another suspension point.
    pub fn observe(&mut self, attempt: u64, error: Option<E>) {
        if let Some(error) = &error {
            self.failures = self.failures.saturating_add(1);
            if self.first.is_none() {
                self.first = Some(Failure {
                    attempt,
                    error: error.clone(),
                });
            }
        }
        self.latest = error;
    }
}

/// Owner-backed progress survives cancellation between detach and drain.
/// No heap allocation, payload staging, or per-message work is introduced.
#[derive(Clone, Debug)]
pub struct StopProgress<E> {
    /// Attempt identifier; zero means no stop has begun.
    pub attempt: u64,
    /// Exact detach failures, separate from the current retry's outcome.
    pub detach: FailureHistory<E>,
    /// Worker failures; `CalledFromWorker` is observable but not completion.
    pub worker: FailureHistory<RegistrationStopError>,
    /// First nonterminal self-stop refusal, kept separately from terminal errors.
    pub first_refusal: Option<Failure<RegistrationStopError>>,
    /// True only after a terminal worker join (including terminal failure).
    pub drained: bool,
}
impl<E> Default for StopProgress<E> {
    fn default() -> Self {
        Self {
            attempt: 0,
            detach: FailureHistory::default(),
            worker: FailureHistory::default(),
            first_refusal: None,
            drained: false,
        }
    }
}
impl<E: Clone> StopProgress<E> {
    /// Commits the synchronous detach outcome before returning a drain future.
    pub fn begin(&mut self, detach: Option<E>) {
        self.attempt = self.attempt.saturating_add(1);
        self.detach.observe(self.attempt, detach);
        self.drained = false;
    }
    /// Commits an actually observed worker result, never an inferred timeout.
    pub fn finish(&mut self, worker: Option<RegistrationStopError>) {
        self.drained = worker != Some(RegistrationStopError::CalledFromWorker);
        if worker == Some(RegistrationStopError::CalledFromWorker) {
            if self.first_refusal.is_none() {
                self.first_refusal = Some(Failure {
                    attempt: self.attempt,
                    error: RegistrationStopError::CalledFromWorker,
                });
            }
            self.worker.latest = worker;
        } else {
            self.worker.observe(self.attempt, worker);
        }
    }
    /// Completion keeps retriable detach failures separate from sticky workers.
    pub fn completion(&self) -> Completion {
        if self.attempt == 0 || !self.drained || self.detach.latest.is_some() {
            Completion::Pending
        } else if self.worker.latest.is_some() {
            Completion::Failed
        } else {
            Completion::Succeeded
        }
    }
}
