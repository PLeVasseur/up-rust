// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Contract of `up_rust::heap_guard::c_level`.
//!
//! Harness-free so the all-thread windows see a quiet process, and run only with the
//! observer preloaded (`test = false`, see the runner scripts). Without the observer
//! the first window panics, which is the contract. Each check is exact: an observer
//! or a window that misses an operation, or charges the wrong level, fails this binary.
//! Thread start-up and exit stay outside every window, because thread exit can free.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    ffi::c_void,
    hint::black_box,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Condvar, Mutex},
};

use up_rust::heap_guard::{c_level, HeapGuard};

#[global_allocator]
static HEAP: HeapGuard = HeapGuard::new(System);

extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(pointer: *mut c_void);
    fn realloc(pointer: *mut c_void, size: usize) -> *mut c_void;
    fn calloc(count: usize, size: usize) -> *mut c_void;
}
#[cfg(target_os = "macos")]
extern "C" {
    fn malloc_size(pointer: *const c_void) -> usize;
}
#[cfg(not(target_os = "macos"))]
extern "C" {
    fn malloc_usable_size(pointer: *mut c_void) -> usize;
}

fn usable_size(pointer: *mut c_void) -> u64 {
    // SAFETY: `pointer` is a live block from the C allocator.
    #[cfg(target_os = "macos")]
    let size = unsafe { malloc_size(pointer) };
    // SAFETY: `pointer` is a live block from the C allocator.
    #[cfg(not(target_os = "macos"))]
    let size = unsafe { malloc_usable_size(pointer) };
    size as u64
}

fn c_malloc(size: usize) -> *mut c_void {
    // SAFETY: plain malloc; the result is checked and released by the caller.
    let pointer = black_box(unsafe { malloc(size) });
    assert!(!pointer.is_null());
    pointer
}

fn message_of(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_owned()))
        .unwrap_or_default()
}

/// A window with no work reads exactly zero at both levels.
fn quiet_window_reads_zero() {
    let (value, report) = c_level::observe(|| black_box(2_u64).wrapping_mul(black_box(3)));
    assert_eq!(value, 6);
    assert_eq!(report.c(), c_level::CHeapCounts::default(), "{report}");
    assert!(report.is_clean(), "{report}");
}

/// A direct C malloc and free are visible only below the Rust guard.
fn direct_c_malloc_free_is_seen_below_the_rust_guard() {
    let mut usable = 0;
    let ((), report) = c_level::observe(|| {
        let pointer = c_malloc(64);
        usable = usable_size(pointer);
        // SAFETY: the block came from malloc above and is not used again.
        unsafe { free(pointer) };
    });
    let c = report.c();
    assert_eq!(c.allocation_calls, 1, "{report}");
    assert_eq!(c.malloc_calls, 1, "{report}");
    assert_eq!(c.requested_bytes, 64, "{report}");
    assert_eq!(c.release_calls, 1, "{report}");
    assert_eq!(c.free_calls, 1, "{report}");
    assert_eq!(c.released_usable_bytes, usable, "{report}");
    assert!(report.rust().is_clean(), "{report}");
    assert!(!report.is_clean(), "the C-level use must fail the window");
}

/// `std`'s direct `System` use is visible only below the guard.
fn rust_system_call_is_seen_below_the_rust_guard() {
    let layout = Layout::array::<u8>(32).unwrap();
    let mut usable = 0;
    let ((), report) = c_level::observe(|| {
        // SAFETY: non-zero layout, released below with the same layout.
        let pointer = unsafe { System.alloc(layout) };
        assert!(!pointer.is_null());
        usable = usable_size(pointer.cast());
        black_box(pointer);
        // SAFETY: `pointer` came from `System.alloc` with `layout`.
        unsafe { System.dealloc(pointer, layout) };
    });
    let c = report.c();
    assert_eq!(c.allocation_calls, 1, "{report}");
    assert_eq!(c.requested_bytes, 32, "{report}");
    assert_eq!(c.release_calls, 1, "{report}");
    assert_eq!(c.released_usable_bytes, usable, "{report}");
    assert!(report.rust().is_clean(), "{report}");
}

/// Global-allocator traffic is visible at both levels, with matching counts.
fn global_allocator_box_is_seen_by_both_levels() {
    let ((), report) = c_level::observe(|| drop(black_box(Box::new([0_u8; 24]))));
    let c = report.c();
    let rust = report.rust().counts();
    assert_eq!(rust.allocations, 1, "{report}");
    assert_eq!(rust.allocated_bytes, 24, "{report}");
    assert_eq!(rust.deallocations, 1, "{report}");
    assert_eq!(c.allocation_calls, 1, "{report}");
    assert_eq!(c.requested_bytes, 24, "{report}");
    assert_eq!(c.release_calls, 1, "{report}");
}

