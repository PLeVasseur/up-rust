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
