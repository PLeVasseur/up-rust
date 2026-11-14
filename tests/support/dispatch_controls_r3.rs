//! R3: repoll refusal and panicking destructors with the pinned erased owner
//! retained outside `catch_unwind`.
//!
//! `InlineFuture::poll` clears its pollable flag before entering the hidden
//! future and restores it only for `Pending`. A repoll after `Ready` or after a
//! caught poll panic therefore panics with exactly `REPOLL_REFUSAL` without
//! re-entering the hidden future, which stays stored until owner drop. These
//! tests observe that refusal, count underlying polls, and check that the
//! hidden future's destructor runs exactly once, including a destructor that
//! panics. Panicking destructors are armed only for drops outside any unwind
//! and stand down if the thread is already panicking, so no assertion failure
//! can become a double panic (process abort).

use super::common::{self, Counts, Path, Plan, Snapshot};
use super::shapes::YieldOnce;
use std::{
    future::Future,
    panic::{catch_unwind, AssertUnwindSafe},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};
use up_rust::inline_dispatch::InlineFuture;

const CAPACITY: usize = 128;
const READY_VALUE: u32 = 7;
const POLL_PANIC: &str = "dispatch-controls probe poll panic";
const DESTRUCTOR_PANIC: &str = "dispatch-controls probe destructor panic";
const ASYNC_PANIC: &str = "dispatch-controls async body panic";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Script {
    PendingThenReady,
    PendingThenPanic,
}

/// Hand-written hidden future: its own destructor is directly observable.
struct Probe<'a> {
    polls: &'a AtomicUsize,
    drops: &'a AtomicUsize,
    script: Script,
    panic_on_drop: bool,
}
impl Future for Probe<'_> {
    type Output = u32;
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<u32> {
        if self.polls.fetch_add(1, Ordering::SeqCst) == 0 {
            context.waker().wake_by_ref();
            return Poll::Pending;
        }
        match self.script {
            Script::PendingThenReady => Poll::Ready(READY_VALUE),
            Script::PendingThenPanic => panic!("{POLL_PANIC}"),
        }
    }
}
impl Drop for Probe<'_> {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
        if self.panic_on_drop && !std::thread::panicking() {
            panic!("{DESTRUCTOR_PANIC}");
        }
    }
}

fn load(value: &AtomicUsize) -> usize {
    value.load(Ordering::SeqCst)
}

fn pollable_debug(pollable: bool) -> String {
    format!("InlineFuture {{ capacity: {CAPACITY}, pollable: {pollable}, .. }}")
}

fn retained_owner_refuses_repoll(script: Script) {
    let polls = AtomicUsize::new(0);
    let drops = AtomicUsize::new(0);
    {
        let mut owner = std::pin::pin!(InlineFuture::<u32, CAPACITY>::new(Probe {
            polls: &polls,
            drops: &drops,
            script,
            panic_on_drop: false,
        }));
        let mut context = Context::from_waker(Waker::noop());
        let first = catch_unwind(AssertUnwindSafe(|| owner.as_mut().poll(&mut context)));
        assert!(matches!(first, Ok(Poll::Pending)));
        assert_eq!(common::owner_debug(owner.as_ref()), pollable_debug(true));

        let second = catch_unwind(AssertUnwindSafe(|| owner.as_mut().poll(&mut context)));
        match script {
            Script::PendingThenReady => {
                assert!(matches!(second, Ok(Poll::Ready(READY_VALUE))));
            }
            Script::PendingThenPanic => {
                assert_eq!(common::panic_message(second).as_deref(), Some(POLL_PANIC));
            }
        }
        assert_eq!(
            (load(&polls), load(&drops)),
            (2, 0),
            "the hidden future ran twice and is retained, not destroyed"
        );
        assert_eq!(common::owner_debug(owner.as_ref()), pollable_debug(false));

        common::assert_repoll_refused(owner.as_mut(), &mut context);
        assert_eq!(
            (load(&polls), load(&drops)),
            (2, 0),
            "refused repolls neither re-enter nor destroy the hidden future"
        );
        assert_eq!(common::owner_debug(owner.as_ref()), pollable_debug(false));
    }
    assert_eq!(
        (load(&polls), load(&drops)),
        (2, 1),
        "owner drop runs the hidden destructor exactly once"
    );
}

