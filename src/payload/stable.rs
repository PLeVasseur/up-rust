/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

use core::{
    marker::PhantomData,
    mem,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
};

use crate::{PayloadEncoding, UWireError, PAYLOAD_ENCODING_PRIVATE_USE_MIN};

const FNV_OFFSET_BASIS: u32 = 0x811c_9dc5;
const FNV_PRIME: u32 = 0x0100_0193;

/// Byte-order discriminator included in a stable payload identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StableMemoryRepresentation {
    /// Little-endian stable memory.
    LittleEndian,
    /// Big-endian stable memory.
    BigEndian,
}

impl StableMemoryRepresentation {
    const fn discriminator(self) -> u8 {
        match self {
            Self::LittleEndian => 0x01,
            Self::BigEndian => 0x02,
        }
    }

    /// Returns the current target's stable-memory representation.
    #[must_use]
    pub const fn native() -> Self {
        if cfg!(target_endian = "little") {
            Self::LittleEndian
        } else {
            Self::BigEndian
        }
    }
}

const fn fnv_byte(hash: u32, byte: u8) -> u32 {
    (hash ^ byte as u32).wrapping_mul(FNV_PRIME)
}

const fn fnv_bytes(bytes: &[u8], hash: u32) -> u32 {
    match bytes.split_first() {
        Some((first, rest)) => fnv_bytes(rest, fnv_byte(hash, *first)),
        None => hash,
    }
}

const fn fnv_le_u32(hash: u32, value: u32) -> u32 {
    let hash = fnv_byte(hash, value as u8);
    let hash = fnv_byte(hash, (value >> 8) as u8);
    let hash = fnv_byte(hash, (value >> 16) as u8);
    fnv_byte(hash, (value >> 24) as u8)
}

/// Derives a stable private-use payload encoding for explicit layout parts.
///
/// # Panics
///
/// Panics in const evaluation when `size` or `alignment` cannot be represented
/// as `u32`.
#[must_use]
pub const fn stable_payload_encoding_id_for(
    type_name: &str,
    variant: u32,
    size: usize,
    alignment: usize,
    representation: StableMemoryRepresentation,
) -> u32 {
    assert!(size <= u32::MAX as usize, "stable payload size exceeds u32");
    assert!(
        alignment <= u32::MAX as usize,
        "stable payload alignment exceeds u32"
    );
    let hash = fnv_bytes(type_name.as_bytes(), FNV_OFFSET_BASIS);
    let hash = fnv_le_u32(hash, variant);
    let hash = fnv_le_u32(hash, size as u32);
    let hash = fnv_le_u32(hash, alignment as u32);
    let hash = fnv_byte(hash, representation.discriminator());
    PAYLOAD_ENCODING_PRIVATE_USE_MIN | (hash & 0x0fff_ffff)
}

/// Derives the native stable private-use payload encoding.
#[must_use]
pub const fn stable_payload_encoding_id(
    type_name: &str,
    variant: u32,
    size: usize,
    alignment: usize,
) -> u32 {
    stable_payload_encoding_id_for(
        type_name,
        variant,
        size,
        alignment,
        StableMemoryRepresentation::native(),
    )
}

/// Field-level bit-validity contract used by stable payload validation.
///
/// # Safety
///
/// Implementations must return `true` only when `bytes` has exactly the layout
/// of `Self` and every represented bit pattern is valid for `Self`.
pub unsafe trait StablePayloadField: Sized {
    /// Whether every bit pattern of exactly `size_of::<Self>()` bytes is valid.
    const FIELD_BITS_ALWAYS_VALID: bool;

    /// Validates one field representation without constructing `Self`.
    fn validate_field_bytes(bytes: &[u8]) -> bool;
}

/// Stable in-memory payload contract for a deployment-agreed ABI domain.
///
/// # Safety
///
/// Implementors must have a fixed layout, fully initialized object
/// representation, and a validator that rejects every invalid bit pattern.
pub unsafe trait StablePayload: StablePayloadField + Sized + 'static {
    /// Cross-language stable type name.
    const TYPE_NAME: &'static str;
    /// Layout variant.
    const VARIANT: u32 = 0;
    /// Private-use payload encoding identity.
    const PAYLOAD_ENCODING_ID: u32 = stable_payload_encoding_id(
        Self::TYPE_NAME,
        Self::VARIANT,
        mem::size_of::<Self>(),
        mem::align_of::<Self>(),
    );
}

/// Typestate marker for an uninitialized stable field.
#[derive(Debug)]
pub struct StablePayloadInitUnset;

/// Typestate marker for an initialized stable field.
#[derive(Debug)]
pub struct StablePayloadInitSet;

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
    storage.fill(MaybeUninit::new(0));
    Ok(storage.as_mut_ptr().cast::<T>())
}

