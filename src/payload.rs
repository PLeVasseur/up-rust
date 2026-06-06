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

use std::{
    error::Error,
    fmt::Display,
    io::Read,
    marker::PhantomData,
    mem,
    ptr::{self, NonNull},
};

use bytes::Bytes;
use mediatype::ReadParams;

use crate::{PayloadEncoding, ProtobufMappable, UCode, UPayloadFormat, UStatus};

const STABLE_CONTAINER_ENCODING_ID: &str = "up.stable-container";
const STABLE_CONTAINER_MEDIA_TYPE: &str = "application/vnd.uprotocol.stable-container";

/// Error type used by serialization-neutral payload helpers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UWireError {
    /// A caller-provided output buffer is too small for the serialized payload.
    BufferTooSmall {
        /// Required output size in bytes.
        expected: usize,
        /// Provided output size in bytes.
        actual: usize,
    },
    /// Payload bytes are malformed for the selected decoder.
    InvalidPayload(String),
    /// A typed decode was requested, but the frame has no payload encoding.
    MissingEncoding,
    /// A typed decode was requested, but the frame has no payload bytes.
    MissingPayload,
    /// The frame's encoding is not compatible with the selected payload codec.
    UnsupportedEncoding {
        /// Encoding declared by the requested codec.
        expected: Box<PayloadEncoding>,
        /// Encoding carried by the frame being decoded.
        actual: Box<PayloadEncoding>,
    },
    /// Serializer or deserializer implementation failed.
    SerializationError(String),
}

impl UWireError {
    /// Creates a [`UWireError::BufferTooSmall`] value.
    #[must_use]
    pub fn buffer_too_small(expected: usize, actual: usize) -> Self {
        Self::BufferTooSmall { expected, actual }
    }

    /// Creates a [`UWireError::InvalidPayload`] value.
    #[must_use]
    pub fn invalid_payload(message: impl Into<String>) -> Self {
        Self::InvalidPayload(message.into())
    }

    /// Creates a stable payload length error.
    #[must_use]
    pub fn invalid_payload_length(expected: usize, actual: usize) -> Self {
        Self::invalid_payload(format!("payload length must be {expected}, got {actual}"))
    }

    /// Creates a [`UWireError::SerializationError`] value.
    #[must_use]
    pub fn serialization_error(message: impl Into<String>) -> Self {
        Self::SerializationError(message.into())
    }
}

impl Display for UWireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooSmall { expected, actual } => f.write_fmt(format_args!(
                "buffer too small: expected at least {expected} bytes, got {actual} bytes"
            )),
            Self::InvalidPayload(message) => {
                f.write_fmt(format_args!("invalid payload: {message}"))
            }
            Self::MissingEncoding => f.write_str("frame payload has no encoding metadata"),
            Self::MissingPayload => f.write_str("frame has no payload"),
            Self::UnsupportedEncoding { expected, actual } => f.write_fmt(format_args!(
                "unsupported payload encoding: expected {expected:?}; got {actual:?}",
            )),
            Self::SerializationError(message) => {
                f.write_fmt(format_args!("serialization error: {message}"))
            }
        }
    }
}

impl Error for UWireError {}

impl From<UWireError> for UStatus {
    fn from(value: UWireError) -> Self {
        UStatus::fail_with_code(UCode::InvalidArgument, value.to_string())
    }
}

/// Byte layout requested by a payload codec.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PayloadLayout {
    len: usize,
    align: usize,
}

impl PayloadLayout {
    /// Creates a payload layout.
    ///
    /// # Errors
    ///
    /// Returns an error when `align` is zero or not a power of two.
    pub fn new(len: usize, align: usize) -> Result<Self, UWireError> {
        if align == 0 {
            return Err(UWireError::invalid_payload(
                "payload alignment must be non-zero",
            ));
        }
        if !align.is_power_of_two() {
            return Err(UWireError::invalid_payload(format!(
                "payload alignment {align} is not a power of two"
            )));
        }
        Ok(Self { len, align })
    }

    /// Returns the payload length in bytes.
    #[must_use]
    pub fn len(self) -> usize {
        self.len
    }

    /// Returns the required payload alignment in bytes.
    #[must_use]
    pub fn align(self) -> usize {
        self.align
    }

    /// Returns whether the payload has zero bytes.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// Completion proof returned by generated stable payload initializers.
///
/// This token is intentionally not a typed reference into the loan. It proves
/// that a generated typestate builder reached `finish()` after initializing all
/// semantic fields and generated padding gaps.
pub struct InitializedStablePayload<T> {
    _marker: PhantomData<fn() -> T>,
}

impl<T> InitializedStablePayload<T> {
    /// Creates a completion proof after an expert initializer has fully initialized `T`.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that the corresponding storage contains one
    /// valid initialized `T`, including all implicit padding bytes, before this
    /// token is returned to a send helper.
    #[doc(hidden)]
    #[must_use]
    pub unsafe fn new_unchecked() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

/// Compile-time identity for an application payload codec.
///
/// ```rust
/// use up_rust::{payload::PayloadFormat, PayloadEncoding, UPayloadFormat};
///
/// struct JsonTelemetry;
///
/// impl PayloadFormat for JsonTelemetry {
///     fn name() -> &'static str {
///         "json-telemetry-v1"
///     }
///
///     fn encoding() -> PayloadEncoding {
///         PayloadEncoding::standard(UPayloadFormat::Json)
///     }
/// }
/// ```
pub trait PayloadFormat {
    /// Stable codec name for logs, diagnostics, and configuration.
    fn name() -> &'static str;

    /// Payload encoding metadata written into frames that use this codec.
    fn encoding() -> PayloadEncoding;
}

/// Payload-layer codec identity used by typed frame helpers.
pub trait PayloadCodec {
    /// Stable codec name for logs, diagnostics, and configuration.
    fn codec_name() -> &'static str;

    /// Payload encoding metadata written into frames that use this codec.
    fn payload_encoding() -> PayloadEncoding;

    /// Verifies frame encoding metadata against this codec.
    ///
    /// # Errors
    ///
    /// Returns an error if the frame is missing payload encoding metadata or if
    /// the metadata is incompatible with this codec.
    fn verify_encoding(actual: Option<&PayloadEncoding>) -> Result<(), UWireError> {
        let expected = Self::payload_encoding();
        let actual = actual.ok_or(UWireError::MissingEncoding)?;
        if !actual.is_compatible_with(&expected) {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(actual.clone()),
            });
        }
        Ok(())
    }
}

impl<F> PayloadCodec for F
where
    F: PayloadFormat,
{
    fn codec_name() -> &'static str {
        <F as PayloadFormat>::name()
    }

    fn payload_encoding() -> PayloadEncoding {
        <F as PayloadFormat>::encoding()
    }
}

