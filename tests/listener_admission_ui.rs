// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Current-compiler diagnostic proof of the native callback guard's thread bound.

#[test]
fn native_callback_guard_is_not_send() {
    if std::env::var_os("UP_RUST_SKIP_TRYBUILD").is_some() {
        assert_eq!(std::env::var("UP_RUST_SKIP_TRYBUILD").as_deref(), Ok("1"));
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let version = std::process::Command::new(rustc)
            .arg("--version")
            .output()
            .unwrap();
        assert!(version.status.success());
        assert!(
            String::from_utf8_lossy(&version.stdout).starts_with("rustc 1.88.0 "),
            "only the recorded MSRV may skip current-compiler diagnostics"
        );
        eprintln!("SKIPPED current-only native callback diagnostic on MSRV 1.88.0");
        return;
    }
    trybuild::TestCases::new().compile_fail("tests/ui/native_callback_guard_not_send.rs");
}
