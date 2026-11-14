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

//! Initialized and two-phase transmit storage with explicit size/alignment proofs.
//!
//! Uninitialized loans become sendable only after their initialization contract
//! is fulfilled. The supplying binding retains all necessary native owners.

use core::mem::MaybeUninit;

use crate::frame::metadata::UFrameMetadata;
use crate::zero_copy::loan::PayloadAlignment;
use crate::{UCode, UStatus};

/// Payload portion of a zero-copy transmit loan request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UTxPayloadSpec {
    /// The frame carries no payload and no payload encoding.
    Absent,
    /// The frame carries a payload, including a present empty payload.
    Present {
        /// Visible payload length.
        len: usize,
        /// Required payload alignment.
        alignment: PayloadAlignment,
    },
}

/// Validated semantic transmit-loan request.
#[derive(Clone, Debug, PartialEq)]
pub struct UTxLoanSpec {
    metadata: ValidatedFrameMetadata,
    payload: UTxPayloadSpec,
}

#[derive(Clone, Debug, PartialEq)]
struct ValidatedFrameMetadata(UFrameMetadata);

impl UTxLoanSpec {
    /// Validates metadata and payload presence for a transmit loan.
    ///
    /// # Errors
    ///
    /// Returns an invalid-argument status for inconsistent metadata or layout.
    pub fn new(metadata: UFrameMetadata, payload: UTxPayloadSpec) -> Result<Self, UStatus> {
        metadata
            .validate()
            .map_err(|error| UStatus::fail_with_code(UCode::InvalidArgument, error.to_string()))?;
        let has_encoding = metadata.payload_encoding().is_some();
        if has_encoding != matches!(payload, UTxPayloadSpec::Present { .. }) {
            return Err(UStatus::fail_with_code(
                UCode::InvalidArgument,
                "payload presence must match payload encoding metadata",
            ));
        }
        Ok(Self {
            metadata: ValidatedFrameMetadata(metadata),
            payload,
        })
    }

    /// Creates a request carrying payload bytes.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid alignment or inconsistent metadata.
    pub fn payload(
        metadata: UFrameMetadata,
        len: usize,
        alignment: usize,
    ) -> Result<Self, UStatus> {
        let alignment = PayloadAlignment::new(alignment).map_err(UStatus::from)?;
        Self::new(metadata, UTxPayloadSpec::Present { len, alignment })
    }

    /// Creates a no-payload request.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata declares a payload encoding.
    pub fn no_payload(metadata: UFrameMetadata) -> Result<Self, UStatus> {
        Self::new(metadata, UTxPayloadSpec::Absent)
    }

    /// Returns frame metadata.
    #[must_use]
    pub fn metadata(&self) -> &UFrameMetadata {
        &self.metadata.0
    }

    /// Returns visible payload length.
    #[must_use]
    pub fn payload_len(&self) -> usize {
        match self.payload {
            UTxPayloadSpec::Absent => 0,
            UTxPayloadSpec::Present { len, .. } => len,
        }
    }

    /// Returns validated payload alignment.
    #[must_use]
    pub fn payload_alignment_proof(&self) -> PayloadAlignment {
        match self.payload {
            UTxPayloadSpec::Absent => PayloadAlignment(1),
            UTxPayloadSpec::Present { alignment, .. } => alignment,
        }
    }
}

/// Initialized zero-copy transmit buffer.
///
/// The buffer retains its backing allocation and native allocator owner until
/// it is committed or dropped, independently of the handle which loaned it.
/// Dropping that handle must not invalidate payload access through this buffer.
pub trait UTxBuffer {
    /// Returns semantic metadata.
    fn metadata(&self) -> &UFrameMetadata;
    /// Returns initialized payload bytes.
    fn payload(&self) -> &[u8];
    /// Returns initialized payload bytes mutably.
    fn payload_mut(&mut self) -> &mut [u8];
}

/// Uninitialized zero-copy transmit buffer.
///
/// The loan retains its allocator owner independently of the originating
/// transport. Initializing it after that transport handle is dropped remains
/// valid; conversion must transfer the owner into the initialized buffer.
pub trait UUninitTxBuffer {
    /// Initialized buffer produced after all payload bytes are written.
    type Initialized: UTxBuffer;

    /// Returns semantic metadata.
    fn metadata(&self) -> &UFrameMetadata;
    /// Returns payload storage as `MaybeUninit` bytes.
    fn payload_uninit_mut(&mut self) -> &mut [MaybeUninit<u8>];

    /// Marks all payload bytes initialized.
    ///
    /// # Safety
    ///
    /// Every byte in `payload_uninit_mut()` must have been initialized.
    unsafe fn assume_payload_initialized(self) -> Self::Initialized;
}

#[derive(Debug)]
struct AlignedBytes {
    allocation: Vec<u8>,
    start: usize,
    len: usize,
    alignment: PayloadAlignment,
}

impl Clone for AlignedBytes {
    fn clone(&self) -> Self {
        // A cloned allocation has a new address: recompute its aligned offset.
        let mut cloned = Self::new(self.len, self.alignment.as_usize())
            .expect("aligned buffer clone allocation failed");
        cloned.as_mut_slice().copy_from_slice(self.as_slice());
        cloned
    }
}

