/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

#[test]
fn zero_copy_uninit_feature_surface() {
    let tests = trybuild::TestCases::new();
    tests.pass("tests/ui/zero_copy_uninit/implementer_present.rs");
    #[cfg(not(feature = "zero-copy-uninit"))]
    tests.compile_fail("tests/ui/zero_copy_uninit/user_absent.rs");
    #[cfg(feature = "zero-copy-uninit")]
    tests.pass("tests/ui/zero_copy_uninit/user_present.rs");
}
