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
    any::{type_name, Any},
    collections::HashMap,
    error::Error,
    fmt::Display,
    io::Read,
    marker::PhantomData,
    mem,
    ptr::{self, NonNull},
    sync::Arc,
};

use bytes::Bytes;
pub use iceoryx2_bb_derive_macros::{PlacementDefault, ZeroCopySend};
pub use iceoryx2_bb_elementary_traits::{
    placement_default::PlacementDefault, zero_copy_send::ZeroCopySend,
};
use mediatype::ReadParams;

use crate::{UCode, UStatus};

use super::frame::{PayloadEncoding, UPayloadFormat};
use super::zero_copy::LoanedPayloadUninitMut;

const STABLE_CONTAINER_ENCODING_ID: &str = "up.stable-container";
const STABLE_CONTAINER_MEDIA_TYPE: &str = "application/vnd.uprotocol.stable-container";

/// Error type used by serialization-neutral frame helpers.
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
    /// Payload bytes are not backed by transport loan/shared-memory storage.
    NotLoanBacked,
    /// Payload storage is segmented where one contiguous payload was required.
    NotContiguous,
    /// Payload length does not match the selected layout.
    InvalidPayloadLength {
        /// Required payload length in bytes.
        expected: usize,
        /// Actual payload length in bytes.
        actual: usize,
    },
    /// Payload address does not satisfy the selected layout alignment.
    InvalidPayloadAlignment {
        /// Required payload alignment in bytes.
        expected: usize,
        /// Actual payload address.
        address: usize,
    },
    /// Stable payload type metadata does not match the selected payload type.
    IncompatibleStablePayload {
        /// Expected stable type detail.
        expected: String,
        /// Actual stable type detail or compatibility failure.
        actual: String,
    },
    /// No runtime codec is registered for the requested encoding.
    CodecNotRegistered(String),
    /// A runtime codec was invoked with the wrong value type.
    CodecTypeMismatch {
        /// Runtime codec name.
        codec: &'static str,
        /// Expected Rust type name.
        expected: &'static str,
    },
    /// A typed decode was requested, but the frame has no payload encoding.
    MissingEncoding,
    /// A typed decode was requested, but the frame has no payload bytes.
    MissingPayload,
    /// The frame's encoding is not compatible with the selected payload codec.
    UnsupportedEncoding {
        /// Encoding declared by the requested [`PayloadFormat`].
        expected: Box<PayloadEncoding>,
        /// Encoding carried by the frame being decoded.
        actual: Box<PayloadEncoding>,
    },
    /// Serializer or deserializer implementation failed.
    SerializationError(String),
}

impl UWireError {
    /// Creates a [`UWireError::BufferTooSmall`] value.
    pub fn buffer_too_small(expected: usize, actual: usize) -> Self {
        Self::BufferTooSmall { expected, actual }
    }

    /// Creates a [`UWireError::InvalidPayload`] value.
    pub fn invalid_payload(message: impl Into<String>) -> Self {
        Self::InvalidPayload(message.into())
    }

    /// Creates an [`UWireError::InvalidPayloadLength`] value.
    pub fn invalid_payload_length(expected: usize, actual: usize) -> Self {
        Self::InvalidPayloadLength { expected, actual }
    }

    /// Creates an [`UWireError::InvalidPayloadAlignment`] value.
    pub fn invalid_payload_alignment(expected: usize, address: usize) -> Self {
        Self::InvalidPayloadAlignment { expected, address }
    }

    /// Creates a [`UWireError::SerializationError`] value.
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
            Self::NotLoanBacked => f.write_str("payload is not backed by a transport loan"),
            Self::NotContiguous => f.write_str("payload is not one contiguous byte region"),
            Self::InvalidPayloadLength { expected, actual } => f.write_fmt(format_args!(
                "invalid payload length: expected {expected} bytes, got {actual} bytes"
            )),
            Self::InvalidPayloadAlignment { expected, address } => f.write_fmt(format_args!(
                "invalid payload alignment: address 0x{address:x} is not aligned to {expected} bytes"
            )),
            Self::IncompatibleStablePayload { expected, actual } => f.write_fmt(format_args!(
                "incompatible stable payload type detail: expected {expected}; got {actual}"
            )),
            Self::CodecNotRegistered(encoding) => {
                f.write_fmt(format_args!("no payload codec registered for {encoding}"))
            }
            Self::CodecTypeMismatch { codec, expected } => f.write_fmt(format_args!(
                "payload codec {codec} expected runtime value type {expected}"
            )),
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
        UStatus::fail_with_code(UCode::INVALID_ARGUMENT, value.to_string())
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

    /// Creates a layout for one stable Rust value.
    pub fn for_type<T>() -> Self {
        Self {
            len: mem::size_of::<T>(),
            align: mem::align_of::<T>(),
        }
    }

    /// Returns the payload length in bytes.
    pub fn len(self) -> usize {
        self.len
    }

    /// Returns the required payload alignment in bytes.
    pub fn align(self) -> usize {
        self.align
    }

    /// Returns whether the payload has zero bytes.
    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// Compile-time identity for an application payload codec.
///
/// ```
/// # use up_rust::{payload::PayloadFormat, PayloadEncoding};
/// struct JsonTelemetry;
///
/// impl PayloadFormat for JsonTelemetry {
///     fn name() -> &'static str {
///         "json-telemetry-v1"
///     }
///
///     fn encoding() -> PayloadEncoding {
///         PayloadEncoding::custom(
///             Self::name(),
///             "application/json",
///         )
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
///
/// `PayloadCodec` is the payload-layer abstraction for new APIs. Existing
/// [`PayloadFormat`] implementations automatically implement this trait, so
/// code using `PayloadFormat`, [`USerializer`], and [`UDeserializer`] continues
/// to work through the compatibility adapter.
pub trait PayloadCodec {
    /// Stable codec name for logs, diagnostics, and configuration.
    fn codec_name() -> &'static str;

    /// Payload encoding metadata written into frames that use this codec.
    fn payload_encoding() -> PayloadEncoding;

    /// Verifies frame encoding metadata against this codec.
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
    fn payload_layout(value: &T) -> Result<PayloadLayout, UWireError>;

    /// Encodes `value` into `dst`.
    fn encode_payload(value: &T, dst: &mut [u8]) -> Result<(), UWireError>;

    /// Encodes `value` into owned bytes.
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
    fn decode_payload(src: &'a [u8]) -> Result<T, UWireError>;
}

/// Decodes a typed value from an ordered payload byte stream.
pub trait ReadDecodePayload<T>: PayloadCodec {
    /// Decodes `T` from `reader`, which must yield exactly `payload_len` bytes.
    fn decode_payload_from_reader<R: Read>(reader: R, payload_len: usize) -> Result<T, UWireError>;
}

/// Borrows a typed view directly from contiguous payload bytes.
pub trait BorrowPayload<T: ?Sized>: PayloadCodec {
    /// Borrows `T` from payload bytes.
    fn borrow_payload(src: &[u8]) -> Result<&T, UWireError>;
}

/// Initializes and borrows a typed value directly in loaned payload storage.
///
/// # Safety
///
/// Implementors must guarantee that `loan_payload` validates the destination
/// byte range and only returns `&mut T` when the range is in one allocation,
/// uniquely borrowed for the returned lifetime, correctly aligned for `T`, and
/// contains one initialized value with a valid bit pattern for the codec's
/// transport representation. These are the reference validity, alignment,
/// aliasing, and initializedness requirements described by the Rust Reference's
/// undefined-behavior rules.
pub unsafe trait LoanPayload<T>: PayloadCodec {
    /// Returns the exact layout required for a typed transmit loan.
    fn loan_layout() -> Result<PayloadLayout, UWireError>;

