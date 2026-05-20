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

use bytes::Bytes;

use super::frame::UOwnedFrame;

/// Error type used by whole-frame wire formats.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UFrameWireError {
    /// The encoded bytes do not contain a valid frame or violate native frame
    /// invariants such as payload/payload-encoding consistency.
    InvalidFrame(String),
    /// The selected wire format cannot faithfully represent the frame payload's
    /// [`UEncoding`](crate::UEncoding).
    UnsupportedPayloadEncoding(String),
    /// The wire format encoder or decoder failed while converting bytes.
    SerializationError(String),
}

impl UFrameWireError {
    /// Creates an [`UFrameWireError::InvalidFrame`] value.
    pub fn invalid_frame(message: impl Into<String>) -> Self {
        Self::InvalidFrame(message.into())
    }

    /// Creates an [`UFrameWireError::UnsupportedPayloadEncoding`] value.
    pub fn unsupported_payload_encoding(message: impl Into<String>) -> Self {
        Self::UnsupportedPayloadEncoding(message.into())
    }

    /// Creates an [`UFrameWireError::SerializationError`] value.
    pub fn serialization_error(message: impl Into<String>) -> Self {
        Self::SerializationError(message.into())
    }
}

impl Display for UFrameWireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFrame(message) => write!(f, "invalid frame: {message}"),
            Self::UnsupportedPayloadEncoding(message) => {
                write!(f, "unsupported payload encoding: {message}")
            }
            Self::SerializationError(message) => write!(f, "frame serialization error: {message}"),
        }
    }
}

impl Error for UFrameWireError {}

/// Whole-frame wire format for transporting a complete uProtocol frame.
///
/// A frame wire format serializes and deserializes the outer carrier: frame
/// attributes, payload encoding metadata, and payload bytes. This is distinct
/// from payload codecs, which only transform an application value into the frame
/// payload bytes.
///
/// Implementations must preserve native [`UOwnedFrame`] metadata and payload
/// presence. If a legacy envelope cannot represent a frame's payload encoding
/// metadata, it should return [`UFrameWireError::UnsupportedPayloadEncoding`]
/// instead of silently dropping information.
///
/// ```no_run
/// # use bytes::Bytes;
/// # use up_rust::{UFrameWireError, UFrameWireFormat, UOwnedFrame};
/// struct MyEnvelope;
///
/// impl UFrameWireFormat for MyEnvelope {
///     fn name() -> &'static str { "my-envelope" }
///     fn content_type() -> &'static str { "application/x-my-frame" }
///     fn serialize_frame(_frame: &UOwnedFrame) -> Result<Bytes, UFrameWireError> {
///         # Ok(Bytes::new())
///     }
///     fn deserialize_frame(_src: &[u8]) -> Result<UOwnedFrame, UFrameWireError> {
///         # Err(UFrameWireError::invalid_frame("not implemented"))
///     }
/// }
/// ```
pub trait UFrameWireFormat {
    /// Stable implementation name for logs, diagnostics, and configuration.
    fn name() -> &'static str;

    /// Media type of bytes emitted by [`Self::serialize_frame`].
    fn content_type() -> &'static str;

    /// Serializes a complete native frame, including metadata and payload bytes.
    fn serialize_frame(frame: &UOwnedFrame) -> Result<Bytes, UFrameWireError>;

    /// Deserializes a complete native frame from this wire format's bytes.
    fn deserialize_frame(src: &[u8]) -> Result<UOwnedFrame, UFrameWireError>;
}
