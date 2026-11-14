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

//! Stable native layouts, field-bit validation and write-once initialization.
//!
//! The contract itself is `const` data in `up-rust-core`
//! ([`up_rust_core::payload::stable`]). This module adds the heap-backed
//! `NativeRepresentation`, built from that data, and the deployment agreement
//! that selects identity.

use core::{marker::PhantomData, mem};

pub use up_rust_core::payload::stable::{
    descriptor_matches_layout, finish_stable_payload_init, prepare_stable_payload_storage,
    write_stable_payload_array, write_stable_payload_array_from,
    write_stable_payload_array_pattern, write_stable_payload_field, InitializedStablePayload,
    StableDescriptor, StableField, StableInitError, StablePayload, StablePayloadField,
    StablePayloadInit, StablePayloadInitSet, StablePayloadInitToken, StablePayloadInitUnset,
    StablePayloadPatternError,
};

use crate::{
    NativeByteOrder, NativeField, NativePayloadIdentity, NativeProfileAgreement,
    NativeRepresentation, NativeTypeToken, PayloadEncoding, UWireError,
};

/// Builds the heap-backed description of a field from its const descriptor.
fn representation_of(descriptor: &StableDescriptor) -> NativeRepresentation {
    let fields = descriptor
        .fields
        .iter()
        .map(|field| {
            NativeField::repeated(
                field.name,
                field.offset,
                field.count,
                representation_of(field.descriptor),
            )
        })
        .collect();
    NativeRepresentation::new(
        descriptor.name,
        descriptor.revision,
        descriptor.size,
        descriptor.alignment,
        NativeByteOrder::native(),
        fields,
    )
    .expect("a const stable descriptor is checked when it is written")
}

/// The heap-backed side of [`StablePayloadField`] (classic names, `up-rust` only).
///
/// In `up-rust::prelude`.
pub trait StablePayloadFieldExt: StablePayloadField {
    /// Describes the complete field representation, including nested structure.
    fn field_representation() -> NativeRepresentation {
        representation_of(Self::DESCRIPTOR)
    }
}

impl<T: StablePayloadField> StablePayloadFieldExt for T {}

/// The heap-backed side of [`StablePayload`] (classic names, `up-rust` only).
///
/// In `up-rust::prelude`.
pub trait StablePayloadExt: StablePayload {
    /// Returns this type's structural representation, independent of allocation.
    fn native_representation() -> NativeRepresentation {
        representation_of(Self::DESCRIPTOR)
    }

    /// Returns the structural token; this is never a payload encoding ID.
    fn native_type_token() -> NativeTypeToken {
        Self::native_representation().token()
    }
}

impl<T: StablePayload> StablePayloadExt for T {}

/// Stable-container payload codec marker.
#[derive(Debug)]
pub struct StableContainerPayload<T>(PhantomData<fn() -> T>);

