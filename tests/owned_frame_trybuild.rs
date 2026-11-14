// SPDX-License-Identifier: Apache-2.0
//! Compile-time contract: only validated owned frames reach a transport.
//!
//! `.stderr` files are recorded on rustc 1.99.1, the current-compiler gate; re-record
//! with `TRYBUILD=overwrite` only on that toolchain. MSRV policy omits this target;
//! direct MSRV skips validate the compiler and name the skipped diagnostics.
#![cfg(feature = "owned-frame-transport")]

#[test]
fn owned_frame_compile_contracts() {
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
        eprintln!("SKIPPED current-only owned-frame diagnostic on MSRV 1.88.0");
        return;
    }
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/owned_frame/*.rs");
}
