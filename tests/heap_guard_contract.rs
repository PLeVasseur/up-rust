// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Contract of `up_rust::heap_guard`.
//!
//! Harness-free so the all-thread windows here see a quiet process. Every check
//! is a negative or exact control: a guard that misses an operation or charges
//! the wrong one must fail this binary. Bindings copy the window and report calls
//! from here; the allocator is the only line a test binary must add.

use std::{
    alloc::System,
    backtrace::BacktraceStatus,
    hint::black_box,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
};
use up_rust::heap_guard::{
    assert_no_heap, observe, HeapCounts, HeapGuard, HeapOp, HeapReport, HeapWindow, Scope,
    WindowToken,
};

#[global_allocator]
static HEAP: HeapGuard = HeapGuard::new(System);

fn wait_for(flag: &AtomicBool) {
    while !flag.load(Ordering::Acquire) {
        std::hint::spin_loop();
    }
}

fn allocation_and_release_are_counted_separately() {
    let ((), report) = observe(Scope::Enrolled, || drop(black_box(Box::new([0_u8; 32]))));
    assert_eq!(
        report.counts(),
        HeapCounts {
            allocations: 1,
            allocated_bytes: 32,
            deallocations: 1,
            deallocated_bytes: 32,
            reallocations: 0,
            reallocated_bytes: 0,
        }
    );
    assert_eq!(report.events().len(), 2);
    assert_eq!(report.events().first().unwrap().op, HeapOp::Allocate);
    assert_eq!(report.events().last().unwrap().op, HeapOp::Deallocate);
}

fn reallocation_is_its_own_operation() {
    let mut vector = Vec::<u8>::with_capacity(8);
    let ((), report) = observe(Scope::Enrolled, || {
        vector.reserve_exact(64);
        black_box(&mut vector);
    });
    let counts = report.counts();
    assert_eq!(counts.reallocations, 1, "{report}");
    assert_eq!(
        counts.allocations, 0,
        "a reallocation is not an allocation: {report}"
    );
    assert_eq!(counts.deallocations, 0, "{report}");
    assert!(matches!(
        report.events().first().unwrap().op,
        HeapOp::Reallocate { old_size: 8 }
    ));
    assert!(!report.is_clean());
    drop(vector);
}

fn zeroed_allocation_is_an_allocation() {
    let ((), report) = observe(Scope::Enrolled, || drop(black_box(vec![0_u8; 32])));
    let counts = report.counts();
    assert_eq!(
        (counts.allocations, counts.deallocations),
        (1, 1),
        "{report}"
    );
    assert_eq!(report.events().first().unwrap().op, HeapOp::AllocateZeroed);
}

fn a_clean_window_is_clean() {
    let (value, report) = observe(Scope::Enrolled, || black_box(1_u32) + 1);
    assert_eq!(value, 2);
    assert!(report.is_clean());
    assert!(report.events().is_empty());
    assert_eq!(report.counts(), HeapCounts::default());
    assert_eq!(assert_no_heap("clean", Scope::Enrolled, || 7_u8), 7);
}

fn a_dirty_window_fails_the_contract_and_says_why() {
    let error = catch_unwind(AssertUnwindSafe(|| {
        assert_no_heap("deliberate heap use", Scope::Enrolled, || {
            let mut vector = Vec::<u8>::with_capacity(4);
            vector.reserve_exact(32);
            drop(black_box(vector));
        });
    }))
    .expect_err("heap use must fail the contract");
    let message = error.downcast_ref::<String>().expect("formatted message");
    assert!(
        message.contains("deliberate heap use"),
        "subject named: {message}"
    );
    for operation in ["allocations", "deallocations", "reallocations"] {
        assert!(
            message.contains(operation),
            "report names {operation}: {message}"
        );
    }
}

