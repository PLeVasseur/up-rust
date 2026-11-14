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

//! Stable native layouts: const descriptors, field-bit validation and in-place initialization.
//!
//! Everything a descriptor says is `const` data, so a stable payload's identity
//! costs nothing at runtime and needs no first-use cache. The heap-backed
//! `NativeRepresentation` lives in `up-rust` and is built from this data through
//! an extension trait, so an ordinary stable type needs nothing from `std`.

use core::{
    fmt,
    marker::PhantomData,
    mem,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr,
};

/// One field, or the repeated element of an array, in a [`StableDescriptor`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StableField {
    /// Deployment-stable field name.
    pub name: &'static str,
    /// Byte offset within the enclosing representation.
    pub offset: u64,
    /// Repetition count, 1 for an ordinary field.
    pub count: u64,
    /// The field's own representation.
    pub descriptor: &'static StableDescriptor,
}

/// The complete structural description of a stable representation.
///
/// Names are deployment-stable cross-language names, not Rust type names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StableDescriptor {
    /// Deployment-stable name of this representation.
    pub name: &'static str,
    /// Layout revision. A changed layout under an existing name needs a new one.
    pub revision: u32,
    /// Complete object size in bytes, including padding.
    pub size: u64,
    /// Required address alignment in bytes.
    pub alignment: u64,
    /// Fields in declaration order; empty for scalars.
    pub fields: &'static [StableField],
}

/// Field-level bit-validity and layout contract, as `const` data.
///
/// # Safety
///
/// Implementations must return `true` from [`Self::validate_field_bytes`] only
/// when `bytes` has exactly the layout of `Self` and every represented bit
/// pattern is valid for `Self`. Every valid value must have fully initialized
/// bytes, with no implicit padding, pointers, borrowed state, interior mutability
/// or drop glue, and [`Self::DESCRIPTOR`] must describe the type exactly. In
/// particular it must not contain `UnsafeCell` or atomic/cell types that could
/// mutate through a shared reference to borrowed bytes. Valid representations
/// must be safely duplicable as plain data; ownership-sensitive handles or
/// uniqueness-dependent state are not stable payload fields.
pub unsafe trait StablePayloadField: Sized {
    /// Whether every bit pattern of exactly `size_of::<Self>()` bytes is valid.
    const FIELD_BITS_ALWAYS_VALID: bool;

    /// The complete structural description of this field.
    const DESCRIPTOR: &'static StableDescriptor;

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
pub unsafe trait StablePayload: StablePayloadField + 'static {
    /// Cross-language stable type name.
    const TYPE_NAME: &'static str;
    /// Layout revision carried in the descriptor; a new revision is a new identity.
    const REVISION: u32 = 0;
}

/// Typestate marker for an uninitialized stable field.
#[derive(Debug)]
pub struct StablePayloadInitUnset;

/// Typestate marker for an initialized stable field.
#[derive(Debug)]
pub struct StablePayloadInitSet;

/// Fixed-size failure from a generated stable array pattern initializer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StablePayloadPatternError {
    /// A nonempty array cannot be initialized from an empty repeated pattern.
    EmptyPattern,
}

impl fmt::Display for StablePayloadPatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPattern => f.write_str("nonempty stable array requires a pattern"),
        }
    }
}

impl core::error::Error for StablePayloadPatternError {}

/// Fixed-size failure from a generated stable initializer. Carries no heap data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StableInitError {
    /// The caller-provided storage is not exactly the payload's size.
    Length {
        /// The payload size.
        expected: usize,
        /// The storage size.
        actual: usize,
    },
    /// The caller-provided storage is not aligned for the payload.
    Misaligned {
        /// Required alignment.
        required: usize,
    },
    /// The descriptor does not match the Rust type.
    Representation,
    /// A borrowed input slice did not have the array's length.
    SliceLength {
        /// The array length.
        expected: usize,
        /// The slice length.
        actual: usize,
    },
}

