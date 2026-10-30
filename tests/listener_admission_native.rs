// SPDX-License-Identifier: Apache-2.0
//! Native callback admission from any callback thread, without allocation.
//!
//! A middleware may run its synchronous callback on a thread that drives an
//! async runtime, and its first callback on a thread arrives after setup.
//! Allocation counting is thread-local: only the measuring thread is charged.
#![cfg(feature = "transport-implementer-api")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use up_rust::ListenerAdmission;

struct CountingAllocator;

thread_local! {
    static WINDOW_OPEN: Cell<bool> = const { Cell::new(false) };
    static WINDOW_ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn record_allocation() {
    let _ = WINDOW_OPEN.try_with(|open| {
        if open.get() {
            let _ = WINDOW_ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}

// SAFETY: every operation is forwarded unchanged to the system allocator. The
// bookkeeping touches only `const`-initialized thread-locals through
// `try_with`, which neither allocates nor panics during thread teardown.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        // SAFETY: The caller's valid layout is forwarded unchanged to System.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The pointer and layout came from this allocator, i.e. System.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        // SAFETY: The caller's valid layout is forwarded unchanged to System.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_allocation();
        // SAFETY: The caller's live allocation, original layout and requested
        // nonzero size are forwarded to the allocator that owns the pointer.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Runs `operation` and returns the allocations it charged to this thread.
fn allocations(operation: impl FnOnce()) -> usize {
    WINDOW_ALLOCATIONS.with(|count| count.set(0));
    WINDOW_OPEN.with(|open| open.set(true));
    operation();
    WINDOW_OPEN.with(|open| open.set(false));
    WINDOW_ALLOCATIONS.with(Cell::get)
}

#[test]
fn native_admission_on_a_thread_driving_a_current_thread_runtime() {
    let admission = ListenerAdmission::new();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        // A synchronous middleware callback invoked while this thread drives the runtime.
        let guard = admission.enter_native_callback().unwrap();
        assert!(admission.is_current_callback());
        drop(guard);
    });
    assert_eq!(admission.entered_count(), 0);
}

#[test]
fn native_admission_on_a_multi_thread_runtime_worker() {
    let admission = Arc::new(ListenerAdmission::new());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .build()
        .unwrap();
    let worker = Arc::clone(&admission);
    let admitted = runtime.block_on(async move {
        tokio::spawn(async move {
            // A synchronous middleware callback running on its runtime's worker.
            worker.enter_native_callback().map(drop).is_some()
        })
        .await
        .unwrap()
    });
    assert!(admitted);
    assert_eq!(admission.entered_count(), 0);
    admission.cancel();
    assert!(admission.enter_native_callback().is_none());
}

#[test]
fn first_repeated_and_nested_native_admission_do_not_allocate() {
    // Eight live guards per thread are within the documented allocation-free depth.
    let admissions: Arc<Vec<ListenerAdmission>> =
        Arc::new((0..8).map(|_| ListenerAdmission::new()).collect());
    let observed = std::thread::spawn({
        let admissions = Arc::clone(&admissions);
        move || {
            let first_admission = admissions.first().unwrap();
            let first = allocations(|| drop(first_admission.enter_native_callback().unwrap()));
            let repeated = allocations(|| drop(first_admission.enter_native_callback().unwrap()));
            // The guard vector is reserved outside the measurement window.
            let mut guards = Vec::with_capacity(admissions.len());
            let nested = allocations(|| {
                guards.extend(
                    admissions
                        .iter()
                        .map(|admission| admission.enter_native_callback().unwrap()),
                );
                assert!(admissions
                    .iter()
                    .all(ListenerAdmission::is_current_callback));
                guards.clear();
            });
            first_admission.cancel();
            let rejected =
                allocations(|| assert!(first_admission.enter_native_callback().is_none()));
            (first, repeated, nested, rejected)
        }
    })
    .join()
    .unwrap();
    assert_eq!(
        observed,
        (0, 0, 0, 0),
        "(first, repeated, nested, rejected)"
    );
}
