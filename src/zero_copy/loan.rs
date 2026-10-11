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

/// Provenance class for one contiguous receive loan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadLoanProvenance {
    /// Storage is owned by a public receive lease.
    OwnedReceiveLease,
    /// Storage is loaned by an opaque physical transport.
    OpaqueTransportLoan,
}

/// Contiguous payload bytes carrying explicit loan provenance.
#[derive(Clone, Copy, Debug)]
pub struct LoanedPayload<'a> {
    bytes: &'a [u8],
    provenance: PayloadLoanProvenance,
}

impl<'a> LoanedPayload<'a> {
    /// Creates a loan after the caller has established its provenance.
    ///
    /// `provenance` is a statement of fact by the transport: `bytes` is the
    /// receive storage the transport actually loaned, not a copy, an
    /// allocation or a coalesced buffer. No memory-safety property depends on
    /// it (every typed borrow re-checks length, alignment and bit validity),
    /// so the constructor is safe; a wrong value misreports zero-copy evidence
    /// through [`provenance`](Self::provenance) and is a transport bug.
    #[must_use]
    pub const fn new(bytes: &'a [u8], provenance: PayloadLoanProvenance) -> Self {
        Self { bytes, provenance }
    }

    /// Returns the contiguous payload bytes.
    #[must_use]
    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Returns the loan provenance.
    #[must_use]
    pub const fn provenance(self) -> PayloadLoanProvenance {
        self.provenance
    }
}