impl fmt::Display for StableInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length { expected, actual } => {
                write!(f, "stable storage length {actual} is not {expected}")
            }
            Self::Misaligned { required } => {
                write!(
                    f,
                    "stable initialization storage is not aligned to {required} bytes"
                )
            }
            Self::Representation => {
                f.write_str("stable representation does not match its Rust type")
            }
            Self::SliceLength { expected, actual } => {
                write!(
                    f,
                    "input slice length {actual} is not array length {expected}"
                )
            }
        }
    }
}

impl core::error::Error for StableInitError {}

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
    fn init(storage: &mut [MaybeUninit<u8>]) -> Result<Self::Initializer<'_>, StableInitError>;
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
                ptr::from_ref::<T>(self.value).cast::<u8>(),
                mem::size_of::<T>(),
            )
        }
    }
}

/// Whether `T`'s const descriptor names `T`'s real size and alignment.
///
/// Every byte-level entry point checks this first, so a manual [`StablePayload`]
/// whose descriptor disagrees with its type is refused before any byte becomes a
/// reference. Costs two integer comparisons, so no per-type memo is needed.
#[must_use]
pub const fn descriptor_matches_layout<T: StablePayloadField>() -> bool {
    T::DESCRIPTOR.size == mem::size_of::<T>() as u64
        && T::DESCRIPTOR.alignment == mem::align_of::<T>() as u64
}

/// Prepares exact aligned storage for a derive-generated initializer.
#[doc(hidden)]
pub fn prepare_stable_payload_storage<T>(
    storage: &mut [MaybeUninit<u8>],
) -> Result<*mut T, StableInitError>
where
    T: StablePayload,
{
    if !descriptor_matches_layout::<T>() {
        return Err(StableInitError::Representation);
    }
    let expected = mem::size_of::<T>();
    if storage.len() != expected {
        return Err(StableInitError::Length {
            expected,
            actual: storage.len(),
        });
    }
    let required = mem::align_of::<T>();
    if !(storage.as_ptr() as usize).is_multiple_of(required) {
        return Err(StableInitError::Misaligned { required });
    }
    for byte in storage.iter_mut() {
        byte.write(0);
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
/// written once. The derive proves the field layout and initialization state.
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
    let mut index = 0_usize;
    while index < N {
        // SAFETY: The derive proves this field is [E; N], and each index is
        // visited exactly once while it is strictly less than N.
        unsafe { array.add(index).write(element(index)) };
        index = index.wrapping_add(1);
    }
}

/// Writes one array field from a borrowed array of the same length, element by element.
///
/// # Safety
///
/// As [`write_stable_payload_array`].
#[doc(hidden)]
pub unsafe fn write_stable_payload_array_from<T, E: Clone, const N: usize>(
    payload: *mut T,
    offset: usize,
    values: &[E; N],
) {
    // SAFETY: The caller's field contract establishes an in-bounds, aligned
    // [E; N] destination in the same prepared allocation, including N == 0.
    let array = unsafe { payload.cast::<u8>().add(offset).cast::<E>() };
    for (index, value) in values.iter().enumerate() {
        // SAFETY: `index < N` because `values` has exactly N elements; each
        // index is visited once.
        unsafe { array.add(index).write(value.clone()) };
    }
}

/// Initializes a stable array field by repeating a borrowed value pattern in place.
///
/// Values are duplicated by their complete stable object representation, without
/// invoking Clone, allocating a payload-sized temporary or leaving unwritten bytes.
/// A longer pattern is truncated to the field length. Empty fields need no pattern.
/// The derive checks that a nonempty field is given a nonempty pattern.
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
    if N == 0 || pattern.is_empty() {
        return;
    }
    // SAFETY: the caller establishes an aligned, in-bounds array field in this
    // prepared allocation. No typed reference to uninitialized elements is made.
    let destination = unsafe { payload.cast::<u8>().add(offset).cast::<E>() };
    let mut initialized = N.min(pattern.len());
    // SAFETY: the pattern is initialized stable plain data and disjoint from the
    // destination; initialized <= pattern.len() and <= N. StablePayloadField
    // allows representation duplication without ownership-sensitive/drop state.
    unsafe {
        ptr::copy_nonoverlapping(pattern.as_ptr(), destination, initialized);
    }
    while initialized < N {
        let count = initialized.min(N.saturating_sub(initialized));
        // SAFETY: the initialized prefix is valid stable data; count <= initialized
        // makes it disjoint from the following destination. Both ranges fit N,
        // and each destination element is written exactly once before publication.
        unsafe {
            ptr::copy_nonoverlapping(destination, destination.add(initialized), count);
        }
        initialized = initialized.saturating_add(count);
    }
}

