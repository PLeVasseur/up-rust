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

//! Callback admission and bounded quiescence for transport implementations.
//!
//! Admission, callback completion and retained storage are distinct contracts.
//! Closing or timing out a gate never revokes a live payload owner.

use std::{
    cell::{Cell, RefCell},
    future::{poll_fn, Future},
    marker::PhantomData,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        OnceLock,
    },
    task::Poll,
};
use tokio::{
    sync::{Mutex, Notify},
    time::{timeout_at, Instant},
};

static NEXT_REGISTRATION: AtomicU64 = AtomicU64::new(1);

thread_local! {
    // Nodes exist only during a synchronous poll and are never stored in a
    // future, moved across threads, or exposed to user code.
    static ASYNC_CALLBACKS: Cell<*const CallbackScope> = const { Cell::new(std::ptr::null()) };
    static NATIVE_CALLBACKS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

struct CallbackScope {
    id: u64,
    task: Option<tokio::task::Id>,
    parent: *const CallbackScope,
}

fn with_callback_scope<R>(id: u64, operation: impl FnOnce() -> R) -> R {
    struct Restore(*const CallbackScope);
    impl Drop for Restore {
        fn drop(&mut self) {
            ASYNC_CALLBACKS.with(|current| current.set(self.0));
        }
    }
    ASYNC_CALLBACKS.with(|current| {
        let scope = CallbackScope {
            id,
            task: tokio::task::try_id(),
            parent: current.get(),
        };
        let restore = Restore(scope.parent);
        current.set(std::ptr::from_ref(&scope));
        let value = operation();
        drop(restore);
        value
    })
}

/// Why a listener registration could not finish bounded quiescence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ListenerQuiesceError {
    /// Admission must be closed before waiting for callbacks to finish.
    #[error("listener admission is still open")]
    AdmissionOpen,
    /// The current callback or its enclosing callback would wait for itself.
    #[error("cannot quiesce an enclosing callback registration")]
    Reentrant,
    /// The deadline elapsed while entered work or its admission barrier remained.
    #[error("listener quiescence timed out")]
    QuiesceTimedOut,
}

/// Admission gate for one asynchronous listener registration.
///
/// Available with `transport-implementer-api`, also enabled by `util`. Transport workers keep their own
/// ordering, queue bounds and shutdown policy. This gate only serializes actual
/// callback entry with unregister. An entered callback may finish, including
/// unregistering itself; queued callbacks cannot enter after [`Self::stop`].
/// Each new registration needs a new gate; stopped gates are never reopened.
///
/// Native paths may additionally close with [`Self::cancel`] and wait using
/// [`Self::quiesce`]. Entered work is counted per registration until completion,
/// cancellation or panic. Quiescence never revokes a retained payload/storage
/// owner. Async context is installed on the current stack during each callback poll;
/// native worker callbacks use [`Self::enter_native_callback`].
#[derive(Debug)]
pub struct ListenerAdmission {
    id: u64,
    active: AtomicBool,
    entry: Mutex<()>,
    entered: AtomicUsize,
    changed: Notify,
    closed_at: OnceLock<Instant>,
}

#[derive(Debug)]
struct EnteredCallback<'a>(&'a ListenerAdmission);

impl Drop for EnteredCallback<'_> {
    fn drop(&mut self) {
        let previous = self.0.entered.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0);
        if previous == 1 {
            self.0.changed.notify_waiters();
        }
    }
}

/// Scoped admission for a synchronous callback on a native worker thread.
///
/// Keep this guard alive through the callback. Dropping it clears this thread's
/// re-entrancy context and releases the entered count, including during panic.
/// It must stay on its entering thread and does not own or revoke payload loans.
///
/// This guard is not `Send`; its thread-bound contract is checked by the
/// dedicated compiler-diagnostic test.
#[derive(Debug)]
#[must_use = "retain this guard through the native callback"]
pub struct NativeCallbackGuard<'a> {
    entered: EnteredCallback<'a>,
    thread_bound: PhantomData<Rc<()>>,
}

impl Drop for NativeCallbackGuard<'_> {
    fn drop(&mut self) {
        NATIVE_CALLBACKS.with(|callbacks| {
            let mut callbacks = callbacks.borrow_mut();
            if let Some(index) = callbacks.iter().rposition(|id| *id == self.entered.0.id) {
                callbacks.remove(index);
            }
        });
    }
}

