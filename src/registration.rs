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
//! Bounded, protocol-independent registration inboxes and generic async workers.
//!
//! Queue storage and the worker are created at registration. Synchronous delivery
//! moves an item into the existing queue; the worker polls the handler's concrete
//! future in its own task state, without a per-item boxed future.

use std::{
    collections::VecDeque,
    future::Future,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::{sync::Notify, task::JoinHandle};

/// Behavior when a registration's reserved inbox is full.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverflowPolicy {
    /// Reject the incoming item without changing queued items.
    DropNewest,
    /// Block the delivering thread until space, closure, or this wait expires.
    ///
    /// This can block a runtime thread. Bindings must qualify their delivery
    /// topology and avoid holding locks needed by the handler or detachment.
    Block {
        /// Maximum wait for this push; zero performs one immediate attempt.
        deadline: Duration,
    },
}

/// Configuration applied once when a registration is created.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistrationConfig {
    /// Logical queue capacity; must be positive.
    pub capacity: usize,
    /// Full-queue behavior.
    pub overflow: OverflowPolicy,
}

/// Setup failure before a registration becomes usable.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RegistrationSetupError {
    /// A bounded queue cannot have zero capacity.
    #[error("registration capacity must be positive")]
    ZeroCapacity,
    /// The requested queue storage could not be reserved.
    #[error("registration capacity cannot be reserved")]
    CapacityUnavailable,
    /// A blocking deadline cannot be represented by the monotonic clock.
    #[error("registration deadline is too large")]
    InvalidDeadline,
    /// No Tokio runtime is available to own the registration worker.
    #[error("registration requires a Tokio runtime")]
    RuntimeUnavailable,
}

/// Fixed-size rejection from a synchronous inbox push.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum InboxPushError {
    /// Admission has closed.
    #[error("registration is closed")]
    Closed,
    /// DropNewest rejected an item because the reserved queue was full.
    #[error("registration inbox is full")]
    Full,
    /// The bounded wait for queue space expired.
    #[error("registration inbox deadline expired")]
    DeadlineExceeded,
}

/// Failure observed while joining a registration worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RegistrationStopError {
    /// The worker was cancelled before completing the drain.
    #[error("registration worker was cancelled")]
    Cancelled,
    /// The application handler panicked.
    #[error("registration handler panicked")]
    HandlerPanicked,
}

/// Atomic counters owned by a registration.
#[derive(Debug, Default)]
pub struct RegistrationStats {
    /// Items accepted into the queue.
    pub admitted: AtomicU64,
    /// Items delivered to the handler.
    pub delivered: AtomicU64,
    /// Items rejected under DropNewest.
    pub full: AtomicU64,
    /// Pushes that waited for queue space at least once.
    pub blocked: AtomicU64,
    /// Pushes whose bounded wait expired.
    pub deadline_expired: AtomicU64,
    /// Pushes rejected after admission closed.
    pub after_close: AtomicU64,
    /// Queued items discarded by cancellation/drop.
    pub discarded: AtomicU64,
}

struct State<T> {
    queue: VecDeque<T>,
    closed: bool,
}

/// Preallocated inbox that can be used by a synchronous binding callback.
///
/// It is generic over its item and has no dependency on payload, native identity,
/// frame, or transport types. Rejected items are released before push returns.
pub struct RegistrationInbox<T> {
    state: Mutex<State<T>>,
    capacity: usize,
    overflow: OverflowPolicy,
    space: Condvar,
    ready: Notify,
    stats: RegistrationStats,
}

impl<T> std::fmt::Debug for RegistrationInbox<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistrationInbox")
            .field("capacity", &self.capacity)
            .field("queued", &self.len())
            .finish_non_exhaustive()
    }
}

impl<T> RegistrationInbox<T> {
    /// Reserves the entire logical capacity before accepting any item.
    ///
    /// # Errors
    /// Rejects zero/unreservable capacity and an unrepresentable deadline.
    pub fn new(config: RegistrationConfig) -> Result<Self, RegistrationSetupError> {
        if config.capacity == 0 {
            return Err(RegistrationSetupError::ZeroCapacity);
        }
        if let OverflowPolicy::Block { deadline } = config.overflow {
            if Instant::now().checked_add(deadline).is_none() {
                return Err(RegistrationSetupError::InvalidDeadline);
            }
        }
        let mut queue = VecDeque::new();
        queue
            .try_reserve_exact(config.capacity)
            .map_err(|_| RegistrationSetupError::CapacityUnavailable)?;
        Ok(Self {
            state: Mutex::new(State {
                queue,
                closed: false,
            }),
            capacity: config.capacity,
            overflow: config.overflow,
            space: Condvar::new(),
            ready: Notify::new(),
            stats: RegistrationStats::default(),
        })
    }