/// Realloc outcomes: a move and a zero-size call are releases; a call in place is not.
fn realloc_outcomes_are_accounted() {
    // Grow until the block moves, keeping the other block in the way.
    let pointer = c_malloc(64);
    let blocker = c_malloc(64);
    let old_usable = usable_size(pointer);
    let (moved, report) = c_level::observe(|| {
        // SAFETY: `pointer` is live and is replaced by the returned block.
        unsafe { realloc(pointer, 1 << 22) }
    });
    assert!(!moved.is_null());
    assert_ne!(moved, pointer, "the block must move for this control");
    let c = report.c();
    assert_eq!(c.realloc_calls, 1, "{report}");
    assert_eq!(c.allocation_calls, 1, "{report}");
    assert_eq!(c.release_calls, 1, "{report}");
    assert_eq!(c.realloc_move_calls, 1, "{report}");
    assert_eq!(c.released_usable_bytes, old_usable, "{report}");

    let moved_usable = usable_size(moved);
    let ((), report) = c_level::observe(|| {
        // SAFETY: `moved` is live and is not used afterwards; a returned block is released.
        let result = unsafe { realloc(moved, 0) };
        if !result.is_null() {
            // SAFETY: macOS returned a new minimal block.
            unsafe { free(result) };
        }
    });
    let c = report.c();
    assert_eq!(c.realloc_calls, 1, "{report}");
    assert_eq!(c.realloc_release_calls, 1, "{report}");
    // SAFETY: `blocker` is live.
    unsafe { free(blocker) };
    if cfg!(target_os = "macos") {
        assert_eq!(c.release_calls, 2, "{report}");
        assert_eq!(c.free_calls, 1, "{report}");
    } else {
        assert_eq!(c.release_calls, 1, "{report}");
        assert_eq!(c.free_calls, 0, "{report}");
        assert_eq!(c.released_usable_bytes, moved_usable, "{report}");
    }

    let pointer = c_malloc(48);
    let usable = usable_size(pointer) as usize;
    let (same, report) = c_level::observe(|| {
        // SAFETY: `pointer` is live, and the new size fits the existing block.
        unsafe { realloc(pointer, usable) }
    });
    assert_eq!(
        same, pointer,
        "the block must stay in place for this control"
    );
    let c = report.c();
    assert_eq!(c.realloc_calls, 1, "{report}");
    assert_eq!(c.release_calls, 0, "{report}");
    assert_eq!(c.realloc_move_calls, 0, "{report}");
    // SAFETY: `same` is live.
    unsafe { free(same) };

    // The zero-sized and zeroed entries.
    let ((), report) = c_level::observe(|| {
        // SAFETY: the pointer from calloc is released below.
        let pointer = unsafe { calloc(4, 8) };
        assert!(!pointer.is_null());
        // SAFETY: the block came from calloc above.
        unsafe { free(pointer) };
    });
    assert_eq!(report.c().calloc_calls, 1, "{report}");
    assert_eq!(report.c().release_calls, 1, "{report}");
}

/// `free(NULL)` releases nothing, is counted alone, and fails the window.
fn free_of_null_is_counted_and_fails_the_window() {
    let ((), report) = c_level::observe(|| {
        // SAFETY: free of null is defined to do nothing.
        unsafe { free(core::ptr::null_mut()) }
    });
    let c = report.c();
    assert_eq!(c.free_null_calls, 1, "{report}");
    assert_eq!(c.release_calls, 0, "{report}");
    assert!(
        !report.is_clean(),
        "free(NULL) must not read as a clean window"
    );
}

/// The combined assertion fails on heap use at the C level only.
fn assertion_fails_on_c_level_heap_use() {
    let panic = catch_unwind(AssertUnwindSafe(|| {
        c_level::assert_no_heap("control", || {
            // SAFETY: released immediately.
            unsafe { free(c_malloc(16)) }
        });
    }))
    .expect_err("a window with C-level heap use must fail");
    let message = message_of(panic);
    assert!(message.contains("control:"), "{message}");
    assert!(
        message.contains("C level: 1 allocation entries"),
        "{message}"
    );
    assert!(message.contains("no heap operations"), "{message}");
}

/// The combined assertion passes for a window with no heap use at either level.
fn assertion_passes_on_a_clean_window() {
    let value = c_level::assert_no_heap("clean", || black_box(5_u32) + 1);
    assert_eq!(value, 6);
}

/// Two windows are refused, as they are for the Rust guard's all-thread windows.
fn nested_window_is_refused() {
    let outer = c_level::Window::open_counts_only();
    // The panic's own allocations fall in this window, so its report is not meaningful.
    let refused = catch_unwind(AssertUnwindSafe(c_level::Window::open_counts_only));
    drop(outer);
    assert!(
        refused.is_err(),
        "a second all-thread window must be refused"
    );
}