#[test]
fn retained_owner_refuses_repoll_after_ready() {
    retained_owner_refuses_repoll(Script::PendingThenReady);
}

#[test]
fn retained_owner_refuses_repoll_after_caught_poll_panic() {
    retained_owner_refuses_repoll(Script::PendingThenPanic);
}

struct DropCount<'a>(&'a AtomicUsize);
impl Drop for DropCount<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// Async-generated hidden future: unwinding destroys its live locals and
/// poisons it; the owner's refusal must fire before the coroutine's own
/// resumed-after-panic check could, and owner drop must not destroy again.
#[test]
fn async_poll_panic_destroys_live_locals_once_and_refuses_reentry() {
    let resumes = AtomicUsize::new(0);
    let local_drops = AtomicUsize::new(0);
    {
        let captured = DropCount(&local_drops);
        let mut owner = std::pin::pin!(InlineFuture::<(), CAPACITY>::new(async {
            let _live = captured;
            YieldOnce::default().await;
            resumes.fetch_add(1, Ordering::SeqCst);
            panic!("{ASYNC_PANIC}");
        }));
        let mut context = Context::from_waker(Waker::noop());
        assert!(owner.as_mut().poll(&mut context).is_pending());
        assert_eq!(load(&local_drops), 0);

        let resumed = catch_unwind(AssertUnwindSafe(|| owner.as_mut().poll(&mut context)));
        assert_eq!(common::panic_message(resumed).as_deref(), Some(ASYNC_PANIC));
        assert_eq!(load(&resumes), 1);
        assert_eq!(
            load(&local_drops),
            1,
            "unwinding destroyed the live local inside the hidden future"
        );

        common::assert_repoll_refused(owner.as_mut(), &mut context);
        assert_eq!((load(&resumes), load(&local_drops)), (1, 1));
    }
    assert_eq!(
        load(&local_drops),
        1,
        "dropping the poisoned hidden future does not destroy its locals again"
    );
}

/// When unwinding leaves the owner's scope, the owner is dropped by that
/// unwind and the hidden destructor still runs exactly once.
#[test]
fn unwinding_out_of_owner_scope_destroys_hidden_future_once() {
    let polls = AtomicUsize::new(0);
    let drops = AtomicUsize::new(0);
    let mut first_pending = false;
    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let mut owner = std::pin::pin!(InlineFuture::<u32, CAPACITY>::new(Probe {
            polls: &polls,
            drops: &drops,
            script: Script::PendingThenPanic,
            panic_on_drop: false,
        }));
        let mut context = Context::from_waker(Waker::noop());
        first_pending = owner.as_mut().poll(&mut context).is_pending();
        owner.as_mut().poll(&mut context)
    }));
    assert_eq!(common::panic_message(unwound).as_deref(), Some(POLL_PANIC));
    assert!(first_pending);
    assert_eq!((load(&polls), load(&drops)), (2, 1));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Unpolled,
    Pending,
    Ready,
    PollPanicked,
}

#[derive(Debug, PartialEq, Eq)]
enum PollRecord {
    Pending,
    Ready(u32),
    Panicked(String),
}

fn record(result: std::thread::Result<Poll<u32>>) -> PollRecord {
    match result {
        Ok(Poll::Pending) => PollRecord::Pending,
        Ok(Poll::Ready(value)) => PollRecord::Ready(value),
        Err(payload) => PollRecord::Panicked(common::panic_text(&*payload)),
    }
}

