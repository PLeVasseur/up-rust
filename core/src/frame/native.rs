/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

//! The native type token carried in metadata.
//!
//! Everything else about native identity (the heap-backed `NativeRepresentation`,
//! `NativeProfile` and `NativeProfileAgreement`, and the build-time agreement
//! table) stays in `up-rust` for now: it is one designed change in M3.

/// An opaque structural native type token, independent of payload encoding IDs.
///
/// All 32-bit values, including zero, can be carried. Equality alone does not
/// authorize typed access: full representation agreement, layout, bit validity
/// and backing lifetime must also be established.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct NativeTypeToken(u32);

impl NativeTypeToken {
    /// Preserves a received token without interpreting its payload.
    #[must_use]
    pub const fn from_u32(value: u32) -> Self {
        Self(value)
    }

    /// Returns the token's complete 32-bit value.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}
