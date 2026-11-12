// SPDX-License-Identifier: Apache-2.0
//! Native callback admission from any callback thread, without allocation.
//!
//! A middleware may run its synchronous callback on a thread that drives an
//! async runtime, and its first callback on a thread arrives after setup.
//! Heap counting covers allocation, release and reallocation, and charges only
//! the measuring thread.
#![cfg(feature = "transport-implementer-api")]

use std::sync::Arc;

use up_rust::heap_guard::{observe, HeapGuard, HeapReport, Scope};
use up_rust::ListenerAdmission;

#[global_allocator]
static ALLOCATOR: HeapGuard = HeapGuard::new(std::alloc::System);

/// Runs `operation` and returns the heap use it charged to this thread.
fn allocations(operation: impl FnOnce()) -> HeapReport {
    observe(Scope::Enrolled, operation).1
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
    let (first, repeated, nested, rejected) = observed;
    first.assert_no_heap("first native admission");
    repeated.assert_no_heap("repeated native admission");
    nested.assert_no_heap("nested native admission");
    rejected.assert_no_heap("rejected native admission");
}