    /// Moves one item into reserved storage, applying the configured full policy.
    ///
    /// Closure and insertion share the same lock, so a racing push cannot enter
    /// after close. The incoming item is released on rejection. No queued item
    /// is replaced to create space.
    ///
    /// # Errors
    /// Returns a Copy reason for closed, full, or expired admission.
    pub fn push(&self, item: T) -> Result<(), InboxPushError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let until = match self.overflow {
            OverflowPolicy::Block { deadline } => Instant::now().checked_add(deadline),
            OverflowPolicy::DropNewest => None,
        };
        let mut waited = false;
        loop {
            if state.closed {
                self.stats.after_close.fetch_add(1, Ordering::Relaxed);
                return Err(InboxPushError::Closed);
            }
            if state.queue.len() < self.capacity {
                state.queue.push_back(item);
                self.stats.admitted.fetch_add(1, Ordering::Relaxed);
                drop(state);
                self.ready.notify_one();
                return Ok(());
            }
            match self.overflow {
                OverflowPolicy::DropNewest => {
                    self.stats.full.fetch_add(1, Ordering::Relaxed);
                    return Err(InboxPushError::Full);
                }
                OverflowPolicy::Block { .. } => {
                    let Some(remaining) =
                        until.and_then(|until| until.checked_duration_since(Instant::now()))
                    else {
                        self.stats.deadline_expired.fetch_add(1, Ordering::Relaxed);
                        return Err(InboxPushError::DeadlineExceeded);
                    };
                    if remaining.is_zero() {
                        self.stats.deadline_expired.fetch_add(1, Ordering::Relaxed);
                        return Err(InboxPushError::DeadlineExceeded);
                    }
                    if !waited {
                        self.stats.blocked.fetch_add(1, Ordering::Relaxed);
                        waited = true;
                    }
                    let (next, _) = self
                        .space
                        .wait_timeout(state, remaining)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    state = next;
                }
            }
        }
    }

    /// Receives the next item, or None after admission closes and the queue drains.
    pub async fn recv(&self) -> Option<T> {
        loop {
            let notified = self.ready.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(item) = state.queue.pop_front() {
                    drop(state);
                    self.space.notify_one();
                    return Some(item);
                }
                if state.closed {
                    return None;
                }
            }
            notified.await;
        }
    }

    /// Closes admission and wakes blocked producers and receivers; queued items remain.
    pub fn close(&self) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .closed = true;
        self.space.notify_all();
        self.ready.notify_waiters();
    }

    /// Releases queued items after closure, outside the inbox lock.
    pub fn discard(&self) {
        self.close();
        loop {
            let item = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .queue
                .pop_front();
            let Some(item) = item else {
                return;
            };
            self.stats.discarded.fetch_add(1, Ordering::Relaxed);
            drop(item);
        }
    }

    /// Number of currently queued items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .queue
            .len()
    }

    /// Whether no item is currently queued.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Counters for this inbox and its worker.
    #[must_use]
    pub fn stats(&self) -> &RegistrationStats {
        &self.stats
    }
}

/// Statically dispatched handler owned by one registration worker.
pub trait RegistrationHandler<T: Send + 'static>: Send + 'static {
    /// Handles one admitted item. The concrete future is stored in the worker.
    fn on_receive(&mut self, item: T) -> impl Future<Output = ()> + Send;
}

/// One inbox, one generic worker and its binding-owned detachment operation.
pub struct Registration<T> {
    inbox: Arc<RegistrationInbox<T>>,
    worker: Option<JoinHandle<()>>,
    detach: Option<Box<dyn FnOnce() + Send>>,
}

struct WorkerCleanup<T>(Arc<RegistrationInbox<T>>);
impl<T> Drop for WorkerCleanup<T> {
    fn drop(&mut self) {
        self.0.discard();
    }
}

impl<T> std::fmt::Debug for Registration<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registration")
            .field("inbox", &self.inbox)
            .finish_non_exhaustive()
    }
}

