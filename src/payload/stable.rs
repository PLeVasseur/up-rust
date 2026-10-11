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
//! Derives establish layout constraints; byte-level entry points check manual
//! descriptors before forming references. Deployment agreement selects identity.

use core::{
    any::TypeId,
    marker::PhantomData,
    mem,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
};
use std::collections::BTreeSet;
use std::sync::{PoisonError, RwLock};

use crate::{
    NativeByteOrder, NativeField, NativePayloadIdentity, NativeProfileAgreement,
    NativeRepresentation, NativeTypeToken, PayloadEncoding, UWireError,
};

/// Field-level bit-validity contract used by stable payload validation.
///
/// # Safety
///
/// Implementations must return `true` only when `bytes` has exactly the layout
/// of `Self` and every represented bit pattern is valid for `Self`.
/// Every valid value must have fully initialized bytes, with no implicit padding,
/// pointers, borrowed state, interior mutability or drop glue. Its structural
/// descriptor must be exact. In particular it must not contain UnsafeCell or
/// atomic/cell types that could mutate through a shared reference to borrowed bytes.
/// Valid representations must be safely duplicable as plain data; ownership-
/// sensitive handles or uniqueness-dependent state are not stable payload fields.
pub unsafe trait StablePayloadField: Sized {
    /// Whether every bit pattern of exactly `size_of::<Self>()` bytes is valid.
    const FIELD_BITS_ALWAYS_VALID: bool;

    /// Describes the complete field representation, including nested structure.
    fn field_representation() -> NativeRepresentation;

    /// Validates one field representation without constructing `Self`.
    fn validate_field_bytes(bytes: &[u8]) -> bool;
}

/// Stable in-memory payload contract for a deployment-agreed ABI domain.
///
/// # Safety
///
/// Implementors must have a fixed layout, fully initialized object
/// representation, and a validator that rejects every invalid bit pattern.
/// The field contract's prohibition on interior mutability also applies: shared
/// typed references must not permit writes through an immutable payload slice.
pub unsafe trait StablePayload: StablePayloadField + Sized + 'static {
    /// Cross-language stable type name.
    const TYPE_NAME: &'static str;
    /// Layout revision carried in the descriptor; a new revision is a new identity.
    const REVISION: u32 = 0;
    /// Returns this type's structural representation, independent of allocation.
    fn native_representation() -> NativeRepresentation {
        Self::field_representation()
    }

    /// Returns the structural token; this is never a payload encoding ID.
    fn native_type_token() -> NativeTypeToken {
        Self::native_representation().token()
    }
}

/// Typestate marker for an uninitialized stable field.
#[derive(Debug)]
pub struct StablePayloadInitUnset;

/// Typestate marker for an initialized stable field.
#[derive(Debug)]
pub struct StablePayloadInitSet;

/// Fixed-size failure from a generated stable array pattern initializer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum StablePayloadPatternError {
    /// A nonempty array cannot be initialized from an empty repeated pattern.
    #[error("nonempty stable array requires a pattern")]
    EmptyPattern,
}

/// Derive-generated safe initialization entry point for a stable payload.
pub trait StablePayloadInit: StablePayload {
    /// Typestate initializer generated for this payload.
    type Initializer<'a>
    where
        Self: 'a;

    /// Starts initializing `Self` in caller-provided byte storage.
    ///
    /// # Errors
    ///
    /// Returns an error unless storage has exact size and alignment.
    fn init(storage: &mut [MaybeUninit<u8>]) -> Result<Self::Initializer<'_>, UWireError>;
}

/// Witness that every field of a stable payload has been initialized.
#[derive(Debug)]
pub struct InitializedStablePayload<'a, T>
where
    T: StablePayload,
{
    value: &'a mut T,
}

impl<T> Deref for InitializedStablePayload<'_, T>
where
    T: StablePayload,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<T> DerefMut for InitializedStablePayload<'_, T>
where
    T: StablePayload,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}

impl<T> InitializedStablePayload<'_, T>
where
    T: StablePayload,
{
    /// Returns the fully initialized object representation.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: The derive-generated typestate reaches this witness only after
        // every field was written; the initializer zeroes all padding first.
        unsafe {
            core::slice::from_raw_parts(
                core::ptr::from_ref::<T>(self.value).cast::<u8>(),
                mem::size_of::<T>(),
            )
        }
    }
}

