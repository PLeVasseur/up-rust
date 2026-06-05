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

use crate::{
    try_project_frame_to_umessage, try_project_umessage_to_frame_metadata, ProtobufMappable,
    SerializationError, UFrameMetadataError, UMessage, UOwnedFrame,
};

/// Error type used by whole-frame wire formats.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UFrameWireError {
    /// The encoded bytes do not contain a valid frame or violate native frame invariants.
    InvalidFrame(String),
    /// The selected wire format cannot faithfully represent the frame payload encoding.
    UnsupportedPayloadEncoding(String),
    /// The wire format encoder or decoder failed while converting bytes.
    SerializationError(String),
}

impl UFrameWireError {
    /// Creates an [`UFrameWireError::InvalidFrame`] value.
    #[must_use]
    pub fn invalid_frame(message: impl Into<String>) -> Self {
        Self::InvalidFrame(message.into())
    }

    /// Creates an [`UFrameWireError::UnsupportedPayloadEncoding`] value.
    #[must_use]
    pub fn unsupported_payload_encoding(message: impl Into<String>) -> Self {
        Self::UnsupportedPayloadEncoding(message.into())
    }

    /// Creates an [`UFrameWireError::SerializationError`] value.
    #[must_use]
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

impl From<SerializationError> for UFrameWireError {
    fn from(value: SerializationError) -> Self {
        Self::serialization_error(value.to_string())
    }
}

impl From<UFrameMetadataError> for UFrameWireError {
    fn from(value: UFrameMetadataError) -> Self {
        match value {
            UFrameMetadataError::CustomEncodingNotRepresentable { id } => {
                Self::unsupported_payload_encoding(format!(
                    "custom payload encoding `{id}` cannot be represented by generated UMessage payload_format"
                ))
            }
            other @ (UFrameMetadataError::EmptyCustomEncodingId
            | UFrameMetadataError::EmptyCustomEncodingContentType
            | UFrameMetadataError::InvalidCustomEncodingContentType(_)
            | UFrameMetadataError::UnspecifiedPayloadFormat
            | UFrameMetadataError::PayloadWithoutEncoding
            | UFrameMetadataError::EncodingWithoutPayload
            | UFrameMetadataError::PayloadFormatMismatch { .. }
            | UFrameMetadataError::PayloadFormatWithCustomEncoding { .. }
            | UFrameMetadataError::PayloadFormatWithoutEncoding { .. }
            | UFrameMetadataError::InvalidAttributes(_)
            | UFrameMetadataError::MessageBuildError(_)) => Self::invalid_frame(other.to_string()),
        }
    }
}

/// Whole-frame wire format for transporting a complete native uProtocol frame.
///
/// This is distinct from payload codecs, which only transform an application
/// value into frame payload bytes. Implementations must preserve native frame
/// metadata and payload presence. If an envelope cannot represent a native-only
/// [`PayloadEncoding`], it should return
/// [`UFrameWireError::UnsupportedPayloadEncoding`] instead of silently dropping
/// metadata.
pub trait UFrameWireFormat {
    /// Stable implementation name for logs, diagnostics, and configuration.
    fn name() -> &'static str;

    /// Media type of bytes emitted by [`Self::serialize_frame`].
    fn content_type() -> &'static str;

    /// Serializes a complete native frame, including metadata and payload bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the frame is invalid or cannot be represented by this
    /// wire format.
    fn serialize_frame(frame: &UOwnedFrame) -> Result<Bytes, UFrameWireError>;

    /// Deserializes a complete native frame from this wire format's bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if `src` is malformed or violates native frame invariants.
    fn deserialize_frame(src: &[u8]) -> Result<UOwnedFrame, UFrameWireError>;
}

/// Whole-frame wire format using the generated `UMessage` Protocol Buffers envelope.
pub struct ProtobufUMessageFrame;

impl UFrameWireFormat for ProtobufUMessageFrame {
    fn name() -> &'static str {
        "protobuf-umessage"
    }

