// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! A window in a binary that did not install the guard must fail, not count nothing.

use up_rust::heap_guard::{HeapWindow, Scope};

#[test]
#[should_panic(expected = "no HeapGuard is installed")]
fn a_window_without_the_guard_is_refused() {
    let _window = HeapWindow::open(Scope::Enrolled);
}