    /// Initializes `dst` and returns a typed mutable view over the loaned payload.
    fn loan_payload(dst: &mut [u8]) -> Result<&mut T, UWireError>;
}

/// Initializes a typed value directly in uninitialized loaned payload storage.
///
/// # Safety
///
/// Implementors must guarantee that the returned [`LoanedUninitPayload`] covers
/// exactly the bytes that will be committed by the transport after the caller
/// returns an initialized marker. The storage must be in one allocation,
/// uniquely borrowed for the loan lifetime, correctly aligned for `T`, and
/// valid for writes of one `MaybeUninit<T>`. If the transport commits
/// `size_of::<T>()` bytes, the implementation must ensure safe initialization
/// paths cannot leave transported bytes, including padding, uninitialized; this
/// follows the `MaybeUninit<T>` initialization contract and the Rust Reference's
/// reference validity rules.
pub unsafe trait LoanUninitPayload<T>: PayloadCodec {
    /// Returns the exact layout required for a typed uninitialized transmit loan.
    fn loan_uninit_layout() -> Result<PayloadLayout, UWireError>;

    /// Validates `dst` and returns an uninitialized typed payload slot.
    fn loan_uninit_payload<'a>(
        dst: LoanedPayloadUninitMut<'a>,
    ) -> Result<LoanedUninitPayload<'a, T>, UWireError>;
}

/// Uninitialized typed payload slot borrowed from a transmit loan.
pub struct LoanedUninitPayload<'a, T> {
    ptr: NonNull<mem::MaybeUninit<T>>,
    _marker: PhantomData<&'a mut mem::MaybeUninit<T>>,
}

impl<'a, T> LoanedUninitPayload<'a, T> {
    /// Creates an uninitialized typed payload slot.
    ///
    /// This constructor is intended for codec and transport-implementation
    /// glue. Application code should obtain slots from typed send helpers.
    ///
    /// # Safety
    ///
    /// `ptr` must be non-null, valid for writes of one `MaybeUninit<T>`,
    /// correctly aligned for `T`, and backed by the transmit loan for `'a`.
    /// No other reference may read or write the same slot until this marker is
    /// consumed. Those obligations match `NonNull::as_mut`, `MaybeUninit<T>`
    /// layout, and Rust Reference aliasing/alignment requirements.
    pub unsafe fn new_unchecked(ptr: NonNull<mem::MaybeUninit<T>>) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Writes `value` into the loaned slot and marks it initialized.
    pub fn write(self, value: T) -> LoanedInitPayload<'a, T> {
        let ptr = self.ptr.as_ptr();
        // SAFETY:
        // - `self.ptr` was created from a caller-proven loan-backed pointer
        //   valid for writes of one `MaybeUninit<T>`.
        // - Per https://doc.rust-lang.org/stable/std/mem/union.MaybeUninit.html#method.write:
        //
        //   "This also returns a mutable reference to the (now safely
        //   initialized) contents of `self`."
        //
        //   We do not read through `ptr` before that write.
        unsafe { (*ptr).write(value) };

        let initialized = self.ptr.cast::<T>();
        // SAFETY:
        // - The `MaybeUninit::write` above initialized one valid `T` in the
        //   loan-backed slot.
        // - `NonNull::cast` preserves the non-null address and only changes the
        //   pointee type from `MaybeUninit<T>` to initialized `T`.
        unsafe { LoanedInitPayload::new_unchecked(initialized) }
    }

    /// Returns the raw typed pointer for field-by-field initialization.
    ///
    /// # Safety
    ///
    /// The returned pointer must not be read until a valid `T` has been fully
    /// initialized. Callers must initialize every byte required by `T`, including
    /// any explicit padding fields, before calling [`Self::assume_init`].
    #[cfg(any(
        feature = "unsafe-stable-payload-init",
        feature = "expert-unsafe-payloads"
    ))]
    pub unsafe fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr.as_ptr().cast::<T>()
    }

    /// Marks the slot initialized after custom field-by-field construction.
    ///
    /// # Safety
    ///
    /// The caller must guarantee the slot contains one fully initialized valid
    /// `T`. Calling this before full initialization is undefined behavior.
    #[cfg(any(
        feature = "unsafe-stable-payload-init",
        feature = "expert-unsafe-payloads"
    ))]
    pub unsafe fn assume_init(self) -> LoanedInitPayload<'a, T> {
        let initialized = self.ptr.cast::<T>();
        // SAFETY:
        // - The caller of this unsafe method guarantees the slot contains one
        //   fully initialized valid `T`.
        // - Per https://doc.rust-lang.org/stable/std/mem/union.MaybeUninit.html#method.assume_init:
        //
        //   "It is up to the caller to guarantee that the `MaybeUninit<T>` really
        //   is in an initialized state. Calling this when the content is not yet
        //   fully initialized causes immediate undefined behavior."
        //
        // - `NonNull::cast` preserves the non-null loan address and changes only
        //   the pointee type.
        unsafe { LoanedInitPayload::new_unchecked(initialized) }
    }
}

/// Initialized typed payload marker returned after constructing a loaned value.
pub struct LoanedInitPayload<'a, T> {
    ptr: NonNull<T>,
    _marker: PhantomData<&'a mut T>,
}

impl<'a, T> LoanedInitPayload<'a, T> {
    /// Creates an initialized typed payload marker.
    ///
    /// This constructor is intended for codec and transport-implementation
    /// glue. Application code should use safe slot initialization methods or the
    /// feature-gated unsafe stable-payload TX hatch.
    ///
    /// # Safety
    ///
    /// `ptr` must point at a valid initialized `T` borrowed from the transmit loan.
    pub unsafe fn new_unchecked(ptr: NonNull<T>) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Returns the initialized payload as a mutable reference.
    #[allow(clippy::should_implement_trait)]
    pub fn as_mut(&mut self) -> &mut T {
        // SAFETY:
        // - `self.ptr` was created only after the slot was marked initialized.
        // - `&mut self` gives exclusive access to the initialized marker, so no
        //   second mutable reference to the same loaned `T` can be produced by
        //   this API at the same time.
        unsafe { self.ptr.as_mut() }
    }
}

impl<T> AsMut<T> for LoanedInitPayload<'_, T> {
    fn as_mut(&mut self) -> &mut T {
        LoanedInitPayload::as_mut(self)
    }
}

/// Completion proof returned by generated stable payload initializers.
///
/// This token is intentionally not a typed reference into the loan. It proves
/// that a generated typestate builder reached `finish()` after initializing all
/// semantic fields and generated padding gaps. The transport helper consumes the
/// token before committing the underlying uninitialized loan.
pub struct InitializedStablePayload<T> {
    _marker: PhantomData<fn() -> T>,
}