/// Encodes a typed value with a [`PayloadCodec`].
pub trait EncodePayload<T: ?Sized>: PayloadCodec {
    /// Returns the exact payload layout required to encode `value`.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be measured for this codec.
    fn payload_layout(value: &T) -> Result<PayloadLayout, UWireError>;

    /// Encodes `value` into `dst`.
    ///
    /// # Errors
    ///
    /// Returns an error if `dst` is too small or serialization fails.
    fn encode_payload(value: &T, dst: &mut [u8]) -> Result<(), UWireError>;

    /// Encodes `value` into owned bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    fn encode_payload_owned(value: &T) -> Result<Bytes, UWireError> {
        let layout = Self::payload_layout(value)?;
        let mut bytes = vec![0_u8; layout.len()];
        Self::encode_payload(value, &mut bytes)?;
        Ok(Bytes::from(bytes))
    }
}

/// Decodes a typed value from contiguous payload bytes.
pub trait DecodePayload<'a, T>: PayloadCodec {
    /// Decodes `T` from payload bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if `src` is malformed for this codec.
    fn decode_payload(src: &'a [u8]) -> Result<T, UWireError>;
}

/// Decodes a typed value from an ordered payload byte stream.
pub trait ReadDecodePayload<T>: PayloadCodec {
    /// Decodes `T` from `reader`, which must yield exactly `payload_len` bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the reader fails, yields an unexpected byte count, or
    /// contains malformed payload bytes for this codec.
    fn decode_payload_from_reader<R: Read>(reader: R, payload_len: usize) -> Result<T, UWireError>;
}

/// Initializes and borrows a typed value directly in initialized transmit storage.
///
/// # Safety
///
/// Implementors must guarantee that [`LoanPayload::loan_payload`] returns
/// `&mut T` only when the destination byte range is uniquely borrowed, has the
/// exact layout returned by [`LoanPayload::loan_layout`], and contains one valid
/// initialized `T` for the returned lifetime.
pub unsafe trait LoanPayload<T>: PayloadCodec {
    /// Returns the exact layout required for a typed initialized transmit loan.
    ///
    /// # Errors
    ///
    /// Returns an error if this codec cannot loan `T` into initialized TX storage.
    fn loan_layout() -> Result<PayloadLayout, UWireError>;

    /// Initializes `dst` and returns a typed mutable view over the loaned payload.
    ///
    /// # Errors
    ///
    /// Returns an error if `dst` does not have the required length or alignment.
    fn loan_payload(dst: &mut [u8]) -> Result<&mut T, UWireError>;
}

/// Marker for codecs whose payload value is already an opaque byte sequence.
pub trait BytePayloadCodec: PayloadCodec {}

/// Built-in raw byte payload codec.
pub struct RawBytes;

impl RawBytes {
    /// Returns the raw-byte payload encoding metadata.
    #[must_use]
    pub fn encoding() -> PayloadEncoding {
        <Self as PayloadFormat>::encoding()
    }
}

impl PayloadFormat for RawBytes {
    fn name() -> &'static str {
        "raw-bytes"
    }

    fn encoding() -> PayloadEncoding {
        PayloadEncoding::standard(UPayloadFormat::Raw)
    }
}

impl BytePayloadCodec for RawBytes {}

impl EncodePayload<[u8]> for RawBytes {
    fn payload_layout(value: &[u8]) -> Result<PayloadLayout, UWireError> {
        PayloadLayout::new(value.len(), 1)
    }

    fn encode_payload(value: &[u8], dst: &mut [u8]) -> Result<(), UWireError> {
        let actual = dst.len();
        let out = dst
            .get_mut(..value.len())
            .ok_or_else(|| UWireError::buffer_too_small(value.len(), actual))?;
        out.copy_from_slice(value);
        Ok(())
    }
}

impl<'a> DecodePayload<'a, &'a [u8]> for RawBytes {
    fn decode_payload(src: &'a [u8]) -> Result<&'a [u8], UWireError> {
        Ok(src)
    }
}

impl<'a> DecodePayload<'a, Vec<u8>> for RawBytes {
    fn decode_payload(src: &'a [u8]) -> Result<Vec<u8>, UWireError> {
        Ok(src.to_vec())
    }
}

impl<'a> DecodePayload<'a, Bytes> for RawBytes {
    fn decode_payload(src: &'a [u8]) -> Result<Bytes, UWireError> {
        Ok(Bytes::copy_from_slice(src))
    }
}

impl ReadDecodePayload<Vec<u8>> for RawBytes {
    fn decode_payload_from_reader<R: Read>(
        reader: R,
        payload_len: usize,
    ) -> Result<Vec<u8>, UWireError> {
        read_exact_payload(reader, payload_len)
    }
}

impl ReadDecodePayload<Bytes> for RawBytes {
    fn decode_payload_from_reader<R: Read>(
        reader: R,
        payload_len: usize,
    ) -> Result<Bytes, UWireError> {
        read_exact_payload(reader, payload_len).map(Bytes::from)
    }
}

/// Stable payload type variant used in stable-container metadata.
///
/// Phase 05A supports one fixed-size value per payload. Runtime-length stable
/// slices are intentionally left for a later API.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StablePayloadVariant {
    /// One fixed-size `T` value with exact payload length `size_of::<T>()`.
    FixedSize,
}

impl StablePayloadVariant {
    /// Returns the stable media-type spelling for this variant.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FixedSize => "fixed",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "fixed" => Some(Self::FixedSize),
            _ => None,
        }
    }
}

impl Display for StablePayloadVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Runtime type detail carried by stable-container metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StableTypeDetail<'a> {
    /// Fixed-size stable-container payload variant.
    pub variant: StablePayloadVariant,
    /// Cross-process and cross-language stable type identity.
    pub type_name: &'a str,
    /// Payload value size in bytes.
    pub size: usize,
    /// Required payload value alignment in bytes.
    pub alignment: usize,
}

impl Display for StableTypeDetail<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "type={};variant={};size={};align={}",
            self.type_name, self.variant, self.size, self.alignment
        )
    }
}

/// Hidden recursive field proof used by the `StablePayload` derive.
///
/// Application code should use `#[derive(StablePayload)]` instead of naming this
/// trait directly. Manual implementations are only for externally-audited FFI or
/// codegen payload fields whose layout and ownership are known to be stable.
///
/// # Safety
///
/// Implementors must not contain process-local references, raw pointers,
/// function pointers, heap ownership, or drop glue that would make the value
/// invalid as part of a stable-container payload representation.
#[doc(hidden)]
pub unsafe trait StablePayloadField: Sized + 'static {
    #[doc(hidden)]
    fn __stable_payload_field_check(&self) {}
}

macro_rules! impl_stable_payload_field {
    ($($ty:ty),* $(,)?) => {
        $(
            // SAFETY: Primitive scalar values have no process-local ownership or
            // drop glue, and arrays recurse through this field proof below.
            unsafe impl StablePayloadField for $ty {}
        )*
    };
}

