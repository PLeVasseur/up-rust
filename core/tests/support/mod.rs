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

//! A per-thread counting allocator, so `up-rust-core`'s own tests can prove a
//! window touches no heap. `up-rust-core` has no dependency, not even on the
//! `heap_guard` in `up-rust`'s test utilities.

#![allow(missing_docs, dead_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static DEALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static REALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

pub struct CountingAllocator;

// SAFETY: every call forwards to `System` after updating per-thread counters.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        charge(&ALLOCATIONS);
        // SAFETY: forwarded unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        charge(&DEALLOCATIONS);
        // SAFETY: forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        charge(&ALLOCATIONS);
        // SAFETY: forwarded unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        charge(&REALLOCATIONS);
        // SAFETY: forwarded unchanged.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

fn charge(counter: &'static std::thread::LocalKey<Cell<u64>>) {
    let _ = COUNTING.try_with(|counting| {
        if counting.get() {
            let _ = counter.try_with(|count| count.set(count.get().wrapping_add(1)));
        }
    });
}

/// Counts heap operations on the current thread while `body` runs.
pub fn count_heap<R>(body: impl FnOnce() -> R) -> (R, [u64; 3]) {
    ALLOCATIONS.with(|c| c.set(0));
    DEALLOCATIONS.with(|c| c.set(0));
    REALLOCATIONS.with(|c| c.set(0));
    COUNTING.with(|c| c.set(true));
    let value = body();
    COUNTING.with(|c| c.set(false));
    let counts = [
        ALLOCATIONS.with(Cell::get),
        DEALLOCATIONS.with(Cell::get),
        REALLOCATIONS.with(Cell::get),
    ];
    (value, counts)
}

/// Asserts that `body` touches no heap on the current thread.
#[track_caller]
pub fn assert_no_heap<R>(label: &str, body: impl FnOnce() -> R) -> R {
    let (value, counts) = count_heap(body);
    assert_eq!(
        counts,
        [0, 0, 0],
        "{label}: heap touched [alloc, dealloc, realloc]"
    );
    value
}
