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

//! Checked borrowing from contiguous payload bytes.
//!
//! Proves representation, length, alignment and field validity before returning
//! a typed reference. Frame-level identity is checked separately at admission.

use core::{fmt, mem};

use super::stable::{descriptor_matches_layout, StablePayload};

/// Why a typed borrow was refused. Fixed size; carries no heap data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BorrowError {
    /// The descriptor does not match the Rust type.
    Representation,
    /// The payload is not exactly the expected size.
    Length {
        /// The payload type's size.
        expected: usize,
        /// The received size.
        actual: usize,
    },
    /// The payload address does not satisfy the type's alignment.
    Misaligned {
        /// Required alignment.
        required: usize,
    },
    /// At least one payload field has an invalid bit pattern.
    InvalidBits,
}

impl fmt::Display for BorrowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Representation => {
                f.write_str("stable representation does not match its Rust type and target")
            }
            Self::Length { expected, actual } => {
                write!(f, "payload length {actual} is not {expected}")
            }
            Self::Misaligned { required } => {
                write!(f, "payload address is not aligned to {required} bytes")
            }
            Self::InvalidBits => f.write_str("payload bytes are not valid for this type"),
        }
    }
}

impl core::error::Error for BorrowError {}

/// Checks shape only: representation, exact size and address alignment.
/// # Safety-usable invariant
/// Ok proves descriptor_matches_layout, exact size_of::<T>() extent and T
/// alignment, including ZST. The input slice already proves initialized bytes,
/// live same-allocation provenance/non-nullness and immutability for its borrow.
/// This does not prove T bit validity; StablePayload validation supplies it.
fn check_shape<T: StablePayload>(src: &[u8]) -> Result<(), BorrowError> {
    if !descriptor_matches_layout::<T>() {
        return Err(BorrowError::Representation);
    }
    let expected = mem::size_of::<T>();
    if src.len() != expected {
        return Err(BorrowError::Length {
            expected,
            actual: src.len(),
        });
    }
    let required = mem::align_of::<T>();
    if !(src.as_ptr() as usize).is_multiple_of(required) {
        return Err(BorrowError::Misaligned { required });
    }
    Ok(())
}

/// Safely borrows `T` from arbitrary bytes after complete validation.
///
/// # Errors
///
/// Returns [`BorrowError`] for an inconsistent descriptor, wrong length, wrong
/// alignment or an invalid bit pattern.
pub fn borrow_checked<T: StablePayload>(src: &[u8]) -> Result<&T, BorrowError> {
    check_shape::<T>(src)?;
    // Types whose every bit pattern is valid need no recursive walk.
    if !T::FIELD_BITS_ALWAYS_VALID && !T::validate_field_bytes(src) {
        return Err(BorrowError::InvalidBits);
    }
    // SAFETY: check_shape's postcondition proves extent/alignment. StablePayload
    // and its unsafe field contract prove valid bit patterns (unconditionally or
    // after this validator), no pointers/ownership/interior mutation/drop glue.
    // src provides immutable initialized bytes for the returned borrow; no
    // callback between validation and cast can mutate those safely borrowed bytes.
    Ok(unsafe { cast_validated(src) })
}

/// Borrows `T`, skipping the recursive field validation.
///
/// # Errors
///
/// Returns [`BorrowError`] for an inconsistent descriptor, wrong length or
/// wrong alignment.
///
/// # Safety
///
/// The caller must prove typed producer construction of this exact `T`,
/// matching native representation and byte order, and valid bits for every field.
pub unsafe fn borrow_trusted<T: StablePayload>(src: &[u8]) -> Result<&T, BorrowError> {
    check_shape::<T>(src)?;
    // SAFETY: borrow_trusted's caller proves this exact T's valid representation;
    // check_shape proves extent/alignment. StablePayload excludes interior
    // mutation and ownership-sensitive state, and src supplies the live immutable
    // allocation for exactly this returned borrow, satisfying cast_validated.
    Ok(unsafe { cast_validated(src) })
}

/// # Safety
/// Caller must prove src has exactly size_of::<T>() bytes, is aligned for T,
/// and those initialized bytes form one valid T. Creating/sharing this T must
/// not enable mutation through the immutable input bytes or assume uniqueness,
/// owned resources or inaccessible provenance. Its references, if any, must
/// remain valid for the returned src borrow. No move, drop, mutable access or
/// invalidation may occur during that borrow. The result references the same
/// allocation/extent and transfers no T ownership.
unsafe fn cast_validated<T>(src: &[u8]) -> &T {
    // SAFETY: the caller contract proves exact size/alignment/valid T and no
    // forbidden interior mutation or ownership claim. src supplies live initialized
    // one-allocation bytes, nonnull pointer (even empty), and the returned borrow's
    // lifetime/exclusion of writes. The raw cast preserves provenance/address.
    unsafe { &*src.as_ptr().cast::<T>() }
}