impl<T> InitializedStablePayload<T> {
    /// Creates a completion proof after an expert initializer has fully initialized `T`.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that the corresponding loan storage contains one
    /// valid initialized `T`, including all implicit padding bytes, before this
    /// token is returned to a send helper.
    #[doc(hidden)]
    pub unsafe fn new_unchecked() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<T> From<LoanedInitPayload<'_, T>> for InitializedStablePayload<T> {
    fn from(_value: LoanedInitPayload<'_, T>) -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

/// Serializes a value into caller-provided storage.
///
/// `encoded_len` must return the number of bytes required by `serialize_into`.
/// If the supplied buffer is too small, implementations should return
/// [`UWireError::BufferTooSmall`] instead of writing a partial payload.
pub trait USerializer<F: PayloadFormat> {
    /// Required alignment for a zero-copy payload buffer.
    const ALIGNMENT: usize = 1;

    /// Returns the exact number of bytes [`Self::serialize_into`] will write.
    fn encoded_len(&self) -> usize;

    /// Serializes this value into `dst` and returns the number of bytes written.
    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError>;

    /// Serializes this value into an owned byte buffer.
    fn serialize_owned(&self) -> Result<Bytes, UWireError> {
        let expected = self.encoded_len();
        let mut bytes = vec![0_u8; expected];
        let written = self.serialize_into(&mut bytes)?;
        if written != expected {
            return Err(UWireError::invalid_payload(format!(
                "serializer wrote {written} bytes but encoded_len returned {expected} bytes"
            )));
        }
        bytes.truncate(written);
        Ok(Bytes::from(bytes))
    }
}

/// Deserializes a value from bytes.
pub trait UDeserializer<'a, F: PayloadFormat>: Sized {
    /// Decodes `Self` from one contiguous payload byte slice.
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError>;
}

/// Deserializes a value from an ordered payload byte stream.
pub trait UReadDeserializer<F: PayloadFormat>: Sized {
    /// Decodes `Self` from a reader over `payload_len` bytes.
    fn deserialize_from_reader<R: Read>(reader: R, payload_len: usize) -> Result<Self, UWireError>;
}

impl<F, T> EncodePayload<T> for F
where
    F: PayloadFormat,
    T: USerializer<F>,
{
    fn payload_layout(value: &T) -> Result<PayloadLayout, UWireError> {
        PayloadLayout::new(value.encoded_len(), T::ALIGNMENT)
    }

    fn encode_payload(value: &T, dst: &mut [u8]) -> Result<(), UWireError> {
        let expected = value.encoded_len();
        let written = value.serialize_into(dst)?;
        if written != expected {
            return Err(UWireError::invalid_payload(format!(
                "serializer wrote {written} bytes but encoded_len returned {expected} bytes"
            )));
        }
        Ok(())
    }
}

impl<'a, F, T> DecodePayload<'a, T> for F
where
    F: PayloadFormat,
    T: UDeserializer<'a, F>,
{
    fn decode_payload(src: &'a [u8]) -> Result<T, UWireError> {
        T::deserialize_from(src)
    }
}

impl<F, T> ReadDecodePayload<T> for F
where
    F: PayloadFormat,
    T: UReadDeserializer<F>,
{
    fn decode_payload_from_reader<R: Read>(reader: R, payload_len: usize) -> Result<T, UWireError> {
        T::deserialize_from_reader(reader, payload_len)
    }
}

/// Marker for codecs whose payload value is already an opaque byte sequence.
///
/// This enables owned-byte fast paths that move [`Bytes`] into frames without a
/// serialize-copy step. Use it only for codecs where the supplied bytes are
/// already the complete encoded payload for that codec.
pub trait BytePayloadCodec: PayloadCodec {}

/// Bytes that are already encoded for payload codec `C`.
///
/// This is the preferred no-extra-copy owned byte path because the codec type is
/// carried with the bytes instead of being supplied separately at the frame or
/// transport call site.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedPayload<C> {
    bytes: Bytes,
    _codec: PhantomData<fn() -> C>,
}

impl<C> EncodedPayload<C>
where
    C: PayloadCodec,
{
    /// Creates an encoded payload from bytes without validating codec-specific contents.
    pub fn from_bytes(bytes: impl Into<Bytes>) -> Self
    where
        C: BytePayloadCodec,
    {
        Self {
            bytes: bytes.into(),
            _codec: PhantomData,
        }
    }

    /// Encodes `value` into owned bytes tagged with codec `C`.
    pub fn encode<T>(value: &T) -> Result<Self, UWireError>
    where
        C: EncodePayload<T>,
        T: ?Sized,
    {
        Ok(Self {
            bytes: C::encode_payload_owned(value)?,
            _codec: PhantomData,
        })
    }

    /// Returns the payload encoding associated with `C`.
    pub fn encoding() -> PayloadEncoding {
        C::payload_encoding()
    }

    /// Returns the encoded bytes.
    pub fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// Returns the encoded payload as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }

    /// Consumes the wrapper and returns the encoded bytes.
    pub fn into_bytes(self) -> Bytes {
        self.bytes
    }
}

/// Object-safe serializer for runtime-selected codecs.
pub trait UErasedSerializer {
    /// Encoding metadata produced by this runtime-selected serializer.
    fn encoding(&self) -> PayloadEncoding;

    /// Required payload alignment for zero-copy transmit loans.
    fn alignment(&self) -> usize {
        1
    }

    /// Returns the exact number of bytes [`Self::serialize_into`] will write.
    fn encoded_len(&self) -> usize;

    /// Serializes this value into `dst` and returns the number of bytes written.
    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError>;
}

/// Runtime-visible payload codec capabilities.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PayloadCodecCapabilities {
    /// Codec can treat already encoded bytes as its payload representation.
    pub byte_payload: bool,
    /// Codec can encode owned runtime values.
    pub encode_owned: bool,
    /// Codec can decode owned runtime values.
    pub decode_owned: bool,
    /// Codec can decode from an ordered reader.
    pub read_decode: bool,
    /// Codec can borrow values from contiguous bytes.
    pub borrow_contiguous: bool,
    /// Codec can borrow values from loan-backed contiguous bytes.
    pub borrow_loaned: bool,
    /// Codec can initialize typed values directly in TX loans.
    pub loan_tx: bool,
}

/// Object-safe owned codec adapter for runtime-selected payload codecs.
///
/// The generic [`PayloadCodec`] traits remain the preferred application API.
/// This trait is for plugin/configuration layers that need to select a codec at
/// runtime and can work with owned decoded values through [`Any`]. Borrowed
/// decode and typed loaned zero-copy remain generic APIs because their lifetimes
/// and layout contracts are intentionally explicit.
pub trait DynPayloadCodec: Send + Sync {
    /// Stable codec name for logs, diagnostics, and configuration.
    fn codec_name(&self) -> &'static str;

    /// Payload encoding metadata produced and consumed by this codec.
    fn payload_encoding(&self) -> PayloadEncoding;

    /// Runtime-visible codec capabilities.
    fn capabilities(&self) -> PayloadCodecCapabilities;

    /// Returns the exact payload layout needed to encode `value`.
    fn payload_layout(&self, value: &dyn Any) -> Result<PayloadLayout, UWireError>;

    /// Encodes `value` into `dst`.
    fn encode_payload(&self, value: &dyn Any, dst: &mut [u8]) -> Result<(), UWireError>;

