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

use bytes::Bytes;

use crate::{UAttributes, UAttributesValidators, UMessage, UMessageError, UPayloadFormat};

/// Identifies the payload representation carried by a native uProtocol frame.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PayloadEncoding {
    /// A uProtocol standard payload format that can be represented by `UMessage`.
    Standard(UPayloadFormat),
    /// A native-only payload encoding that cannot be represented by `UMessage`.
    Custom { id: String, content_type: String },
}

impl PayloadEncoding {
    /// Creates a standard payload encoding.
    #[must_use]
    pub fn standard(format: UPayloadFormat) -> Self {
        Self::Standard(format)
    }

    /// Creates a validated custom payload encoding.
    ///
    /// # Errors
    ///
    /// Returns an error when `id` is empty, `content_type` is empty, or
    /// `content_type` is not a valid media type.
    pub fn custom(
        id: impl Into<String>,
        content_type: impl Into<String>,
    ) -> Result<Self, UFrameMetadataError> {
        let encoding = Self::Custom {
            id: id.into(),
            content_type: content_type.into(),
        };
        encoding.validate()?;
        Ok(encoding)
    }

    /// Returns the standard payload format when this is a standard encoding.
    #[must_use]
    pub fn standard_format(&self) -> Option<UPayloadFormat> {
        match self {
            Self::Standard(format) => Some(*format),
            Self::Custom { .. } => None,
        }
    }

    /// Returns the custom encoding identity when this is a custom encoding.
    #[must_use]
    pub fn custom_identity(&self) -> Option<(&str, &str)> {
        match self {
            Self::Standard(_) => None,
            Self::Custom { id, content_type } => Some((id.as_str(), content_type.as_str())),
        }
    }

    fn validate(&self) -> Result<(), UFrameMetadataError> {
        match self {
            Self::Standard(UPayloadFormat::Unspecified) => {
                Err(UFrameMetadataError::UnspecifiedPayloadFormat)
            }
            Self::Standard(
                UPayloadFormat::Json
                | UPayloadFormat::Protobuf
                | UPayloadFormat::ProtobufWrappedInAny
                | UPayloadFormat::Raw
                | UPayloadFormat::Shm
                | UPayloadFormat::Someip
                | UPayloadFormat::SomeipTlv
                | UPayloadFormat::Text,
            ) => Ok(()),
            Self::Custom { id, content_type } => {
                if id.is_empty() {
                    return Err(UFrameMetadataError::EmptyCustomEncodingId);
                }
                if content_type.is_empty() {
                    return Err(UFrameMetadataError::EmptyCustomEncodingContentType);
                }
                mediatype::MediaType::parse(content_type).map_err(|error| {
                    UFrameMetadataError::InvalidCustomEncodingContentType(error.to_string())
                })?;
                Ok(())
            }
        }
    }
}

/// Errors returned by native frame metadata projection helpers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UFrameMetadataError {
    EmptyCustomEncodingId,
    EmptyCustomEncodingContentType,
    InvalidCustomEncodingContentType(String),
    UnspecifiedPayloadFormat,
    PayloadWithoutEncoding,
    EncodingWithoutPayload,
    CustomEncodingNotRepresentable {
        id: String,
    },
    PayloadFormatMismatch {
        attributes: UPayloadFormat,
        encoding: UPayloadFormat,
    },
    PayloadFormatWithCustomEncoding {
        attributes: UPayloadFormat,
    },
    PayloadFormatWithoutEncoding {
        attributes: UPayloadFormat,
    },
    InvalidAttributes(String),
    MessageBuildError(String),
}

