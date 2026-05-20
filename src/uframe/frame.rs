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

use crate::{UAttributesError, UAttributesValidators, UCode, UUri, UUID};

use super::payload::{PayloadFormat, RawBytes, UDeserializer, USerializer, UWireError};

/// Native uProtocol message kind carried in a frame metadata.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UMessageType {
    /// Publish message sent to a topic URI.
    Publish,
    /// Notification message sent from an origin URI to a destination URI.
    Notification,
    /// RPC request sent to a method URI with a reply-to URI.
    Request,
    /// RPC response sent back to a requester's reply-to URI.
    Response,
}

/// Native uProtocol priority class.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum UPriority {
    /// Lowest priority class.
    CS0,
    /// Default priority class.
    #[default]
    CS1,
    /// Priority class 2.
    CS2,
    /// Priority class 3.
    CS3,
    /// RPC priority class used by request/response helpers.
    CS4,
    /// Priority class 5.
    CS5,
    /// Highest priority class.
    CS6,
}

impl UPriority {
    /// Gets the numeric priority class value.
    pub fn value(self) -> u8 {
        match self {
            Self::CS0 => 0,
            Self::CS1 => 1,
            Self::CS2 => 2,
            Self::CS3 => 3,
            Self::CS4 => 4,
            Self::CS5 => 5,
            Self::CS6 => 6,
        }
    }
}

/// Native uProtocol attributes. This intentionally does not wrap a generated message envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UAttributes {
    id: UUID,
    source: UUri,
    sink: Option<UUri>,
    message_type: UMessageType,
    priority: UPriority,
    ttl: Option<u32>,
    request_id: Option<UUID>,
    traceparent: Option<String>,
    token: Option<String>,
    permission_level: Option<u32>,
    commstatus: Option<UCode>,
}

impl UAttributes {
    /// Creates native uProtocol attributes with default priority and no optional fields.
    ///
    /// Prefer [`crate::UFrameBuilder`] or checked [`UFrameMetadata`] constructors
    /// for application frames so message-type-specific invariants are validated.
    pub fn new(id: UUID, source: UUri, sink: Option<UUri>, message_type: UMessageType) -> Self {
        Self {
            id,
            source,
            sink,
            message_type,
            priority: UPriority::default(),
            ttl: None,
            request_id: None,
            traceparent: None,
            token: None,
            permission_level: None,
            commstatus: None,
        }
    }

    /// Returns the frame identifier.
    pub fn id(&self) -> &UUID {
        &self.id
    }

    /// Returns the source URI.
    pub fn source(&self) -> &UUri {
        &self.source
    }

    /// Returns the optional sink URI.
    ///
    /// Publish frames normally have no sink. Notifications, requests, and
    /// responses carry a sink according to their message type.
    pub fn sink(&self) -> Option<&UUri> {
        self.sink.as_ref()
    }

    /// Returns the message kind these attributes describe.
    pub fn message_type(&self) -> UMessageType {
        self.message_type
    }

    /// Returns the uProtocol priority class.
    pub fn priority(&self) -> UPriority {
        self.priority
    }

    /// Returns the optional time-to-live in milliseconds.
    pub fn ttl(&self) -> Option<u32> {
        self.ttl
    }

    /// Returns the request identifier for response frames.
    pub fn request_id(&self) -> Option<&UUID> {
        self.request_id.as_ref()
    }

    /// Returns the optional trace context parent value.
    pub fn traceparent(&self) -> Option<&str> {
        self.traceparent.as_deref()
    }

    /// Returns the optional authorization token.
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// Returns the optional permission level.
    pub fn permission_level(&self) -> Option<u32> {
        self.permission_level
    }

    /// Returns the optional communication status for response frames.
    pub fn commstatus(&self) -> Option<UCode> {
        self.commstatus
    }

    /// Validates these attributes according to their message type.
    pub fn validate(&self) -> Result<(), UAttributesError> {
        UAttributesValidators::get_validator_for_attributes(self).validate(self)
    }