/// What a derive-generated initializer holds: its prepared storage and which of
/// its fields are written so far, as a typestate parameter `S`.
///
/// `new` and `advance` are `unsafe`, and the only way to change `S`, so the
/// builder's fields being visible in the deriving module does not let safe code
/// declare a payload finished. The generated setters are the unsafe callers.
#[doc(hidden)]
pub struct StablePayloadInitToken<'a, T, S> {
    payload: *mut T,
    marker: PhantomData<(&'a mut T, S)>,
}

impl<T, S> fmt::Debug for StablePayloadInitToken<'_, T, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StablePayloadInitToken")
            .finish_non_exhaustive()
    }
}

impl<'a, T, S> StablePayloadInitToken<'a, T, S> {
    /// A token over freshly prepared storage.
    ///
    /// # Safety
    /// `payload` must be live, aligned, exclusively writable storage prepared for
    /// `T` for all of `'a`, and `S` must say that nothing is written yet.
    #[doc(hidden)]
    #[must_use]
    pub unsafe fn new(payload: *mut T) -> Self {
        Self {
            payload,
            marker: PhantomData,
        }
    }

    /// The prepared storage.
    #[doc(hidden)]
    #[must_use]
    pub fn payload(&self) -> *mut T {
        self.payload
    }

    /// Moves to a new written-fields state.
    ///
    /// # Safety
    /// Every field `S2` says is set must be initialized, each exactly once.
    #[doc(hidden)]
    #[must_use]
    pub unsafe fn advance<S2>(self) -> StablePayloadInitToken<'a, T, S2> {
        StablePayloadInitToken {
            payload: self.payload,
            marker: PhantomData,
        }
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

const fn scalar_descriptor<T>(name: &'static str) -> StableDescriptor {
    StableDescriptor {
        name,
        revision: 0,
        size: mem::size_of::<T>() as u64,
        alignment: mem::align_of::<T>() as u64,
        fields: &[],
    }
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
                const DESCRIPTOR: &'static StableDescriptor =
                    &scalar_descriptor::<Self>(stringify!($ty));

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
    const DESCRIPTOR: &'static StableDescriptor = &scalar_descriptor::<Self>("bool");

    fn validate_field_bytes(bytes: &[u8]) -> bool {
        matches!(bytes, [0] | [1])
    }
}

// SAFETY: A char is four bytes whose valid patterns are exactly the Unicode
// scalar values, which the validator checks through `char::from_u32`.
unsafe impl StablePayloadField for char {
    const FIELD_BITS_ALWAYS_VALID: bool = false;
    const DESCRIPTOR: &'static StableDescriptor = &scalar_descriptor::<Self>("char");

    fn validate_field_bytes(bytes: &[u8]) -> bool {
        let [a, b, c, d] = bytes else {
            return false;
        };
        char::from_u32(u32::from_ne_bytes([*a, *b, *c, *d])).is_some()
    }
}

// SAFETY: An array is N contiguous elements with no padding between them; its
// bits are valid exactly when every element's bits are valid, which the
// validator checks element by element, and it inherits the element's freedom
// from pointers, borrowed state and drop glue.
unsafe impl<T, const N: usize> StablePayloadField for [T; N]
where
    T: StablePayloadField,
{
    const FIELD_BITS_ALWAYS_VALID: bool = T::FIELD_BITS_ALWAYS_VALID;
    const DESCRIPTOR: &'static StableDescriptor = &StableDescriptor {
        name: "array",
        revision: 0,
        size: mem::size_of::<Self>() as u64,
        alignment: mem::align_of::<Self>() as u64,
        fields: &[StableField {
            name: "element",
            offset: 0,
            count: N as u64,
            descriptor: T::DESCRIPTOR,
        }],
    };

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