impl std::fmt::Display for UFrameMetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyCustomEncodingId => f.write_str("custom payload encoding id is empty"),
            Self::EmptyCustomEncodingContentType => {
                f.write_str("custom payload encoding content_type is empty")
            }
            Self::InvalidCustomEncodingContentType(error) => f.write_fmt(format_args!(
                "custom payload encoding content_type is invalid: {error}"
            )),
            Self::UnspecifiedPayloadFormat => {
                f.write_str("UPayloadFormat::Unspecified is not a concrete payload encoding")
            }
            Self::PayloadWithoutEncoding => f.write_str("payload bytes require a payload encoding"),
            Self::EncodingWithoutPayload => f.write_str("payload encoding requires payload bytes"),
            Self::CustomEncodingNotRepresentable { id } => f.write_fmt(format_args!(
                "custom payload encoding `{id}` cannot be represented by UMessage"
            )),
            Self::PayloadFormatMismatch {
                attributes,
                encoding,
            } => f.write_fmt(format_args!(
                "attribute payload format {attributes:?} does not match metadata encoding {encoding:?}"
            )),
            Self::PayloadFormatWithCustomEncoding { attributes } => f.write_fmt(format_args!(
                "attribute payload format {attributes:?} cannot be combined with custom metadata encoding"
            )),
            Self::PayloadFormatWithoutEncoding { attributes } => f.write_fmt(format_args!(
                "attribute payload format {attributes:?} requires matching metadata encoding"
            )),
            Self::InvalidAttributes(error) => {
                f.write_fmt(format_args!("invalid frame metadata attributes: {error}"))
            }
            Self::MessageBuildError(error) => {
                f.write_fmt(format_args!("failed to build UMessage from frame metadata: {error}"))
            }
        }
    }
}

impl std::error::Error for UFrameMetadataError {}

impl From<UMessageError> for UFrameMetadataError {
    fn from(value: UMessageError) -> Self {
        Self::MessageBuildError(value.to_string())
    }
}

/// Native frame metadata for owned or zero-copy transport APIs.
#[derive(Clone, Debug, PartialEq)]
pub struct UFrameMetadata {
    attributes: UAttributes,
    payload_encoding: Option<PayloadEncoding>,
}

impl UFrameMetadata {
    /// Creates metadata and validates the attribute/payload-encoding invariant.
    ///
    /// # Errors
    ///
    /// Returns an error if the attributes are invalid, if a custom encoding is
    /// malformed, or if the attributes' `payload_format` disagrees with the
    /// native payload encoding.
    pub fn new(
        attributes: UAttributes,
        payload_encoding: Option<PayloadEncoding>,
    ) -> Result<Self, UFrameMetadataError> {
        let metadata = Self::new_unchecked(attributes, payload_encoding);
        metadata.validate()?;
        Ok(metadata)
    }

    /// Creates metadata without validation.
    #[must_use]
    pub fn new_unchecked(
        attributes: UAttributes,
        payload_encoding: Option<PayloadEncoding>,
    ) -> Self {
        Self {
            attributes,
            payload_encoding,
        }
    }

    /// Returns the uProtocol attributes carried by this metadata.
    #[must_use]
    pub fn attributes(&self) -> &UAttributes {
        &self.attributes
    }

    /// Consumes this metadata and returns its attributes.
    #[must_use]
    pub fn into_attributes(self) -> UAttributes {
        self.attributes
    }

    /// Returns the native payload encoding, if one is present.
    #[must_use]
    pub fn payload_encoding(&self) -> Option<&PayloadEncoding> {
        self.payload_encoding.as_ref()
    }

    /// Consumes this metadata and returns its native payload encoding.
    #[must_use]
    pub fn into_payload_encoding(self) -> Option<PayloadEncoding> {
        self.payload_encoding
    }

    /// Validates the attribute/payload-encoding invariant.
    ///
    /// # Errors
    ///
    /// Returns an error if attributes are invalid or if their standard
    /// `payload_format` conflicts with the native payload encoding.
    pub fn validate(&self) -> Result<(), UFrameMetadataError> {
        UAttributesValidators::get_validator_for_attributes(&self.attributes)
            .validate(&self.attributes)
            .map_err(|error| UFrameMetadataError::InvalidAttributes(error.to_string()))?;

        if let Some(encoding) = &self.payload_encoding {
            encoding.validate()?;
        }

        if let Some(attributes_format) = self.attributes.payload_format() {
            if attributes_format != UPayloadFormat::Unspecified {
                return self.validate_payload_format(attributes_format);
            }
        }

        Ok(())
    }