    /// Returns a copy of these attributes with `priority` set.
    #[must_use]
    pub fn with_priority(mut self, priority: UPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Returns a copy of these attributes with a time-to-live in milliseconds.
    #[must_use]
    pub fn with_ttl(mut self, ttl: u32) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Returns a copy of these attributes with an RPC request identifier.
    #[must_use]
    pub fn with_request_id(mut self, request_id: UUID) -> Self {
        self.request_id = Some(request_id);
        self
    }

    /// Returns a copy of these attributes with a trace context parent value.
    #[must_use]
    pub fn with_traceparent(mut self, traceparent: impl Into<String>) -> Self {
        self.traceparent = Some(traceparent.into());
        self
    }

    /// Returns a copy of these attributes with an authorization token.
    #[must_use]
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Returns a copy of these attributes with a permission level.
    #[must_use]
    pub fn with_permission_level(mut self, permission_level: u32) -> Self {
        self.permission_level = Some(permission_level);
        self
    }

    /// Returns a copy of these attributes with a communication status.
    #[must_use]
    pub fn with_commstatus(mut self, commstatus: UCode) -> Self {
        self.commstatus = Some(commstatus);
        self
    }

    /// Sets the communication status using Rust-style word separation.
    #[must_use]
    pub fn with_comm_status(self, commstatus: UCode) -> Self {
        self.with_commstatus(commstatus)
    }

    /// Returns whether this frame has expired according to its UUID timestamp and TTL.
    pub fn is_expired(&self) -> bool {
        let Some(ttl) = self.ttl else {
            return false;
        };
        let Some(created_at_ms) = self.id.get_time() else {
            return false;
        };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis() as u64);
        now_ms.saturating_sub(created_at_ms) > u64::from(ttl)
    }
}

/// Identifies the payload representation carried by a frame.
///
/// `format_id` and `content_type` identify the codec family. `schema_ref`
/// narrows that codec to a concrete schema when the decoder requires one.
/// Empty schema references are normalized to absent.
/// Decoders with no schema requirement can still accept matching format and
/// content type when a frame carries a schema reference.
///
/// ```
/// # use up_rust::UEncoding;
/// let generic_json = UEncoding::without_schema_ref("json", "application/json");
/// let typed_json = UEncoding::with_schema_ref(
///     "json",
///     "application/json",
///     "urn:example:Telemetry:v1",
/// );
///
/// assert!(typed_json.is_compatible_with(&generic_json));
/// assert!(typed_json.is_compatible_with(&typed_json));
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct UEncoding {
    format_id: String,
    content_type: String,
    schema_ref: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UEncodingError {
    /// The `format_id` field was empty.
    EmptyFormatId,
    /// The `content_type` field was empty.
    EmptyContentType,
    /// The `content_type` field was not a valid media type.
    InvalidContentType(String),
}

impl Display for UEncodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyFormatId => f.write_str("encoding format_id must not be empty"),
            Self::EmptyContentType => f.write_str("encoding content_type must not be empty"),
            Self::InvalidContentType(error) => {
                f.write_fmt(format_args!("encoding content_type is not valid: {error}"))
            }
        }
    }
}

impl Error for UEncodingError {}

impl UEncoding {
    /// Creates encoding metadata from all fields, panicking if static inputs are invalid.
    ///
    /// Use [`Self::try_new`] for runtime input and transport decode paths.
    pub fn new(
        format_id: impl Into<String>,
        content_type: impl Into<String>,
        schema_ref: Option<impl Into<String>>,
    ) -> Self {
        Self::try_new(format_id, content_type, schema_ref)
            .expect("UEncoding::new requires non-empty format_id and content_type")
    }

    /// Creates encoding metadata from runtime input.
    pub fn try_new(
        format_id: impl Into<String>,
        content_type: impl Into<String>,
        schema_ref: Option<impl Into<String>>,
    ) -> Result<Self, UEncodingError> {
        let format_id = format_id.into();
        if format_id.is_empty() {
            return Err(UEncodingError::EmptyFormatId);
        }
        let content_type = content_type.into();
        if content_type.is_empty() {
            return Err(UEncodingError::EmptyContentType);
        }
        content_type
            .parse::<mediatype::MediaTypeBuf>()
            .map_err(|error| UEncodingError::InvalidContentType(error.to_string()))?;
        Ok(Self::new_unchecked(format_id, content_type, schema_ref))
    }

