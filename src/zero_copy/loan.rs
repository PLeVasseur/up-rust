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

//! Contiguous loan bytes, checked alignment and descriptive storage provenance.
//!
//! A provenance label does not replace payload validation or backing ownership.

use crate::UWireError;

/// Validated power-of-two payload alignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PayloadAlignment(pub(crate) usize);

impl PayloadAlignment {
    /// Validates a payload alignment.
    ///
    /// # Errors
    ///
    /// Returns an error for zero or non-power-of-two values.
    pub fn new(alignment: usize) -> Result<Self, UWireError> {
        if alignment == 0 || !alignment.is_power_of_two() {
            return Err(UWireError::invalid_payload(format!(
                "payload alignment {alignment} must be a non-zero power of two"
            )));
        }
        Ok(Self(alignment))
    }

    /// Returns the alignment as `usize`.
    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

pub use up_rust_core::transport::{LoanedPayload, PayloadLoanProvenance};