impl Default for ListenerAdmission {
    fn default() -> Self {
        Self::new()
    }
}

impl ListenerAdmission {
    /// Creates an active gate for a new registration.
    ///
    /// # Panics
    ///
    /// Panics if the process exhausts the non-reused 64-bit registration keys.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: NEXT_REGISTRATION
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .expect("listener registration key space exhausted"),
            active: AtomicBool::new(true),
            entry: Mutex::new(()),
            entered: AtomicUsize::new(0),
            changed: Notify::new(),
            closed_at: OnceLock::new(),
        }
    }

    /// Cancels pending admission without waiting for a racing first poll.
    ///
    /// Records the admission-close instant once. Classic asynchronous unregister
    /// uses [`Self::stop`] for its first-poll barrier. A native quiescing path can
    /// cancel immediately, derive its deadline from [`Self::closed_at`], and call
    /// [`Self::quiesce`], which includes that barrier in the bounded wait.
    pub fn cancel(&self) {
        self.closed_at.get_or_init(Instant::now);
        self.active.store(false, Ordering::Release);
    }

    /// Returns the recorded admission-close instant, or `None` while open.
    #[must_use]
    pub fn closed_at(&self) -> Option<Instant> {
        self.closed_at.get().copied()
    }

    /// Returns the number of admitted async/native callback entries still live.
    ///
    /// Queue length and retained payload/loan owners are separate quantities.
    #[must_use]
    pub fn entered_count(&self) -> usize {
        self.entered.load(Ordering::Acquire)
    }

    /// Whether this registration is in the current callback's enclosing context.
    ///
    /// Async context is re-established each poll, including after thread migration. Native
    /// context is thread-local. Unrelated spawned Tokio tasks do not inherit the
    /// native thread's marker or another task's callback context.
    #[must_use]
    pub fn is_current_callback(&self) -> bool {
        let task = tokio::task::try_id();
        ASYNC_CALLBACKS.with(|current| {
            let mut pointer = current.get();
            while !pointer.is_null() {
                // SAFETY: with_callback_scope installs only pointers to live,
                // immobile stack nodes on this thread, restoring the parent
                // before a node dies, including during unwind. This traversal
                // cannot suspend or let any reference escape the closure.
                let scope = unsafe { &*pointer };
                if scope.task != task {
                    break;
                }
                if scope.id == self.id {
                    return true;
                }
                pointer = scope.parent;
            }
            task.is_none()
                && NATIVE_CALLBACKS.with(|callbacks| callbacks.borrow().contains(&self.id))
        })
    }

    fn entered(&self) -> EnteredCallback<'_> {
        self.entered
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                count.checked_add(1)
            })
            .expect("listener entered callback count overflow");
        EnteredCallback(self)
    }

    /// Admits synchronous callback work on a native worker thread, if still open.
    ///
    /// This guard-based admission is counted before returning to the caller; it
    /// may finish after admission closes. Native teardown uses quiescence unless
    /// it is re-entrant, retaining all live callback/storage owners in either case.
    ///
    /// # Panics
    ///
    /// Panics if called in asynchronous execution (use [`Self::dispatch`] there),
    /// or if the per-registration entered count cannot be incremented.
    pub fn enter_native_callback(&self) -> Option<NativeCallbackGuard<'_>> {
        let _entry = self.entry.blocking_lock();
        if !self.active.load(Ordering::Acquire) {
            return None;
        }
        let entered = self.entered();
        NATIVE_CALLBACKS.with(|callbacks| callbacks.borrow_mut().push(self.id));
        Some(NativeCallbackGuard {
            entered,
            thread_bound: PhantomData,
        })
    }

    /// Waits for the closed admission barrier and all entered callbacks.
    ///
    /// Native callers first cancel admission, then calculate `deadline` from
    /// [`Self::closed_at`] using their documented deadline interval. The counter
    /// drains on completion, cancellation and panic. This does not drain queues
    /// or revoke payload loans; a timeout never authorizes freeing live storage.
    ///
    /// # Errors
    ///
    /// Returns [`ListenerQuiesceError::AdmissionOpen`] if not closed,
    /// [`ListenerQuiesceError::Reentrant`] when waiting would include the current
    /// callback, or [`ListenerQuiesceError::QuiesceTimedOut`] on expiry. Admission
    /// stays closed after timeout. Native self-unregister skips this wait.
    ///
    /// # Panics
    ///
    /// Requires a Tokio runtime with time enabled when waiting. As with other
    /// async deadlines, callbacks must not block all executor threads.
    pub async fn quiesce(&self, deadline: Instant) -> Result<(), ListenerQuiesceError> {
        if self.is_current_callback() {
            return Err(ListenerQuiesceError::Reentrant);
        }
        if self.active.load(Ordering::Acquire) {
            return Err(ListenerQuiesceError::AdmissionOpen);
        }
        // A racing first poll/native admission may have observed active before
        // cancel. Include that serialization barrier in the same deadline.
        let entry = timeout_at(deadline, self.entry.lock())
            .await
            .map_err(|_| ListenerQuiesceError::QuiesceTimedOut)?;
        drop(entry);
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.entered_count() == 0 {
                return Ok(());
            }
            timeout_at(deadline, changed)
                .await
                .map_err(|_| ListenerQuiesceError::QuiesceTimedOut)?;
        }
    }

    /// Closes admission, serialized with a callback's first poll.
    ///
    /// This is an admission boundary, not a callback-draining operation. Returning
    /// from `stop` does not mean that previously entered callbacks have completed.
    /// Callbacks that have already yielded may finish. As with other async code,
    /// their first poll must yield promptly rather than block the executor.
    pub async fn stop(&self) {
        let _entry = self.entry.lock().await;
        self.cancel();
    }

    /// Enters a callback if this registration is still active.
    ///
    /// Future construction alone is not callback entry. The first poll is
    /// serialized with [`Self::stop`], then the gate is released so a callback
    /// can finish or unregister itself. A rejected callback is never constructed.
    /// Entered accounting remains live across awaits. Callback context is rebuilt
    /// on the poll stack without a per-dispatch allocation, and removed at each
    /// yield or unwind. Cancellation releases the entered count.
    ///
    /// # Panics
    ///
    /// Propagates callback panics; also panics if the entered count overflows.
    pub async fn dispatch<F: Future<Output = ()>>(&self, callback: impl FnOnce() -> F) {
        let entry = self.entry.lock().await;
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        let _entered = self.entered();
        let callback = with_callback_scope(self.id, callback);
        let mut callback = std::pin::pin!(callback);
        let pending = poll_fn(|cx| {
            Poll::Ready(with_callback_scope(self.id, || callback.as_mut().poll(cx)).is_pending())
        })
        .await;
        drop(entry);
        if pending {
            poll_fn(|cx| with_callback_scope(self.id, || callback.as_mut().poll(cx))).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, time::Duration};

    #[tokio::test]
    async fn stopped_registration_never_constructs_a_callback() {
        let admission = ListenerAdmission::new();
        admission.stop().await;
        admission
            .dispatch(|| -> std::future::Ready<()> {
                panic!("callback constructed after unregister")
            })
            .await;
    }

    #[tokio::test]
    async fn callback_can_unregister_itself() {
        let admission = ListenerAdmission::new();
        tokio::time::timeout(
            Duration::from_secs(5),
            admission.dispatch(|| async {
                admission.stop().await;
            }),
        )
        .await
        .expect("self-unregister must not hold admission across a yield");
    }

    #[tokio::test]
    async fn queued_callback_cannot_enter_after_unregister() {
        let admission = Arc::new(ListenerAdmission::new());
        let ready = Arc::new(tokio::sync::Notify::new());
        let task = tokio::spawn({
            let admission = Arc::clone(&admission);
            let ready = Arc::clone(&ready);
            async move {
                ready.notified().await;
                admission
                    .dispatch(|| async { panic!("queued callback entered") })
                    .await;
            }
        });
        admission.stop().await;
        ready.notify_one();
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn stop_returns_while_an_entered_callback_is_pending() {
        let admission = Arc::new(ListenerAdmission::new());
        let (entered, entry) = tokio::sync::oneshot::channel();
        let (release, completion) = tokio::sync::oneshot::channel();
        let delivery = tokio::spawn({
            let admission = admission.clone();
            async move {
                admission
                    .dispatch(|| async move {
                        entered.send(()).unwrap();
                        completion.await.expect("release entered callback");
                    })
                    .await;
            }
        });
        tokio::time::timeout(Duration::from_secs(5), entry)
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), admission.stop())
            .await
            .unwrap();
        assert!(
            !delivery.is_finished(),
            "stop must not imply callback completion"
        );
        admission
            .dispatch(|| async { panic!("new entry after stop") })
            .await;
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), delivery)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unregister_waits_for_the_callbacks_first_poll_to_yield() {
        let admission = Arc::new(ListenerAdmission::new());
        let entered = Arc::new(tokio::sync::Notify::new());
        let (release, wait) = std::sync::mpsc::channel();
        let delivery = tokio::spawn({
            let admission = Arc::clone(&admission);
            let entered = Arc::clone(&entered);
            async move {
                admission
                    .dispatch(|| async move {
                        entered.notify_one();
                        wait.recv_timeout(Duration::from_secs(5))
                            .expect("release first poll");
                        std::future::pending::<()>().await;
                    })
                    .await;
            }
        });
        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .unwrap();
        let mut stop = std::pin::pin!(admission.stop());
        assert!(poll_fn(|cx| Poll::Ready(stop.as_mut().poll(cx)))
            .await
            .is_pending());
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), stop)
            .await
            .unwrap();
        delivery.abort();
        assert!(delivery.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn quiesce_requires_closed_admission_and_observes_completion() {
        let admission = Arc::new(ListenerAdmission::new());
        assert_eq!(
            admission.quiesce(Instant::now()).await,
            Err(ListenerQuiesceError::AdmissionOpen)
        );
        let (entered, entry) = tokio::sync::oneshot::channel();
        let (release, completion) = tokio::sync::oneshot::channel();
        let delivery = tokio::spawn({
            let admission = admission.clone();
            async move {
                admission
                    .dispatch(|| async move {
                        entered.send(()).unwrap();
                        completion.await.unwrap();
                    })
                    .await;
            }
        });
        entry.await.unwrap();
        assert_eq!(admission.entered_count(), 1);
        admission.cancel();
        let closed = admission.closed_at().unwrap();
        admission.cancel();
        assert_eq!(admission.closed_at(), Some(closed));
        let mut wait = std::pin::pin!(admission.quiesce(closed + Duration::from_secs(5)));
        assert!(poll_fn(|cx| Poll::Ready(wait.as_mut().poll(cx)))
            .await
            .is_pending());
        release.send(()).unwrap();
        wait.await.unwrap();
        delivery.await.unwrap();
        assert_eq!(admission.entered_count(), 0);
    }

    #[tokio::test]
    async fn timeout_retains_live_callback_ownership_until_cancellation() {
        let admission = Arc::new(ListenerAdmission::new());
        let owner = Arc::new(17u8);
        let weak = Arc::downgrade(&owner);
        let (entered, entry) = tokio::sync::oneshot::channel();
        let delivery = tokio::spawn({
            let admission = admission.clone();
            async move {
                admission
                    .dispatch(|| async move {
                        let _retained_owner = owner;
                        entered.send(()).unwrap();
                        std::future::pending::<()>().await;
                    })
                    .await;
            }
        });
        entry.await.unwrap();
        admission.cancel();
        assert_eq!(
            admission
                .quiesce(Instant::now() + Duration::from_millis(5))
                .await,
            Err(ListenerQuiesceError::QuiesceTimedOut)
        );
        assert!(weak.upgrade().is_some());
        assert_eq!(admission.entered_count(), 1);
        delivery.abort();
        assert!(delivery.await.unwrap_err().is_cancelled());
        admission
            .quiesce(Instant::now() + Duration::from_secs(5))
            .await
            .unwrap();
        assert!(weak.upgrade().is_none());
    }

    #[tokio::test]
    async fn async_self_and_nested_registration_contexts_are_distinct() {
        let a = ListenerAdmission::new();
        let b = ListenerAdmission::new();
        let other = ListenerAdmission::new();
        a.dispatch(|| async {
            assert!(a.is_current_callback());
            b.dispatch(|| async {
                assert!(a.is_current_callback() && b.is_current_callback());
                assert!(!other.is_current_callback());
                a.cancel();
                b.cancel();
                other.cancel();
                assert_eq!(
                    a.quiesce(Instant::now()).await,
                    Err(ListenerQuiesceError::Reentrant)
                );
                assert_eq!(
                    b.quiesce(Instant::now()).await,
                    Err(ListenerQuiesceError::Reentrant)
                );
                other
                    .quiesce(Instant::now() + Duration::from_secs(5))
                    .await
                    .unwrap();
            })
            .await;
        })
        .await;
        assert!(!a.is_current_callback() && !b.is_current_callback());
        a.quiesce(Instant::now() + Duration::from_secs(5))
            .await
            .unwrap();
        b.quiesce(Instant::now() + Duration::from_secs(5))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn async_panic_releases_counter_and_context() {
        let admission = Arc::new(ListenerAdmission::new());
        let delivery = tokio::spawn({
            let admission = admission.clone();
            async move {
                admission
                    .dispatch(|| async { panic!("callback failure") })
                    .await;
            }
        });
        assert!(delivery.await.unwrap_err().is_panic());
        assert_eq!(admission.entered_count(), 0);
        assert!(!admission.is_current_callback());
        admission.cancel();
        admission
            .quiesce(Instant::now() + Duration::from_secs(5))
            .await
            .unwrap();
    }

    #[test]
    fn native_counter_guard_releases_on_panic_and_rejects_closed_entry() {
        let admission = ListenerAdmission::new();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = admission.enter_native_callback().unwrap();
            assert_eq!(admission.entered_count(), 1);
            assert!(admission.is_current_callback());
            panic!("native callback failure");
        }));
        assert!(outcome.is_err());
        assert_eq!(admission.entered_count(), 0);
        assert!(!admission.is_current_callback());
        admission.cancel();
        assert!(admission.enter_native_callback().is_none());
    }

    #[test]
    fn native_context_does_not_leak_into_an_unrelated_tokio_task() {
        let admission = Arc::new(ListenerAdmission::new());
        let guard = admission.enter_native_callback().unwrap();
        admission.cancel();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            assert!(admission.is_current_callback());
            assert_eq!(
                admission.quiesce(Instant::now()).await,
                Err(ListenerQuiesceError::Reentrant)
            );
            let worker = tokio::spawn({
                let admission = admission.clone();
                async move {
                    assert!(!admission.is_current_callback());
                    admission
                        .quiesce(Instant::now() + Duration::from_millis(5))
                        .await
                }
            });
            assert_eq!(
                worker.await.unwrap(),
                Err(ListenerQuiesceError::QuiesceTimedOut)
            );
        });
        drop(guard);
        assert_eq!(admission.entered_count(), 0);
    }

    #[test]
    fn task_context_survives_future_migration_between_threads() {
        let admission = Arc::new(ListenerAdmission::new());
        let release = Arc::new(AtomicBool::new(false));
        let future = Box::pin({
            let admission = admission.clone();
            let release = release.clone();
            async move {
                admission
                    .dispatch(|| {
                        poll_fn(|_| {
                            assert!(admission.is_current_callback());
                            if release.load(Ordering::Acquire) {
                                Poll::Ready(())
                            } else {
                                Poll::Pending
                            }
                        })
                    })
                    .await;
            }
        });
        let future = std::thread::spawn(move || {
            let mut future = future;
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            assert!(future.as_mut().poll(&mut context).is_pending());
            future
        })
        .join()
        .unwrap();
        assert_eq!(admission.entered_count(), 1);
        assert!(!admission.is_current_callback());
        release.store(true, Ordering::Release);
        std::thread::spawn(move || {
            let mut future = future;
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            assert!(future.as_mut().poll(&mut context).is_ready());
        })
        .join()
        .unwrap();
        assert_eq!(admission.entered_count(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quiesce_deadline_includes_a_racing_first_poll_barrier() {
        let admission = Arc::new(ListenerAdmission::new());
        let (entered, entry) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let delivery = tokio::spawn({
            let admission = admission.clone();
            async move {
                admission
                    .dispatch(|| async move {
                        entered.send(()).unwrap();
                        wait.recv_timeout(Duration::from_secs(5)).unwrap();
                    })
                    .await;
            }
        });
        entry.await.unwrap();
        admission.cancel();
        assert_eq!(
            admission
                .quiesce(admission.closed_at().unwrap() + Duration::from_millis(5))
                .await,
            Err(ListenerQuiesceError::QuiesceTimedOut)
        );
        assert_eq!(admission.entered_count(), 1);
        release.send(()).unwrap();
        delivery.await.unwrap();
        admission
            .quiesce(Instant::now() + Duration::from_secs(5))
            .await
            .unwrap();
    }
}