    /// Creates encoding metadata without validation.
    ///
    /// This is for low-level adapters that must preserve wire-level values before
    /// explicit validation. Application code should prefer [`Self::new`] or
    /// [`Self::try_new`].
    pub fn new_unchecked(
        format_id: impl Into<String>,
        content_type: impl Into<String>,
        schema_ref: Option<impl Into<String>>,
    ) -> Self {
        Self {
            format_id: format_id.into(),
            content_type: content_type.into(),
            schema_ref: schema_ref.and_then(|schema_ref| {
                let schema_ref = schema_ref.into();
                (!schema_ref.is_empty()).then_some(schema_ref)
            }),
        }
    }

    /// Creates encoding metadata without a schema reference.
    pub fn without_schema_ref(
        format_id: impl Into<String>,
        content_type: impl Into<String>,
    ) -> Self {
        Self::new(format_id, content_type, None::<String>)
    }

    /// Creates encoding metadata with a schema reference.
    pub fn with_schema_ref(
        format_id: impl Into<String>,
        content_type: impl Into<String>,
        schema_ref: impl Into<String>,
    ) -> Self {
        Self::new(format_id, content_type, Some(schema_ref))
    }

    /// Creates encoding metadata whose `format_id` and `content_type` are the same value.
    ///
    /// This is useful when a media type alone is sufficient to identify the
    /// payload representation.
    pub fn from_content_type(content_type: impl Into<String>) -> Self {
        let content_type = content_type.into();
        Self::without_schema_ref(content_type.clone(), content_type)
    }

    /// Returns the codec family identifier.
    pub fn format_id(&self) -> &str {
        &self.format_id
    }

    /// Returns the payload media type.
    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    /// Returns the optional schema reference used to narrow the codec family.
    pub fn schema_ref(&self) -> Option<&str> {
        self.schema_ref.as_deref()
    }

    /// Returns whether this actual frame encoding can be decoded by a decoder
    /// that declares `expected`.
    ///
    /// `format_id` and `content_type` must match exactly. If `expected` carries
    /// a schema reference, the frame must carry the same schema reference. If
    /// `expected` has no schema reference, the decoder is treated as generic for
    /// the matching format/content-type pair.
    pub fn is_compatible_with(&self, expected: &Self) -> bool {
        self.format_id == expected.format_id
            && self.content_type == expected.content_type
            && expected
                .schema_ref
                .as_ref()
                .is_none_or(|expected_schema_ref| {
                    self.schema_ref.as_ref() == Some(expected_schema_ref)
                })
    }
}

impl Default for UEncoding {
    fn default() -> Self {
        RawBytes::encoding()
    }
}

/// Native frame metadata used by transport APIs.
///
/// `UFrameMetadata` groups native uProtocol [`UAttributes`] with optional
/// payload [`UEncoding`]. It is not a generated protocol envelope and it is not a
/// replacement for `UAttributes`; transports should project these fields into
/// their own metadata channels where possible. Prefer [`crate::UFrameBuilder`]
/// for constructing application frames because it validates message-type-specific
/// attribute rules before producing an owned frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UFrameMetadata {
    attributes: UAttributes,
    encoding: Option<UEncoding>,
}

impl UFrameMetadata {
    /// Creates frame metadata from native attributes and optional payload encoding.
    ///
    /// The encoding must be `Some` exactly when the corresponding frame carries a
    /// payload. Use [`Self::without_payload_encoding`] for no-payload frames.
    pub fn new(attributes: UAttributes, encoding: impl Into<Option<UEncoding>>) -> Self {
        Self {
            attributes,
            encoding: encoding.into(),
        }
    }

    /// Creates metadata for a frame with no payload bytes.
    pub fn without_payload_encoding(attributes: UAttributes) -> Self {
        Self::new(attributes, None::<UEncoding>)
    }

    /// Creates unchecked Publish metadata.
    ///
    /// Prefer [`Self::try_publish`] or [`crate::UFrameBuilder::publish`] for application frames.
    pub fn publish(topic: UUri) -> Self {
        Self::new(
            UAttributes::new(UUID::build(), topic, None, UMessageType::Publish),
            None::<UEncoding>,
        )
    }

