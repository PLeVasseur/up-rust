// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! A C-level window in a process without the observer must fail, not count nothing.

use std::alloc::System;

use up_rust::heap_guard::{c_level, HeapGuard};

#[global_allocator]
static HEAP: HeapGuard = HeapGuard::new(System);

#[test]
#[should_panic(expected = "the C-level allocation observer is not loaded")]
fn a_window_without_the_observer_is_refused() {
    let _window = c_level::Window::open();
}