    /// Encodes `value` into owned bytes.
    fn encode_payload_owned(&self, value: &dyn Any) -> Result<Bytes, UWireError> {
        let layout = self.payload_layout(value)?;
        let mut bytes = vec![0_u8; layout.len()];
        self.encode_payload(value, &mut bytes)?;
        Ok(Bytes::from(bytes))
    }

    /// Decodes an owned value from contiguous payload bytes.
    fn decode_payload_owned(&self, src: &[u8]) -> Result<Box<dyn Any + Send>, UWireError>;
}

/// Object-safe wrapper for one statically typed [`PayloadCodec`].
pub struct TypedPayloadCodec<C, T> {
    capabilities: PayloadCodecCapabilities,
    _marker: PhantomData<fn() -> (C, T)>,
}

impl<C, T> TypedPayloadCodec<C, T> {
    /// Creates a typed runtime codec wrapper.
    pub const fn new() -> Self {
        Self {
            capabilities: PayloadCodecCapabilities {
                byte_payload: false,
                encode_owned: true,
                decode_owned: true,
                read_decode: false,
                borrow_contiguous: false,
                borrow_loaned: false,
                loan_tx: false,
            },
            _marker: PhantomData,
        }
    }

    /// Creates a typed runtime codec wrapper with explicit capabilities.
    pub const fn with_capabilities(capabilities: PayloadCodecCapabilities) -> Self {
        Self {
            capabilities,
            _marker: PhantomData,
        }
    }
}

impl<C, T> Default for TypedPayloadCodec<C, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C, T> DynPayloadCodec for TypedPayloadCodec<C, T>
where
    C: PayloadCodec + EncodePayload<T> + for<'a> DecodePayload<'a, T> + Send + Sync + 'static,
    T: Any + Send + Sync + 'static,
{
    fn codec_name(&self) -> &'static str {
        C::codec_name()
    }

    fn payload_encoding(&self) -> PayloadEncoding {
        C::payload_encoding()
    }

    fn capabilities(&self) -> PayloadCodecCapabilities {
        self.capabilities
    }

    fn payload_layout(&self, value: &dyn Any) -> Result<PayloadLayout, UWireError> {
        let value = value
            .downcast_ref::<T>()
            .ok_or(UWireError::CodecTypeMismatch {
                codec: C::codec_name(),
                expected: type_name::<T>(),
            })?;
        C::payload_layout(value)
    }

    fn encode_payload(&self, value: &dyn Any, dst: &mut [u8]) -> Result<(), UWireError> {
        let value = value
            .downcast_ref::<T>()
            .ok_or(UWireError::CodecTypeMismatch {
                codec: C::codec_name(),
                expected: type_name::<T>(),
            })?;
        C::encode_payload(value, dst)
    }

    fn decode_payload_owned(&self, src: &[u8]) -> Result<Box<dyn Any + Send>, UWireError> {
        C::decode_payload(src).map(|value| Box::new(value) as Box<dyn Any + Send>)
    }
}

/// Registry for runtime-selected owned payload codecs.
#[derive(Clone, Default)]
pub struct PayloadCodecRegistry {
    codecs: HashMap<PayloadEncoding, Arc<dyn DynPayloadCodec>>,
}

impl PayloadCodecRegistry {
    /// Creates an empty codec registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one statically typed codec wrapper.
    pub fn register<C, T>(&mut self) -> Option<Arc<dyn DynPayloadCodec>>
    where
        C: PayloadCodec + EncodePayload<T> + for<'a> DecodePayload<'a, T> + Send + Sync + 'static,
        T: Any + Send + Sync + 'static,
    {
        self.insert(Arc::new(TypedPayloadCodec::<C, T>::new()))
    }

    /// Registers one statically typed codec wrapper with explicit capabilities.
    pub fn register_with_capabilities<C, T>(
        &mut self,
        capabilities: PayloadCodecCapabilities,
    ) -> Option<Arc<dyn DynPayloadCodec>>
    where
        C: PayloadCodec + EncodePayload<T> + for<'a> DecodePayload<'a, T> + Send + Sync + 'static,
        T: Any + Send + Sync + 'static,
    {
        self.insert(Arc::new(TypedPayloadCodec::<C, T>::with_capabilities(
            capabilities,
        )))
    }

    /// Inserts an object-safe codec, returning any previous codec for the same
    /// encoding.
    pub fn insert(&mut self, codec: Arc<dyn DynPayloadCodec>) -> Option<Arc<dyn DynPayloadCodec>> {
        self.codecs.insert(codec.payload_encoding(), codec)
    }

    /// Gets a registered codec by payload encoding.
    pub fn get(&self, encoding: &PayloadEncoding) -> Option<Arc<dyn DynPayloadCodec>> {
        self.codecs.get(encoding).cloned()
    }

    /// Gets a registered codec compatible with `encoding`.
    ///
    /// Exact lookup remains the map semantic used by [`Self::get`]. This helper
    /// additionally applies stable-container compatibility rules: matching stable
    /// type name, `variant=fixed`, exact size, and advertised alignment greater
    /// than or equal to the locally registered codec alignment.
    pub fn get_compatible(&self, encoding: &PayloadEncoding) -> Option<Arc<dyn DynPayloadCodec>> {
        if let Some(codec) = self.get(encoding) {
            return Some(codec);
        }

        let actual = StableContainerPayloadInfo::parse(encoding).ok()?;
        self.codecs.values().find_map(|codec| {
            let expected = StableContainerPayloadInfo::parse(&codec.payload_encoding()).ok()?;
            (actual.type_name == expected.type_name
                && actual.variant == expected.variant
                && actual.size == expected.size
                && actual.alignment >= expected.alignment)
                .then(|| codec.clone())
        })
    }

    /// Gets runtime-visible capabilities for a registered codec.
    pub fn capabilities(
        &self,
        encoding: &PayloadEncoding,
    ) -> Result<PayloadCodecCapabilities, UWireError> {
        let codec = self.get_compatible(encoding).ok_or_else(|| {
            UWireError::CodecNotRegistered(format!(
                "no exact or compatible codec registered for {encoding:?}"
            ))
        })?;
        Ok(codec.capabilities())
    }

    /// Encodes a typed runtime value with the registered codec for `encoding`.
    pub fn encode_as<T>(&self, encoding: &PayloadEncoding, value: &T) -> Result<Bytes, UWireError>
    where
        T: Any + Send + Sync + 'static,
    {
        self.encode_payload_owned(encoding, value)
    }

    /// Decodes a typed runtime value with the registered codec for `encoding`.
    pub fn decode_as<T>(&self, encoding: &PayloadEncoding, src: &[u8]) -> Result<T, UWireError>
    where
        T: Any + Send + 'static,
    {
        let codec = self.get_compatible(encoding).ok_or_else(|| {
            UWireError::CodecNotRegistered(format!(
                "no exact or compatible codec registered for {encoding:?}"
            ))
        })?;
        let value = codec.decode_payload_owned(src)?;
        value
            .downcast::<T>()
            .map(|value| *value)
            .map_err(|_| UWireError::CodecTypeMismatch {
                codec: codec.codec_name(),
                expected: type_name::<T>(),
            })
    }