/// On pthread targets `std`'s `Mutex` and `Condvar` box their native primitive on first
/// use, and free it on drop. The Linux futex implementation neither allocates nor frees.
// `drop` of a lock is the operation these windows measure, also where it has no `Drop`.
#[allow(clippy::drop_non_drop)]
fn std_lazy_locks_show_at_both_levels() {
    const EXPECT_LAZY: bool = cfg!(target_os = "macos");
    let mutex = Mutex::new(0_u32);
    let (guard, first_lock) = c_level::observe(|| mutex.lock().unwrap());
    drop(guard);
    let (guard, second_lock) = c_level::observe(|| mutex.lock().unwrap());
    drop(guard);
    let ((), mutex_drop) = c_level::observe(|| drop(mutex));

    assert_eq!(
        second_lock.c(),
        c_level::CHeapCounts::default(),
        "a second lock is quiet: {second_lock}"
    );
    assert!(second_lock.rust().is_clean(), "{second_lock}");

    let condvar = Condvar::new();
    let ((), first_notify) = c_level::observe(|| condvar.notify_one());
    let ((), second_notify) = c_level::observe(|| condvar.notify_one());
    let ((), condvar_drop) = c_level::observe(|| drop(condvar));

    assert_eq!(
        second_notify.c(),
        c_level::CHeapCounts::default(),
        "{second_notify}"
    );
    assert!(second_notify.rust().is_clean(), "{second_notify}");

    if EXPECT_LAZY {
        let c = first_lock.c();
        assert_eq!(c.allocation_calls, 1, "{first_lock}");
        assert_eq!(c.malloc_calls, 1, "{first_lock}");
        assert_eq!(c.requested_bytes, 64, "{first_lock}");
        assert_eq!(c.release_calls, 0, "{first_lock}");
        assert_eq!(first_lock.rust().counts().allocations, 1, "{first_lock}");
        let c = mutex_drop.c();
        assert_eq!(c.release_calls, 1, "{mutex_drop}");
        assert_eq!(c.free_calls, 1, "{mutex_drop}");
        assert_eq!(c.allocation_calls, 0, "{mutex_drop}");
        assert_eq!(mutex_drop.rust().counts().deallocations, 1, "{mutex_drop}");

        let c = first_notify.c();
        assert_eq!(c.allocation_calls, 1, "{first_notify}");
        assert_eq!(c.requested_bytes, 48, "{first_notify}");
        assert_eq!(condvar_drop.c().release_calls, 1, "{condvar_drop}");
    } else {
        for report in [&first_lock, &mutex_drop, &first_notify, &condvar_drop] {
            assert_eq!(report.c(), c_level::CHeapCounts::default(), "{report}");
            assert!(report.rust().is_clean(), "{report}");
        }
    }
    println!("mutex first lock: {first_lock}");
    println!("mutex drop: {mutex_drop}");
    println!("condvar first notify: {first_notify}");
}

fn assert_refused(expected: &str) -> ! {
    let refused = catch_unwind(AssertUnwindSafe(|| {
        let _window = c_level::Window::open();
    }))
    .expect_err("the window must be refused");
    let message = message_of(refused);
    assert!(message.contains(expected), "unexpected refusal: {message}");
    println!("PASS refused: {message}");
    std::process::exit(0);
}

const CHECKS: &[(&str, fn())] = &[
    ("quiet_window_reads_zero", quiet_window_reads_zero),
    (
        "direct_c_malloc_free_is_seen_below_the_rust_guard",
        direct_c_malloc_free_is_seen_below_the_rust_guard,
    ),
    (
        "rust_system_call_is_seen_below_the_rust_guard",
        rust_system_call_is_seen_below_the_rust_guard,
    ),
    (
        "global_allocator_box_is_seen_by_both_levels",
        global_allocator_box_is_seen_by_both_levels,
    ),
    (
        "realloc_outcomes_are_accounted",
        realloc_outcomes_are_accounted,
    ),
    (
        "free_of_null_is_counted_and_fails_the_window",
        free_of_null_is_counted_and_fails_the_window,
    ),
    (
        "assertion_fails_on_c_level_heap_use",
        assertion_fails_on_c_level_heap_use,
    ),
    (
        "assertion_passes_on_a_clean_window",
        assertion_passes_on_a_clean_window,
    ),
    ("nested_window_is_refused", nested_window_is_refused),
    (
        "std_lazy_locks_show_at_both_levels",
        std_lazy_locks_show_at_both_levels,
    ),
];

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--expect-refused-not-loaded") => {
            assert_refused("the C-level allocation observer is not loaded")
        }
        Some("--expect-refused-version") => assert_refused("is version 1"),
        Some("--expect-refused-unmapped") => assert_refused("has no counter file mapped"),
        Some("--expect-refused-saturated") => {
            // A saturated counter makes every later window refuse.
            // SAFETY: small allocation released at once.
            unsafe { free(c_malloc(8)) };
            // SAFETY: the size calculation overflows; calloc returns null and charges SIZE_MAX.
            let refused = unsafe { calloc(usize::MAX, 2) };
            assert!(refused.is_null());
            let refusal = catch_unwind(AssertUnwindSafe(|| {
                let _window = c_level::Window::open();
            }))
            .expect_err("a saturated observer must be refused");
            let message = message_of(refusal);
            assert!(
                message.contains("saturated"),
                "unexpected refusal: {message}"
            );
            println!("PASS refused: {message}");
        }
        Some(other) => panic!("unknown mode {other}"),
        None => {
            for (name, check) in CHECKS {
                check();
                println!("PASS {name}");
            }
            println!("all {} c_level checks passed", CHECKS.len());
        }
    }
}
