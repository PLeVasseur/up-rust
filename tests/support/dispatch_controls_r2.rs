//! R2: an implementation future holding a real `&mut` borrow into its own
//! async-local state across `Pending`, whose guard destructor dereferences
//! that borrow, driven only through public dyn `UOwnedTransport::send_owned`.
//!
//! Each composition (direct `InlineOwnedTransport`; preset `UWireTransport`
//! over `InlineOwnedCore`; provisioned-context `OwnedWireTransport` over
//! `InlineOwnedCore`) is checked for completion, cancellation while `Pending`
//! and unwinding out of a poll. Shared inspection of the pinned erased owner
//! happens between polls. Exact counters prove lazy encoding, single core
//! entry and single destruction of guard, state and frame payload.

use super::common::{self, Counts, Path, Plan, Snapshot};
use std::{
    future::Future,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
    task::{Context, Poll, Waker},
};
use up_rust::inline_dispatch::SEND_FUTURE_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Complete,
    Cancel,
    Unwind,
}

fn exercise(path: Path, outcome: Outcome) {
    let counts = Arc::new(Counts::default());
    let plan = match outcome {
        Outcome::Complete | Outcome::Cancel => Plan::Complete,
        Outcome::Unwind => Plan::PanicOnResume,
    };
    common::with_public(path, &counts, plan, |public| {
        let built = counts.snapshot();
        assert_eq!(
            built,
            path.construction(),
            "adapter setup has an exact, separate codec cost"
        );

        // Cancelling before the first poll runs neither the codec nor the core;
        // the captured frame is still destroyed exactly once.
        drop(public.send_owned(common::frame_with_probe(&counts)));
        let unpolled = Snapshot {
            payload_drops: built.payload_drops + 1,
            ..built
        };
        assert_eq!(counts.snapshot(), unpolled);

        let suspended = Snapshot {
            encodes: built.encodes + path.encodes_per_send(),
            entries: 1,
            polls: 1,
            ..unpolled
        };
        let destroyed_suspended = Snapshot {
            guard_drops: 1,
            state_drops: 1,
            payload_drops: unpolled.payload_drops + 1,
            guard_seen: common::SUSPENDED_GUARD_VALUE,
            state_seen: common::SUSPENDED_STATE_VALUE,
            ..suspended
        };
        let completed = Snapshot {
            polls: 2,
            completions: 1,
            guard_seen: common::COMPLETED_GUARD_VALUE,
            state_seen: common::COMPLETED_STATE_VALUE,
            ..destroyed_suspended
        };
        let unwound = Snapshot {
            polls: 2,
            ..destroyed_suspended
        };

        let input = common::frame_with_probe(&counts);
        {
            let mut future = std::pin::pin!(public.send_owned(input));
            assert_eq!(counts.snapshot(), unpolled, "creating the send is lazy");
            let mut context = Context::from_waker(Waker::noop());
            assert!(future.as_mut().poll(&mut context).is_pending());
            assert_eq!(
                counts.snapshot(),
                suspended,
                "first poll encodes once and enters the core once"
            );

            // Shared inspection of the erased owner while its hidden future
            // holds live borrows into its own storage.
            assert_eq!(
                common::owner_debug(future.as_ref()),
                format!("InlineFuture {{ capacity: {SEND_FUTURE_BYTES}, pollable: true, .. }}")
            );
            let _shared = std::hint::black_box(future.as_ref().get_ref());
            assert_eq!(counts.snapshot(), suspended);

            match outcome {
                Outcome::Complete => {
                    assert!(matches!(
                        future.as_mut().poll(&mut context),
                        Poll::Ready(Ok(()))
                    ));
                    assert_eq!(counts.snapshot(), completed);
                    common::assert_repoll_refused(future.as_mut(), &mut context);
                    assert_eq!(counts.snapshot(), completed, "repoll re-entered nothing");
                }
                Outcome::Cancel => {
                    assert_eq!(counts.snapshot(), suspended);
                }
                Outcome::Unwind => {
                    let resumed =
                        catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(&mut context)));
                    assert_eq!(
                        common::panic_message(resumed).as_deref(),
                        Some(common::RESUME_PANIC)
                    );
                    assert_eq!(
                        counts.snapshot(),
                        unwound,
                        "unwinding destroyed guard, state and frame exactly once"
                    );
                    common::assert_repoll_refused(future.as_mut(), &mut context);
                    assert_eq!(counts.snapshot(), unwound, "repoll re-entered nothing");
                }
            }
        }
        // The pinned erased owner has now been dropped.
        let after_drop = match outcome {
            Outcome::Complete => completed,
            Outcome::Cancel => destroyed_suspended,
            Outcome::Unwind => unwound,
        };
        assert_eq!(
            counts.snapshot(),
            after_drop,
            "owner drop destroys each live value exactly once"
        );
    });
}

#[test]
fn direct_self_borrow_completes() {
    exercise(Path::Direct, Outcome::Complete);
}

#[test]
fn direct_self_borrow_cancels_while_pending() {
    exercise(Path::Direct, Outcome::Cancel);
}

#[test]
fn direct_self_borrow_poll_unwinds() {
    exercise(Path::Direct, Outcome::Unwind);
}

#[test]
fn preset_wire_self_borrow_completes() {
    exercise(Path::Preset, Outcome::Complete);
}

#[test]
fn preset_wire_self_borrow_cancels_while_pending() {
    exercise(Path::Preset, Outcome::Cancel);
}

#[test]
fn preset_wire_self_borrow_poll_unwinds() {
    exercise(Path::Preset, Outcome::Unwind);
}

#[test]
fn configured_wire_self_borrow_completes() {
    exercise(Path::Configured, Outcome::Complete);
}

#[test]
fn configured_wire_self_borrow_cancels_while_pending() {
    exercise(Path::Configured, Outcome::Cancel);
}

#[test]
fn configured_wire_self_borrow_poll_unwinds() {
    exercise(Path::Configured, Outcome::Unwind);
}