    /// Encodes a runtime value with the registered codec for `encoding`.
    pub fn encode_payload_owned(
        &self,
        encoding: &PayloadEncoding,
        value: &dyn Any,
    ) -> Result<Bytes, UWireError> {
        let codec = self.get(encoding).ok_or_else(|| {
            UWireError::CodecNotRegistered(format!(
                "no exact codec registered for encode request {encoding:?}"
            ))
        })?;
        codec.encode_payload_owned(value)
    }

    /// Decodes an owned runtime value with the registered codec for `encoding`.
    pub fn decode_payload_owned(
        &self,
        encoding: &PayloadEncoding,
        src: &[u8],
    ) -> Result<Box<dyn Any + Send>, UWireError> {
        let codec = self.get_compatible(encoding).ok_or_else(|| {
            UWireError::CodecNotRegistered(format!(
                "no exact or compatible codec registered for {encoding:?}"
            ))
        })?;
        codec.decode_payload_owned(src)
    }
}

/// Built-in raw byte payload codec.
pub struct RawBytes;

impl RawBytes {
    /// Returns the raw-byte payload encoding metadata.
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

impl USerializer<RawBytes> for &[u8] {
    fn encoded_len(&self) -> usize {
        self.len()
    }

    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        let actual = dst.len();
        let out = dst
            .get_mut(..self.len())
            .ok_or_else(|| UWireError::buffer_too_small(self.len(), actual))?;
        out.copy_from_slice(self);
        Ok(self.len())
    }
}

impl USerializer<RawBytes> for Bytes {
    fn encoded_len(&self) -> usize {
        self.len()
    }

    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        self.as_ref().serialize_into(dst)
    }
}

impl USerializer<RawBytes> for Vec<u8> {
    fn encoded_len(&self) -> usize {
        self.len()
    }

    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        self.as_slice().serialize_into(dst)
    }
}

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

impl<'a> UDeserializer<'a, RawBytes> for &'a [u8] {
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
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

impl BorrowPayload<[u8]> for RawBytes {
    fn borrow_payload(src: &[u8]) -> Result<&[u8], UWireError> {
        Ok(src)
    }
}

impl UReadDeserializer<RawBytes> for Vec<u8> {
    fn deserialize_from_reader<R: Read>(
        mut reader: R,
        payload_len: usize,
    ) -> Result<Self, UWireError> {
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
}

impl UReadDeserializer<RawBytes> for Bytes {
    fn deserialize_from_reader<R: Read>(reader: R, payload_len: usize) -> Result<Self, UWireError> {
        Vec::<u8>::deserialize_from_reader(reader, payload_len).map(Bytes::from)
    }
}

/// Stable payload type variant used in stable-container metadata.
///
/// Only fixed-size payload values are supported by `StableContainerPayload` in
/// this API. Runtime-length slice payloads need a separate dynamic/slice loaning
/// API and are intentionally left for future work.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StablePayloadVariant {
    /// One fixed-size `T` value with exact payload length `size_of::<T>()`.
    FixedSize,
}

impl StablePayloadVariant {
    /// Stable media-type parameter spelling for this variant.
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
    /// Cross-process and cross-language type identity.
    pub type_name: &'a str,
    /// Payload value size in bytes.
    pub size: usize,
    /// Required payload value alignment in bytes.
    pub alignment: usize,
}

/// Type-agnostic stable-container metadata parsed from a payload encoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StableContainerPayloadInfo {
    /// Cross-process and cross-language type identity.
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
    pub fn parse(encoding: &PayloadEncoding) -> Result<Self, UWireError> {
        let expected = PayloadEncoding::custom(Self::ENCODING_ID, STABLE_CONTAINER_MEDIA_TYPE);
        let Some(custom) = encoding.custom_encoding() else {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(encoding.clone()),
            });
        };
        if custom.id() != Self::ENCODING_ID {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(encoding.clone()),
            });
        }
        Self::parse_content_type(custom.content_type()).map_err(UWireError::invalid_payload)
    }

    /// Returns whether this metadata is compatible with local stable payload `T`.
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

impl Display for StableTypeDetail<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "type={};variant={};size={};align={}",
            self.type_name, self.variant, self.size, self.alignment
        )
    }
}

/// Stable zero-copy payload contract for values shared across processes or
/// languages.
///
/// `StablePayload` follows the iceoryx2 [`ZeroCopySend`] safety model. The
/// stable type name is the cross-process identity. Runtime compatibility checks
/// use type name, variant, exact size, and sufficient advertised alignment.
/// Those checks are necessary but not sufficient to prove arbitrary untrusted
/// bytes form a valid Rust value. Stable-container typed receive is therefore a
/// trusted-domain sender/receiver contract: senders must construct a valid `T`,
/// and receivers may borrow `&T` only after metadata, length, alignment, and
/// transport-loan provenance have been checked.
///
/// For untrusted inputs, validate or deserialize the bytes into an owned safe
/// representation instead of borrowing them as `T`. Future APIs may add a
/// stricter any-bit-pattern or validation-before-borrow contract for payloads
/// intended to cross untrusted boundaries.
///
/// # Safety
///
/// Implementors must guarantee that valid senders initialize a valid `Self` in
/// loaned storage and receivers may view bytes as `Self` after the
/// stable-container encoding, length, alignment, and provenance checks succeed.
pub unsafe trait StablePayload: ZeroCopySend + Sized + 'static {
    /// Stable-container variant supported by this type.
    const VARIANT: StablePayloadVariant = StablePayloadVariant::FixedSize;

    /// Whether this type's full byte representation can be initialized through
    /// byte-backed stable-container TX and encode paths.
    ///
    /// Broad `StablePayload` eligibility follows iceoryx2 `ZeroCopySend` and may
    /// include layouts with implicit padding. The byte-backed uninit path is
    /// stricter because it transports `size_of::<Self>()` bytes. Derived payloads
    /// set this to `true` only when their top-level fields exactly cover the
    /// type's size, every field is recursively byte-backed, and the type does
    /// not need drop glue. Manual impls must be conservative.
    const SUPPORTS_BYTE_BACKED_UNINIT: bool = false;

    /// Cross-process and cross-language stable type identity.
    fn stable_type_name() -> &'static str {
        // SAFETY: `StablePayload` inherits the `ZeroCopySend` safety contract;
        // implementors must provide a stable type identity for `Self`.
        unsafe { <Self as ZeroCopySend>::type_name() }
    }

    /// Returns the stable type detail expected by the codec.
    fn stable_type_detail() -> StableTypeDetail<'static> {
        StableTypeDetail {
            variant: Self::VARIANT,
            type_name: Self::stable_type_name(),
            size: mem::size_of::<Self>(),
            alignment: mem::align_of::<Self>(),
        }
    }
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

    impl<T> Sealed for T where T: super::StablePayload {}
}

/// Recursive proof that a field's bytes are initialized by safe construction.
///
/// This trait is private derive-support plumbing. It is intentionally sealed and
/// is not an application extension point; use [`ByteBackedStablePayload`] for
/// payload-level bounds.
#[doc(hidden)]
#[allow(private_bounds)]
pub trait ByteBackedStablePayloadField:
    byte_backed_stable_field_seal::Sealed + Sized + 'static
{
    #[doc(hidden)]
    const SUPPORTS_BYTE_BACKED_STABLE_FIELD: bool;
    #[doc(hidden)]
    const BYTE_BACKED_STABLE_FIELD_CHECK: ();
}

