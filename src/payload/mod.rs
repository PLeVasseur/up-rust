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

use std::{error::Error, fmt::Display};

#[cfg(feature = "native-identity")]
use crate::NativeProfileError;
use crate::{NativeTypeToken, PayloadEncoding, UCode, UStatus};

/// Payload codec traits: measure, encode, and decode typed payloads.
#[cfg(feature = "native-identity")]
pub mod codec;
#[cfg(feature = "zero-copy-transport")]
/// Checked typed borrowing contracts for contiguous payload bytes.
pub mod loan;
#[cfg(feature = "zero-copy-transport")]
/// Stable-memory payload representation and identity contracts.
pub mod stable;

/// Error type used by serialization-neutral payload helpers.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
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
    /// The received payload schema does not match the codec's declared schema.
    SchemaMismatch {
        /// Zero-based field index, or `None` for a schema-wide mismatch.
        field_index: Option<usize>,
        /// Expected schema property, described without a codec-specific dependency.
        expected: String,
        /// Observed schema property.
        actual: String,
    },
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
    /// A carried native token is missing, unexpected or foreign to the codec.
    UnsupportedNativeTypeToken {
        /// Token resolved under the codec's agreed representation, if native.
        expected: Option<NativeTypeToken>,
        /// Token carried by the frame, if any.
        actual: Option<NativeTypeToken>,
    },
    /// Resolving the scoped native representation failed.
    #[cfg(feature = "native-identity")]
    NativeProfile(NativeProfileError),
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
            Self::SchemaMismatch {
                field_index,
                expected,
                actual,
            } => {
                if let Some(index) = field_index {
                    write!(
                        f,
                        "schema mismatch at field {index}: expected {expected}, got {actual}"
                    )
                } else {
                    write!(f, "schema mismatch: expected {expected}, got {actual}")
                }
            }
            Self::MissingPayload => f.write_str("frame has no payload"),
            Self::UnsupportedEncoding { expected, actual } => f.write_fmt(format_args!(
                "unsupported payload encoding: expected {expected:?}; got {actual:?}",
            )),
            Self::SerializationError(message) => {
                f.write_fmt(format_args!("serialization error: {message}"))
            }
            Self::UnsupportedNativeTypeToken { expected, actual } => write!(
                f,
                "native type token mismatch: expected {expected:?}; got {actual:?}"
            ),
            #[cfg(feature = "native-identity")]
            Self::NativeProfile(error) => Display::fmt(error, f),
        }
    }
}

impl Error for UWireError {}

#[cfg(feature = "native-identity")]
impl From<NativeProfileError> for UWireError {
    fn from(value: NativeProfileError) -> Self {
        Self::NativeProfile(value)
    }
}

#[cfg(feature = "zero-copy-transport")]
impl From<up_rust_core::payload::stable::StableInitError> for UWireError {
    fn from(error: up_rust_core::payload::stable::StableInitError) -> Self {
        use up_rust_core::payload::stable::StableInitError as Core;
        match error {
            Core::Length { expected, actual } | Core::SliceLength { expected, actual } => {
                Self::invalid_payload_length(expected, actual)
            }
            Core::Misaligned { required } => Self::invalid_payload(format!(
                "stable initialization storage is not aligned to {required} bytes"
            )),
            Core::Representation => Self::invalid_payload(
                "stable representation does not match its Rust type and target",
            ),
            _ => Self::invalid_payload("stable initialization failed"),
        }
    }
}

impl From<UWireError> for UStatus {
    fn from(value: UWireError) -> Self {
        UStatus::fail_with_code(UCode::InvalidArgument, value.to_string())
    }
}