    fn validate_payload_format(
        &self,
        attributes_format: UPayloadFormat,
    ) -> Result<(), UFrameMetadataError> {
        match &self.payload_encoding {
            Some(PayloadEncoding::Standard(encoding_format)) => {
                if attributes_format == *encoding_format {
                    Ok(())
                } else {
                    Err(UFrameMetadataError::PayloadFormatMismatch {
                        attributes: attributes_format,
                        encoding: *encoding_format,
                    })
                }
            }
            Some(PayloadEncoding::Custom { .. }) => {
                Err(UFrameMetadataError::PayloadFormatWithCustomEncoding {
                    attributes: attributes_format,
                })
            }
            None => Err(UFrameMetadataError::PayloadFormatWithoutEncoding {
                attributes: attributes_format,
            }),
        }
    }
}

/// Projects protobuf-compatible `UMessage` metadata into native frame metadata.
///
/// # Errors
///
/// Returns an error when a message carries payload bytes without a concrete
/// standard payload format.
pub fn try_project_umessage_to_frame_metadata(
    message: &UMessage,
) -> Result<UFrameMetadata, UFrameMetadataError> {
    let payload_encoding = match (message.payload(), message.payload_format()) {
        (Some(_), Some(UPayloadFormat::Unspecified)) | (Some(_), None) => {
            return Err(UFrameMetadataError::PayloadWithoutEncoding);
        }
        (Some(_), Some(format)) => Some(PayloadEncoding::Standard(format)),
        (None, Some(UPayloadFormat::Unspecified)) | (None, None) => None,
        (None, Some(_)) => return Err(UFrameMetadataError::EncodingWithoutPayload),
    };

    UFrameMetadata::new(message.attributes().clone(), payload_encoding)
}