impl_stable_payload_field!(
    (),
    bool,
    char,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64,
);

// SAFETY: Arrays are stable fields when their element type is stable by the same
// recursive field proof.
unsafe impl<T, const N: usize> StablePayloadField for [T; N]
where
    T: StablePayloadField,
{
    fn __stable_payload_field_check(&self) {
        for item in self {
            item.__stable_payload_field_check();
        }
    }
}

/// Stable payload identity used by `StableContainerPayload<T>`.
///
/// This phase uses the trait only to build and verify stable-container metadata.
/// Future borrow/no-zero phases consume the same identity when proving that bytes
/// may safely be viewed as `T`.
///
/// # Safety
///
/// Implementors must choose a stable cross-process type name and only implement
/// the trait for types whose size/alignment and initialized byte representation
/// are suitable for the stable-container contract used by the application.
pub unsafe trait StablePayload: Sized + 'static {
    /// Stable-container variant supported by this type.
    const VARIANT: StablePayloadVariant = StablePayloadVariant::FixedSize;

    /// Stable cross-process type name.
    const TYPE_NAME: &'static str;

    /// Returns the stable cross-process type name.
    #[must_use]
    fn stable_type_name() -> &'static str {
        Self::TYPE_NAME
    }

    /// Returns the stable metadata detail for this type.
    #[must_use]
    fn stable_type_detail() -> StableTypeDetail<'static> {
        StableTypeDetail {
            variant: Self::VARIANT,
            type_name: Self::stable_type_name(),
            size: mem::size_of::<Self>(),
            alignment: mem::align_of::<Self>(),
        }
    }

    /// Hidden recursive field check emitted by the `StablePayload` derive.
    #[doc(hidden)]
    fn __stable_payload_field_check(&self) {}
}

mod byte_backed_stable_field_seal {
    pub trait Sealed {}

    macro_rules! impl_sealed_field {
        ($($ty:ty),* $(,)?) => {
            $(
                impl Sealed for $ty {}
            )*
        };
    }

    impl_sealed_field!(
        (),
        bool,
        char,
        u8,
        u16,
        u32,
        u64,
        u128,
        usize,
        i8,
        i16,
        i32,
        i64,
        i128,
        isize,
        f32,
        f64,
    );

    impl<T, const N: usize> Sealed for [T; N] where T: Sealed {}

    impl<T> Sealed for T where T: super::ByteBackedStablePayload {}
}

/// Hidden recursive proof that a field is safe for byte-backed stable payloads.
///
/// This trait is derive-support plumbing, not an application extension point.
/// Use [`ByteBackedStablePayload`] for payload-level public bounds.
#[doc(hidden)]
#[allow(private_bounds)]
pub trait ByteBackedStablePayloadField:
    byte_backed_stable_field_seal::Sealed + Sized + 'static
{
    #[doc(hidden)]
    const SUPPORTS_BYTE_BACKED_STABLE_FIELD: bool;
}

macro_rules! impl_byte_backed_stable_field {
    ($($ty:ty),* $(,)?) => {
        $(
            impl ByteBackedStablePayloadField for $ty {
                const SUPPORTS_BYTE_BACKED_STABLE_FIELD: bool = true;
            }
        )*
    };
}

impl_byte_backed_stable_field!(
    (),
    bool,
    char,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64,
);

impl<T, const N: usize> ByteBackedStablePayloadField for [T; N]
where
    T: ByteBackedStablePayloadField,
{
    const SUPPORTS_BYTE_BACKED_STABLE_FIELD: bool = T::SUPPORTS_BYTE_BACKED_STABLE_FIELD;
}

impl<T> ByteBackedStablePayloadField for T
where
    T: ByteBackedStablePayload,
{
    const SUPPORTS_BYTE_BACKED_STABLE_FIELD: bool = true;
}

/// Stronger stable payload proof for byte-backed stable-container paths.
///
/// `StablePayload` proves stable type identity, representation eligibility, and
/// receive-side metadata compatibility. `ByteBackedStablePayload` additionally
/// proves that the type has no implicit inter-field or trailing padding and that
/// every field is recursively byte-backed.
///
/// Prefer `#[derive(StablePayload, ByteBackedStablePayload)]` for hand-written
/// payload types. Manual implementations are reserved for externally audited FFI
/// or code-generated layouts with an equivalent whole-object initialization
/// proof.
///
/// # Safety
///
/// Implementors must guarantee that copying or initializing exactly
/// `size_of::<Self>()` bytes is a sound representation of one valid `Self` for
/// stable-container paths. That requires stable layout, no implicit padding, no
/// drop glue, and recursively byte-backed fields.
pub unsafe trait ByteBackedStablePayload: StablePayload {
    /// Whether this type's full object representation can be used by
    /// byte-backed stable-container paths.
    const SUPPORTS_BYTE_BACKED_UNINIT: bool = true;

    #[doc(hidden)]
    const BYTE_BACKED_STABLE_PAYLOAD_CHECK: () = {
        assert!(Self::SUPPORTS_BYTE_BACKED_UNINIT);
        assert!(!mem::needs_drop::<Self>());
    };
}

/// Returns whether `T` can use byte-backed stable-container paths.
#[must_use]
pub const fn stable_payload_supports_byte_backed_uninit<T: ByteBackedStablePayload>() -> bool {
    T::SUPPORTS_BYTE_BACKED_UNINIT && !mem::needs_drop::<T>()
}

/// Compile-time assertion for byte-backed stable-container eligibility.
pub const fn assert_stable_payload_byte_backed_uninit<T>()
where
    T: ByteBackedStablePayload,
{
    assert!(T::SUPPORTS_BYTE_BACKED_UNINIT);
    assert!(!mem::needs_drop::<T>());
    #[allow(path_statements)]
    T::BYTE_BACKED_STABLE_PAYLOAD_CHECK;
}

mod stable_payload_init_complete_value_seal {
    pub trait Sealed {}

    impl<T> Sealed for T where T: super::ByteBackedStablePayload {}

    impl<T, const N: usize> Sealed for [T; N] where T: super::ByteBackedStablePayloadField {}
}

/// Hidden proof that a complete by-value nested initializer can be moved into a field.
///
/// This is derive support, not an application extension point. It is implemented
/// only for byte-backed stable payloads, where moving the complete value into
/// loan storage cannot expose uninitialized implicit padding bytes.
#[doc(hidden)]
#[allow(private_bounds)]
pub trait StablePayloadInitCompleteValue<T>:
    stable_payload_init_complete_value_seal::Sealed + Sized
{
    #[doc(hidden)]
    fn into_complete_value(self) -> T;
}

impl<T> StablePayloadInitCompleteValue<T> for T
where
    T: ByteBackedStablePayload,
{
    fn into_complete_value(self) -> T {
        self
    }
}