impl<T> StableContainerPayload<T>
where
    T: StablePayload,
{
    /// Resolves this complete type under an explicit deployment agreement.
    ///
    /// # Errors
    ///
    /// Rejects a wrong descriptor or a type absent from the agreed profile.
    pub fn identity(
        agreement: &NativeProfileAgreement,
    ) -> Result<NativePayloadIdentity, UWireError> {
        agreement.resolve_type_identity::<T, UWireError>(Self::validate_representation)
    }

    /// Returns the profile's encoding allocation for this complete type.
    ///
    /// # Errors
    ///
    /// Returns the same representation/membership errors as [`Self::identity`].
    pub fn encoding(agreement: &NativeProfileAgreement) -> Result<PayloadEncoding, UWireError> {
        Self::identity(agreement).map(NativePayloadIdentity::encoding)
    }

    /// Checks a descriptor against the actual Rust layout and target.
    ///
    /// # Errors
    ///
    /// Rejects inconsistent type name, revision, size, alignment or byte order.
    pub fn validate_representation() -> Result<NativeRepresentation, UWireError> {
        let representation = T::native_representation();
        if representation.target() != &crate::NativeTarget::current()
            || representation.name() != T::TYPE_NAME
            || representation.revision() != T::REVISION
            || representation.size() != mem::size_of::<T>() as u64
            || representation.alignment() != mem::align_of::<T>() as u64
            || representation.byte_order() != NativeByteOrder::native()
        {
            return Err(UWireError::invalid_payload(
                "stable representation does not match its Rust type and target",
            ));
        }
        Ok(representation)
    }

    /// Whether `T`'s const descriptor matches its Rust layout.
    ///
    /// A proof of this kind used to need a per-type process-wide memo; a const
    /// descriptor makes it two integer comparisons, so no state exists.
    ///
    /// # Errors
    ///
    /// Returns the error of [`Self::validate_representation`].
    pub fn representation_proven() -> Result<(), UWireError> {
        if descriptor_matches_layout::<T>() {
            Ok(())
        } else {
            Err(UWireError::invalid_payload(
                "stable representation does not match its Rust type and target",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::MaybeUninit;

    #[repr(C)]
    #[derive(Debug, Eq, PartialEq, crate::StablePayload, crate::StablePayloadInit)]
    #[stable_payload(type_name = "uprotocol.test.Initialized")]
    struct Initialized {
        tag: u32,
        bytes: [u8; 4],
        valid: bool,
        padding: [u8; 3],
    }

    fn storage_for<T>(value: &mut MaybeUninit<T>) -> &mut [MaybeUninit<u8>] {
        // SAFETY: MaybeUninit<T> is writable storage of exactly size_of::<T>()
        // bytes and retains T's alignment.
        unsafe {
            core::slice::from_raw_parts_mut(
                core::ptr::from_mut(value).cast::<MaybeUninit<u8>>(),
                mem::size_of::<T>(),
            )
        }
    }

    #[repr(C)]
    #[derive(crate::StablePayload)]
    #[stable_payload(type_name = "uprotocol.test.SameLayout")]
    struct Unsigned {
        value: u32,
    }

    #[repr(C)]
    #[derive(crate::StablePayload)]
    #[stable_payload(type_name = "uprotocol.test.SameLayout")]
    struct Signed {
        value: i32,
    }

    #[repr(C)]
    #[derive(crate::StablePayload)]
    #[stable_payload(type_name = "uprotocol.test.SameLayout", revision = 1)]
    struct Revised {
        value: u32,
    }

    #[test_case::test_case(Signed::native_representation(); "same name size and alignment different field semantics")]
    #[test_case::test_case(Revised::native_representation(); "layout revision changes token")]
    fn derived_identity_includes_structure_and_revision(other: NativeRepresentation) {
        let baseline = Unsigned::native_representation();
        assert_eq!(baseline.size(), other.size());
        assert_eq!(baseline.alignment(), other.alignment());
        assert_ne!(baseline.token(), other.token());
    }

    #[test]
    fn constrained_fields_fail_closed() {
        assert!(bool::validate_field_bytes(&[0]));
        assert!(bool::validate_field_bytes(&[1]));
        assert!(!bool::validate_field_bytes(&[2]));
        assert!(char::validate_field_bytes(&('x' as u32).to_ne_bytes()));
        assert!(!char::validate_field_bytes(&0x0011_0000_u32.to_ne_bytes()));
        assert!(!char::validate_field_bytes(&0xD800_u32.to_ne_bytes()));
        assert!(!<[bool; 2]>::validate_field_bytes(&[0, 2]));
    }

    #[repr(C)]
    struct LiesAboutSize {
        value: u32,
    }

    // SAFETY: This test type is one fully initialized u32 without padding or
    // drop glue. Its intentionally wrong descriptor is used only to exercise
    // rejection by the representation proof, before any byte-to-reference cast.
    unsafe impl StablePayloadField for LiesAboutSize {
        const FIELD_BITS_ALWAYS_VALID: bool = true;
        const DESCRIPTOR: &'static StableDescriptor = &StableDescriptor {
            name: "example.LiesAboutSize",
            revision: 0,
            size: 8,
            alignment: 4,
            fields: &[],
        };
        fn validate_field_bytes(bytes: &[u8]) -> bool {
            bytes.len() == mem::size_of::<Self>()
        }
    }

    // SAFETY: The fixed memory layout above is valid; this negative fixture
    // checks that its size claim is rejected without exposing a typed borrow.
    unsafe impl StablePayload for LiesAboutSize {
        const TYPE_NAME: &'static str = "example.LiesAboutSize";
    }

    #[test]
    fn a_descriptor_that_lies_about_size_is_not_a_local_representation() {
        assert!(StableContainerPayload::<LiesAboutSize>::representation_proven().is_err());
    }

    #[test]
    fn derive_initializer_writes_each_field_without_whole_array_values() {
        let mut storage = MaybeUninit::<Initialized>::uninit();
        let initialized = Initialized::init(storage_for(&mut storage))
            .unwrap()
            .tag(7)
            .bytes_from_slice(b"test")
            .unwrap()
            .valid(true)
            .padding_fill(0)
            .finish();

        assert_eq!(initialized.tag, 7);
        assert_eq!(initialized.bytes, *b"test");
        assert!(initialized.valid);
        assert!(Initialized::validate_field_bytes(initialized.as_bytes()));
    }

    #[repr(C)]
    #[derive(Debug, crate::StablePayload, crate::StablePayloadInit)]
    #[stable_payload(type_name = "uprotocol.test.RepeatedStableArray")]
    struct Repeated {
        values: [u16; 7],
        trailer: [u8; 2],
    }

    #[test_case::test_case(&[3, 8, 13], [3, 8, 13, 3, 8, 13, 3]; "partial final pattern")]
    #[test_case::test_case(&[9], [9; 7]; "single repeated value")]
    #[test_case::test_case(&[1, 2, 3, 4, 5, 6, 7, 8], [1, 2, 3, 4, 5, 6, 7]; "pattern longer than field")]
    fn repeated_pattern_initializes_exact_field_extent(pattern: &[u16], expected: [u16; 7]) {
        let mut storage = MaybeUninit::<Repeated>::uninit();
        let value = Repeated::init(storage_for(&mut storage))
            .unwrap()
            .trailer_from_slice(&[21, 34])
            .unwrap()
            .values_fill_pattern(pattern)
            .unwrap()
            .finish();
        assert_eq!(value.values, expected);
        assert_eq!(value.trailer, [21, 34]);
        assert!(Repeated::validate_field_bytes(value.as_bytes()));
    }

    #[test]
    fn empty_pattern_is_rejected_but_empty_array_is_initialized() {
        let mut storage = MaybeUninit::<Repeated>::uninit();
        assert!(matches!(
            Repeated::init(storage_for(&mut storage))
                .unwrap()
                .values_fill_pattern(&[]),
            Err(StablePayloadPatternError::EmptyPattern)
        ));
        #[repr(C)]
        #[derive(Debug, crate::StablePayload, crate::StablePayloadInit)]
        #[stable_payload(type_name = "uprotocol.test.EmptyRepeatedStableArray")]
        struct Empty {
            values: [u32; 0],
            tag: u32,
        }
        let mut storage = MaybeUninit::<Empty>::uninit();
        let value = Empty::init(storage_for(&mut storage))
            .unwrap()
            .values_fill_pattern(&[])
            .unwrap()
            .tag(42)
            .finish();
        assert!(value.values.is_empty());
        assert_eq!(value.tag, 42);
    }

    #[test]
    fn pattern_duplicates_stable_representation_without_clone() {
        #[repr(C)]
        #[derive(Debug, crate::StablePayload)]
        #[stable_payload(type_name = "uprotocol.test.StablePatternElement")]
        struct Element {
            value: u32,
        }
        impl Clone for Element {
            fn clone(&self) -> Self {
                panic!("pattern initialization does not call Clone");
            }
        }
        #[repr(C)]
        #[derive(Debug, crate::StablePayload, crate::StablePayloadInit)]
        #[stable_payload(type_name = "uprotocol.test.StablePatternElements")]
        struct Elements {
            elements: [Element; 5],
        }
        let mut storage = MaybeUninit::<Elements>::uninit();
        let value = Elements::init(storage_for(&mut storage))
            .unwrap()
            .elements_fill_pattern(&[Element { value: 7 }, Element { value: 9 }])
            .unwrap()
            .finish();
        assert_eq!(
            value.elements.each_ref().map(|element| element.value),
            [7, 9, 7, 9, 7]
        );
    }
}