    /// Creates checked Publish metadata.
    pub fn try_publish(topic: UUri) -> Result<Self, UAttributesError> {
        let metadata = Self::publish(topic);
        metadata.validate()?;
        Ok(metadata)
    }

    /// Creates unchecked Notification metadata.
    ///
    /// Prefer [`Self::try_notification`] or [`crate::UFrameBuilder::notification`] for application frames.
    pub fn notification(origin: UUri, destination: UUri) -> Self {
        Self::new(
            UAttributes::new(
                UUID::build(),
                origin,
                Some(destination),
                UMessageType::Notification,
            ),
            None::<UEncoding>,
        )
    }

    /// Creates checked Notification metadata.
    pub fn try_notification(origin: UUri, destination: UUri) -> Result<Self, UAttributesError> {
        let metadata = Self::notification(origin, destination);
        metadata.validate()?;
        Ok(metadata)
    }

    /// Creates unchecked RPC Request metadata.
    ///
    /// Prefer [`Self::try_request`] or [`crate::UFrameBuilder::request`] for application frames.
    pub fn request(method_to_invoke: UUri, reply_to_address: UUri, ttl: u32) -> Self {
        Self::new(
            UAttributes::new(
                UUID::build(),
                reply_to_address,
                Some(method_to_invoke),
                UMessageType::Request,
            )
            .with_priority(UPriority::CS4)
            .with_ttl(ttl),
            None::<UEncoding>,
        )
    }

    /// Creates checked RPC Request metadata.
    pub fn try_request(
        method_to_invoke: UUri,
        reply_to_address: UUri,
        ttl: u32,
    ) -> Result<Self, UAttributesError> {
        let metadata = Self::request(method_to_invoke, reply_to_address, ttl);
        metadata.validate()?;
        Ok(metadata)
    }

    /// Creates unchecked RPC Response metadata.
    ///
    /// Prefer [`Self::try_response`] or [`crate::UFrameBuilder::response`] for application frames.
    pub fn response(reply_to_address: UUri, request_id: UUID, invoked_method: UUri) -> Self {
        Self::new(
            UAttributes::new(
                UUID::build(),
                invoked_method,
                Some(reply_to_address),
                UMessageType::Response,
            )
            .with_priority(UPriority::CS4)
            .with_request_id(request_id),
            None::<UEncoding>,
        )
    }

    /// Creates checked RPC Response metadata.
    pub fn try_response(
        reply_to_address: UUri,
        request_id: UUID,
        invoked_method: UUri,
    ) -> Result<Self, UAttributesError> {
        let metadata = Self::response(reply_to_address, request_id, invoked_method);
        metadata.validate()?;
        Ok(metadata)
    }

    /// Validates metadata attributes according to their message type.
    pub fn validate(&self) -> Result<(), UAttributesError> {
        UAttributesValidators::get_validator_for_attributes(&self.attributes)
            .validate(&self.attributes)
    }

    /// Returns the native uProtocol attributes.
    pub fn attributes(&self) -> &UAttributes {
        &self.attributes
    }

    /// Returns the source URI from the contained attributes.
    pub fn source(&self) -> &UUri {
        self.attributes.source()
    }

    /// Returns the optional sink URI from the contained attributes.
    pub fn sink(&self) -> Option<&UUri> {
        self.attributes.sink()
    }

    /// Returns the payload encoding metadata when this metadata describes a payload frame.
    pub fn encoding(&self) -> Option<&UEncoding> {
        self.encoding.as_ref()
    }

    /// Returns metadata with payload encoding set to `encoding`.
    #[must_use]
    pub fn with_encoding(mut self, encoding: UEncoding) -> Self {
        self.encoding = Some(encoding);
        self
    }

    /// Returns metadata with payload encoding removed.
    ///
    /// Use this when constructing a no-payload frame from metadata that may have
    /// been cloned from a payload-bearing frame.
    #[must_use]
    pub fn without_encoding(mut self) -> Self {
        self.encoding = None;
        self
    }
}

