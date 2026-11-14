// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
use up_rust::ListenerAdmission;

fn requires_send<T: Send>(_: T) {}

fn main() {
    let admission = ListenerAdmission::new();
    let guard = admission.enter_native_callback().unwrap();
    requires_send(guard);
}
