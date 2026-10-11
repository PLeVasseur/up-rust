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
//! The codec proves representation, length, alignment and field validity before
//! returning a typed reference. Frame-level identity is verified separately.

use core::mem;

use crate::payload::codec::{DecodePayload, PayloadCodec, PayloadIdentity};
use crate::payload::stable::{StableContainerPayload, StablePayload};
use crate::{NativeProfileAgreement, NativeProfileError, UWireError};

/// Typed borrowing contract for a payload codec.
///
/// This is the application-selected byte-level operation. Frame/lease helpers
/// must first use [`PayloadCodec::verify_metadata`] with their scoped context;
/// this byte-only method cannot establish the identity of an enclosing frame.
///
/// # Safety
///
/// Implementations must not return a reference until size, alignment and every
/// bit-validity requirement of `T` have been established. Identity (which
/// representation the bytes claim to be) is a separate, metadata-level check
/// performed through [`PayloadCodec::verify_metadata`].
pub unsafe trait BorrowPayload<T>: PayloadCodec {
    /// Safely borrows `T` from arbitrary payload bytes after complete validation.
    ///
    /// # Errors
    ///
    /// Returns an error for incompatible length, alignment or bit patterns.
    fn borrow_payload(src: &[u8]) -> Result<&T, UWireError>;

    /// Borrows `T` through the codec's unchecked lane.
    ///
    /// The default remains checked. A codec may override only with an explicit
    /// safety argument proving every omitted validation obligation.
    ///
    /// # Safety
    ///
    /// The caller must satisfy every additional invariant documented by an
    /// overriding implementation. The inherited implementation is always safe.
    unsafe fn borrow_payload_unchecked(src: &[u8]) -> Result<&T, UWireError> {
        Self::borrow_payload(src)
    }
}

impl<T> PayloadCodec for StableContainerPayload<T>
where
    T: StablePayload,
{
    fn codec_name() -> &'static str {
        T::TYPE_NAME
    }

    fn payload_identity(
        agreement: Option<&NativeProfileAgreement>,
    ) -> Result<PayloadIdentity, UWireError> {
        let agreement = agreement.ok_or(NativeProfileError::InvalidProfile(
            "native operation requires a scoped agreement",
        ))?;
        Self::identity(agreement).map(PayloadIdentity::Native)
    }
}

// SAFETY: This implementation validates exact layout, address alignment and all
// field bit patterns before its only byte-to-reference cast.
unsafe impl<T> BorrowPayload<T> for StableContainerPayload<T>
where
    T: StablePayload,
{
    fn borrow_payload(src: &[u8]) -> Result<&T, UWireError> {
        StableContainerPayload::<T>::representation_proven()?;
        if src.len() != mem::size_of::<T>() {
            return Err(UWireError::invalid_payload_length(
                mem::size_of::<T>(),
                src.len(),
            ));
        }
        if !(src.as_ptr() as usize).is_multiple_of(mem::align_of::<T>()) {
            return Err(UWireError::invalid_payload(format!(
                "payload address is not aligned to {} bytes",
                mem::align_of::<T>()
            )));
        }
        // Types whose every bit pattern is valid need no recursive walk; the
        // const makes that explicit instead of relying on the optimizer to
        // prove the derived validator trivially true.
        if !T::FIELD_BITS_ALWAYS_VALID && !T::validate_field_bytes(src) {
            return Err(UWireError::invalid_payload(format!(
                "payload bytes are not valid for {}",
                T::TYPE_NAME
            )));
        }
        // SAFETY: The representation proof, exact size and alignment were
        // checked above, and either every bit pattern of T is valid or the
        // stable validator proved every field's bit validity before this cast.
        Ok(unsafe { cast_validated_payload(src) })
    }

    /// Borrows without recursively validating field bits.
    ///
    /// # Safety
    ///
    /// The caller must prove typed producer construction of this exact `T`,
    /// matching native representation/endianness and valid bits for every field.
    /// Representation, exact size and alignment remain checked.
    unsafe fn borrow_payload_unchecked(src: &[u8]) -> Result<&T, UWireError> {
        StableContainerPayload::<T>::representation_proven()?;
        if src.len() != mem::size_of::<T>() {
            return Err(UWireError::invalid_payload_length(
                mem::size_of::<T>(),
                src.len(),
            ));
        }
        if !(src.as_ptr() as usize).is_multiple_of(mem::align_of::<T>()) {
            return Err(UWireError::invalid_payload(format!(
                "payload address is not aligned to {} bytes",
                mem::align_of::<T>()
            )));
        }
        // SAFETY: The caller proves exact type, target/endianness and recursive
        // bit validity. This override retains representation, size and alignment
        // checks and elides only the recursive field validator.
        Ok(unsafe { cast_validated_payload(src) })
    }
}

impl<'a, T> DecodePayload<'a, &'a T> for StableContainerPayload<T>
where
    T: StablePayload,
{
    fn decode_payload(src: &'a [u8]) -> Result<&'a T, UWireError> {
        Self::borrow_payload(src)
    }
}

unsafe fn cast_validated_payload<T>(src: &[u8]) -> &T {
    // SAFETY: The caller has proved exact size/alignment and T bit validity.
    unsafe { &*src.as_ptr().cast::<T>() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    #[derive(Debug, Eq, PartialEq, crate::StablePayload)]
    #[stable_payload(type_name = "uprotocol.test.Borrowed")]
    struct Borrowed {
        flag: bool,
        padding: [u8; 3],
        letter: char,
    }

    #[test]
    fn arbitrary_slice_borrow_checks_bits_and_alignment() {
        let value = Borrowed {
            flag: true,
            padding: [0; 3],
            letter: 'A',
        };
        // SAFETY: `Borrowed` is a repr(C) stable payload with explicit padding,
        // so its object representation is fully initialized; the slice covers
        // exactly `size_of::<Borrowed>()` bytes of a live value.
        let bytes = unsafe {
            core::slice::from_raw_parts(
                core::ptr::from_ref(&value).cast::<u8>(),
                mem::size_of::<Borrowed>(),
            )
        };
        assert_eq!(
            StableContainerPayload::<Borrowed>::borrow_payload(bytes).unwrap(),
            &value
        );

        #[repr(align(8))]
        struct Aligned([u8; mem::size_of::<Borrowed>() + 1]);
        let mut invalid = Aligned([0; mem::size_of::<Borrowed>() + 1]);
        let invalid_bytes = invalid.0.get_mut(..bytes.len()).unwrap();
        invalid_bytes.copy_from_slice(bytes);
        *invalid_bytes.first_mut().expect("non-empty stable payload") = 2;
        assert!(
            matches!(StableContainerPayload::<Borrowed>::borrow_payload(invalid_bytes),
            Err(UWireError::InvalidPayload(reason)) if reason.contains("not valid"))
        );
        assert!(matches!(StableContainerPayload::<Borrowed>::borrow_payload(
            bytes.get(..1).expect("one-byte prefix")
        ), Err(UWireError::InvalidPayload(reason)) if reason.contains("length")));

        let mut misaligned = Aligned([0; mem::size_of::<Borrowed>() + 1]);
        let shifted = misaligned.0.get_mut(1..).unwrap();
        shifted.copy_from_slice(bytes);
        assert!(!(shifted.as_ptr() as usize).is_multiple_of(mem::align_of::<Borrowed>()));
        assert!(
            matches!(StableContainerPayload::<Borrowed>::borrow_payload(shifted),
            Err(UWireError::InvalidPayload(reason)) if reason.contains("not aligned"))
        );
    }
}