fn windows_do_not_charge_threads_outside_their_scope() {
    let start = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let worker = thread::spawn({
        let (start, finished) = (Arc::clone(&start), Arc::clone(&finished));
        move || {
            wait_for(&start);
            drop(black_box(Box::new([1_u8; 16])));
            finished.store(true, Ordering::Release);
        }
    });
    let ((), report) = observe(Scope::Enrolled, || {
        start.store(true, Ordering::Release);
        wait_for(&finished);
    });
    worker.join().unwrap();
    assert!(
        report.is_clean(),
        "an unenrolled thread was charged: {report}"
    );
}

fn an_enrolled_thread_is_charged_and_leaves_with_its_guard() {
    // The mutex is locked once before any window so that a lazily allocated
    // mutex on pthread-based targets cannot be charged to the window below.
    let token_slot = Arc::new(Mutex::new(None::<WindowToken>));
    drop(token_slot.lock().unwrap());
    let go = Arc::new(AtomicBool::new(false));
    let after_enrolment = Arc::new(AtomicBool::new(false));
    let leave = Arc::new(AtomicBool::new(false));
    let left = Arc::new(AtomicBool::new(false));
    let worker = thread::spawn({
        let (token_slot, go) = (Arc::clone(&token_slot), Arc::clone(&go));
        let (after_enrolment, leave, left) = (
            Arc::clone(&after_enrolment),
            Arc::clone(&leave),
            Arc::clone(&left),
        );
        move || {
            wait_for(&go);
            let token = token_slot.lock().unwrap().unwrap();
            let enrolment = token.enrol_this_thread();
            drop(black_box(Box::new([2_u8; 24])));
            after_enrolment.store(true, Ordering::Release);
            wait_for(&leave);
            drop(enrolment);
            // Not enrolled any more: this must not be charged.
            drop(black_box(Box::new([3_u8; 40])));
            left.store(true, Ordering::Release);
        }
    });
    let window = HeapWindow::open(Scope::Enrolled);
    *token_slot.lock().unwrap() = Some(window.token());
    go.store(true, Ordering::Release);
    wait_for(&after_enrolment);
    leave.store(true, Ordering::Release);
    wait_for(&left);
    let report = window.finish();
    worker.join().unwrap();
    let counts = report.counts();
    assert_eq!(
        (counts.allocations, counts.deallocations),
        (1, 1),
        "{report}"
    );
    assert!(
        report.events().iter().all(|event| !event.on_opening_thread),
        "{report}"
    );
}

fn an_all_thread_window_sees_every_thread() {
    let ready = Arc::new(AtomicBool::new(false));
    let start = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let worker = thread::spawn({
        let (ready, start, finished) = (
            Arc::clone(&ready),
            Arc::clone(&start),
            Arc::clone(&finished),
        );
        move || {
            ready.store(true, Ordering::Release);
            wait_for(&start);
            drop(black_box(Box::new([4_u8; 16])));
            finished.store(true, Ordering::Release);
        }
    });
    // Thread start-up frees happen before the first line of the closure above.
    wait_for(&ready);
    let window = HeapWindow::open(Scope::AllThreads);
    start.store(true, Ordering::Release);
    wait_for(&finished);
    let report = window.finish();
    worker.join().unwrap();
    let counts = report.counts();
    assert_eq!(
        (counts.allocations, counts.deallocations),
        (1, 1),
        "{report}"
    );
    assert!(!report.events().first().unwrap().on_opening_thread);
}

#[inline(never)]
fn culprit_for_the_heap_guard_contract() {
    drop(black_box(Box::new(1_u64)));
}

fn a_backtrace_names_the_culprit() {
    let ((), report) = observe(Scope::Enrolled, culprit_for_the_heap_guard_contract);
    let first = report.events().first().expect("recorded event");
    let backtrace = first.backtrace.as_ref().expect("captured backtrace");
    assert_eq!(backtrace.status(), BacktraceStatus::Captured);
    let text = backtrace.to_string();
    assert!(
        text.contains("culprit_for_the_heap_guard_contract"),
        "backtrace must name the culprit:\n{text}"
    );
    let culprit = first.culprit(1);
    assert!(
        culprit
            .first()
            .is_some_and(|frame| frame.contains("culprit_for_the_heap_guard_contract")),
        "the culprit frame is the nearest non-library frame: {culprit:?}"
    );
    let rendered = report.to_string();
    assert!(rendered.contains("allocation of 8 bytes"), "{rendered}");
    assert!(
        rendered.contains("culprit_for_the_heap_guard_contract"),
        "{rendered}"
    );
}

