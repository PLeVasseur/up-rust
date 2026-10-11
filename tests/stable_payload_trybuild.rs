/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

//! Compile-time contracts for stable payload initialization and decode limits.
//!
//! `.stderr` files are recorded on rustc 1.95.0, the current-compiler gate; re-record
//! with `TRYBUILD=overwrite` only on that toolchain. The MSRV policy omits this
//! current-only target. Direct MSRV skips validate the compiler and print their scope.
#![cfg(feature = "native-inventory")]

#[test]
fn stable_payload_compile_contracts() {
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
            "only MSRV may skip current-compiler diagnostics"
        );
        eprintln!("SKIPPED current-only stable-payload diagnostics on MSRV 1.88.0");
        return;
    }
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/stable_payload_pass/*.rs");
    cases.compile_fail("tests/ui/stable_payload/*.rs");
}