impl<T: Send + 'static> Registration<T> {
    /// Creates queue storage and one runtime task, taking ownership of the handler.
    ///
    /// `detach` removes the binding's sink-table entry under its own lock. It is
    /// called once. Admission closes first to unblock full-queue producers before
    /// acquiring that lock. No binding lock may be held while awaiting a handler.
    ///
    /// # Errors
    /// Returns a setup error before creating a usable registration.
    pub fn new<H, D>(
        config: RegistrationConfig,
        mut handler: H,
        detach: D,
    ) -> Result<Self, RegistrationSetupError>
    where
        H: RegistrationHandler<T>,
        D: FnOnce() + Send + 'static,
    {
        let inbox = Arc::new(RegistrationInbox::new(config)?);
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| RegistrationSetupError::RuntimeUnavailable)?;
        let incoming = Arc::clone(&inbox);
        // Construct before spawning so even an unpolled, aborted worker closes
        // admission and releases queued owners. A handler panic has the same
        // fail-closed behavior instead of leaving an inbox with no consumer.
        let cleanup = WorkerCleanup(Arc::clone(&inbox));
        let worker = runtime.spawn(async move {
            let _cleanup = cleanup;
            while let Some(item) = incoming.recv().await {
                incoming.stats.delivered.fetch_add(1, Ordering::Relaxed);
                handler.on_receive(item).await;
            }
        });
        Ok(Self {
            inbox,
            worker: Some(worker),
            detach: Some(Box::new(detach)),
        })
    }

    /// Clones a delivery handle at setup; pushes through it do not allocate queue storage.
    #[must_use]
    pub fn inbox(&self) -> Arc<RegistrationInbox<T>> {
        Arc::clone(&self.inbox)
    }

    /// Closes admission, detaches, drains queued work and waits for the worker.
    ///
    /// From that worker's own handler, returns immediately after detachment; the
    /// worker finishes the current callback and drains earlier admissions. If an
    /// external caller cancels this future while waiting, Drop cancels the worker.
    /// Retained application items remain independently owned.
    ///
    /// # Errors
    /// Reports worker cancellation or a handler panic without formatting heap text.
    /// A terminated worker closes admission and releases its queued items.
    pub async fn stop(mut self) -> Result<(), RegistrationStopError> {
        self.detach();
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| tokio::task::try_id() == Some(worker.id()))
        {
            self.worker.take();
            return Ok(());
        }
        if let Some(worker) = self.worker.as_mut() {
            worker.await.map_err(|error| {
                if error.is_panic() {
                    RegistrationStopError::HandlerPanicked
                } else {
                    RegistrationStopError::Cancelled
                }
            })?;
        }
        self.worker.take();
        Ok(())
    }
}

impl<T> Registration<T> {
    fn detach(&mut self) {
        self.inbox.close();
        if let Some(detach) = self.detach.take() {
            detach();
        }
    }

    /// Current queue length.
    #[must_use]
    pub fn queued(&self) -> usize {
        self.inbox.len()
    }

    /// Registration counters.
    #[must_use]
    pub fn stats(&self) -> &RegistrationStats {
        self.inbox.stats()
    }
}