fn counts_only_windows_capture_no_backtraces() {
    let window = HeapWindow::open_counts_only(Scope::Enrolled);
    culprit_for_the_heap_guard_contract();
    let report = window.finish();
    assert_eq!(report.counts().operations(), 2);
    assert!(report
        .events()
        .iter()
        .all(|event| event.backtrace.is_none()));
}

fn events_beyond_the_limit_are_counted_not_recorded() {
    let mut held = Vec::with_capacity(32);
    let limit = HeapReport::EVENT_LIMIT;
    let total = limit + 5;
    let window = HeapWindow::open_counts_only(Scope::Enrolled);
    for _ in 0..total {
        held.push(black_box(Box::new(0_u8)));
    }
    let report = window.finish();
    assert_eq!(report.counts().allocations, total);
    assert_eq!(report.events().len(), limit);
    assert_eq!(report.unrecorded_events(), total - limit);
    drop(held);
}

fn a_panic_inside_a_window_closes_it() {
    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let _window = HeapWindow::open(Scope::Enrolled);
        panic!("deliberate");
    }));
    assert!(unwound.is_err());
    // The thread is free to open another window afterwards.
    let ((), report) = observe(Scope::Enrolled, || {});
    assert!(report.is_clean());
}

fn windows_cannot_nest_on_one_thread() {
    let outer = HeapWindow::open(Scope::Enrolled);
    let nested = catch_unwind(AssertUnwindSafe(|| HeapWindow::open(Scope::Enrolled)));
    assert!(nested.is_err(), "nested windows are refused");
    drop(outer);
}

fn only_one_all_thread_window_can_be_open() {
    let window = HeapWindow::open(Scope::AllThreads);
    let second = catch_unwind(AssertUnwindSafe(|| HeapWindow::open(Scope::AllThreads)));
    assert!(second.is_err(), "concurrent all-thread windows are refused");
    // The refused open panicked inside this all-thread window, and a panic
    // allocates, so this report is not expected to be clean.
    drop(window.finish());
    // The refused attempt released its slot and the closed window released its own.
    assert!(HeapWindow::open(Scope::AllThreads).finish().is_clean());
}

fn a_token_for_a_closed_window_is_refused() {
    let window = HeapWindow::open(Scope::Enrolled);
    let token = window.token();
    drop(window);
    let stale = catch_unwind(AssertUnwindSafe(|| drop(token.enrol_this_thread())));
    assert!(stale.is_err());
}

/// Each window sees exactly its own thread's operations while others run.
fn concurrent_enrolled_windows_are_independent() {
    const THREADS: usize = 8;
    const ROUNDS: usize = 100;
    let start = Arc::new(AtomicBool::new(false));
    let completed = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (1..=THREADS)
        .map(|allocations| {
            let (start, completed) = (Arc::clone(&start), Arc::clone(&completed));
            thread::spawn(move || {
                wait_for(&start);
                for _ in 0..ROUNDS {
                    let ((), report) = observe(Scope::Enrolled, || {
                        for _ in 0..allocations {
                            drop(black_box(Box::new([5_u8; 8])));
                        }
                    });
                    assert_eq!(report.counts().allocations, allocations, "{report}");
                    assert_eq!(report.counts().deallocations, allocations, "{report}");
                }
                completed.fetch_add(1, Ordering::Release);
            })
        })
        .collect();
    start.store(true, Ordering::Release);
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(completed.load(Ordering::Acquire), THREADS);
}