/// Prepares exact aligned storage for a derive-generated initializer.
#[doc(hidden)]
pub fn prepare_stable_payload_storage<T>(
    storage: &mut [MaybeUninit<u8>],
) -> Result<*mut T, UWireError>
where
    T: StablePayload,
{
    StableContainerPayload::<T>::representation_proven()?;
    if storage.len() != mem::size_of::<T>() {
        return Err(UWireError::invalid_payload_length(
            mem::size_of::<T>(),
            storage.len(),
        ));
    }
    if !(storage.as_ptr() as usize).is_multiple_of(mem::align_of::<T>()) {
        return Err(UWireError::invalid_payload(format!(
            "stable initialization storage is not aligned to {} bytes",
            mem::align_of::<T>()
        )));
    }
    // SAFETY: storage is an exclusive writable slice of byte-sized MaybeUninit
    // elements. Zero is valid for those storage elements; no T reference is formed
    // until its generated initializer has written every typed field.
    unsafe {
        core::ptr::write_bytes(storage.as_mut_ptr(), 0, storage.len());
    }
    Ok(storage.as_mut_ptr().cast::<T>())
}

/// Writes one derive-validated field into prepared storage.
///
/// # Safety
///
/// `payload` must point to live, exclusively writable storage prepared for `T`.
/// `offset` must identify an in-bounds, correctly aligned field of exactly `F`,
/// and that field must not have been initialized already. No reference may alias
/// the destination during the write. The derive's offset checks and typestate
/// uphold this contract; manual callers must establish the same obligations.
#[doc(hidden)]
pub unsafe fn write_stable_payload_field<T, F>(payload: *mut T, offset: usize, value: F) {
    // SAFETY: The derive computes an in-bounds field offset for F and permits
    // this function only once for that field through typestate.
    unsafe { payload.cast::<u8>().add(offset).cast::<F>().write(value) };
}

/// Writes one array field element directly without constructing the array.
///
/// # Safety
///
/// `payload` must point to live, exclusively writable prepared storage for `T`.
/// `offset` must identify a correctly aligned, in-bounds `[E; N]` field, with all
/// element storage uninitialized and no aliasing references. The field's total
/// extent and every element address must fit the allocation. Each element is
/// written once; if `element` panics, callers must not finalize the partially
/// initialized `T`. The derive proves the field layout and initialization state.
#[doc(hidden)]
pub unsafe fn write_stable_payload_array<T, E, const N: usize, F>(
    payload: *mut T,
    offset: usize,
    mut element: F,
) where
    F: FnMut(usize) -> E,
{
    // SAFETY: The caller's field contract establishes an in-bounds, aligned
    // [E; N] destination in the same prepared allocation, including N == 0.
    let array = unsafe { payload.cast::<u8>().add(offset).cast::<E>() };
    let mut index = 0;
    while index < N {
        // SAFETY: The derive proves this field is [E; N], and each index is
        // visited exactly once while it is strictly less than N.
        unsafe { array.add(index).write(element(index)) };
        index += 1;
    }
}

/// Initializes a stable array field by repeating a borrowed value pattern in place.
///
/// Values are duplicated by their complete stable object representation, without
/// invoking Clone, allocating a payload-sized temporary or leaving unwritten bytes.
/// A longer pattern is truncated to the field length. Empty fields need no pattern.
///
/// # Safety
/// `payload` must point to live, aligned, exclusively writable prepared storage
/// for T. `offset` must identify its in-bounds, correctly aligned `[E; N]` field,
/// not previously initialized and not aliased by `pattern`. E's stable-field
/// contract must hold, including initialized padding and safely duplicable plain
/// data. The pattern must be nonempty when N is nonzero. No reference may alias
/// the destination while it is written, and all extents must fit the allocation.
#[doc(hidden)]
pub unsafe fn write_stable_payload_array_pattern<T, E: StablePayloadField, const N: usize>(
    payload: *mut T,
    offset: usize,
    pattern: &[E],
) {
    if N == 0 {
        return;
    }
    assert!(
        !pattern.is_empty(),
        "nonempty stable array requires a pattern"
    );
    // SAFETY: the caller establishes an aligned, in-bounds array field in this
    // prepared allocation. No typed reference to uninitialized elements is made.
    let destination = unsafe { payload.cast::<u8>().add(offset).cast::<E>() };
    let mut initialized = N.min(pattern.len());
    // SAFETY: the pattern is initialized stable plain data and disjoint from the
    // destination; initialized <= pattern.len() and <= N. StablePayloadField
    // allows representation duplication without ownership-sensitive/drop state.
    unsafe {
        core::ptr::copy_nonoverlapping(pattern.as_ptr(), destination, initialized);
    }
    while initialized < N {
        let count = initialized.min(N - initialized);
        // SAFETY: the initialized prefix is valid stable data; count <= initialized
        // makes it disjoint from the following destination. Both ranges fit N,
        // and each destination element is written exactly once before publication.
        unsafe {
            core::ptr::copy_nonoverlapping(destination, destination.add(initialized), count);
        }
        initialized += count;
    }
}