    fn content_type() -> &'static str {
        "application/x-uprotocol-umessage+protobuf"
    }

    fn serialize_frame(frame: &UOwnedFrame) -> Result<Bytes, UFrameWireError> {
        let message =
            try_project_frame_to_umessage(frame.metadata().clone(), frame.payload().cloned())?;
        message
            .write_to_protobuf_bytes()
            .map(Bytes::from)
            .map_err(UFrameWireError::from)
    }

    fn deserialize_frame(src: &[u8]) -> Result<UOwnedFrame, UFrameWireError> {
        let message = UMessage::parse_from_protobuf_bytes(src)?;
        let metadata = try_project_umessage_to_frame_metadata(&message)?;
        let payload = message.payload().map(Bytes::copy_from_slice);
        UOwnedFrame::new(metadata, payload).map_err(UFrameWireError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PayloadEncoding, RawBytes, UFrameMetadata, UMessageBuilder, UPayloadFormat, UUri};

    fn topic() -> UUri {
        UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).expect("topic")
    }

    fn metadata_with_raw_encoding() -> UFrameMetadata {
        let message = UMessageBuilder::publish(topic())
            .build_with_payload(Bytes::new(), UPayloadFormat::Raw)
            .expect("message");
        UFrameMetadata::new(message.attributes().clone(), Some(RawBytes::encoding()))
            .expect("metadata")
    }

    #[test]
    fn protobuf_umessage_frame_round_trips_raw_payload() {
        let frame = UOwnedFrame::with_payload(
            metadata_with_raw_encoding(),
            Bytes::from_static(b"raw payload"),
        )
        .expect("frame");

        let encoded = ProtobufUMessageFrame::serialize_frame(&frame).expect("serialize frame");
        let decoded =
            ProtobufUMessageFrame::deserialize_frame(&encoded).expect("deserialize frame");

        assert_eq!(decoded, frame);
    }

    #[test]
    fn protobuf_umessage_frame_rejects_custom_payload_encoding() {
        let message = UMessageBuilder::publish(topic()).build().expect("message");
        let metadata = UFrameMetadata::new(
            message.attributes().clone(),
            Some(
                PayloadEncoding::custom("com.example.native", "application/vnd.example.native")
                    .expect("custom encoding"),
            ),
        )
        .expect("metadata");
        let frame = UOwnedFrame::with_payload(metadata, Bytes::from_static(b"native payload"))
            .expect("frame");

        let error = ProtobufUMessageFrame::serialize_frame(&frame).unwrap_err();

        assert!(matches!(
            error,
            UFrameWireError::UnsupportedPayloadEncoding(message)
                if message.contains("custom payload encoding")
        ));
    }

    #[test]
    fn protobuf_umessage_frame_rejects_invalid_metadata() {
        let message = UMessageBuilder::publish(topic()).build().expect("message");
        let metadata = UFrameMetadata::new_unchecked(message.attributes().clone(), None);
        let frame = UOwnedFrame::with_payload_unchecked(metadata, Bytes::from_static(b"payload"));

        let error = ProtobufUMessageFrame::serialize_frame(&frame).unwrap_err();

        assert!(matches!(error, UFrameWireError::InvalidFrame(_)));
    }

    #[test]
    fn protobuf_umessage_frame_round_trips_unspecified_absent_payload() {
        let message = UMessageBuilder::publish(topic()).build().expect("message");
        let metadata = UFrameMetadata::new(message.attributes().clone(), None).expect("metadata");
        let frame = UOwnedFrame::without_payload(metadata).expect("frame");

        let encoded = ProtobufUMessageFrame::serialize_frame(&frame).expect("serialize frame");
        let decoded =
            ProtobufUMessageFrame::deserialize_frame(&encoded).expect("deserialize frame");

        assert_eq!(decoded.metadata().payload_encoding(), None);
        assert!(!decoded.has_payload());
    }

    #[test]
    fn protobuf_umessage_frame_rejects_unknown_payload_format_with_payload() {
        let message = UMessageBuilder::publish(topic())
            .build_with_payload(Bytes::from_static(b"payload"), UPayloadFormat::Unspecified)
            .expect("message");
        let encoded = message
            .write_to_protobuf_bytes()
            .expect("serialize message");

        let error = ProtobufUMessageFrame::deserialize_frame(&encoded).unwrap_err();

        assert!(matches!(error, UFrameWireError::InvalidFrame(_)));
    }
}