impl<T, const N: usize> StablePayloadInitCompleteValue<[T; N]> for [T; N]
where
    T: ByteBackedStablePayloadField,
{
    fn into_complete_value(self) -> [T; N] {
        self
    }
}

/// Hidden typestate marker used by `StablePayloadInit` derive output.
#[doc(hidden)]
pub enum StablePayloadInitUnset {}

/// Hidden typestate marker used by `StablePayloadInit` derive output.
#[doc(hidden)]
pub enum StablePayloadInitSet {}

/// Safe generated initialization proof for stable-container payloads.
///
/// Derived implementations initialize one stable payload directly in
/// uninitialized storage. Generated builders expose named typed setters and make
/// `finish()` available only after all required fields are set. Implementations
/// must also initialize any implicit and trailing padding bytes they own without
/// blanket-zeroing the full payload.
///
/// # Safety
///
/// Implementors must guarantee that every successful `finish()` for `Self::Init`
/// returns only after the full transported `size_of::<Self>()` byte range,
/// including implicit padding, contains one valid initialized `Self` in the
/// stable-container representation. Ordinary payload types should use the derive
/// macro; manual implementations are an expert unsafe extension point.
pub unsafe trait StablePayloadInit: StablePayload {
    /// Generated any-order typestate initializer for this payload type.
    type Init<'a>;

    /// Creates a generated initializer from an exact uninitialized payload range.
    ///
    /// # Errors
    ///
    /// Returns an error if the byte range does not match this stable payload's
    /// size and alignment.
    fn init_from_uninit_bytes<'a>(
        payload: &'a mut [mem::MaybeUninit<u8>],
    ) -> Result<Self::Init<'a>, UWireError>;

    /// Creates a generated initializer from a nested stable payload slot.
    #[doc(hidden)]
    fn __init_from_slot<'a>(
        slot: StablePayloadInitSlot<'a, Self>,
    ) -> Result<Self::Init<'a>, UWireError>;
}

/// Hidden typed view over uninitialized storage used by generated initializers.
///
/// The type is public only so derive output in downstream crates can name it.
/// Application code should use `#[derive(StablePayloadInit)]` and the generated
/// setters instead of constructing or manipulating slots directly.
#[doc(hidden)]
pub struct StablePayloadInitSlot<'a, T> {
    ptr: NonNull<mem::MaybeUninit<T>>,
    _marker: PhantomData<&'a mut mem::MaybeUninit<T>>,
}

