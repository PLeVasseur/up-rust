/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

//! Compile-time boundary for the selected-wire unchecked receive lane.
//!
//! `.stderr` files are recorded on rustc 1.99.1, the current-compiler gate; re-record
//! with `TRYBUILD=overwrite` only on that toolchain. MSRV policy omits this target;
//! direct MSRV skips validate the compiler. Native constructor const assertions
//! are tested separately with code generation on both compiler roles.
#![cfg(all(feature = "zero-copy-transport", feature = "transport-implementer-api"))]

#[test]
fn selected_wire_unsafe_boundary() {
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
        eprintln!("SKIPPED current-only selected-wire diagnostics on MSRV 1.88.0");
        return;
    }
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/selected_wire_payload_pass/*.rs");
    cases.compile_fail("tests/ui/selected_wire_payload/*.rs");
}
