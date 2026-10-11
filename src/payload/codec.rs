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

//! Payload codec contracts for sizing, encoding and bounded decoding.
//!
//! Codecs describe their encoding identity and byte layout. Transport bindings
//! own storage and delivery; callers retain the distinction between absent and
//! present-empty payloads. Decoding validates the declared encoding before
//! interpreting bytes, and bounded reads reject short or overlong input.

use std::io::Read;

use bytes::Bytes;

#[cfg(feature = "protobuf-support")]
use crate::ProtobufMappable;
use crate::{
    NativePayloadIdentity, NativeProfileAgreement, NativeTypeToken, PayloadEncoding, UFrameMetadata,
};

use super::UWireError;

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

/// Fixed encoding identity for an application payload codec.
/// Native codecs resolve deployment context through [`PayloadCodec`].
///
/// ```rust
/// use up_rust::{payload::codec::PayloadCodecIdentity, PayloadEncoding};
///
/// struct JsonTelemetry;
///
/// impl PayloadCodecIdentity for JsonTelemetry {
///     fn name() -> &'static str {
///         "json-telemetry-v1"
///     }
///
///     fn encoding() -> PayloadEncoding {
///         PayloadEncoding::JSON
///     }
/// }
/// ```
pub trait PayloadCodecIdentity {
    /// Stable codec name for logs, diagnostics, and configuration.
    fn name() -> &'static str;

    /// Payload encoding metadata written into frames that use this codec.
    fn encoding() -> PayloadEncoding;
}

/// Explicit maximum encoded payload length accepted from a reader.
///
/// This bounds encoded input, not allocations needed to construct decoded values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PayloadDecodeLimit(usize);

impl PayloadDecodeLimit {
    /// Creates an encoded-input limit, including zero for empty payloads.
    pub const fn new(max_payload_bytes: usize) -> Self {
        Self(max_payload_bytes)
    }

    /// Returns the maximum accepted encoded payload length.
    pub const fn max_payload_bytes(self) -> usize {
        self.0
    }
}

/// Codec identity resolved under an explicit deployment agreement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadIdentity {
    /// A fixed encoding without a native representation claim.
    Fixed(PayloadEncoding),
    /// A native representation resolved from the agreed table or operation contract.
    Native(NativePayloadIdentity),
}

impl PayloadIdentity {
    /// Returns the payload encoding, including explicit ID zero.
    pub fn encoding(self) -> PayloadEncoding {
        match self {
            Self::Fixed(encoding) => encoding,
            Self::Native(identity) => identity.encoding(),
        }
    }

    /// Returns the separate structural token for a native representation.
    pub fn native_type_token(self) -> Option<NativeTypeToken> {
        match self {
            Self::Fixed(_) => None,
            Self::Native(identity) => Some(identity.token()),
        }
    }

    /// Checks both carried identity components before typed payload access.
    ///
    /// # Errors
    ///
    /// Rejects missing/foreign encodings or missing, foreign or unexpected tokens.
    pub fn verify(
        self,
        encoding: Option<&PayloadEncoding>,
        token: Option<NativeTypeToken>,
    ) -> Result<(), UWireError> {
        let expected = self.encoding();
        let actual = encoding.ok_or(UWireError::MissingEncoding)?;
        if actual != &expected {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(*actual),
            });
        }
        if token != self.native_type_token() {
            return Err(UWireError::UnsupportedNativeTypeToken {
                expected: self.native_type_token(),
                actual: token,
            });
        }
        Ok(())
    }

    /// Sets the resolved identity of a newly encoded payload.
    ///
    /// # Errors
    ///
    /// Rejects invalid metadata or a fixed encoding replacing a carried native claim.
    pub fn apply_to(self, metadata: UFrameMetadata) -> Result<UFrameMetadata, UWireError> {
        let result = match self {
            Self::Fixed(encoding) => {
                if metadata.native_type_token().is_some() {
                    return Err(UWireError::invalid_payload(
                        "fixed codec cannot retain a native type token",
                    ));
                }
                metadata.with_payload_encoding(encoding)
            }
            Self::Native(identity) => metadata.with_native_payload_identity(identity),
        };
        result.map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

/// Payload-layer codec identity used by typed frame helpers.
pub trait PayloadCodec {
    /// Stable codec name for logs, diagnostics, and configuration.
    fn codec_name() -> &'static str;

    /// Resolves identity under the caller's scoped deployment agreement.
    ///
    /// Fixed codecs may ignore the agreement. Native codecs require it and resolve
    /// the complete representation, never hash or truncate a payload ID.
    ///
    /// # Errors
    ///
    /// Rejects missing agreement or an unagreed native representation.
    fn payload_identity(
        agreement: Option<&NativeProfileAgreement>,
    ) -> Result<PayloadIdentity, UWireError>;

    /// Resolves the encoding component of this codec's identity.
    ///
    /// # Errors
    ///
    /// Returns the context/membership errors from [`Self::payload_identity`].
    fn payload_encoding(
        agreement: Option<&NativeProfileAgreement>,
    ) -> Result<PayloadEncoding, UWireError> {
        Self::payload_identity(agreement).map(PayloadIdentity::encoding)
    }

    /// Verifies the frame's complete payload identity against this codec.
    ///
    /// # Errors
    ///
    /// Rejects absent or incompatible encoding/token metadata and invalid agreement.
    fn verify_metadata(
        metadata: &UFrameMetadata,
        agreement: Option<&NativeProfileAgreement>,
    ) -> Result<(), UWireError> {
        Self::payload_identity(agreement)?
            .verify(metadata.payload_encoding(), metadata.native_type_token())
    }
}

impl<F> PayloadCodec for F
where
    F: PayloadCodecIdentity,
{
    fn codec_name() -> &'static str {
        <F as PayloadCodecIdentity>::name()
    }

    fn payload_identity(
        _agreement: Option<&NativeProfileAgreement>,
    ) -> Result<PayloadIdentity, UWireError> {
        Ok(PayloadIdentity::Fixed(
            <F as PayloadCodecIdentity>::encoding(),
        ))
    }
}