macro_rules! impl_byte_backed_stable_field {
    ($($ty:ty),* $(,)?) => {
        $(
            impl ByteBackedStablePayloadField for $ty {
                const SUPPORTS_BYTE_BACKED_STABLE_FIELD: bool = true;
                const BYTE_BACKED_STABLE_FIELD_CHECK: () = ();
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
    const BYTE_BACKED_STABLE_FIELD_CHECK: () = T::BYTE_BACKED_STABLE_FIELD_CHECK;
}

impl<T> ByteBackedStablePayloadField for T
where
    T: StablePayload,
{
    const SUPPORTS_BYTE_BACKED_STABLE_FIELD: bool =
        stable_payload_supports_byte_backed_uninit::<T>();
    const BYTE_BACKED_STABLE_FIELD_CHECK: () = {
        assert!(T::SUPPORTS_BYTE_BACKED_UNINIT);
        assert!(!mem::needs_drop::<T>());
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __up_rust_byte_backed_stable_field_supported {
    ($ty:ty) => {
        <$ty as $crate::__derive_support::ByteBackedStablePayloadField>::SUPPORTS_BYTE_BACKED_STABLE_FIELD
    };
}

/// Stronger stable payload proof for safe byte-backed TX and raw encode paths.
///
/// `StablePayload` remains broad and may include layouts with implicit padding.
/// `ByteBackedStablePayload` is the stricter boundary for APIs that transport
/// exactly `size_of::<T>()` bytes copied from, or initialized as, a Rust value.
/// Derive this trait with `#[derive(StablePayload, ByteBackedStablePayload)]`
/// when the payload has no implicit padding, does not need drop glue, and every
/// field is recursively byte-backed.
///
/// Direct manual implementations are an expert unsafe escape hatch and are
/// always available. Prefer the derive macro for hand-written payload types.
///
/// # Safety
///
/// Implementors must guarantee that copying or initializing exactly
/// `size_of::<Self>()` bytes is a sound representation of one valid `Self` for
/// stable-container TX and raw encode paths. That requires all of the following:
///
/// - The type satisfies the broader [`StablePayload`] contract for stable type
///   identity, exact size, alignment, and receive-side provenance checks.
/// - Safe construction paths initialize every byte in the object representation,
///   including implicit padding. The Rust Reference lists reading uninitialized
///   bytes as undefined behavior.
/// - Every field is recursively byte-backed, has a stable layout, and does not
///   require drop glue that could be bypassed by byte transport.
/// - The type has no invalid bit patterns that safe byte-backed TX or raw encode
///   paths can produce.
///
/// Prefer `#[derive(StablePayload, ByteBackedStablePayload)]`, which proves the
/// supported structural cases. Manual impls are for expert FFI/codegen payloads
/// with an external layout proof and a whole-object initialization strategy.
pub unsafe trait ByteBackedStablePayload: StablePayload {
    #[doc(hidden)]
    const BYTE_BACKED_STABLE_PAYLOAD_CHECK: () = {
        assert!(Self::SUPPORTS_BYTE_BACKED_UNINIT);
        assert!(!mem::needs_drop::<Self>());
    };
}

/// Returns whether `T` can use safe byte-backed stable-container TX paths.
pub const fn stable_payload_supports_byte_backed_uninit<T: StablePayload>() -> bool {
    T::SUPPORTS_BYTE_BACKED_UNINIT && !mem::needs_drop::<T>()
}

/// Compile-time assertion for byte-backed stable-container uninit eligibility.
///
/// This is primarily useful for macro UI tests and application-side static
/// checks. Runtime APIs return [`UWireError`] instead of panicking.
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
/// uninitialized transport loan storage. Generated builders expose named typed
/// setters and make `finish()` available only after all required fields are set.
/// Implementations must also initialize any implicit and trailing padding bytes
/// they own without blanket-zeroing the full payload.
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

    /// Creates a generated initializer from a transport loan's visible payload range.
    fn init_from_uninit_payload<'a>(
        payload: LoanedPayloadUninitMut<'a>,
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
    /// Creates a slot from a transport uninitialized payload view after exact
    /// stable-container length and alignment validation.
    #[doc(hidden)]
    pub fn from_uninit_payload(mut payload: LoanedPayloadUninitMut<'a>) -> Result<Self, UWireError>
    where
        T: StablePayload,
    {
        let bytes = payload.as_uninit_bytes_mut_internal();
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
    pub unsafe fn assume_init(self) -> LoanedInitPayload<'a, T> {
        // SAFETY: The generated caller invokes this only from the all-set
        // typestate state after all semantic fields and generated padding gaps
        // have been initialized.
        unsafe { LoanedInitPayload::new_unchecked(self.ptr.cast()) }
    }
}

/// Unsafe non-byte-backed stable-container TX slot.
///
/// This type is exposed only for the expert unsafe TX hatch. Safe byte-backed TX
/// should use [`LoanedUninitPayload`] through [`LoanUninitPayload`] instead.
#[cfg(any(
    feature = "unsafe-stable-payload-tx",
    feature = "expert-unsafe-payloads"
))]
pub struct UnsafeStablePayloadTxSlot<'a, T> {
    payload: LoanedPayloadUninitMut<'a>,
    _marker: PhantomData<&'a mut mem::MaybeUninit<T>>,
}

/// Zero-initialized non-byte-backed stable-container TX slot.
///
/// This is the preferred state for padded stable payloads: every transported
/// byte starts initialized to zero, and callers then use raw field writes to
/// construct a valid `T` without reintroducing uninitialized implicit padding.
#[cfg(any(
    feature = "unsafe-stable-payload-tx",
    feature = "expert-unsafe-payloads"
))]
pub struct ZeroedStablePayloadTxSlot<'a, T> {
    payload: LoanedPayloadUninitMut<'a>,
    _marker: PhantomData<&'a mut mem::MaybeUninit<T>>,
}

#[cfg(any(
    feature = "unsafe-stable-payload-tx",
    feature = "expert-unsafe-payloads"
))]
impl<'a, T> UnsafeStablePayloadTxSlot<'a, T>
where
    T: StablePayload,
{
    /// Creates an unsafe stable payload TX slot after layout validation.
    pub(crate) fn new(mut payload: LoanedPayloadUninitMut<'a>) -> Result<Self, UWireError> {
        StableContainerPayload::<T>::check_uninit_layout(payload.as_uninit_bytes_mut_internal())?;
        Ok(Self {
            payload,
            _marker: PhantomData,
        })
    }

    /// Zero-initializes every transported byte, including implicit padding.
    pub fn zeroed(mut self) -> ZeroedStablePayloadTxSlot<'a, T> {
        for slot in self.payload.as_uninit_bytes_mut_internal() {
            slot.write(0);
        }
        ZeroedStablePayloadTxSlot {
            payload: self.payload,
            _marker: PhantomData,
        }
    }

    /// Returns raw uninitialized payload bytes for custom initialization.
    ///
    /// # Safety
    ///
    /// The caller must initialize every byte before committing the loan. Prefer
    /// [`Self::zeroed`] plus typed raw field writes when using padded types.
    #[cfg(any(
        feature = "unsafe-uninit-payload-bytes",
        feature = "expert-unsafe-payloads"
    ))]
    pub unsafe fn as_uninit_bytes_mut(&mut self) -> &mut [mem::MaybeUninit<u8>] {
        self.payload.as_uninit_bytes_mut_internal()
    }

    /// Marks the slot initialized after custom byte construction.
    ///
    /// Prefer [`Self::zeroed`] for padded typed initialization. This method is
    /// only for callers that initialized every transported byte through a custom
    /// byte-oriented strategy.
    ///
    /// # Safety
    ///
    /// The caller must guarantee the full `size_of::<T>()` transported byte
    /// range, including padding, contains one valid initialized `T`.
    pub unsafe fn assume_init(mut self) -> LoanedInitPayload<'a, T> {
        let ptr: NonNull<mem::MaybeUninit<T>> = NonNull::new(
            self.payload
                .as_uninit_bytes_mut_internal()
                .as_mut_ptr()
                .cast(),
        )
        .expect("stable payload slot pointer is not null");
        // SAFETY:
        // - The caller of this unsafe method guarantees every transported byte,
        //   including padding, contains one valid initialized `T`.
        // - The pointer is derived from the retained loan after exact
        //   length/alignment checks, avoiding stale raw pointers across moves of
        //   the loan wrapper.
        unsafe { LoanedInitPayload::new_unchecked(ptr.cast()) }
    }
}