impl<'a, T> StablePayloadInitSlot<'a, T> {
    /// Creates a slot from uninitialized payload bytes after exact stable layout validation.
    #[doc(hidden)]
    pub fn from_uninit_bytes(bytes: &'a mut [mem::MaybeUninit<u8>]) -> Result<Self, UWireError>
    where
        T: StablePayload,
    {
        StableContainerPayload::<T>::check_uninit_layout(bytes)?;
        let ptr =
            NonNull::new(bytes.as_mut_ptr().cast::<mem::MaybeUninit<T>>()).ok_or_else(|| {
                UWireError::invalid_payload("stable payload init slot pointer is null")
            })?;
        Ok(Self {
            ptr,
            _marker: PhantomData,
        })
    }

    fn byte_ptr(&self) -> *mut mem::MaybeUninit<u8> {
        self.ptr.as_ptr().cast::<mem::MaybeUninit<u8>>()
    }

    /// Initializes an implicit or trailing padding gap to zero.
    ///
    /// # Safety
    ///
    /// `offset..offset + len` must be in bounds for the slot and must not overlap
    /// any semantic field that will later be initialized by a generated setter.
    #[doc(hidden)]
    pub unsafe fn write_padding(&mut self, offset: usize, len: usize) {
        let start = self.byte_ptr().cast::<u8>();
        // SAFETY: The caller guarantees the padding gap is in bounds for the
        // slot and each byte is written at most once before commit.
        unsafe { ptr::write_bytes(start.add(offset), 0, len) };
    }

    /// Writes one typed field at a generated byte offset.
    ///
    /// # Safety
    ///
    /// `offset` must be the start of a properly aligned `U` field within this
    /// slot, and the field must not have been initialized before this call.
    #[doc(hidden)]
    pub unsafe fn write_field<U>(&mut self, offset: usize, value: U) {
        let ptr = self.byte_ptr().cast::<u8>();
        // SAFETY: The generated caller proves `offset` names one uninitialized
        // field of type `U` inside this slot.
        unsafe { ptr.add(offset).cast::<U>().write(value) };
    }

    /// Copies exact bytes into a `[u8; N]` field.
    ///
    /// # Safety
    ///
    /// `offset..offset + src.len()` must be the target byte array field and must
    /// not overlap initialized bytes.
    #[doc(hidden)]
    pub unsafe fn write_bytes(&mut self, offset: usize, src: &[u8]) {
        let ptr = self.byte_ptr().cast::<u8>();
        // SAFETY: The generated caller proves the target byte array field is in
        // bounds and currently uninitialized.
        unsafe { ptr::copy_nonoverlapping(src.as_ptr(), ptr.add(offset), src.len()) };
    }

    /// Fills exact bytes in a `[u8; N]` field.
    ///
    /// # Safety
    ///
    /// `offset..offset + len` must be the target byte array field and must not
    /// overlap initialized bytes.
    #[doc(hidden)]
    pub unsafe fn fill_bytes(&mut self, offset: usize, len: usize, value: u8) {
        let ptr = self.byte_ptr().cast::<u8>();
        // SAFETY: The generated caller proves the byte range is in bounds for
        // the uninitialized byte array field.
        unsafe { ptr::write_bytes(ptr.add(offset), value, len) };
    }

    /// Fills exact bytes in a `[u8; N]` field using an index-based generator.
    ///
    /// # Safety
    ///
    /// `offset..offset + len` must be the target byte array field and must not
    /// overlap initialized bytes.
    #[doc(hidden)]
    pub unsafe fn fill_bytes_with(
        &mut self,
        offset: usize,
        len: usize,
        mut value: impl FnMut(usize) -> u8,
    ) {
        let ptr = self.byte_ptr().cast::<u8>();
        for index in 0..len {
            // SAFETY: The generated caller proves the target byte is in bounds
            // for the uninitialized byte array field.
            unsafe { ptr.add(offset + index).write(value(index)) };
        }
    }

    /// Copies a typed array field from a slice after an exact length check.
    ///
    /// # Safety
    ///
    /// `offset` must be the start of an uninitialized `[U; expected]` field.
    #[doc(hidden)]
    pub unsafe fn copy_array_from_slice<U: Copy>(
        &mut self,
        offset: usize,
        src: &[U],
        expected: usize,
    ) -> Result<(), UWireError> {
        if src.len() != expected {
            return Err(UWireError::invalid_payload_length(expected, src.len()));
        }
        let dst = self.byte_ptr().cast::<u8>();
        // SAFETY: The generated caller proves the target array field is in
        // bounds and uninitialized. `src` contains valid initialized `U` values.
        unsafe { ptr::copy_nonoverlapping(src.as_ptr(), dst.add(offset).cast::<U>(), expected) };
        Ok(())
    }

    /// Fills a typed array field with a valid copied element value.
    ///
    /// # Safety
    ///
    /// `offset` must be the start of an uninitialized `[U; len]` field.
    #[doc(hidden)]
    pub unsafe fn fill_array<U: Copy>(&mut self, offset: usize, len: usize, value: U) {
        let dst = self.byte_ptr().cast::<u8>();
        for index in 0..len {
            // SAFETY: The generated caller proves the target element is in
            // bounds for the uninitialized array field.
            unsafe { dst.add(offset).cast::<U>().add(index).write(value) };
        }
    }

    /// Returns a typed nested field slot.
    ///
    /// # Safety
    ///
    /// `offset` must be the start of a properly aligned uninitialized `U` field
    /// inside this slot.
    #[doc(hidden)]
    pub unsafe fn field_slot<U>(&mut self, offset: usize) -> StablePayloadInitSlot<'a, U> {
        let ptr = self.byte_ptr().cast::<u8>();
        // SAFETY: The generated caller proves `offset` names a nested field of
        // type `U` in this slot.
        let ptr = unsafe { ptr.add(offset).cast::<mem::MaybeUninit<U>>() };
        let ptr = NonNull::new(ptr).expect("stable payload nested field slot pointer is null");
        StablePayloadInitSlot {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Returns a typed nested array element slot.
    ///
    /// # Safety
    ///
    /// `offset` must be the start of an array field, and `index` must be in
    /// bounds for that array.
    #[doc(hidden)]
    pub unsafe fn array_element_slot<U>(
        &mut self,
        offset: usize,
        index: usize,
    ) -> StablePayloadInitSlot<'a, U> {
        let ptr = self.byte_ptr().cast::<u8>();
        let element_offset = offset + index * mem::size_of::<U>();
        // SAFETY: The generated caller proves `element_offset` names one nested
        // element of type `U` in this slot.
        let ptr = unsafe { ptr.add(element_offset).cast::<mem::MaybeUninit<U>>() };
        let ptr = NonNull::new(ptr).expect("stable payload array element slot pointer is null");
        StablePayloadInitSlot {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Marks the slot as initialized after generated typestate completion.
    ///
    /// # Safety
    ///
    /// Every byte in the slot, including implicit padding, must contain one valid
    /// initialized `T`.
    #[doc(hidden)]
    #[must_use]
    pub unsafe fn assume_init(self) -> InitializedStablePayload<T> {
        // SAFETY: The generated caller invokes this only from the all-set
        // typestate state after all semantic fields and generated padding gaps
        // have been initialized.
        unsafe { InitializedStablePayload::new_unchecked() }
    }
}

/// Type-agnostic stable-container metadata parsed from a payload encoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StableContainerPayloadInfo {
    /// Cross-process and cross-language stable type identity.
    pub type_name: String,
    /// Stable-container payload variant.
    pub variant: StablePayloadVariant,
    /// Exact payload size in bytes.
    pub size: usize,
    /// Advertised payload alignment in bytes.
    pub alignment: usize,
}

impl StableContainerPayloadInfo {
    /// Native custom encoding id for stable-container payloads.
    pub const ENCODING_ID: &'static str = STABLE_CONTAINER_ENCODING_ID;

    /// Parses stable-container metadata from a payload encoding.
    ///
    /// # Errors
    ///
    /// Returns an error if the encoding is not stable-container metadata or if
    /// the stable-container content type is malformed.
    pub fn parse(encoding: &PayloadEncoding) -> Result<Self, UWireError> {
        let expected = PayloadEncoding::custom(Self::ENCODING_ID, STABLE_CONTAINER_MEDIA_TYPE)
            .expect("stable-container media type is valid");
        let Some((id, content_type)) = encoding.custom_identity() else {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(encoding.clone()),
            });
        };
        if id != Self::ENCODING_ID {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(encoding.clone()),
            });
        }
        Self::parse_content_type(content_type).map_err(UWireError::invalid_payload)
    }

    /// Returns whether this metadata is compatible with local stable payload `T`.
    #[must_use]
    pub fn is_compatible_with<T>(&self) -> bool
    where
        T: StablePayload,
    {
        let expected = T::stable_type_detail();
        self.type_name == expected.type_name
            && self.variant == expected.variant
            && self.size == expected.size
            && self.alignment >= expected.alignment
    }

    fn parse_content_type(content_type: &str) -> Result<Self, String> {
        let media_type = mediatype::MediaType::parse(content_type)
            .map_err(|error| format!("invalid media type: {error}"))?;
        let expected_media_type = mediatype::MediaType::parse(STABLE_CONTAINER_MEDIA_TYPE)
            .expect("stable-container media type is valid");
        if media_type.essence() != expected_media_type {
            return Err(format!(
                "media type must be {}",
                expected_media_type.essence()
            ));
        }

        let type_name = required_stable_parameter(&media_type, "type")?;
        if type_name.is_empty() {
            return Err("type parameter must not be empty".to_string());
        }
        let variant = required_stable_parameter(&media_type, "variant")?;
        let variant = StablePayloadVariant::parse(&variant).ok_or_else(|| {
            format!(
                "variant parameter must be {}",
                StablePayloadVariant::FixedSize
            )
        })?;
        let size = required_stable_parameter(&media_type, "size")?
            .parse::<usize>()
            .map_err(|error| format!("invalid size parameter: {error}"))?;
        let alignment = required_stable_parameter(&media_type, "align")?
            .parse::<usize>()
            .map_err(|error| format!("invalid align parameter: {error}"))?;
        PayloadLayout::new(size, alignment).map_err(|error| error.to_string())?;

        Ok(Self {
            type_name,
            variant,
            size,
            alignment,
        })
    }
}

fn required_stable_parameter(
    media_type: &mediatype::MediaType<'_>,
    name: &'static str,
) -> Result<String, String> {
    media_type
        .get_param(mediatype::Name::new_unchecked(name))
        .map(|value| value.unquoted_str().into_owned())
        .ok_or_else(|| format!("missing {name} parameter"))
}

/// Transport-independent stable-container payload identity.
///
/// Phase 05A uses this type for metadata and owned-byte preservation. Typed
/// zero-copy borrows are intentionally deferred to the Phase 05B loan-backed
/// receive proof API.
pub struct StableContainerPayload<T>(PhantomData<T>);

impl<T: StablePayload> StableContainerPayload<T> {
    /// Native custom encoding id for stable-container payloads.
    pub const ENCODING_ID: &'static str = STABLE_CONTAINER_ENCODING_ID;

    const MEDIA_TYPE: &'static str = STABLE_CONTAINER_MEDIA_TYPE;