/// Owned, serialization-neutral uProtocol frame.
///
/// `UOwnedFrame` carries native frame metadata plus optional owned payload bytes.
/// Payload bytes are never interpreted by the frame itself; typed send/receive
/// helpers use [`PayloadFormat`] implementations to set or verify [`UEncoding`]
/// before serializing or deserializing application values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UOwnedFrame {
    metadata: UFrameMetadata,
    payload: Option<Bytes>,
}

impl UOwnedFrame {
    /// Creates a payload-bearing frame.
    ///
    /// `metadata.encoding()` should be `Some` for frames created with this
    /// constructor. Use [`Self::from_serializable`] or [`crate::UFrameBuilder`]
    /// when the payload codec should set the encoding automatically.
    pub fn new(metadata: UFrameMetadata, payload: impl Into<Bytes>) -> Self {
        Self::with_payload(metadata, payload)
    }

    /// Creates a payload-bearing frame from metadata and payload bytes.
    pub fn with_payload(metadata: UFrameMetadata, payload: impl Into<Bytes>) -> Self {
        Self {
            metadata,
            payload: Some(payload.into()),
        }
    }

    /// Creates a frame with no payload and no payload encoding.
    ///
    /// Any encoding present in `metadata` is removed so payload presence and
    /// payload encoding remain consistent.
    pub fn without_payload(metadata: UFrameMetadata) -> Self {
        Self {
            metadata: metadata.without_encoding(),
            payload: None,
        }
    }

    /// Serializes `value` with payload codec `F` and returns an owned frame.
    ///
    /// The returned frame carries `F::encoding()` in its metadata. Serialization
    /// errors are reported as [`UWireError`] values.
    pub fn from_serializable<F, T>(metadata: UFrameMetadata, value: &T) -> Result<Self, UWireError>
    where
        F: PayloadFormat,
        T: USerializer<F>,
    {
        let payload = value.serialize_owned()?;
        Ok(Self::new(metadata.with_encoding(F::encoding()), payload))
    }

    /// Returns frame metadata.
    pub fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    /// Returns mutable frame metadata for owned-frame adapters.
    ///
    /// Keep payload presence and `metadata.encoding()` consistent before sending
    /// or delivering a mutated frame.
    pub fn metadata_mut(&mut self) -> &mut UFrameMetadata {
        &mut self.metadata
    }

    /// Gets the payload bytes if the frame carries a payload.
    pub fn payload(&self) -> Option<&Bytes> {
        self.payload.as_ref()
    }

    /// Gets the payload bytes, or an empty slice when the frame has no payload.
    ///
    /// Use [`Self::payload`] when payload presence matters, for example to
    /// distinguish an absent payload from a present but empty payload.
    pub fn payload_bytes(&self) -> &[u8] {
        self.payload.as_deref().unwrap_or_default()
    }

    /// Returns whether the frame carries a payload, including a present empty payload.
    pub fn has_payload(&self) -> bool {
        self.payload.is_some()
    }

    /// Consumes the frame and returns its metadata.
    pub fn into_metadata(self) -> UFrameMetadata {
        self.metadata
    }

    /// Consumes the frame and returns its optional payload bytes.
    pub fn into_payload(self) -> Option<Bytes> {
        self.payload
    }

    /// Deserializes the payload with codec `F` after verifying encoding metadata.
    ///
    /// The frame must carry a payload and a compatible [`UEncoding`]. A decoder
    /// with no schema reference accepts a frame with a more specific schema
    /// reference as long as `format_id` and `content_type` match.
    pub fn deserialize<'a, F, T>(&'a self) -> Result<T, UWireError>
    where
        F: PayloadFormat,
        T: UDeserializer<'a, F>,
    {
        let expected = F::encoding();
        let payload = self.payload.as_deref().ok_or(UWireError::MissingPayload)?;
        let actual = self
            .metadata
            .encoding()
            .ok_or(UWireError::MissingEncoding)?;
        if !actual.is_compatible_with(&expected) {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(actual.clone()),
            });
        }
        T::deserialize_from(payload)
    }
}

impl AsRef<[u8]> for UOwnedFrame {
    fn as_ref(&self) -> &[u8] {
        self.payload_bytes()
    }
}