/// Encodes a typed value with a [`PayloadCodec`].
#[diagnostic::on_unimplemented(
    message = "the payload codec `{Self}` cannot encode `{T}`",
    label = "no `EncodePayload<{T}>` implementation",
    note = "wire implementers provide `payload_layout` and `encode_payload`; see the wire-implementer walkthrough"
)]
pub trait EncodePayload<T: ?Sized>: PayloadCodec {
    /// Returns the exact payload layout required to encode `value`.
    ///
    /// This is a measurement operation, not necessarily a constant-time size
    /// lookup. A variable-length codec may serialize or traverse `value` here
    /// and perform the work again in [`Self::encode_payload`]. Benchmarks and
    /// callers that care about this cost should label the probe and write phases
    /// separately. The returned length and alignment are the complete contract
    /// for the destination passed to `encode_payload`.
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
    /// Implementations must treat `payload_len` as a finite allocation and read
    /// bound before reserving payload-sized storage. They must consume exactly
    /// that many bytes, distinguish an early EOF from malformed payload data,
    /// and check for an additional byte so overlong input is rejected. Reader
    /// errors and malformed input must return [`UWireError`], not panic. A codec
    /// may enforce a lower implementation-specific maximum, but must report that
    /// limit rather than allocating first and rejecting later.
    ///
    /// # Errors
    ///
    /// Returns an error if the reader fails, yields an unexpected byte count, or
    /// contains malformed payload bytes for this codec.
    fn decode_payload_from_reader<R: Read>(
        reader: R,
        payload_len: usize,
        limit: PayloadDecodeLimit,
    ) -> Result<T, UWireError>;
}

/// Built-in raw byte payload codec.
#[derive(Debug)]
pub struct RawBytes;

impl RawBytes {
    /// Returns the raw-byte payload encoding metadata.
    #[must_use]
    pub fn encoding() -> PayloadEncoding {
        <Self as PayloadCodecIdentity>::encoding()
    }
}

impl PayloadCodecIdentity for RawBytes {
    fn name() -> &'static str {
        "raw-bytes"
    }

    fn encoding() -> PayloadEncoding {
        PayloadEncoding::RAW
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

impl<'a> DecodePayload<'a, &'a [u8]> for RawBytes {
    fn decode_payload(src: &'a [u8]) -> Result<&'a [u8], UWireError> {
        Ok(src)
    }
}

impl DecodePayload<'_, Vec<u8>> for RawBytes {
    fn decode_payload(src: &[u8]) -> Result<Vec<u8>, UWireError> {
        Ok(src.to_vec())
    }
}

impl DecodePayload<'_, Bytes> for RawBytes {
    fn decode_payload(src: &[u8]) -> Result<Bytes, UWireError> {
        Ok(Bytes::copy_from_slice(src))
    }
}

impl ReadDecodePayload<Vec<u8>> for RawBytes {
    fn decode_payload_from_reader<R: Read>(
        reader: R,
        payload_len: usize,
        limit: PayloadDecodeLimit,
    ) -> Result<Vec<u8>, UWireError> {
        read_exact_payload(reader, payload_len, limit)
    }
}

impl ReadDecodePayload<Bytes> for RawBytes {
    fn decode_payload_from_reader<R: Read>(
        reader: R,
        payload_len: usize,
        limit: PayloadDecodeLimit,
    ) -> Result<Bytes, UWireError> {
        read_exact_payload(reader, payload_len, limit).map(Bytes::from)
    }
}

