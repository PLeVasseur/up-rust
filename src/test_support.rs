// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Private fixtures shared by SDK capability tests.

#[cfg(feature = "owned-frame-transport")]
pub(crate) fn topic(resource: u16) -> crate::UUri {
    crate::UUri::try_from_parts("vehicle", 0x4210, 1, resource).expect("fixture topic")
}
