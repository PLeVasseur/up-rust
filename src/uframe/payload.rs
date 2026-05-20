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

use std::{error::Error, fmt::Display, io::Read};

use bytes::Bytes;

use crate::{UCode, UStatus};

use super::frame::UEncoding;

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
    /// A typed decode was requested, but the frame has no payload encoding.
    MissingEncoding,
    /// A typed decode was requested, but the frame has no payload bytes.
    MissingPayload,
    /// The frame's encoding is not compatible with the selected payload codec.
    UnsupportedEncoding {
        /// Encoding declared by the requested [`PayloadFormat`].
        expected: Box<UEncoding>,
        /// Encoding carried by the frame being decoded.
        actual: Box<UEncoding>,
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
            Self::InvalidPayload(message) => f.write_fmt(format_args!("invalid payload: {message}")),
            Self::MissingEncoding => f.write_str("frame payload has no encoding metadata"),
            Self::MissingPayload => f.write_str("frame has no payload"),
            Self::UnsupportedEncoding { expected, actual } => f.write_fmt(format_args!(
                "unsupported encoding: expected format_id={}, content_type={}, schema_ref={:?}; got format_id={}, content_type={}, schema_ref={:?}",
                expected.format_id(),
                expected.content_type(),
                expected.schema_ref(),
                actual.format_id(),
                actual.content_type(),
                actual.schema_ref(),
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

/// Compile-time identity for an application payload codec.
///
/// ```
/// # use up_rust::{payload::PayloadFormat, UEncoding};
/// struct JsonTelemetry;
///
/// impl PayloadFormat for JsonTelemetry {
///     fn name() -> &'static str {
///         "json-telemetry-v1"
///     }
///
///     fn encoding() -> UEncoding {
///         UEncoding::with_schema_ref(
///             Self::name(),
///             "application/json",
///             "urn:example:Telemetry:v1",
///         )
///     }
/// }
/// ```
pub trait PayloadFormat {
    /// Stable codec name for logs, diagnostics, and configuration.
    fn name() -> &'static str;

    /// Payload encoding metadata written into frames that use this codec.
    fn encoding() -> UEncoding;
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

/// Object-safe serializer for runtime-selected codecs.
pub trait UErasedSerializer {
    /// Encoding metadata produced by this runtime-selected serializer.
    fn encoding(&self) -> UEncoding;

    /// Required payload alignment for zero-copy transmit loans.
    fn alignment(&self) -> usize {
        1
    }

    /// Returns the exact number of bytes [`Self::serialize_into`] will write.
    fn encoded_len(&self) -> usize;

    /// Serializes this value into `dst` and returns the number of bytes written.
    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError>;
}

/// Built-in raw byte payload codec.
pub struct RawBytes;

impl RawBytes {
    /// Returns the raw-byte payload encoding metadata.
    pub fn encoding() -> UEncoding {
        <Self as PayloadFormat>::encoding()
    }
}

impl PayloadFormat for RawBytes {
    fn name() -> &'static str {
        "raw-bytes"
    }

    fn encoding() -> UEncoding {
        UEncoding::without_schema_ref("raw-bytes", "application/octet-stream")
    }
}

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

impl<'a> UDeserializer<'a, RawBytes> for &'a [u8] {
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
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