/// The owner is created and pinned inside the outer `catch_unwind`; each
/// poll is caught separately so the owner survives it. Leaving the closure
/// normally drops the owner, and only then does the hidden destructor panic.
/// All observations are asserted after the catch.
fn panicking_destructor(stage: Stage) {
    let polls = AtomicUsize::new(0);
    let drops = AtomicUsize::new(0);
    let (script, poll_count, expected_records) = match stage {
        Stage::Unpolled => (Script::PendingThenReady, 0, Vec::new()),
        Stage::Pending => (Script::PendingThenReady, 1, vec![PollRecord::Pending]),
        Stage::Ready => (
            Script::PendingThenReady,
            2,
            vec![PollRecord::Pending, PollRecord::Ready(READY_VALUE)],
        ),
        Stage::PollPanicked => (
            Script::PendingThenPanic,
            2,
            vec![
                PollRecord::Pending,
                PollRecord::Panicked(POLL_PANIC.to_owned()),
            ],
        ),
    };
    let mut records = Vec::new();
    let mut drops_before_destruction = None;
    let destroyed = catch_unwind(AssertUnwindSafe(|| {
        let mut owner = std::pin::pin!(InlineFuture::<u32, CAPACITY>::new(Probe {
            polls: &polls,
            drops: &drops,
            script,
            panic_on_drop: true,
        }));
        let mut context = Context::from_waker(Waker::noop());
        for _ in 0..poll_count {
            records.push(record(catch_unwind(AssertUnwindSafe(|| {
                owner.as_mut().poll(&mut context)
            }))));
        }
        drops_before_destruction = Some(load(&drops));
    }));
    assert_eq!(
        common::panic_message(destroyed).as_deref(),
        Some(DESTRUCTOR_PANIC)
    );
    assert_eq!(records, expected_records);
    assert_eq!(drops_before_destruction, Some(0));
    assert_eq!(load(&polls), poll_count);
    assert_eq!(load(&drops), 1, "the panicking destructor ran exactly once");
}

#[test]
fn panicking_destructor_unpolled_runs_once() {
    panicking_destructor(Stage::Unpolled);
}

#[test]
fn panicking_destructor_after_pending_runs_once() {
    panicking_destructor(Stage::Pending);
}

#[test]
fn panicking_destructor_after_ready_runs_once() {
    panicking_destructor(Stage::Ready);
}

#[test]
fn panicking_destructor_after_caught_poll_panic_runs_once() {
    panicking_destructor(Stage::PollPanicked);
}

/// Cancels a pending public dyn send whose implementation guard destructor
/// panics. Unwinding crosses one (direct) or two (wire) erased owners; the
/// guard, async-local state and frame payload are each destroyed exactly once.
fn guard_destructor_panic_during_pending_cancellation(path: Path) {
    let counts = Arc::new(Counts::default());
    common::with_public(path, &counts, Plan::PanicInGuardDrop, |public| {
        let built = counts.snapshot();
        let input = common::frame_with_probe(&counts);
        let mut first_pending = false;
        let cancelled = catch_unwind(AssertUnwindSafe(|| {
            let mut future = std::pin::pin!(public.send_owned(input));
            first_pending = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending();
        }));
        assert_eq!(
            common::panic_message(cancelled).as_deref(),
            Some(common::GUARD_DROP_PANIC)
        );
        assert!(first_pending);
        assert_eq!(
            counts.snapshot(),
            Snapshot {
                encodes: built.encodes + path.encodes_per_send(),
                entries: 1,
                polls: 1,
                guard_drops: 1,
                state_drops: 1,
                payload_drops: built.payload_drops + 1,
                guard_seen: common::SUSPENDED_GUARD_VALUE,
                state_seen: common::SUSPENDED_STATE_VALUE,
                ..built
            }
        );
    });
}

#[test]
fn direct_guard_destructor_panic_during_pending_cancellation() {
    guard_destructor_panic_during_pending_cancellation(Path::Direct);
}

#[test]
fn preset_wire_guard_destructor_panic_during_pending_cancellation() {
    guard_destructor_panic_during_pending_cancellation(Path::Preset);
}

#[test]
fn configured_wire_guard_destructor_panic_during_pending_cancellation() {
    guard_destructor_panic_during_pending_cancellation(Path::Configured);
}