    /// Returns the stable-container payload encoding for `T`.
    #[must_use]
    pub fn encoding() -> PayloadEncoding {
        <Self as PayloadCodec>::payload_encoding()
    }

    fn stable_content_type() -> String {
        let detail = T::stable_type_detail();
        format!(
            "{};type={};variant={};size={};align={}",
            Self::MEDIA_TYPE,
            Self::quote_parameter(detail.type_name),
            detail.variant,
            detail.size,
            detail.alignment
        )
    }

    fn quote_parameter(value: &str) -> String {
        let quoted = mediatype::Value::quote(value);
        if quoted.starts_with('"') {
            quoted.into_owned()
        } else {
            format!("\"{quoted}\"")
        }
    }

    fn verify_content_type(content_type: &str) -> Result<(), String> {
        let info = StableContainerPayloadInfo::parse_content_type(content_type)?;
        let expected = T::stable_type_detail();
        if info.type_name != expected.type_name {
            return Err(format!("type parameter must be {}", expected.type_name));
        }
        if info.variant != expected.variant {
            return Err(format!(
                "variant parameter must be {}",
                expected.variant.as_str()
            ));
        }
        if info.size != expected.size {
            return Err(format!("size parameter must be {}", expected.size));
        }
        if info.alignment < expected.alignment {
            return Err(format!(
                "align parameter must be at least {}",
                expected.alignment
            ));
        }
        Ok(())
    }

    pub(crate) fn borrow_checked_payload(src: &[u8]) -> Result<&T, UWireError> {
        let expected_len = mem::size_of::<T>();
        if src.len() != expected_len {
            return Err(UWireError::invalid_payload(format!(
                "payload length must be {expected_len}, got {}",
                src.len()
            )));
        }

        let alignment = mem::align_of::<T>();
        let address = src.as_ptr() as usize;
        if !address.is_multiple_of(alignment) {
            return Err(UWireError::invalid_payload(format!(
                "payload address {address} is not aligned to {alignment}"
            )));
        }

        // SAFETY: length and alignment were verified above. Reaching this helper
        // requires the Phase 05B loan-backed receive proof, and `T: StablePayload`
        // is the unsafe contract that the bytes represent one initialized `T`.
        Ok(unsafe { &*src.as_ptr().cast::<T>() })
    }

    pub(crate) fn check_uninit_layout(src: &[mem::MaybeUninit<u8>]) -> Result<(), UWireError> {
        let expected_len = mem::size_of::<T>();
        if src.len() != expected_len {
            return Err(UWireError::invalid_payload_length(expected_len, src.len()));
        }

        let alignment = mem::align_of::<T>();
        let address = src.as_ptr() as usize;
        if !address.is_multiple_of(alignment) {
            return Err(UWireError::invalid_payload(format!(
                "payload address {address} is not aligned to {alignment}"
            )));
        }
        Ok(())
    }

    fn check_initialized_layout(src: &[u8]) -> Result<(), UWireError> {
        let expected_len = mem::size_of::<T>();
        if src.len() != expected_len {
            return Err(UWireError::invalid_payload_length(expected_len, src.len()));
        }

        if expected_len == 0 {
            return Ok(());
        }

        let alignment = mem::align_of::<T>();
        let address = src.as_ptr() as usize;
        if !address.is_multiple_of(alignment) {
            return Err(UWireError::invalid_payload(format!(
                "payload address {address} is not aligned to {alignment}"
            )));
        }
        Ok(())
    }
}

impl<T> PayloadCodec for StableContainerPayload<T>
where
    T: StablePayload,
{
    fn codec_name() -> &'static str {
        "stable-container"
    }

    fn payload_encoding() -> PayloadEncoding {
        PayloadEncoding::custom(Self::ENCODING_ID, Self::stable_content_type())
            .expect("stable-container payload encoding is valid")
    }

    fn verify_encoding(actual: Option<&PayloadEncoding>) -> Result<(), UWireError> {
        let expected = Self::payload_encoding();
        let actual = actual.ok_or(UWireError::MissingEncoding)?;
        let Some((id, content_type)) = actual.custom_identity() else {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(actual.clone()),
            });
        };
        if id != Self::ENCODING_ID {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(actual.clone()),
            });
        }
        Self::verify_content_type(content_type).map_err(|reason| {
            UWireError::invalid_payload(format!(
                "incompatible stable payload: expected {}; actual {} ({reason})",
                T::stable_type_detail(),
                content_type
            ))
        })
    }
}

// SAFETY:
// - `loan_payload` verifies exact length and alignment before casting the loaned
//   destination to `*mut T`.
// - `T: ByteBackedStablePayload + Default` provides a no-padding stable payload
//   proof and a safe initialized value before `&mut T` is exposed to callers.
unsafe impl<T> LoanPayload<T> for StableContainerPayload<T>
where
    T: ByteBackedStablePayload + Default,
{
    fn loan_layout() -> Result<PayloadLayout, UWireError> {
        PayloadLayout::new(mem::size_of::<T>(), mem::align_of::<T>())
    }

    fn loan_payload(dst: &mut [u8]) -> Result<&mut T, UWireError> {
        Self::check_initialized_layout(dst)?;
        dst.fill(0);
        let ptr = if mem::size_of::<T>() == 0 {
            NonNull::<T>::dangling().as_ptr()
        } else {
            dst.as_mut_ptr().cast::<T>()
        };
        // SAFETY: the checked byte range has the exact `T` layout, or `T` is a
        // zero-sized byte-backed type using an aligned dangling pointer. Writing
        // `T::default()` creates one valid initialized `T` before returning the
        // unique mutable reference tied to `dst`'s borrow.
        unsafe { ptr.write(T::default()) };
        // SAFETY: `ptr` now points to one initialized `T`; `dst` is exclusively
        // borrowed for the returned lifetime and no other reference to the value
        // is created by this helper.
        Ok(unsafe { &mut *ptr })
    }
}

/// Protocol Buffers application payload codec.
///
/// `ProtobufPayload` serializes and deserializes only application payload bytes.
/// It does not wrap a complete uProtocol frame and does not serialize frame
/// metadata. Use `ProtobufUMessageFrame` when an entire native frame must be
/// encoded as a generated `UMessage` envelope.
pub struct ProtobufPayload;

impl ProtobufPayload {
    /// Returns the generic Protocol Buffers payload encoding metadata.
    #[must_use]
    pub fn encoding() -> PayloadEncoding {
        <Self as PayloadFormat>::encoding()
    }
}

impl PayloadFormat for ProtobufPayload {
    fn name() -> &'static str {
        "protobuf"
    }

    fn encoding() -> PayloadEncoding {
        PayloadEncoding::standard(UPayloadFormat::Protobuf)
    }
}