/// Creates the final witness after derive-generated typestate reaches all-set.
///
/// # Safety
///
/// `payload` must be non-null, aligned and point to a completely initialized,
/// valid `T` in the prepared storage, including its canonical zeroed padding.
/// That allocation must remain live and exclusively borrowed for all of `'a`;
/// no other access may alias the returned mutable reference. Every field must
/// have been initialized exactly once before this call. The derive's all-set
/// typestate supplies this proof; manual callers assume the same obligations.
#[doc(hidden)]
pub unsafe fn finish_stable_payload_init<'a, T>(payload: *mut T) -> InitializedStablePayload<'a, T>
where
    T: StablePayload,
{
    InitializedStablePayload {
        // SAFETY: The caller's contract: `payload` is non-null, aligned, points
        // at a completely initialized `T` with zeroed padding, and that storage
        // is live and exclusively borrowed for `'a`.
        value: unsafe { &mut *payload },
    }
}

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
        agreement.resolve_type_identity::<T, UWireError>(Self::prove_representation)
    }

    /// Returns the profile's encoding allocation for this complete type.
    ///
    /// # Errors
    ///
    /// Returns the same representation/membership errors as [`Self::identity`].
    pub fn encoding(agreement: &NativeProfileAgreement) -> Result<PayloadEncoding, UWireError> {
        Self::identity(agreement).map(NativePayloadIdentity::encoding)
    }

    /// Checks manual descriptors against the actual Rust layout and target.
    ///
    /// This builds the full descriptor and is therefore not free. It runs once
    /// per type and generation through [`Self::identity`], and once per type
    /// and process through [`Self::representation_proven`], never per message.
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

    /// Proves once per process that `T`'s descriptor matches its Rust layout,
    /// then answers from a per-type memo without building the descriptor.
    ///
    /// Every byte-level entry point (both borrow lanes and storage preparation)
    /// calls this first, so a manual [`StablePayload`] whose descriptor
    /// disagrees with its type is rejected at the first use of that type and
    /// not only when an agreement resolves it. The memo records type identities
    /// whose proof succeeded; a failed proof is not recorded and is repeated.
    ///
    /// # Errors
    ///
    /// Returns the error of [`Self::validate_representation`].
    pub fn representation_proven() -> Result<(), UWireError> {
        if PROVEN_REPRESENTATIONS
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&TypeId::of::<T>())
        {
            return Ok(());
        }
        Self::prove_representation().map(drop)
    }

    /// Builds and validates the descriptor, recording the proof for `T`.
    fn prove_representation() -> Result<NativeRepresentation, UWireError> {
        let representation = Self::validate_representation()?;
        PROVEN_REPRESENTATIONS
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(TypeId::of::<T>());
        Ok(representation)
    }
}

/// Type identities whose descriptor was proven to match their Rust layout.
///
/// A proof is a property of the type and the compilation target, not of any
/// agreement, so one process-wide, append-only memo is the right scope. It
/// never maps identifiers to types and is not consulted for identity.
static PROVEN_REPRESENTATIONS: RwLock<BTreeSet<TypeId>> = RwLock::new(BTreeSet::new());

fn scalar_representation<T>(name: &str) -> NativeRepresentation {
    NativeRepresentation::new(
        name,
        0,
        mem::size_of::<T>() as u64,
        mem::align_of::<T>() as u64,
        NativeByteOrder::native(),
        vec![],
    )
    .expect("Rust scalar layout")
}

macro_rules! always_valid_field {
    ($($ty:ty),+ $(,)?) => {
        $(
            // SAFETY: Each listed scalar is a fixed-width integer or float whose
            // every bit pattern is a valid value, with no padding, pointers,
            // borrowed state or drop glue; its descriptor is its exact size and
            // alignment on this target.
            unsafe impl StablePayloadField for $ty {
                const FIELD_BITS_ALWAYS_VALID: bool = true;

                fn field_representation() -> NativeRepresentation {
                    scalar_representation::<Self>(stringify!($ty))
                }

                fn validate_field_bytes(bytes: &[u8]) -> bool {
                    bytes.len() == mem::size_of::<Self>()
                }
            }
        )+
    };
}

