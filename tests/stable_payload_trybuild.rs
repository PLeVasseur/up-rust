/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

#[test]
fn stable_payload_layout_compile_tests() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/stable_payload_pass/*.rs");
    cases.compile_fail("tests/ui/stable_payload/*.rs");
}

#[cfg(all(
    feature = "owned-frame-transport",
    not(any(
        feature = "unsafe-stable-payload-tx",
        feature = "unsafe-stable-payload-init",
        feature = "unsafe-uninit-payload-bytes",
        feature = "expert-unsafe-payloads"
    ))
))]
#[test]
fn unsafe_feature_disabled_compile_tests() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/unsafe_feature_disabled/*.rs");
}