/// Hammers window open and close while another thread allocates constantly. The
/// worker only ever allocates 16 byte blocks, so totals from a reused slot or a
/// torn event would break the byte/operation ratio or trip the sanitizing asserts.
fn windows_survive_concurrent_allocation_and_slot_reuse() {
    let stop = Arc::new(AtomicBool::new(false));
    let churning = Arc::new(AtomicBool::new(false));
    let worker = thread::spawn({
        let (stop, churning) = (Arc::clone(&stop), Arc::clone(&churning));
        move || {
            while !stop.load(Ordering::Acquire) {
                drop(black_box(Box::new([6_u8; 16])));
                churning.store(true, Ordering::Release);
            }
        }
    });
    // Thread start-up frees are not the worker's churn; wait until they are over.
    wait_for(&churning);
    for _ in 0..2000 {
        let window = HeapWindow::open_counts_only(Scope::AllThreads);
        std::hint::black_box(&window);
        let report = window.finish();
        let counts = report.counts();
        assert_eq!(counts.allocated_bytes, 16 * counts.allocations, "{report}");
        assert_eq!(
            counts.deallocated_bytes,
            16 * counts.deallocations,
            "{report}"
        );
        assert_eq!(counts.reallocations, 0, "{report}");
    }
    stop.store(true, Ordering::Release);
    worker.join().unwrap();
}

fn run(name: &str, check: fn(), failures: &mut usize) {
    match catch_unwind(check) {
        Ok(()) => println!("ok   {name}"),
        Err(_) => {
            println!("FAIL {name}");
            *failures += 1;
        }
    }
}

fn main() {
    let mut failures = 0;
    let checks: &[(&str, fn())] = &[
        (
            "allocation_and_release_are_counted_separately",
            allocation_and_release_are_counted_separately,
        ),
        (
            "reallocation_is_its_own_operation",
            reallocation_is_its_own_operation,
        ),
        (
            "zeroed_allocation_is_an_allocation",
            zeroed_allocation_is_an_allocation,
        ),
        ("a_clean_window_is_clean", a_clean_window_is_clean),
        (
            "a_dirty_window_fails_the_contract_and_says_why",
            a_dirty_window_fails_the_contract_and_says_why,
        ),
        (
            "windows_do_not_charge_threads_outside_their_scope",
            windows_do_not_charge_threads_outside_their_scope,
        ),
        (
            "an_enrolled_thread_is_charged_and_leaves_with_its_guard",
            an_enrolled_thread_is_charged_and_leaves_with_its_guard,
        ),
        (
            "an_all_thread_window_sees_every_thread",
            an_all_thread_window_sees_every_thread,
        ),
        (
            "a_backtrace_names_the_culprit",
            a_backtrace_names_the_culprit,
        ),
        (
            "counts_only_windows_capture_no_backtraces",
            counts_only_windows_capture_no_backtraces,
        ),
        (
            "events_beyond_the_limit_are_counted_not_recorded",
            events_beyond_the_limit_are_counted_not_recorded,
        ),
        (
            "a_panic_inside_a_window_closes_it",
            a_panic_inside_a_window_closes_it,
        ),
        (
            "windows_cannot_nest_on_one_thread",
            windows_cannot_nest_on_one_thread,
        ),
        (
            "only_one_all_thread_window_can_be_open",
            only_one_all_thread_window_can_be_open,
        ),
        (
            "a_token_for_a_closed_window_is_refused",
            a_token_for_a_closed_window_is_refused,
        ),
        (
            "concurrent_enrolled_windows_are_independent",
            concurrent_enrolled_windows_are_independent,
        ),
        (
            "windows_survive_concurrent_allocation_and_slot_reuse",
            windows_survive_concurrent_allocation_and_slot_reuse,
        ),
    ];
    for (name, check) in checks {
        run(name, *check, &mut failures);
    }
    println!("heap_guard_contract: {failures} failed of {}", checks.len());
    if failures != 0 {
        std::process::exit(1);
    }
}