always_valid_field!(
    (),
    u8,
    i8,
    u16,
    i16,
    u32,
    i32,
    u64,
    i64,
    u128,
    i128,
    usize,
    isize,
    f32,
    f64
);

// SAFETY: A bool is one byte whose only valid patterns are 0 and 1, which the
// validator checks exactly; it has no padding, pointers or drop glue.
unsafe impl StablePayloadField for bool {
    const FIELD_BITS_ALWAYS_VALID: bool = false;

    fn field_representation() -> NativeRepresentation {
        scalar_representation::<Self>("bool")
    }

    fn validate_field_bytes(bytes: &[u8]) -> bool {
        matches!(bytes, [0] | [1])
    }
}

// SAFETY: A char is four bytes whose valid patterns are exactly the Unicode
// scalar values, which the validator checks through `char::from_u32`.
unsafe impl StablePayloadField for char {
    const FIELD_BITS_ALWAYS_VALID: bool = false;

    fn field_representation() -> NativeRepresentation {
        scalar_representation::<Self>("char")
    }

    fn validate_field_bytes(bytes: &[u8]) -> bool {
        let [a, b, c, d] = bytes else {
            return false;
        };
        char::from_u32(u32::from_ne_bytes([*a, *b, *c, *d])).is_some()
    }
}

// SAFETY: An array is N contiguous elements with no padding between them;
// its bits are valid exactly when every element's bits are valid, which the
// validator checks element by element, and it inherits the element's
// freedom from pointers, borrowed state and drop glue.
unsafe impl<T, const N: usize> StablePayloadField for [T; N]
where
    T: StablePayloadField,
{
    const FIELD_BITS_ALWAYS_VALID: bool = T::FIELD_BITS_ALWAYS_VALID;

    fn field_representation() -> NativeRepresentation {
        NativeRepresentation::new(
            "array",
            0,
            mem::size_of::<Self>() as u64,
            mem::align_of::<Self>() as u64,
            NativeByteOrder::native(),
            vec![NativeField::repeated(
                "element",
                0,
                N as u64,
                T::field_representation(),
            )],
        )
        .expect("Rust stable array layout")
    }

    fn validate_field_bytes(bytes: &[u8]) -> bool {
        if bytes.len() != mem::size_of::<Self>() {
            return false;
        }
        if N == 0 {
            return true;
        }
        let field_size = mem::size_of::<T>();
        if field_size == 0 {
            return T::validate_field_bytes(&[]);
        }
        let mut rest = bytes;
        while let Some((field, tail)) = rest.split_at_checked(field_size) {
            if !T::validate_field_bytes(field) {
                return false;
            }
            rest = tail;
            if rest.is_empty() {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    struct ImportedTarget {
        value: u32,
    }

    // SAFETY: This test type is one fully initialized u32 without padding or
    // drop glue. Its intentionally foreign descriptor is used only to exercise
    // rejection by the representation proof, before any byte-to-reference cast.
    unsafe impl StablePayloadField for ImportedTarget {
        const FIELD_BITS_ALWAYS_VALID: bool = true;
        fn field_representation() -> NativeRepresentation {
            NativeRepresentation::for_target(
                crate::NativeTarget::new("foreign-unknown-target", usize::BITS as u8).unwrap(),
                "example.ImportedTarget",
                0,
                mem::size_of::<Self>() as u64,
                mem::align_of::<Self>() as u64,
                NativeByteOrder::native(),
                vec![],
            )
            .unwrap()
        }
        fn validate_field_bytes(bytes: &[u8]) -> bool {
            bytes.len() == mem::size_of::<Self>()
        }
    }

    // SAFETY: The fixed memory layout above is valid; this negative fixture
    // checks that its target claim is rejected without exposing a typed borrow.
    unsafe impl StablePayload for ImportedTarget {
        const TYPE_NAME: &'static str = "example.ImportedTarget";
    }

    #[test]
    fn equal_layout_from_a_foreign_target_is_not_a_local_representation() {
        assert!(
            matches!(StableContainerPayload::<ImportedTarget>::validate_representation(),
            Err(UWireError::InvalidPayload(reason)) if reason.contains("Rust type and target"))
        );
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