/// Writes one derive-validated field into prepared storage.
#[doc(hidden)]
pub unsafe fn write_stable_payload_field<T, F>(payload: *mut T, offset: usize, value: F) {
    // SAFETY: The derive computes an in-bounds field offset for F and permits
    // this function only once for that field through typestate.
    unsafe { payload.cast::<u8>().add(offset).cast::<F>().write(value) };
}

/// Writes one array field element directly without constructing the array.
#[doc(hidden)]
pub unsafe fn write_stable_payload_array<T, E, const N: usize, F>(
    payload: *mut T,
    offset: usize,
    mut element: F,
) where
    F: FnMut(usize) -> E,
{
    let array = unsafe { payload.cast::<u8>().add(offset).cast::<E>() };
    let mut index = 0;
    while index < N {
        // SAFETY: The derive proves this field is [E; N], and each index is
        // visited exactly once while it is strictly less than N.
        unsafe { array.add(index).write(element(index)) };
        index += 1;
    }
}

/// Creates the final witness after derive-generated typestate reaches all-set.
#[doc(hidden)]
pub unsafe fn finish_stable_payload_init<'a, T>(payload: *mut T) -> InitializedStablePayload<'a, T>
where
    T: StablePayload,
{
    // SAFETY: All fields were initialized exactly once and padding was zeroed.
    InitializedStablePayload {
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
    /// Returns the deployment-private stable payload encoding.
    #[must_use]
    pub const fn encoding() -> PayloadEncoding {
        PayloadEncoding::from_registry_entry(T::PAYLOAD_ENCODING_ID)
    }

    /// Checks the stable type's explicit private-use identity.
    ///
    /// # Errors
    ///
    /// Returns an error if a manual implementation declares a registered ID.
    pub fn validate_identity() -> Result<(), UWireError> {
        if T::PAYLOAD_ENCODING_ID < PAYLOAD_ENCODING_PRIVATE_USE_MIN {
            return Err(UWireError::invalid_payload(
                "stable payload encoding must be in the private-use range",
            ));
        }
        Ok(())
    }
}

macro_rules! always_valid_field {
    ($($ty:ty),+ $(,)?) => {
        $(
            unsafe impl StablePayloadField for $ty {
                const FIELD_BITS_ALWAYS_VALID: bool = true;

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

unsafe impl StablePayloadField for bool {
    const FIELD_BITS_ALWAYS_VALID: bool = false;

    fn validate_field_bytes(bytes: &[u8]) -> bool {
        matches!(bytes, [0] | [1])
    }
}

unsafe impl StablePayloadField for char {
    const FIELD_BITS_ALWAYS_VALID: bool = false;

    fn validate_field_bytes(bytes: &[u8]) -> bool {
        let [a, b, c, d] = bytes else {
            return false;
        };
        char::from_u32(u32::from_ne_bytes([*a, *b, *c, *d])).is_some()
    }
}

unsafe impl<T, const N: usize> StablePayloadField for [T; N]
where
    T: StablePayloadField,
{
    const FIELD_BITS_ALWAYS_VALID: bool = T::FIELD_BITS_ALWAYS_VALID;

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

    #[test]
    fn stable_id_vectors_are_frozen() {
        let cases = [
            (
                "uprotocol.test.Component",
                0,
                16,
                8,
                StableMemoryRepresentation::LittleEndian,
                0x1b0a_0f0a,
            ),
            (
                "uprotocol.test.Component",
                0,
                16,
                8,
                StableMemoryRepresentation::BigEndian,
                0x1a0a_0d77,
            ),
            (
                "uprotocol.test.Component2",
                0,
                16,
                8,
                StableMemoryRepresentation::LittleEndian,
                0x1098_e992,
            ),
            (
                "uprotocol.test.Component",
                1,
                16,
                8,
                StableMemoryRepresentation::LittleEndian,
                0x1f0d_936d,
            ),
            (
                "uprotocol.test.Component",
                0,
                24,
                8,
                StableMemoryRepresentation::LittleEndian,
                0x1adb_9a72,
            ),
            (
                "uprotocol.test.Component",
                0,
                16,
                4,
                StableMemoryRepresentation::LittleEndian,
                0x1b32_b1e6,
            ),
        ];
        for (name, variant, size, align, representation, expected) in cases {
            assert_eq!(
                stable_payload_encoding_id_for(name, variant, size, align, representation),
                expected
            );
        }
    }

    #[test]
    fn constrained_fields_fail_closed() {
        assert!(bool::validate_field_bytes(&[0]));
        assert!(bool::validate_field_bytes(&[1]));
        assert!(!bool::validate_field_bytes(&[2]));
        assert!(char::validate_field_bytes(&('x' as u32).to_ne_bytes()));
        assert!(!char::validate_field_bytes(&0x0011_0000_u32.to_ne_bytes()));
        assert!(!<[bool; 2]>::validate_field_bytes(&[0, 2]));
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
            .finish();

        assert_eq!(initialized.tag, 7);
        assert_eq!(initialized.bytes, *b"test");
        assert!(initialized.valid);
        assert!(Initialized::validate_field_bytes(initialized.as_bytes()));
    }
}