impl<T> EncodePayload<T> for ProtobufPayload
where
    T: ProtobufMappable,
{
    fn payload_layout(value: &T) -> Result<PayloadLayout, UWireError> {
        PayloadLayout::new(Self::encode_payload_owned(value)?.len(), 1)
    }

    fn encode_payload(value: &T, dst: &mut [u8]) -> Result<(), UWireError> {
        copy_encoded_payload(Self::encode_payload_owned(value)?, dst)
    }

    fn encode_payload_owned(value: &T) -> Result<Bytes, UWireError> {
        value
            .write_to_protobuf_bytes()
            .map(Bytes::from)
            .map_err(|error| UWireError::serialization_error(error.to_string()))
    }
}

impl<'a, T> DecodePayload<'a, T> for ProtobufPayload
where
    T: ProtobufMappable,
{
    fn decode_payload(src: &'a [u8]) -> Result<T, UWireError> {
        T::parse_from_protobuf_bytes(src)
            .map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

impl<T> ReadDecodePayload<T> for ProtobufPayload
where
    T: ProtobufMappable,
{
    fn decode_payload_from_reader<R: Read>(reader: R, payload_len: usize) -> Result<T, UWireError> {
        let bytes = read_exact_payload(reader, payload_len)?;
        T::parse_from_protobuf_bytes(&bytes)
            .map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

/// Protocol Buffers `google.protobuf.Any` application payload codec.
pub struct ProtobufAnyPayload;

impl ProtobufAnyPayload {
    /// Returns the protobuf-Any payload encoding metadata.
    #[must_use]
    pub fn encoding() -> PayloadEncoding {
        <Self as PayloadFormat>::encoding()
    }
}

impl PayloadFormat for ProtobufAnyPayload {
    fn name() -> &'static str {
        "protobuf-any"
    }

    fn encoding() -> PayloadEncoding {
        PayloadEncoding::standard(UPayloadFormat::ProtobufWrappedInAny)
    }
}

impl<T> EncodePayload<T> for ProtobufAnyPayload
where
    T: ProtobufMappable,
{
    fn payload_layout(value: &T) -> Result<PayloadLayout, UWireError> {
        PayloadLayout::new(Self::encode_payload_owned(value)?.len(), 1)
    }

    fn encode_payload(value: &T, dst: &mut [u8]) -> Result<(), UWireError> {
        copy_encoded_payload(Self::encode_payload_owned(value)?, dst)
    }

    fn encode_payload_owned(value: &T) -> Result<Bytes, UWireError> {
        value
            .write_to_packed_protobuf_bytes()
            .map(Bytes::from)
            .map_err(|error| UWireError::serialization_error(error.to_string()))
    }
}

impl<'a, T> DecodePayload<'a, T> for ProtobufAnyPayload
where
    T: ProtobufMappable,
{
    fn decode_payload(src: &'a [u8]) -> Result<T, UWireError> {
        T::parse_from_packed_protobuf_bytes(src)
            .map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

impl<T> ReadDecodePayload<T> for ProtobufAnyPayload
where
    T: ProtobufMappable,
{
    fn decode_payload_from_reader<R: Read>(reader: R, payload_len: usize) -> Result<T, UWireError> {
        let bytes = read_exact_payload(reader, payload_len)?;
        T::parse_from_packed_protobuf_bytes(&bytes)
            .map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

fn copy_encoded_payload(bytes: Bytes, dst: &mut [u8]) -> Result<(), UWireError> {
    let actual = dst.len();
    let out = dst
        .get_mut(..bytes.len())
        .ok_or_else(|| UWireError::buffer_too_small(bytes.len(), actual))?;
    out.copy_from_slice(&bytes);
    Ok(())
}

fn read_exact_payload<R: Read>(mut reader: R, payload_len: usize) -> Result<Vec<u8>, UWireError> {
    let mut bytes = Vec::with_capacity(payload_len);
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| UWireError::invalid_payload(error.to_string()))?;
    if bytes.len() != payload_len {
        return Err(UWireError::invalid_payload(format!(
            "payload reader yielded {} bytes but payload_len returned {payload_len} bytes",
            bytes.len()
        )));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::io::{Chain, Cursor};

    use protobuf::well_known_types::wrappers::StringValue;

    #[cfg(feature = "owned-frame-transport")]
    use crate::UOwnedFrame;
    use crate::{UFrameMetadata, UFrameView, UMessageBuilder, UUri};

    use super::*;

    fn topic() -> UUri {
        UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).expect("topic")
    }

    fn message(value: &str) -> StringValue {
        StringValue {
            value: value.to_string(),
            ..Default::default()
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct VehiclePose {
        x: i32,
        y: i32,
    }

    unsafe impl StablePayload for VehiclePose {
        const TYPE_NAME: &'static str = "example.vehicle.VehiclePose";
    }

    fn stable_container_encoding(
        type_name: &str,
        variant: &str,
        size: usize,
        align: usize,
    ) -> PayloadEncoding {
        PayloadEncoding::custom(
            StableContainerPayload::<VehiclePose>::ENCODING_ID,
            format!(
                "application/vnd.uprotocol.stable-container;type=\"{type_name}\";variant={variant};size={size};align={align}"
            ),
        )
        .expect("valid custom encoding")
    }

    #[test]
    fn raw_bytes_owned_encode_decode_round_trips() {
        let payload = b"raw payload".as_slice();

        let encoded = RawBytes::encode_payload_owned(payload).expect("encode raw bytes");
        let decoded: Vec<u8> = RawBytes::decode_payload(&encoded).expect("decode raw bytes");

        assert_eq!(encoded.as_ref(), payload);
        assert_eq!(decoded, payload);
        assert_eq!(
            RawBytes::encoding(),
            PayloadEncoding::standard(UPayloadFormat::Raw)
        );
    }

    #[test]
    fn raw_bytes_rejects_too_small_output_buffer() {
        let error = RawBytes::encode_payload(b"payload".as_slice(), &mut [0_u8; 3]).unwrap_err();

        assert_eq!(
            error,
            UWireError::BufferTooSmall {
                expected: 7,
                actual: 3,
            }
        );
    }

    #[test]
    fn stable_payload_type_detail_is_used_in_encoding() {
        let detail = VehiclePose::stable_type_detail();

        assert_eq!(detail.variant, StablePayloadVariant::FixedSize);
        assert_eq!(detail.type_name, "example.vehicle.VehiclePose");
        assert_eq!(detail.size, mem::size_of::<VehiclePose>());
        assert_eq!(detail.alignment, mem::align_of::<VehiclePose>());

        let encoding = StableContainerPayload::<VehiclePose>::encoding();
        let (id, content_type) = encoding.custom_identity().expect("custom encoding");
        assert_eq!(id, StableContainerPayload::<VehiclePose>::ENCODING_ID);
        assert_eq!(
            content_type,
            "application/vnd.uprotocol.stable-container;type=\"example.vehicle.VehiclePose\";variant=fixed;size=8;align=4"
        );
    }

    #[test]
    fn stable_container_payload_info_parses_type_agnostic_metadata() {
        let info =
            StableContainerPayloadInfo::parse(&StableContainerPayload::<VehiclePose>::encoding())
                .expect("parse stable metadata");

        assert_eq!(info.type_name, "example.vehicle.VehiclePose");
        assert_eq!(info.variant, StablePayloadVariant::FixedSize);
        assert_eq!(info.size, mem::size_of::<VehiclePose>());
        assert_eq!(info.alignment, mem::align_of::<VehiclePose>());
        assert!(info.is_compatible_with::<VehiclePose>());
    }

    #[test]
    fn stable_container_payload_info_rejects_malformed_metadata() {
        let encoding = PayloadEncoding::custom(
            StableContainerPayloadInfo::ENCODING_ID,
            "application/vnd.uprotocol.stable-container;variant=fixed;size=8;align=4",
        )
        .expect("custom encoding");

        let error = StableContainerPayloadInfo::parse(&encoding).unwrap_err();

        assert!(
            matches!(error, UWireError::InvalidPayload(message) if message.contains("missing type"))
        );
    }

    #[test]
    fn stable_container_verify_accepts_larger_advertised_alignment() {
        let encoding = stable_container_encoding(
            "example.vehicle.VehiclePose",
            "fixed",
            mem::size_of::<VehiclePose>(),
            mem::align_of::<VehiclePose>() * 2,
        );

        StableContainerPayload::<VehiclePose>::verify_encoding(Some(&encoding))
            .expect("larger alignment is compatible");
    }

    #[test]
    fn stable_container_verify_rejects_incompatible_metadata() {
        let wrong_type = stable_container_encoding(
            "example.vehicle.OtherPose",
            "fixed",
            mem::size_of::<VehiclePose>(),
            mem::align_of::<VehiclePose>(),
        );
        let wrong_size = stable_container_encoding(
            "example.vehicle.VehiclePose",
            "fixed",
            mem::size_of::<VehiclePose>() + 1,
            mem::align_of::<VehiclePose>(),
        );
        let insufficient_alignment = stable_container_encoding(
            "example.vehicle.VehiclePose",
            "fixed",
            mem::size_of::<VehiclePose>(),
            mem::align_of::<VehiclePose>() / 2,
        );

        for encoding in [wrong_type, wrong_size, insufficient_alignment] {
            assert!(matches!(
                StableContainerPayload::<VehiclePose>::verify_encoding(Some(&encoding)),
                Err(UWireError::InvalidPayload(_))
            ));
        }
    }

    #[cfg(feature = "owned-frame-transport")]
    #[test]
    fn stable_container_owned_frame_preserves_bytes_and_custom_metadata() {
        let payload = Bytes::from_static(b"\x0a\x00\x00\x00\x14\x00\x00\x00");
        assert_eq!(payload.len(), mem::size_of::<VehiclePose>());
        let message = UMessageBuilder::publish(topic()).build().expect("message");
        let metadata = UFrameMetadata::new(
            message.attributes().clone(),
            Some(StableContainerPayload::<VehiclePose>::encoding()),
        )
        .expect("metadata");

        let frame = UOwnedFrame::with_payload(metadata, payload.clone()).expect("owned frame");

        assert_eq!(
            frame.metadata().payload_encoding(),
            Some(&StableContainerPayload::<VehiclePose>::encoding())
        );
        assert_eq!(frame.payload(), Some(&payload));
        assert_eq!(frame.payload_bytes(), payload.as_ref());
    }

    #[test]
    fn protobuf_payload_encode_decode_round_trips() {
        let input = message("protobuf payload");

        let encoded = ProtobufPayload::encode_payload_owned(&input).expect("encode protobuf");
        let decoded: StringValue =
            ProtobufPayload::decode_payload(&encoded).expect("decode protobuf");

        assert_eq!(decoded.value, input.value);
        assert_eq!(
            ProtobufPayload::encoding(),
            PayloadEncoding::standard(UPayloadFormat::Protobuf)
        );
    }

    #[test]
    fn protobuf_any_payload_encode_decode_round_trips() {
        let input = message("protobuf any payload");

        let encoded = ProtobufAnyPayload::encode_payload_owned(&input).expect("encode any");
        let decoded: StringValue =
            ProtobufAnyPayload::decode_payload(&encoded).expect("decode any");

        assert_eq!(decoded.value, input.value);
        assert_eq!(
            ProtobufAnyPayload::encoding(),
            PayloadEncoding::standard(UPayloadFormat::ProtobufWrappedInAny)
        );
    }

    #[test]
    fn protobuf_payload_reader_decode_round_trips() {
        let input = message("protobuf reader payload");
        let encoded = ProtobufPayload::encode_payload_owned(&input).expect("encode protobuf");

        let decoded: StringValue = ProtobufPayload::decode_payload_from_reader(
            Cursor::new(encoded.as_ref()),
            encoded.len(),
        )
        .expect("decode protobuf from reader");

        assert_eq!(decoded.value, input.value);
    }

    #[test]
    fn segmented_frame_view_decodes_from_reader_without_contiguous_payload() {
        let message = UMessageBuilder::publish(topic()).build().expect("message");
        let metadata = UFrameMetadata::new(
            message.attributes().clone(),
            Some(PayloadEncoding::standard(UPayloadFormat::Raw)),
        )
        .expect("metadata");
        let frame = SegmentedFrame {
            metadata,
            first: b"seg".to_vec(),
            second: b"mented".to_vec(),
        };

        let decoded: Vec<u8> = frame
            .decode_payload_from_reader_as::<RawBytes, _>()
            .expect("decode segmented payload");

        assert_eq!(decoded, b"segmented".as_slice());
        assert!(frame.try_contiguous_payload().is_none());
    }

    struct SegmentedFrame {
        metadata: UFrameMetadata,
        first: Vec<u8>,
        second: Vec<u8>,
    }

    impl UFrameView for SegmentedFrame {
        type PayloadReader<'a>
            = Chain<Cursor<&'a [u8]>, Cursor<&'a [u8]>>
        where
            Self: 'a;
        type PayloadSlices<'a>
            = std::array::IntoIter<&'a [u8], 2>
        where
            Self: 'a;

        fn metadata(&self) -> &UFrameMetadata {
            &self.metadata
        }

        fn payload_len(&self) -> usize {
            self.first.len() + self.second.len()
        }

        fn has_payload(&self) -> bool {
            true
        }

        fn payload_reader(&self) -> Self::PayloadReader<'_> {
            Cursor::new(self.first.as_slice()).chain(Cursor::new(self.second.as_slice()))
        }

        fn payload_slices(&self) -> Self::PayloadSlices<'_> {
            [self.first.as_slice(), self.second.as_slice()].into_iter()
        }
    }
}