#[cfg(any(
    feature = "unsafe-stable-payload-tx",
    feature = "expert-unsafe-payloads"
))]
impl<'a, T> ZeroedStablePayloadTxSlot<'a, T>
where
    T: StablePayload,
{
    /// Returns a raw typed pointer for field-by-field initialization.
    ///
    /// # Safety
    ///
    /// The pointer must not be read until a valid `T` has been fully initialized.
    /// Callers must preserve initialization of every transported byte, including
    /// implicit padding, before calling [`Self::assume_init`].
    #[cfg(any(
        feature = "unsafe-stable-payload-init",
        feature = "expert-unsafe-payloads"
    ))]
    pub unsafe fn as_mut_ptr(&mut self) -> *mut T {
        self.payload
            .as_uninit_bytes_mut_internal()
            .as_mut_ptr()
            .cast()
    }

    /// Marks the slot initialized after custom byte/field construction.
    ///
    /// # Safety
    ///
    /// The caller must guarantee the full `size_of::<T>()` transported byte
    /// range, including padding, contains one valid initialized `T`.
    pub unsafe fn assume_init(mut self) -> LoanedInitPayload<'a, T> {
        let ptr: NonNull<mem::MaybeUninit<T>> = NonNull::new(
            self.payload
                .as_uninit_bytes_mut_internal()
                .as_mut_ptr()
                .cast(),
        )
        .expect("stable payload slot pointer is not null");
        // SAFETY:
        // - The caller of this unsafe method guarantees `zeroed()` plus any raw
        //   field writes left the full transported byte range as one valid `T`.
        // - The pointer is derived from the retained layout-checked loan at
        //   commit time, avoiding stale raw pointers across moves of the loan
        //   wrapper.
        unsafe { LoanedInitPayload::new_unchecked(ptr.cast()) }
    }
}

/// Transport-independent typed stable-container payload codec.
///
/// This codec treats the payload bytes as one exact `T` value. Transmit loans are
/// default-initialized in place before user code receives `&mut T`; receive
/// borrows require exact length, alignment, and matching [`PayloadEncoding`]
/// metadata before exposing `&T`.
pub struct StableContainerPayload<T>(PhantomData<T>);

impl<T: StablePayload> StableContainerPayload<T> {
    /// Native custom encoding id for stable-container payloads.
    pub const ENCODING_ID: &'static str = STABLE_CONTAINER_ENCODING_ID;

    const MEDIA_TYPE: &'static str = STABLE_CONTAINER_MEDIA_TYPE;

    /// Returns the stable-container payload encoding for `T`.
    pub fn encoding() -> PayloadEncoding
    where
        T: StablePayload,
    {
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

    fn check_layout(src: &[u8]) -> Result<(), UWireError> {
        Self::check_len(src.len(), mem::size_of::<T>())?;

        let address = src.as_ptr() as usize;
        let alignment = mem::align_of::<T>();
        if !address.is_multiple_of(alignment) {
            return Err(UWireError::invalid_payload_alignment(alignment, address));
        }
        Ok(())
    }

    fn check_layout_mut(dst: &mut [u8]) -> Result<(), UWireError> {
        Self::check_layout(dst)
    }

    fn check_uninit_layout(dst: &mut [mem::MaybeUninit<u8>]) -> Result<(), UWireError> {
        Self::check_len(dst.len(), mem::size_of::<T>())?;

        let address = dst.as_ptr() as usize;
        let alignment = mem::align_of::<T>();
        if !address.is_multiple_of(alignment) {
            return Err(UWireError::invalid_payload_alignment(alignment, address));
        }
        Ok(())
    }

    fn check_len(actual: usize, expected: usize) -> Result<(), UWireError> {
        if actual != expected {
            return Err(UWireError::invalid_payload_length(expected, actual));
        }
        Ok(())
    }

    fn check_byte_backed_uninit() -> Result<(), UWireError> {
        if !stable_payload_supports_byte_backed_uninit::<T>() {
            return Err(UWireError::invalid_payload(format!(
                "stable payload {} has implicit padding, needs drop glue, or has unknown byte initialization and cannot use safe byte-backed stable-container TX",
                T::stable_type_name()
            )));
        }
        Ok(())
    }

    pub(crate) fn borrow_checked_payload(src: &[u8]) -> Result<&T, UWireError> {
        Self::check_layout(src)?;
        // SAFETY:
        // - `check_layout` verified that `src.len() == size_of::<T>()` and that
        //   `src.as_ptr()` satisfies `align_of::<T>()`.
        // - The caller reached this method through the loan-backed receive API,
        //   which ties the reference lifetime to the RX lease.
        // - `T: StablePayload` is the unsafe contract that stable-container
        //   metadata plus the transport's loan-backed receive path represents one
        //   valid initialized `T`.
        Ok(unsafe { &*src.as_ptr().cast::<T>() })
    }
}

impl<T> StableContainerPayload<T>
where
    T: ByteBackedStablePayload,
{
    fn assert_byte_backed() {
        assert!(T::SUPPORTS_BYTE_BACKED_UNINIT);
        assert!(!mem::needs_drop::<T>());
        #[allow(path_statements)]
        T::BYTE_BACKED_STABLE_PAYLOAD_CHECK;
    }

    fn check_byte_backed() -> Result<(), UWireError> {
        Self::assert_byte_backed();
        Self::check_byte_backed_uninit()
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
    }

    fn verify_encoding(actual: Option<&PayloadEncoding>) -> Result<(), UWireError> {
        let expected = Self::payload_encoding();
        let actual = actual.ok_or(UWireError::MissingEncoding)?;
        let Some(custom) = actual.custom_encoding() else {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(actual.clone()),
            });
        };
        if custom.id() != Self::ENCODING_ID {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(actual.clone()),
            });
        }
        Self::verify_content_type(custom.content_type()).map_err(|reason| {
            UWireError::IncompatibleStablePayload {
                expected: T::stable_type_detail().to_string(),
                actual: format!("{} ({reason})", custom.content_type()),
            }
        })
    }
}