impl AlignedBytes {
    fn new(len: usize, alignment: usize) -> Result<Self, UStatus> {
        let alignment = PayloadAlignment::new(alignment).map_err(UStatus::from)?;
        let extra = alignment.as_usize().saturating_sub(1);
        let capacity = len.checked_add(extra).ok_or_else(|| {
            UStatus::fail_with_code(UCode::ResourceExhausted, "TX payload allocation overflow")
        })?;
        let mut allocation = Vec::<u8>::new();
        allocation.try_reserve_exact(capacity).map_err(|error| {
            UStatus::fail_with_code(
                UCode::ResourceExhausted,
                format!("TX payload allocation failed: {error}"),
            )
        })?;
        // SAFETY: try_reserve_exact established capacity writable u8 slots. Zero
        // initializes every slot before its length is exposed, including the
        // alignment gap. A zero count uses Vec's valid aligned dangling pointer.
        unsafe {
            core::ptr::write_bytes(allocation.as_mut_ptr(), 0, capacity);
            allocation.set_len(capacity);
        }
        let address = allocation.as_ptr() as usize;
        // For power-of-two alignment, this offset is in 0..=extra and does not
        // add to (and potentially overflow) the integer address. The checked
        // capacity above therefore contains the complete aligned payload range.
        let start = address.wrapping_neg() & extra;
        Ok(Self {
            allocation,
            start,
            len,
            alignment,
        })
    }

    fn as_slice(&self) -> &[u8] {
        self.allocation
            .get(self.start..self.start + self.len)
            .expect("validated aligned allocation range")
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        self.allocation
            .get_mut(self.start..self.start + self.len)
            .expect("validated aligned allocation range")
    }
}

/// In-memory initialized transmit buffer used by adapters and tests.
#[derive(Clone, Debug)]
pub struct UVecTxBuffer {
    metadata: UFrameMetadata,
    payload: AlignedBytes,
}

impl UVecTxBuffer {
    /// Allocates zero-initialized payload storage with validated alignment.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid alignment, allocation-size overflow or
    /// allocation failure. Size/allocation failures report ResourceExhausted.
    pub fn with_alignment(
        metadata: UFrameMetadata,
        payload_len: usize,
        payload_alignment: usize,
    ) -> Result<Self, UStatus> {
        Ok(Self {
            metadata,
            payload: AlignedBytes::new(payload_len, payload_alignment)?,
        })
    }

    /// Returns semantic metadata.
    #[must_use]
    pub fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    /// Returns initialized payload bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        self.payload.as_slice()
    }
}

impl UTxBuffer for UVecTxBuffer {
    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload(&self) -> &[u8] {
        self.payload.as_slice()
    }

    fn payload_mut(&mut self) -> &mut [u8] {
        self.payload.as_mut_slice()
    }
}

/// In-memory two-phase transmit buffer.
#[derive(Debug)]
pub struct UVecUninitTxBuffer(UVecTxBuffer);

impl UVecUninitTxBuffer {
    /// Allocates aligned two-phase payload storage.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid alignment, allocation-size overflow or
    /// allocation failure. Size/allocation failures report ResourceExhausted.
    pub fn with_alignment(
        metadata: UFrameMetadata,
        payload_len: usize,
        payload_alignment: usize,
    ) -> Result<Self, UStatus> {
        UVecTxBuffer::with_alignment(metadata, payload_len, payload_alignment).map(Self)
    }
}

impl UUninitTxBuffer for UVecUninitTxBuffer {
    type Initialized = UVecTxBuffer;

    fn metadata(&self) -> &UFrameMetadata {
        self.0.metadata()
    }

    fn payload_uninit_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        let payload = self.0.payload_mut();
        // SAFETY: MaybeUninit<u8> has the same layout and permits all byte states.
        unsafe {
            core::slice::from_raw_parts_mut(
                payload.as_mut_ptr().cast::<MaybeUninit<u8>>(),
                payload.len(),
            )
        }
    }

    unsafe fn assume_payload_initialized(self) -> Self::Initialized {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test_case::test_case(usize::MAX, 1; "vector capacity overflow")]
    #[test_case::test_case(usize::MAX, 8; "alignment addition overflow")]
    fn impossible_allocation_sizes_return_resource_exhausted(length: usize, alignment: usize) {
        let metadata = UFrameMetadata::publish(
            crate::UUri::try_from_parts("vehicle", 0x0001_4210, 1, 0x9000).unwrap(),
        )
        .with_payload_encoding(crate::PayloadEncoding::RAW)
        .build()
        .unwrap();
        assert_eq!(
            UVecTxBuffer::with_alignment(metadata.clone(), length, alignment)
                .unwrap_err()
                .code(),
            UCode::ResourceExhausted
        );
        assert_eq!(
            UVecUninitTxBuffer::with_alignment(metadata, length, alignment)
                .unwrap_err()
                .code(),
            UCode::ResourceExhausted
        );
    }
}