/// Protocol Buffers application payload codec.
///
/// `ProtobufPayload` serializes and deserializes only application payload bytes.
/// It does not wrap a complete uProtocol frame and does not serialize frame
/// metadata.
#[derive(Debug)]
pub struct ProtobufPayload;

impl ProtobufPayload {
    /// Returns the generic Protocol Buffers payload encoding metadata.
    #[must_use]
    pub fn encoding() -> PayloadEncoding {
        <Self as PayloadCodecIdentity>::encoding()
    }
}

impl PayloadCodecIdentity for ProtobufPayload {
    fn name() -> &'static str {
        "protobuf"
    }

    fn encoding() -> PayloadEncoding {
        PayloadEncoding::PROTOBUF
    }
}

#[cfg(feature = "protobuf-support")]
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

#[cfg(feature = "protobuf-support")]
impl<'a, T> DecodePayload<'a, T> for ProtobufPayload
where
    T: ProtobufMappable,
{
    fn decode_payload(src: &'a [u8]) -> Result<T, UWireError> {
        T::parse_from_protobuf_bytes(src)
            .map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

#[cfg(feature = "protobuf-support")]
impl<T> ReadDecodePayload<T> for ProtobufPayload
where
    T: ProtobufMappable,
{
    fn decode_payload_from_reader<R: Read>(
        reader: R,
        payload_len: usize,
        limit: PayloadDecodeLimit,
    ) -> Result<T, UWireError> {
        let bytes = read_exact_payload(reader, payload_len, limit)?;
        T::parse_from_protobuf_bytes(&bytes)
            .map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

/// Protocol Buffers `google.protobuf.Any` application payload codec.
#[derive(Debug)]
pub struct ProtobufAnyPayload;

impl ProtobufAnyPayload {
    /// Returns the protobuf-Any payload encoding metadata.
    #[must_use]
    pub fn encoding() -> PayloadEncoding {
        <Self as PayloadCodecIdentity>::encoding()
    }
}

impl PayloadCodecIdentity for ProtobufAnyPayload {
    fn name() -> &'static str {
        "protobuf-any"
    }

    fn encoding() -> PayloadEncoding {
        PayloadEncoding::PROTOBUF_WRAPPED_IN_ANY
    }
}

#[cfg(feature = "protobuf-support")]
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

#[cfg(feature = "protobuf-support")]
impl<'a, T> DecodePayload<'a, T> for ProtobufAnyPayload
where
    T: ProtobufMappable,
{
    fn decode_payload(src: &'a [u8]) -> Result<T, UWireError> {
        T::parse_from_packed_protobuf_bytes(src)
            .map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

#[cfg(feature = "protobuf-support")]
impl<T> ReadDecodePayload<T> for ProtobufAnyPayload
where
    T: ProtobufMappable,
{
    fn decode_payload_from_reader<R: Read>(
        reader: R,
        payload_len: usize,
        limit: PayloadDecodeLimit,
    ) -> Result<T, UWireError> {
        let bytes = read_exact_payload(reader, payload_len, limit)?;
        T::parse_from_packed_protobuf_bytes(&bytes)
            .map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

#[cfg(feature = "protobuf-support")]
fn copy_encoded_payload(bytes: Bytes, dst: &mut [u8]) -> Result<(), UWireError> {
    let actual = dst.len();
    let out = dst
        .get_mut(..bytes.len())
        .ok_or_else(|| UWireError::buffer_too_small(bytes.len(), actual))?;
    out.copy_from_slice(&bytes);
    Ok(())
}

fn read_exact_payload<R: Read>(
    reader: R,
    payload_len: usize,
    limit: PayloadDecodeLimit,
) -> Result<Vec<u8>, UWireError> {
    if payload_len > limit.max_payload_bytes() {
        return Err(UWireError::invalid_payload(format!(
            "advertised payload length {payload_len} exceeds configured input limit {}",
            limit.max_payload_bytes()
        )));
    }
    let reader_limit = payload_len
        .checked_add(1)
        .and_then(|length| u64::try_from(length).ok())
        .ok_or_else(|| UWireError::invalid_payload("payload length cannot be bounded"))?;
    let mut reader = reader.take(reader_limit);
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let count = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(UWireError::invalid_payload(error.to_string())),
        };
        let data = chunk
            .get(..count)
            .ok_or_else(|| UWireError::invalid_payload("reader returned an invalid byte count"))?;
        bytes.try_reserve(count).map_err(|error| {
            UWireError::invalid_payload(format!("payload allocation failed: {error}"))
        })?;
        bytes.extend_from_slice(data);
    }
    match bytes.len().cmp(&payload_len) {
        core::cmp::Ordering::Less => Err(UWireError::invalid_payload(format!(
            "payload reader ended early: expected {payload_len} bytes, got {}",
            bytes.len()
        ))),
        core::cmp::Ordering::Equal => Ok(bytes),
        core::cmp::Ordering::Greater => Err(UWireError::invalid_payload(format!(
            "payload reader yielded more than advertised {payload_len} bytes"
        ))),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "protobuf-support")]
    use protobuf::well_known_types::wrappers::StringValue;

    use super::*;

    #[test_case::test_case(b"ab", 3, 3, "ended early"; "early EOF")]
    #[test_case::test_case(b"abcd", 3, 3, "more than advertised"; "overrun")]
    #[test_case::test_case(b"abcd", 4, 3, "exceeds configured input limit"; "policy limit")]
    fn bounded_reader_rejects_invalid_lengths(
        bytes: &[u8],
        length: usize,
        maximum: usize,
        reason: &str,
    ) {
        assert!(
            matches!(<RawBytes as ReadDecodePayload<Vec<u8>>>::decode_payload_from_reader(bytes, length, PayloadDecodeLimit::new(maximum)),
            Err(UWireError::InvalidPayload(message)) if message.contains(reason))
        );
    }

    #[test]
    fn bounded_reader_accepts_explicit_zero_and_exact_limits() {
        let empty: Vec<u8> =
            RawBytes::decode_payload_from_reader(std::io::empty(), 0, PayloadDecodeLimit::new(0))
                .unwrap();
        assert!(empty.is_empty());
        let exact: Vec<u8> =
            RawBytes::decode_payload_from_reader(&b"abc"[..], 3, PayloadDecodeLimit::new(3))
                .unwrap();
        assert_eq!(exact, b"abc");
    }

    #[test]
    fn reader_does_not_run_past_declared_payload_boundary() {
        struct Probe<'a>(&'a mut usize);
        impl Read for Probe<'_> {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                assert!(buffer.len() <= 3, "reader requested beyond length plus one");
                *self.0 += buffer.len();
                assert!(*self.0 <= 3, "reader consumed beyond length plus one");
                buffer.fill(1);
                Ok(buffer.len())
            }
        }
        let mut consumed = 0;
        assert!(matches!(
            read_exact_payload(Probe(&mut consumed), 2, PayloadDecodeLimit::new(2)),
            Err(UWireError::InvalidPayload(_))
        ));
        assert_eq!(consumed, 3);
        assert!(matches!(
            read_exact_payload(std::io::empty(), 2, PayloadDecodeLimit::new(2)),
            Err(UWireError::InvalidPayload(_))
        ));
        assert_eq!(
            read_exact_payload(&b"ok"[..], 2, PayloadDecodeLimit::new(2)).unwrap(),
            b"ok"
        );
    }

    #[test]
    fn unbounded_declared_length_is_a_typed_error() {
        assert!(matches!(
            read_exact_payload(
                std::io::repeat(1),
                usize::MAX,
                PayloadDecodeLimit::new(usize::MAX)
            ),
            Err(UWireError::InvalidPayload(_))
        ));
    }

    #[cfg(feature = "protobuf-support")]
    fn message(value: &str) -> StringValue {
        StringValue {
            value: value.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn raw_bytes_owned_encode_decode_round_trips() {
        let payload = b"raw payload".as_slice();

        let encoded = RawBytes::encode_payload_owned(payload).expect("encode raw bytes");
        let decoded: Vec<u8> = RawBytes::decode_payload(&encoded).expect("decode raw bytes");

        assert_eq!(encoded.as_ref(), payload);
        assert_eq!(decoded, payload);
        assert_eq!(RawBytes::encoding(), PayloadEncoding::RAW);
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

    #[cfg(feature = "protobuf-support")]
    #[test]
    fn protobuf_payload_encode_decode_round_trips() {
        let input = message("protobuf payload");

        let encoded = ProtobufPayload::encode_payload_owned(&input).expect("encode protobuf");
        let decoded: StringValue =
            ProtobufPayload::decode_payload(&encoded).expect("decode protobuf");

        assert_eq!(decoded.value, input.value);
        assert_eq!(ProtobufPayload::encoding(), PayloadEncoding::PROTOBUF);
    }

    #[cfg(feature = "protobuf-support")]
    #[test]
    fn protobuf_any_payload_encode_decode_round_trips() {
        let input = message("protobuf any payload");

        let encoded = ProtobufAnyPayload::encode_payload_owned(&input).expect("encode any");
        let decoded: StringValue =
            ProtobufAnyPayload::decode_payload(&encoded).expect("decode any");

        assert_eq!(decoded.value, input.value);
        assert_eq!(
            ProtobufAnyPayload::encoding(),
            PayloadEncoding::PROTOBUF_WRAPPED_IN_ANY
        );
    }
}
