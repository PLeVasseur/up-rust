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

use crate::{UFrameMetadata, UFrameMetadataError};

/// Owned, serialization-neutral native uProtocol frame.
///
/// `UOwnedFrame` carries Phase 01 frame metadata plus optional owned payload
/// bytes. It is available only with the `owned-frame-transport` feature and is
/// additive to the ordinary `UTransport`/`UMessage` compatibility path.
#[derive(Clone, Debug, PartialEq)]
pub struct UOwnedFrame {
    metadata: UFrameMetadata,
    payload: Option<Bytes>,
}

impl UOwnedFrame {
    /// Creates a frame from metadata and optional payload bytes after validation.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata validation fails or payload presence does
    /// not match metadata payload encoding presence.
    pub fn new(
        metadata: UFrameMetadata,
        payload: Option<Bytes>,
    ) -> Result<Self, UFrameMetadataError> {
        let frame = Self::new_unchecked(metadata, payload);
        frame.validate()?;
        Ok(frame)
    }

    /// Creates a frame without validation.
    #[must_use]
    pub fn new_unchecked(metadata: UFrameMetadata, payload: Option<Bytes>) -> Self {
        Self { metadata, payload }
    }

    /// Creates a payload-bearing frame after validation.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata validation fails or the metadata has no
    /// payload encoding.
    pub fn with_payload(
        metadata: UFrameMetadata,
        payload: impl Into<Bytes>,
    ) -> Result<Self, UFrameMetadataError> {
        Self::new(metadata, Some(payload.into()))
    }

    /// Creates a payload-bearing frame without validation.
    #[must_use]
    pub fn with_payload_unchecked(metadata: UFrameMetadata, payload: impl Into<Bytes>) -> Self {
        Self::new_unchecked(metadata, Some(payload.into()))
    }

    /// Creates a no-payload frame after validation.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata validation fails or the metadata still has
    /// a payload encoding.
    pub fn without_payload(metadata: UFrameMetadata) -> Result<Self, UFrameMetadataError> {
        Self::new(metadata, None)
    }

    /// Validates metadata and payload presence consistency.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata is invalid, if payload bytes are present
    /// without encoding metadata, or if encoding metadata is present without
    /// payload bytes.
    pub fn validate(&self) -> Result<(), UFrameMetadataError> {
        self.metadata.validate()?;
        match (
            self.payload.is_some(),
            self.metadata.payload_encoding().is_some(),
        ) {
            (true, true) | (false, false) => Ok(()),
            (true, false) => Err(UFrameMetadataError::PayloadWithoutEncoding),
            (false, true) => Err(UFrameMetadataError::EncodingWithoutPayload),
        }
    }

    /// Returns the frame metadata.
    #[must_use]
    pub fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    /// Returns the payload bytes when the frame carries a payload.
    #[must_use]
    pub fn payload(&self) -> Option<&Bytes> {
        self.payload.as_ref()
    }

    /// Returns the payload bytes, or an empty slice when payload is absent.
    ///
    /// Use [`Self::payload`] when distinguishing absent payload from present
    /// empty payload matters.
    #[must_use]
    pub fn payload_bytes(&self) -> &[u8] {
        self.payload.as_deref().unwrap_or_default()
    }

    /// Returns whether the frame carries payload bytes, including present empty payloads.
    #[must_use]
    pub fn has_payload(&self) -> bool {
        self.payload.is_some()
    }

    /// Consumes the frame and returns its metadata.
    #[must_use]
    pub fn into_metadata(self) -> UFrameMetadata {
        self.metadata
    }

    /// Consumes the frame and returns its optional payload bytes.
    #[must_use]
    pub fn into_payload(self) -> Option<Bytes> {
        self.payload
    }

    /// Consumes the frame and returns metadata plus optional payload bytes.
    #[must_use]
    pub fn into_parts(self) -> (UFrameMetadata, Option<Bytes>) {
        (self.metadata, self.payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PayloadEncoding, UMessageBuilder, UPayloadFormat, UUri};

    fn topic() -> UUri {
        UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).expect("failed to create topic")
    }

    fn metadata_without_encoding() -> UFrameMetadata {
        let message = UMessageBuilder::publish(topic()).build().expect("message");
        UFrameMetadata::new(message.attributes().clone(), None).expect("metadata")
    }

    fn metadata_with_encoding() -> UFrameMetadata {
        let message = UMessageBuilder::publish(topic()).build().expect("message");
        UFrameMetadata::new(
            message.attributes().clone(),
            Some(PayloadEncoding::Standard(UPayloadFormat::Raw)),
        )
        .expect("metadata")
    }

    #[test]
    fn owned_frame_rejects_payload_without_encoding() {
        let error =
            UOwnedFrame::with_payload(metadata_without_encoding(), Bytes::from_static(b"payload"))
                .unwrap_err();

        assert_eq!(error, UFrameMetadataError::PayloadWithoutEncoding);
    }

    #[test]
    fn owned_frame_rejects_encoding_without_payload() {
        let error = UOwnedFrame::without_payload(metadata_with_encoding()).unwrap_err();

        assert_eq!(error, UFrameMetadataError::EncodingWithoutPayload);
    }

    #[test]
    fn owned_frame_accepts_absent_payload_without_encoding() {
        let frame = UOwnedFrame::without_payload(metadata_without_encoding()).unwrap();

        assert!(!frame.has_payload());
        assert_eq!(frame.payload_bytes(), [].as_slice());
        assert!(frame.metadata().payload_encoding().is_none());
    }

    #[test]
    fn owned_frame_accepts_present_empty_payload_with_standard_encoding() {
        let frame = UOwnedFrame::with_payload(metadata_with_encoding(), Bytes::new()).unwrap();

        assert!(frame.has_payload());
        assert_eq!(frame.payload(), Some(&Bytes::new()));
        assert_eq!(frame.payload_bytes(), [].as_slice());
        assert_eq!(
            frame.metadata().payload_encoding(),
            Some(&PayloadEncoding::Standard(UPayloadFormat::Raw))
        );
    }
}