impl<T> EncodePayload<T> for StableContainerPayload<T>
where
    T: ByteBackedStablePayload,
{
    fn payload_layout(_value: &T) -> Result<PayloadLayout, UWireError> {
        Self::check_byte_backed()?;
        PayloadLayout::new(mem::size_of::<T>(), mem::align_of::<T>())
    }

    fn encode_payload(value: &T, dst: &mut [u8]) -> Result<(), UWireError> {
        Self::check_byte_backed()?;
        Self::check_len(dst.len(), mem::size_of::<T>())?;
        // SAFETY:
        // - `value` is a valid shared reference to one `T`, so its address is
        //   non-null, aligned, and valid for reads of `size_of::<T>()` bytes.
        // - `T: ByteBackedStablePayload` proves every byte in the transported
        //   `size_of::<T>()` representation is initialized by safe construction.
        // - Per stable `slice::from_raw_parts` docs: "data must be non-null,
        //   valid for reads for `len * size_of::<T>()` many bytes, and it must
        //   be properly aligned" and "data must point to `len` consecutive
        //   properly initialized values of type `T`". Here the slice element is
        //   `u8`, `len` is `size_of::<T>()`, and the memory is contained in the
        //   single allocation occupied by `value`.
        let src = unsafe {
            std::slice::from_raw_parts((value as *const T).cast::<u8>(), mem::size_of::<T>())
        };
        dst.copy_from_slice(src);
        Ok(())
    }
}

// SAFETY:
// - `loan_payload` checks byte-backed eligibility, exact length, and alignment
//   before casting the destination to `*mut T`.
// - The destination is filled with zeros before `PlacementDefault` runs, so
//   byte-backed payload padding remains initialized.
// - `T: ByteBackedStablePayload + PlacementDefault` provides the initialized
//   byte representation and placement-construction contracts used below.
unsafe impl<T> LoanPayload<T> for StableContainerPayload<T>
where
    T: ByteBackedStablePayload + PlacementDefault,
{
    fn loan_layout() -> Result<PayloadLayout, UWireError> {
        Self::check_byte_backed()?;
        PayloadLayout::new(mem::size_of::<T>(), mem::align_of::<T>())
    }

    fn loan_payload(dst: &mut [u8]) -> Result<&mut T, UWireError> {
        Self::check_byte_backed()?;
        Self::check_layout_mut(dst)?;
        dst.fill(0);
        let ptr = dst.as_mut_ptr().cast::<T>();
        // SAFETY:
        // - `check_layout_mut` verified exact length and `T` alignment for the
        //   destination slice.
        // - `dst.fill(0)` initialized every transported byte before placement
        //   construction, including bytes that would otherwise be padding.
        // - `PlacementDefault::placement_default` initializes one valid `T` at
        //   `ptr`, and `&mut [u8]` gives exclusive access to the storage.
        unsafe { T::placement_default(ptr) };
        // SAFETY:
        // - The placement construction above initialized one valid `T` at `ptr`.
        // - `check_layout_mut` verified the exact length and alignment, and the
        //   mutable slice borrow is still the unique access path for this loan.
        // - Per https://doc.rust-lang.org/reference/behavior-considered-undefined.html,
        //   producing a reference requires a non-null, aligned, initialized
        //   pointee; those requirements were established before this cast.
        Ok(unsafe { &mut *ptr })
    }
}

// SAFETY:
// - `loan_uninit_payload` checks byte-backed eligibility and exact uninit
//   length/alignment before constructing a typed uninit slot.
// - `T: ByteBackedStablePayload` is the proof required by safe uninit TX paths
//   that committing `size_of::<T>()` bytes cannot expose uninitialized padding.
unsafe impl<T> LoanUninitPayload<T> for StableContainerPayload<T>
where
    T: ByteBackedStablePayload,
{
    fn loan_uninit_layout() -> Result<PayloadLayout, UWireError> {
        Self::check_byte_backed()?;
        PayloadLayout::new(mem::size_of::<T>(), mem::align_of::<T>())
    }

    fn loan_uninit_payload<'a>(
        mut dst: LoanedPayloadUninitMut<'a>,
    ) -> Result<LoanedUninitPayload<'a, T>, UWireError> {
        Self::check_byte_backed()?;
        let bytes = dst.as_uninit_bytes_mut_internal();
        Self::check_uninit_layout(bytes)?;
        let ptr = NonNull::new(bytes.as_mut_ptr().cast::<mem::MaybeUninit<T>>())
            .ok_or_else(|| UWireError::invalid_payload("stable payload slot pointer is null"))?;
        // SAFETY:
        // - `check_uninit_layout` verified that the byte slice has exactly
        //   `size_of::<T>()` bytes and satisfies `align_of::<T>()`.
        // - Per stable `MaybeUninit<T>` layout docs, `MaybeUninit<T>` has the
        //   same size and alignment as `T`, so the checked byte range can back
        //   one `MaybeUninit<T>`.
        // - The slice came from `LoanedPayloadUninitMut`, preserving loan
        //   provenance and exclusive mutable access for `'a`.
        Ok(unsafe { LoanedUninitPayload::new_unchecked(ptr) })
    }
}

/// MCAP archive payload codec.
///
/// MCAP carries schema and channel identity inside the archive bytes. This codec
/// only declares the `application/mcap` payload representation and supports
/// byte encode/decode plus borrowed `&[u8]`; it does not provide typed in-place
/// payload construction.
pub struct McapPayload;

impl McapPayload {
    /// Returns the MCAP payload encoding metadata.
    pub fn encoding() -> PayloadEncoding {
        <Self as PayloadCodec>::payload_encoding()
    }
}

impl PayloadCodec for McapPayload {
    fn codec_name() -> &'static str {
        "mcap"
    }

    fn payload_encoding() -> PayloadEncoding {
        PayloadEncoding::custom("application/mcap", "application/mcap")
    }
}

impl BytePayloadCodec for McapPayload {}

impl EncodePayload<[u8]> for McapPayload {
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

impl<'a> DecodePayload<'a, &'a [u8]> for McapPayload {
    fn decode_payload(src: &'a [u8]) -> Result<&'a [u8], UWireError> {
        Ok(src)
    }
}

impl<'a> DecodePayload<'a, Vec<u8>> for McapPayload {
    fn decode_payload(src: &'a [u8]) -> Result<Vec<u8>, UWireError> {
        Ok(src.to_vec())
    }
}

impl<'a> DecodePayload<'a, Bytes> for McapPayload {
    fn decode_payload(src: &'a [u8]) -> Result<Bytes, UWireError> {
        Ok(Bytes::copy_from_slice(src))
    }
}

impl ReadDecodePayload<Vec<u8>> for McapPayload {
    fn decode_payload_from_reader<R: Read>(
        mut reader: R,
        payload_len: usize,
    ) -> Result<Vec<u8>, UWireError> {
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
}

impl ReadDecodePayload<Bytes> for McapPayload {
    fn decode_payload_from_reader<R: Read>(
        reader: R,
        payload_len: usize,
    ) -> Result<Bytes, UWireError> {
        <Self as ReadDecodePayload<Vec<u8>>>::decode_payload_from_reader(reader, payload_len)
            .map(Bytes::from)
    }
}

impl BorrowPayload<[u8]> for McapPayload {
    fn borrow_payload(src: &[u8]) -> Result<&[u8], UWireError> {
        Ok(src)
    }
}