/// Projects native frame metadata and optional payload bytes into a `UMessage`.
///
/// Custom native encodings are rejected because `UMessage` has only standard
/// `UPayloadFormat` metadata.
///
/// # Errors
///
/// Returns an error if payload bytes and payload encoding are not both present,
/// if the encoding is custom, or if metadata invariants are violated.
pub fn try_project_frame_to_umessage(
    metadata: UFrameMetadata,
    payload: Option<Bytes>,
) -> Result<UMessage, UFrameMetadataError> {
    metadata.validate()?;

    let mut attributes = metadata.attributes;
    let payload = match (payload, metadata.payload_encoding) {
        (Some(payload), Some(PayloadEncoding::Standard(format))) => {
            if format == UPayloadFormat::Unspecified {
                return Err(UFrameMetadataError::UnspecifiedPayloadFormat);
            }
            attributes.payload_format = Some(format);
            Some(payload)
        }
        (Some(_), Some(PayloadEncoding::Custom { id, .. })) => {
            return Err(UFrameMetadataError::CustomEncodingNotRepresentable { id });
        }
        (Some(_), None) => return Err(UFrameMetadataError::PayloadWithoutEncoding),
        (None, Some(PayloadEncoding::Standard(_)) | Some(PayloadEncoding::Custom { .. })) => {
            return Err(UFrameMetadataError::EncodingWithoutPayload);
        }
        (None, None) => {
            attributes.payload_format = Some(UPayloadFormat::Unspecified);
            None
        }
    };

    UMessage::new(attributes, payload).map_err(UFrameMetadataError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UCode, UMessageBuilder, UUri};

    fn topic() -> UUri {
        UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).expect("failed to create test URI")
    }

    #[test]
    fn message_without_payload_projects_to_metadata_without_encoding() {
        let message = UMessageBuilder::publish(topic()).build().expect("message");

        let metadata = try_project_umessage_to_frame_metadata(&message).expect("metadata");

        assert!(message.payload().is_none());
        assert!(metadata.payload_encoding().is_none());
        assert_eq!(metadata.attributes().id(), message.id());
    }

    #[test]
    fn empty_present_payload_keeps_standard_encoding() {
        let message = UMessageBuilder::publish(topic())
            .build_with_payload(Bytes::new(), UPayloadFormat::Raw)
            .expect("message");

        let metadata = try_project_umessage_to_frame_metadata(&message).expect("metadata");

        assert_eq!(message.payload(), Some([].as_slice()));
        assert_eq!(
            metadata.payload_encoding(),
            Some(&PayloadEncoding::Standard(UPayloadFormat::Raw))
        );

        let projected =
            try_project_frame_to_umessage(metadata, Some(Bytes::new())).expect("projected message");
        assert_eq!(projected.payload(), Some([].as_slice()));
        assert_eq!(projected.payload_format(), Some(UPayloadFormat::Raw));
    }

    #[test]
    fn standard_payload_formats_round_trip() {
        for format in [
            UPayloadFormat::Protobuf,
            UPayloadFormat::ProtobufWrappedInAny,
            UPayloadFormat::Raw,
            UPayloadFormat::Json,
            UPayloadFormat::Text,
            UPayloadFormat::Someip,
            UPayloadFormat::SomeipTlv,
            UPayloadFormat::Shm,
        ] {
            let message = UMessageBuilder::publish(topic())
                .build_with_payload(Bytes::from_static(b"payload"), format)
                .expect("message");
            let metadata = try_project_umessage_to_frame_metadata(&message).expect("metadata");
            assert_eq!(
                metadata.payload_encoding(),
                Some(&PayloadEncoding::Standard(format))
            );

            let projected =
                try_project_frame_to_umessage(metadata, Some(Bytes::from_static(b"payload")))
                    .expect("projected message");
            assert_eq!(projected.payload(), Some(b"payload".as_slice()));
            assert_eq!(projected.payload_format(), Some(format));
        }
    }

    #[test]
    fn unspecified_payload_format_with_payload_is_rejected() {
        let message = UMessageBuilder::publish(topic())
            .build_with_payload(Bytes::from_static(b"payload"), UPayloadFormat::Unspecified)
            .expect("message");

        let error = try_project_umessage_to_frame_metadata(&message).unwrap_err();

        assert_eq!(error, UFrameMetadataError::PayloadWithoutEncoding);
    }

    #[test]
    fn metadata_rejects_payload_format_mismatch() {
        let message = UMessageBuilder::publish(topic())
            .build_with_payload(Bytes::from_static(b"payload"), UPayloadFormat::Raw)
            .expect("message");

        let error = UFrameMetadata::new(
            message.attributes().clone(),
            Some(PayloadEncoding::Standard(UPayloadFormat::Json)),
        )
        .unwrap_err();

        assert_eq!(
            error,
            UFrameMetadataError::PayloadFormatMismatch {
                attributes: UPayloadFormat::Raw,
                encoding: UPayloadFormat::Json,
            }
        );
    }

    #[test]
    fn custom_encoding_projection_to_umessage_is_rejected() {
        let attributes = UMessageBuilder::publish(topic())
            .build()
            .expect("message")
            .attributes()
            .clone();
        let metadata = UFrameMetadata::new(
            attributes,
            Some(PayloadEncoding::custom("native", "application/vnd.example.native").unwrap()),
        )
        .expect("metadata");

        let error = try_project_frame_to_umessage(metadata, Some(Bytes::from_static(b"payload")))
            .unwrap_err();

        assert_eq!(
            error,
            UFrameMetadataError::CustomEncodingNotRepresentable {
                id: "native".to_string(),
            }
        );
    }

    #[test]
    fn custom_encoding_is_validated() {
        assert_eq!(
            PayloadEncoding::custom("", "application/vnd.example.native").unwrap_err(),
            UFrameMetadataError::EmptyCustomEncodingId
        );
        assert_eq!(
            PayloadEncoding::custom("native", "").unwrap_err(),
            UFrameMetadataError::EmptyCustomEncodingContentType
        );
        assert!(matches!(
            PayloadEncoding::custom("native", "not a media type"),
            Err(UFrameMetadataError::InvalidCustomEncodingContentType(_))
        ));
    }

    #[test]
    fn pr328_enum_names_compile() {
        assert_eq!(UCode::InvalidArgument as i32, 3);
        assert_eq!(UCode::Unimplemented as i32, 12);
        assert_eq!(UPayloadFormat::ProtobufWrappedInAny.as_i32(), 1);
        assert_eq!(UPayloadFormat::Someip.as_i32(), 4);
        assert_eq!(UPayloadFormat::SomeipTlv.as_i32(), 5);
    }
}