impl<T> Drop for Registration<T> {
    fn drop(&mut self) {
        self.detach();
        if let Some(worker) = self.worker.take() {
            self.inbox.discard();
            if tokio::task::try_id() != Some(worker.id()) {
                worker.abort();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn config(capacity: usize) -> RegistrationConfig {
        RegistrationConfig {
            capacity,
            overflow: OverflowPolicy::DropNewest,
        }
    }

    struct Sum(Arc<AtomicUsize>);
    impl RegistrationHandler<usize> for Sum {
        async fn on_receive(&mut self, value: usize) {
            tokio::task::yield_now().await;
            self.0.fetch_add(value, Ordering::Relaxed);
        }
    }

    #[test]
    fn setup_rejects_zero_or_unreservable_capacity() {
        assert!(matches!(
            RegistrationInbox::<u8>::new(config(0)),
            Err(RegistrationSetupError::ZeroCapacity)
        ));
        assert!(matches!(
            RegistrationInbox::<u8>::new(config(usize::MAX)),
            Err(RegistrationSetupError::CapacityUnavailable)
        ));
    }

    #[tokio::test]
    async fn drop_newest_keeps_original_item_and_close_drains() {
        let inbox = RegistrationInbox::new(config(1)).unwrap();
        inbox.push(1).unwrap();
        assert_eq!(inbox.push(2), Err(InboxPushError::Full));
        assert_eq!(inbox.stats().full.load(Ordering::Relaxed), 1);
        inbox.close();
        assert_eq!(inbox.push(3), Err(InboxPushError::Closed));
        assert_eq!(inbox.recv().await, Some(1));
        assert_eq!(inbox.recv().await, None);
    }

    #[test]
    fn zero_deadline_returns_without_overwriting_queue() {
        let inbox = RegistrationInbox::new(RegistrationConfig {
            capacity: 1,
            overflow: OverflowPolicy::Block {
                deadline: Duration::ZERO,
            },
        })
        .unwrap();
        inbox.push(1).unwrap();
        assert_eq!(inbox.push(2), Err(InboxPushError::DeadlineExceeded));
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox.stats().deadline_expired.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn close_wakes_a_blocked_delivery_thread() {
        let inbox = Arc::new(
            RegistrationInbox::new(RegistrationConfig {
                capacity: 1,
                overflow: OverflowPolicy::Block {
                    deadline: Duration::from_secs(5),
                },
            })
            .unwrap(),
        );
        inbox.push(1).unwrap();
        let sender = Arc::clone(&inbox);
        let blocked = std::thread::spawn(move || sender.push(2));
        let until = Instant::now() + Duration::from_secs(5);
        while inbox.stats().blocked.load(Ordering::Relaxed) == 0 {
            assert!(
                Instant::now() < until,
                "delivery thread did not reach its wait"
            );
            std::thread::yield_now();
        }
        inbox.close();
        assert_eq!(blocked.join().unwrap(), Err(InboxPushError::Closed));
        assert_eq!(inbox.len(), 1);
    }

    #[tokio::test]
    async fn stop_detaches_once_and_drains_prior_admissions() {
        let sum = Arc::new(AtomicUsize::new(0));
        let detachments = Arc::new(AtomicUsize::new(0));
        let detached = Arc::clone(&detachments);
        let registration = Registration::new(config(16), Sum(Arc::clone(&sum)), move || {
            detached.fetch_add(1, Ordering::Relaxed);
        })
        .unwrap();
        let inbox = registration.inbox();
        for value in 1..=16 {
            inbox.push(value).unwrap();
        }
        registration.stop().await.unwrap();
        assert_eq!(sum.load(Ordering::Relaxed), 136);
        assert_eq!(detachments.load(Ordering::Relaxed), 1);
        assert_eq!(inbox.stats().delivered.load(Ordering::Relaxed), 16);
        assert_eq!(inbox.push(1000), Err(InboxPushError::Closed));
        tokio::task::yield_now().await;
        assert_eq!(sum.load(Ordering::Relaxed), 136);
    }

    #[tokio::test]
    async fn external_drop_discards_before_worker_delivery() {
        let sum = Arc::new(AtomicUsize::new(0));
        let registration = Registration::new(config(1), Sum(Arc::clone(&sum)), || {}).unwrap();
        let inbox = registration.inbox();
        inbox.push(42).unwrap();
        drop(registration);
        tokio::task::yield_now().await;
        assert_eq!(sum.load(Ordering::Relaxed), 0);
        assert_eq!(inbox.stats().discarded.load(Ordering::Relaxed), 1);
        assert_eq!(inbox.push(43), Err(InboxPushError::Closed));
    }

    #[tokio::test]
    async fn handler_can_stop_its_own_registration() {
        struct SelfStop {
            registration: Arc<Mutex<Option<Registration<usize>>>>,
            done: Arc<Notify>,
        }
        impl RegistrationHandler<usize> for SelfStop {
            async fn on_receive(&mut self, _: usize) {
                let registration = self.registration.lock().unwrap().take().unwrap();
                registration.stop().await.unwrap();
                self.done.notify_one();
            }
        }
        let cell = Arc::new(Mutex::new(None));
        let done = Arc::new(Notify::new());
        let registration = Registration::new(
            config(1),
            SelfStop {
                registration: Arc::clone(&cell),
                done: Arc::clone(&done),
            },
            || {},
        )
        .unwrap();
        let inbox = registration.inbox();
        *cell.lock().unwrap() = Some(registration);
        inbox.push(1).unwrap();
        tokio::time::timeout(Duration::from_secs(5), done.notified())
            .await
            .unwrap();
        assert_eq!(inbox.push(2), Err(InboxPushError::Closed));
    }

    struct Held {
        entered: Arc<Notify>,
        release: Arc<Notify>,
        finished: Arc<AtomicUsize>,
    }
    impl RegistrationHandler<Arc<u8>> for Held {
        async fn on_receive(&mut self, owner: Arc<u8>) {
            self.entered.notify_one();
            self.release.notified().await;
            assert_eq!(*owner, 42);
            self.finished.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[tokio::test]
    async fn external_stop_waits_for_handler_and_closes_new_admission() {
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let finished = Arc::new(AtomicUsize::new(0));
        let registration = Registration::new(
            config(1),
            Held {
                entered: Arc::clone(&entered),
                release: Arc::clone(&release),
                finished: Arc::clone(&finished),
            },
            || {},
        )
        .unwrap();
        let inbox = registration.inbox();
        inbox.push(Arc::new(42)).unwrap();
        entered.notified().await;
        let mut stop = std::pin::pin!(registration.stop());
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(stop.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        assert_eq!(inbox.push(Arc::new(42)), Err(InboxPushError::Closed));
        assert_eq!(finished.load(Ordering::Relaxed), 0);
        release.notify_one();
        stop.await.unwrap();
        assert_eq!(finished.load(Ordering::Relaxed), 1);
        tokio::task::yield_now().await;
        assert_eq!(inbox.stats().delivered.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn cancelled_stop_releases_backlog_and_inflight_but_not_application_owner() {
        let entered = Arc::new(Notify::new());
        let finished = Arc::new(AtomicUsize::new(0));
        let registration = Registration::new(
            config(2),
            Held {
                entered: Arc::clone(&entered),
                release: Arc::new(Notify::new()),
                finished: Arc::clone(&finished),
            },
            || {},
        )
        .unwrap();
        let inbox = registration.inbox();
        let owner = Arc::new(42);
        let weak = Arc::downgrade(&owner);
        inbox.push(Arc::clone(&owner)).unwrap();
        entered.notified().await;
        inbox.push(Arc::clone(&owner)).unwrap();
        inbox.push(Arc::clone(&owner)).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(1), registration.stop())
                .await
                .is_err()
        );
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while Arc::strong_count(&owner) != 1 {
            assert!(tokio::time::Instant::now() < deadline);
            tokio::task::yield_now().await;
        }
        assert_eq!(inbox.stats().discarded.load(Ordering::Relaxed), 2);
        assert_eq!(finished.load(Ordering::Relaxed), 0);
        assert!(weak.upgrade().is_some());
        assert_eq!(*owner, 42);
        drop(owner);
        assert!(weak.upgrade().is_none());
    }

    #[tokio::test]
    async fn panicked_worker_closes_admission_and_reclaims_backlog() {
        struct Panics;
        impl RegistrationHandler<Arc<u8>> for Panics {
            async fn on_receive(&mut self, _: Arc<u8>) {
                panic!("handler panic control");
            }
        }
        let registration = Registration::new(config(2), Panics, || {}).unwrap();
        let inbox = registration.inbox();
        let owner = Arc::new(42);
        inbox.push(Arc::clone(&owner)).unwrap();
        inbox.push(Arc::clone(&owner)).unwrap();
        assert_eq!(
            registration.stop().await,
            Err(RegistrationStopError::HandlerPanicked)
        );
        assert_eq!(Arc::strong_count(&owner), 1);
        assert_eq!(inbox.stats().discarded.load(Ordering::Relaxed), 1);
        assert_eq!(inbox.push(owner), Err(InboxPushError::Closed));
    }

    #[tokio::test]
    async fn self_drop_finishes_current_callback_and_discards_backlog() {
        struct Drops {
            registration: Arc<Mutex<Option<Registration<usize>>>>,
            finished: Arc<Notify>,
        }
        impl RegistrationHandler<usize> for Drops {
            async fn on_receive(&mut self, value: usize) {
                assert_eq!(value, 1);
                let registration = self.registration.lock().unwrap().take().unwrap();
                drop(registration);
                tokio::task::yield_now().await;
                self.finished.notify_one();
            }
        }
        let cell = Arc::new(Mutex::new(None));
        let finished = Arc::new(Notify::new());
        let registration = Registration::new(
            config(2),
            Drops {
                registration: Arc::clone(&cell),
                finished: Arc::clone(&finished),
            },
            || {},
        )
        .unwrap();
        let inbox = registration.inbox();
        *cell.lock().unwrap() = Some(registration);
        inbox.push(1).unwrap();
        inbox.push(2).unwrap();
        tokio::time::timeout(Duration::from_secs(5), finished.notified())
            .await
            .unwrap();
        assert_eq!(inbox.stats().delivered.load(Ordering::Relaxed), 1);
        assert_eq!(inbox.stats().discarded.load(Ordering::Relaxed), 1);
        assert_eq!(inbox.push(3), Err(InboxPushError::Closed));
    }

    #[tokio::test]
    async fn self_stop_drains_previously_admitted_items_in_order() {
        struct Stops {
            registration: Arc<Mutex<Option<Registration<usize>>>>,
            order: Arc<Mutex<Vec<usize>>>,
            finished: Arc<Notify>,
        }
        impl RegistrationHandler<usize> for Stops {
            async fn on_receive(&mut self, value: usize) {
                let registration = self.registration.lock().unwrap().take();
                if let Some(registration) = registration {
                    registration.stop().await.unwrap();
                }
                tokio::task::yield_now().await;
                self.order.lock().unwrap().push(value);
                if value == 3 {
                    self.finished.notify_one();
                }
            }
        }
        let cell = Arc::new(Mutex::new(None));
        let order = Arc::new(Mutex::new(Vec::new()));
        let finished = Arc::new(Notify::new());
        let registration = Registration::new(
            config(3),
            Stops {
                registration: Arc::clone(&cell),
                order: Arc::clone(&order),
                finished: Arc::clone(&finished),
            },
            || {},
        )
        .unwrap();
        let inbox = registration.inbox();
        *cell.lock().unwrap() = Some(registration);
        for value in 1..=3 {
            inbox.push(value).unwrap();
        }
        tokio::time::timeout(Duration::from_secs(5), finished.notified())
            .await
            .unwrap();
        assert_eq!(*order.lock().unwrap(), [1, 2, 3]);
        assert_eq!(inbox.push(4), Err(InboxPushError::Closed));
    }

    #[test_case::test_case(false; "reentrant enqueue uses available capacity")]
    #[test_case::test_case(true; "reentrant full Block queue reaches deadline")]
    #[tokio::test]
    async fn reentrant_delivery_obeys_reserved_capacity(block: bool) {
        struct Reentrant {
            inbox: Arc<Mutex<Option<Arc<RegistrationInbox<usize>>>>>,
            blocked: bool,
            done: Arc<Notify>,
        }
        impl RegistrationHandler<usize> for Reentrant {
            async fn on_receive(&mut self, value: usize) {
                if value == 1 {
                    let inbox = self.inbox.lock().unwrap().as_ref().unwrap().clone();
                    if self.blocked {
                        inbox.push(4).unwrap();
                    }
                    assert_eq!(
                        inbox.push(3),
                        if self.blocked {
                            Err(InboxPushError::DeadlineExceeded)
                        } else {
                            Ok(())
                        }
                    );
                    self.done.notify_one();
                }
            }
        }
        let cell = Arc::new(Mutex::new(None));
        let done = Arc::new(Notify::new());
        let registration = Registration::new(
            RegistrationConfig {
                capacity: 2,
                overflow: if block {
                    OverflowPolicy::Block {
                        deadline: Duration::from_millis(1),
                    }
                } else {
                    OverflowPolicy::DropNewest
                },
            },
            Reentrant {
                inbox: Arc::clone(&cell),
                blocked: block,
                done: Arc::clone(&done),
            },
            || {},
        )
        .unwrap();
        let inbox = registration.inbox();
        *cell.lock().unwrap() = Some(Arc::clone(&inbox));
        inbox.push(1).unwrap();
        if block {
            inbox.push(2).unwrap();
        }
        tokio::time::timeout(Duration::from_secs(5), done.notified())
            .await
            .unwrap();
        registration.stop().await.unwrap();
        assert!(inbox.is_empty());
        assert_eq!(
            inbox.stats().deadline_expired.load(Ordering::Relaxed),
            u64::from(block)
        );
        cell.lock().unwrap().take();
    }
}
