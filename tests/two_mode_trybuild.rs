/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

//! Both directions of the mode boundary do not compile.
//!
//! `.stderr` files are recorded per compiler role: `tests/ui/two_mode_merged` on the
//! current compiler and `tests/ui/two_mode_merged_msrv` on the MSRV, because their
//! diagnostics differ. Re-record with `TRYBUILD=overwrite` on the matching
//! toolchain. The `.rs` cases in the MSRV directory are copies; a test pins them.
#![cfg(all(feature = "test-util", feature = "zero-copy-transport"))]

use std::path::Path;

const CURRENT: &str = "tests/ui/two_mode_merged";
const MSRV: &str = "tests/ui/two_mode_merged_msrv";

fn rustc_version() -> String {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = std::process::Command::new(rustc)
        .arg("--version")
        .output()
        .expect("rustc --version");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn mode_boundaries_do_not_compile() {
    let cases = trybuild::TestCases::new();
    if rustc_version().starts_with("rustc 1.88.0") {
        cases.compile_fail(format!("{MSRV}/*.rs"));
    } else {
        cases.compile_fail(format!("{CURRENT}/*.rs"));
    }
}

/// The MSRV copies must not drift from the cases they mirror.
#[test]
fn msrv_cases_are_exact_copies() {
    let names = |dir: &str| -> Vec<std::ffi::OsString> {
        let mut names: Vec<_> = std::fs::read_dir(dir)
            .expect("case directory")
            .map(|entry| entry.expect("entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
            .map(|path| path.file_name().expect("file name").to_os_string())
            .collect();
        names.sort();
        names
    };
    let cases = names(CURRENT);
    assert_eq!(cases, names(MSRV), "the case lists must match");
    assert!(!cases.is_empty());
    for name in cases {
        let current = std::fs::read(Path::new(CURRENT).join(&name)).expect("case");
        let copy = std::fs::read(Path::new(MSRV).join(&name)).expect("copy");
        assert_eq!(current, copy, "{name:?} differs from its MSRV copy");
    }
}
