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

//! Serialization-neutral frame and wire-format primitives.

use std::{error::Error, fmt::Display};

use bytes::Bytes;

use crate::{UAttributesError, UAttributesValidators, UCode, UStatus, UUri, UUID};

/// Native uProtocol message kind carried in a frame metadata.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UMessageType {
    Publish,
    Notification,
    Request,
    Response,
}

/// Native uProtocol priority class.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum UPriority {
    CS0,
    #[default]
    CS1,
    CS2,
    CS3,
    CS4,
    CS5,
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

    pub fn id(&self) -> &UUID {
        &self.id
    }

    pub fn source(&self) -> &UUri {
        &self.source
    }

    pub fn sink(&self) -> Option<&UUri> {
        self.sink.as_ref()
    }

    pub fn message_type(&self) -> UMessageType {
        self.message_type
    }

    pub fn priority(&self) -> UPriority {
        self.priority
    }

    pub fn ttl(&self) -> Option<u32> {
        self.ttl
    }

    pub fn request_id(&self) -> Option<&UUID> {
        self.request_id.as_ref()
    }

    pub fn traceparent(&self) -> Option<&str> {
        self.traceparent.as_deref()
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn permission_level(&self) -> Option<u32> {
        self.permission_level
    }

    pub fn commstatus(&self) -> Option<UCode> {
        self.commstatus
    }

    /// Validates these attributes according to their message type.
    pub fn validate(&self) -> Result<(), UAttributesError> {
        UAttributesValidators::get_validator_for_attributes(self).validate(self)
    }

    #[must_use]
    pub fn with_priority(mut self, priority: UPriority) -> Self {
        self.priority = priority;
        self
    }

    #[must_use]
    pub fn with_ttl(mut self, ttl: u32) -> Self {
        self.ttl = Some(ttl);
        self
    }

    #[must_use]
    pub fn with_request_id(mut self, request_id: UUID) -> Self {
        self.request_id = Some(request_id);
        self
    }

    #[must_use]
    pub fn with_traceparent(mut self, traceparent: impl Into<String>) -> Self {
        self.traceparent = Some(traceparent.into());
        self
    }

    #[must_use]
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    #[must_use]
    pub fn with_permission_level(mut self, permission_level: u32) -> Self {
        self.permission_level = Some(permission_level);
        self
    }

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

impl UEncoding {
    /// Creates encoding metadata from all fields.
    pub fn new(
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

    pub fn from_content_type(content_type: impl Into<String>) -> Self {
        let content_type = content_type.into();
        Self::without_schema_ref(content_type.clone(), content_type)
    }

    pub fn format_id(&self) -> &str {
        &self.format_id
    }

    pub fn content_type(&self) -> &str {
        &self.content_type
    }

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

/// Error type used by serialization-neutral frame helpers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UWireError {
    BufferTooSmall {
        expected: usize,
        actual: usize,
    },
    InvalidPayload(String),
    UnsupportedEncoding {
        expected: UEncoding,
        actual: UEncoding,
    },
    SerializationError(String),
}

impl UWireError {
    pub fn buffer_too_small(expected: usize, actual: usize) -> Self {
        Self::BufferTooSmall { expected, actual }
    }

    pub fn invalid_payload(message: impl Into<String>) -> Self {
        Self::InvalidPayload(message.into())
    }

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

/// Error type used by the native [`UMessageBuilder`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UMessageBuilderError {
    AttributesValidationError(UAttributesError),
    Payload(UWireError),
}

impl UMessageBuilderError {
    fn invalid_attributes(message: impl Into<String>) -> Self {
        Self::AttributesValidationError(UAttributesError::validation_error(message))
    }
}

impl Display for UMessageBuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AttributesValidationError(error) => {
                f.write_fmt(format_args!("invalid frame attributes: {error}"))
            }
            Self::Payload(error) => f.write_fmt(format_args!("invalid frame payload: {error}")),
        }
    }
}

impl Error for UMessageBuilderError {}

impl From<UWireError> for UMessageBuilderError {
    fn from(value: UWireError) -> Self {
        Self::Payload(value)
    }
}

impl From<UAttributesError> for UMessageBuilderError {
    fn from(value: UAttributesError) -> Self {
        Self::AttributesValidationError(value)
    }
}

/// Native frame metadata used by transport APIs.
///
/// `UFrameMetadata` groups native uProtocol [`UAttributes`] with payload
/// [`UEncoding`]. It is not a generated protocol envelope and it is not a
/// replacement for `UAttributes`; transports should project these fields into
/// their own metadata channels where possible. Prefer [`UMessageBuilder`] for
/// constructing application frames because it validates message-type-specific
/// attribute rules before producing an owned frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UFrameMetadata {
    attributes: UAttributes,
    encoding: UEncoding,
}

impl UFrameMetadata {
    pub fn new(attributes: UAttributes, encoding: UEncoding) -> Self {
        Self {
            attributes,
            encoding,
        }
    }

    /// Creates unchecked Publish metadata.
    ///
    /// Prefer [`Self::try_publish`] or [`UMessageBuilder::publish`] for application frames.
    pub fn publish(topic: UUri) -> Self {
        Self::new(
            UAttributes::new(UUID::build(), topic, None, UMessageType::Publish),
            UEncoding::default(),
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
    /// Prefer [`Self::try_notification`] or [`UMessageBuilder::notification`] for application frames.
    pub fn notification(origin: UUri, destination: UUri) -> Self {
        Self::new(
            UAttributes::new(
                UUID::build(),
                origin,
                Some(destination),
                UMessageType::Notification,
            ),
            UEncoding::default(),
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
    /// Prefer [`Self::try_request`] or [`UMessageBuilder::request`] for application frames.
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
            UEncoding::default(),
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
    /// Prefer [`Self::try_response`] or [`UMessageBuilder::response`] for application frames.
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
            UEncoding::default(),
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

    pub fn attributes(&self) -> &UAttributes {
        &self.attributes
    }

    pub fn attributes_mut(&mut self) -> &mut UAttributes {
        &mut self.attributes
    }

    pub fn source(&self) -> &UUri {
        self.attributes.source()
    }

    pub fn sink(&self) -> Option<&UUri> {
        self.attributes.sink()
    }

    pub fn encoding(&self) -> &UEncoding {
        &self.encoding
    }

    pub fn encoding_mut(&mut self) -> &mut UEncoding {
        &mut self.encoding
    }

    #[must_use]
    pub fn with_encoding(mut self, encoding: UEncoding) -> Self {
        self.encoding = encoding;
        self
    }
}

/// Owned, serialization-neutral uProtocol frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UOwnedFrame {
    metadata: UFrameMetadata,
    payload: Bytes,
}

impl UOwnedFrame {
    pub fn new(metadata: UFrameMetadata, payload: impl Into<Bytes>) -> Self {
        Self {
            metadata,
            payload: payload.into(),
        }
    }

    pub fn from_serializable<F, T>(metadata: UFrameMetadata, value: &T) -> Result<Self, UWireError>
    where
        F: WireFormat,
        T: USerializer<F>,
    {
        let payload = value.serialize_owned()?;
        Ok(Self::new(metadata.with_encoding(F::encoding()), payload))
    }

    pub fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut UFrameMetadata {
        &mut self.metadata
    }

    pub fn payload(&self) -> &Bytes {
        &self.payload
    }

    pub fn payload_bytes(&self) -> &[u8] {
        self.payload.as_ref()
    }

    pub fn into_metadata(self) -> UFrameMetadata {
        self.metadata
    }

    pub fn into_payload(self) -> Bytes {
        self.payload
    }

    pub fn deserialize<'a, F, T>(&'a self) -> Result<T, UWireError>
    where
        F: WireFormat,
        T: UDeserializer<'a, F>,
    {
        let expected = F::encoding();
        if !self.metadata.encoding().is_compatible_with(&expected) {
            return Err(UWireError::UnsupportedEncoding {
                expected,
                actual: self.metadata.encoding().clone(),
            });
        }
        T::deserialize_from(self.payload_bytes())
    }
}

impl AsRef<[u8]> for UOwnedFrame {
    fn as_ref(&self) -> &[u8] {
        self.payload_bytes()
    }
}

/// Native builder for creating [`UOwnedFrame`]s.
///
/// This restores the old `UMessageBuilder` ergonomics without reintroducing
/// generated message envelopes. The builder output is a native owned frame.
///
/// ```
/// # use up_rust::{UMessageBuilder, UUri};
/// # fn build() -> Result<(), Box<dyn std::error::Error>> {
/// let topic = UUri::try_from("//vehicle/4210/1/B24D")?;
/// let frame = UMessageBuilder::publish(topic)
///     .build_with_raw_payload(b"reading".to_vec())?;
/// assert_eq!(frame.payload_bytes(), b"reading");
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct UMessageBuilder {
    commstatus: Option<UCode>,
    encoding: UEncoding,
    message_id: Option<UUID>,
    message_type: UMessageType,
    payload: Option<Bytes>,
    permission_level: Option<u32>,
    priority: UPriority,
    request_id: Option<UUID>,
    sink: Option<UUri>,
    source: Option<UUri>,
    token: Option<String>,
    traceparent: Option<String>,
    ttl: Option<u32>,
}

impl UMessageBuilder {
    /// Creates a builder for a Publish frame.
    pub fn publish(topic: UUri) -> Self {
        Self {
            message_type: UMessageType::Publish,
            source: Some(topic),
            ..Self::default()
        }
    }

    /// Creates a builder for a Notification frame.
    pub fn notification(origin: UUri, destination: UUri) -> Self {
        Self {
            message_type: UMessageType::Notification,
            source: Some(origin),
            sink: Some(destination),
            ..Self::default()
        }
    }

    /// Creates a builder for an RPC Request frame.
    pub fn request(method_to_invoke: UUri, reply_to_address: UUri, ttl: u32) -> Self {
        Self {
            message_type: UMessageType::Request,
            priority: UPriority::CS4,
            sink: Some(method_to_invoke),
            source: Some(reply_to_address),
            ttl: Some(ttl),
            ..Self::default()
        }
    }

    /// Creates a builder for an RPC Response frame.
    pub fn response(reply_to_address: UUri, request_id: UUID, invoked_method: UUri) -> Self {
        Self {
            message_type: UMessageType::Response,
            priority: UPriority::CS4,
            request_id: Some(request_id),
            sink: Some(reply_to_address),
            source: Some(invoked_method),
            ..Self::default()
        }
    }

    /// Creates a response builder initialized from request attributes.
    pub fn response_for_request(request_attributes: &UAttributes) -> Self {
        Self {
            message_type: UMessageType::Response,
            priority: request_attributes.priority(),
            request_id: Some(request_attributes.id().clone()),
            sink: Some(request_attributes.source().clone()),
            source: request_attributes.sink().cloned(),
            ttl: request_attributes.ttl(),
            ..Self::default()
        }
    }

    /// Sets the frame identifier.
    pub fn with_message_id(mut self, message_id: UUID) -> Self {
        self.message_id = Some(message_id);
        self
    }

    /// Sets the frame priority.
    pub fn with_priority(mut self, priority: UPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Sets the frame time-to-live in milliseconds.
    pub fn with_ttl(mut self, ttl: u32) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Sets the authorization token. Only RPC Request frames may carry tokens.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Sets the permission level. Only RPC Request frames may carry permission levels.
    pub fn with_permission_level(mut self, permission_level: u32) -> Self {
        self.permission_level = Some(permission_level);
        self
    }

    /// Sets the communication status. Only RPC Response frames may carry communication status.
    pub fn with_commstatus(mut self, commstatus: UCode) -> Self {
        self.commstatus = Some(commstatus);
        self
    }

    /// Sets the communication status using Rust-style word separation.
    pub fn with_comm_status(self, commstatus: UCode) -> Self {
        self.with_commstatus(commstatus)
    }

    /// Sets the W3C trace context identifier.
    pub fn with_traceparent(mut self, traceparent: impl Into<String>) -> Self {
        self.traceparent = Some(traceparent.into());
        self
    }

    /// Sets explicit payload encoding metadata for subsequent [`Self::build`] calls.
    pub fn with_encoding(mut self, encoding: UEncoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// Builds only the frame metadata.
    pub fn build_metadata(self) -> Result<UFrameMetadata, UMessageBuilderError> {
        Ok(UFrameMetadata::new(self.build_attributes()?, self.encoding))
    }

    /// Builds an owned frame with the currently configured payload, if any.
    pub fn build(self) -> Result<UOwnedFrame, UMessageBuilderError> {
        let attributes = self.build_attributes()?;
        Ok(UOwnedFrame::new(
            UFrameMetadata::new(attributes, self.encoding),
            self.payload.unwrap_or_default(),
        ))
    }

    /// Builds an owned frame with raw bytes.
    pub fn build_with_raw_payload<T: Into<Bytes>>(
        self,
        payload: T,
    ) -> Result<UOwnedFrame, UMessageBuilderError> {
        self.build_with_payload(payload, RawBytes::encoding())
    }

    /// Builds an owned frame with explicit encoding metadata.
    pub fn build_with_payload<T: Into<Bytes>>(
        mut self,
        payload: T,
        encoding: UEncoding,
    ) -> Result<UOwnedFrame, UMessageBuilderError> {
        self.payload = Some(payload.into());
        self.encoding = encoding;
        self.build()
    }

    /// Serializes a typed payload and builds an owned frame using the selected wire format.
    pub fn build_with_serializable<F, T>(
        mut self,
        value: &T,
    ) -> Result<UOwnedFrame, UMessageBuilderError>
    where
        F: WireFormat,
        T: USerializer<F>,
    {
        self.payload = Some(value.serialize_owned()?);
        self.encoding = F::encoding();
        self.build()
    }

    /// Serializes a Protocol Buffers payload and builds an owned frame.
    #[cfg(feature = "protobuf-wire")]
    pub fn build_with_protobuf_payload<T>(
        self,
        value: &T,
    ) -> Result<UOwnedFrame, UMessageBuilderError>
    where
        T: USerializer<crate::ProtobufWire>,
    {
        self.build_with_serializable::<crate::ProtobufWire, _>(value)
    }

    fn build_attributes(&self) -> Result<UAttributes, UMessageBuilderError> {
        let id = self.message_id.clone().unwrap_or_else(UUID::build);
        if !id.is_uprotocol_uuid() {
            return Err(UMessageBuilderError::invalid_attributes(
                "message ID must be a valid uProtocol UUID",
            ));
        }
        let source = self
            .source
            .clone()
            .ok_or_else(|| UMessageBuilderError::invalid_attributes("source URI is required"))?;
        let sink = self.sink.clone();
        let mut attributes =
            UAttributes::new(id, source, sink, self.message_type).with_priority(self.priority);
        if let Some(ttl) = self.ttl {
            attributes = attributes.with_ttl(ttl);
        }
        if let Some(request_id) = self.request_id.clone() {
            attributes = attributes.with_request_id(request_id);
        }
        if let Some(traceparent) = self.traceparent.clone() {
            attributes = attributes.with_traceparent(traceparent);
        }
        if let Some(token) = self.token.clone() {
            attributes = attributes.with_token(token);
        }
        if let Some(permission_level) = self.permission_level {
            attributes = attributes.with_permission_level(permission_level);
        }
        if let Some(commstatus) = self.commstatus {
            attributes = attributes.with_comm_status(commstatus);
        }
        attributes.validate()?;
        Ok(attributes)
    }
}

impl Default for UMessageBuilder {
    fn default() -> Self {
        Self {
            commstatus: None,
            encoding: UEncoding::default(),
            message_id: None,
            message_type: UMessageType::Publish,
            payload: None,
            permission_level: None,
            priority: UPriority::default(),
            request_id: None,
            sink: None,
            source: None,
            token: None,
            traceparent: None,
            ttl: None,
        }
    }
}

impl UZeroCopyRxFrame for UOwnedFrame {
    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload(&self) -> &[u8] {
        self.payload_bytes()
    }
}

/// Compile-time identity for a payload wire representation.
///
/// ```
/// # use up_rust::{UEncoding, WireFormat};
/// struct JsonTelemetry;
///
/// impl WireFormat for JsonTelemetry {
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
pub trait WireFormat {
    fn name() -> &'static str;
    fn encoding() -> UEncoding;
}

/// Serializes a value into caller-provided storage.
///
/// `encoded_len` must return the number of bytes required by `serialize_into`.
/// If the supplied buffer is too small, implementations should return
/// [`UWireError::BufferTooSmall`] instead of writing a partial payload.
pub trait USerializer<F: WireFormat> {
    const ALIGNMENT: usize = 1;
    fn encoded_len(&self) -> usize;
    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError>;

    fn serialize_owned(&self) -> Result<Bytes, UWireError> {
        let mut bytes = vec![0_u8; self.encoded_len()];
        let written = self.serialize_into(&mut bytes)?;
        bytes.truncate(written);
        Ok(Bytes::from(bytes))
    }
}

/// Deserializes a value from bytes.
pub trait UDeserializer<'a, F: WireFormat>: Sized {
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError>;
}

/// Object-safe serializer for runtime-selected codecs.
pub trait UErasedSerializer {
    fn encoding(&self) -> UEncoding;
    fn alignment(&self) -> usize {
        1
    }
    fn encoded_len(&self) -> usize;
    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError>;
}

/// Built-in raw byte wire format.
pub struct RawBytes;

impl WireFormat for RawBytes {
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

/// Mutable transmit storage reserved from a zero-copy transport.
pub trait UTxBuffer {
    fn metadata(&self) -> &UFrameMetadata;
    fn metadata_mut(&mut self) -> &mut UFrameMetadata;
    fn payload(&self) -> &[u8];
    fn payload_mut(&mut self) -> &mut [u8];
}

/// Receive-side zero-copy frame lease.
pub trait UZeroCopyRxFrame {
    fn metadata(&self) -> &UFrameMetadata;
    fn payload(&self) -> &[u8];

    fn deserialize_borrowed<'a, F, T>(&'a self) -> Result<T, UWireError>
    where
        F: WireFormat,
        T: UDeserializer<'a, F>,
    {
        let expected = F::encoding();
        if !self.metadata().encoding().is_compatible_with(&expected) {
            return Err(UWireError::UnsupportedEncoding {
                expected,
                actual: self.metadata().encoding().clone(),
            });
        }
        T::deserialize_from(self.payload())
    }
}

/// Owned buffer useful for tests, examples, and adapters that emulate a transmit loan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UVecTxBuffer {
    metadata: UFrameMetadata,
    payload: Vec<u8>,
}

impl UVecTxBuffer {
    pub fn new(metadata: UFrameMetadata, payload_len: usize) -> Self {
        Self {
            metadata,
            payload: vec![0_u8; payload_len],
        }
    }

    pub fn into_frame(self) -> UOwnedFrame {
        UOwnedFrame::new(self.metadata, self.payload)
    }
}

impl AsRef<[u8]> for UVecTxBuffer {
    fn as_ref(&self) -> &[u8] {
        self.payload()
    }
}

impl UTxBuffer for UVecTxBuffer {
    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut UFrameMetadata {
        &mut self.metadata
    }

    fn payload(&self) -> &[u8] {
        self.payload.as_ref()
    }

    fn payload_mut(&mut self) -> &mut [u8] {
        self.payload.as_mut_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_bytes_serialize_and_deserialize_without_copying_on_read() {
        let input: &[u8] = &[1, 2, 3, 4];

        let payload = input.serialize_owned().unwrap();
        let decoded = <&[u8] as UDeserializer<RawBytes>>::deserialize_from(&payload).unwrap();

        assert_eq!(decoded, input);
    }

    #[test]
    fn encoding_treats_empty_schema_ref_as_absent() {
        let encoding = UEncoding::new("json", "application/json", Some(""));

        assert_eq!(encoding.schema_ref(), None);
    }

    #[test]
    fn owned_frame_uses_selected_wire_format() {
        let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
        let frame = UOwnedFrame::from_serializable::<RawBytes, _>(
            UFrameMetadata::publish(topic),
            &&[0x0a_u8, 0x0b_u8][..],
        )
        .unwrap();

        assert_eq!(frame.metadata().encoding(), &RawBytes::encoding());
        assert_eq!(frame.payload_bytes(), &[0x0a_u8, 0x0b_u8]);
    }

    struct OtherWire;

    impl WireFormat for OtherWire {
        fn name() -> &'static str {
            "other"
        }

        fn encoding() -> UEncoding {
            UEncoding::new(Self::name(), "application/x-other", None::<String>)
        }
    }

    impl<'a> UDeserializer<'a, OtherWire> for &'a [u8] {
        fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
            Ok(src)
        }
    }

    struct OtherSchemaWire;

    impl WireFormat for OtherSchemaWire {
        fn name() -> &'static str {
            "raw-other-schema"
        }

        fn encoding() -> UEncoding {
            UEncoding::with_schema_ref(
                "raw-bytes",
                "application/octet-stream",
                "urn:example:Other:v1",
            )
        }
    }

    impl<'a> UDeserializer<'a, OtherSchemaWire> for &'a [u8] {
        fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
            Ok(src)
        }
    }

    #[test]
    fn owned_frame_deserialize_rejects_wrong_wire_format() {
        let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
        let frame = UOwnedFrame::from_serializable::<RawBytes, _>(
            UFrameMetadata::publish(topic),
            &&[0x0a_u8, 0x0b_u8][..],
        )
        .unwrap();

        assert!(matches!(
            frame.deserialize::<OtherWire, &[u8]>(),
            Err(UWireError::UnsupportedEncoding { .. })
        ));
    }

    #[test]
    fn owned_frame_deserialize_allows_generic_decoder_for_schema_ref() {
        let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
        let frame = UOwnedFrame::new(
            UFrameMetadata::publish(topic).with_encoding(UEncoding::with_schema_ref(
                "raw-bytes",
                "application/octet-stream",
                "urn:example:Bytes:v1",
            )),
            vec![0x0a_u8, 0x0b_u8],
        );

        assert_eq!(
            frame.deserialize::<RawBytes, &[u8]>().unwrap(),
            &[0x0a_u8, 0x0b_u8]
        );
    }

    #[test]
    fn owned_frame_deserialize_rejects_wrong_schema_ref() {
        let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
        let frame = UOwnedFrame::new(
            UFrameMetadata::publish(topic).with_encoding(UEncoding::with_schema_ref(
                "raw-bytes",
                "application/octet-stream",
                "urn:example:Bytes:v1",
            )),
            vec![0x0a_u8, 0x0b_u8],
        );

        assert!(matches!(
            frame.deserialize::<OtherSchemaWire, &[u8]>(),
            Err(UWireError::UnsupportedEncoding { .. })
        ));
    }

    #[test]
    fn message_builder_builds_publish_frame_with_raw_payload() {
        let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
        let message_id = UUID::build();
        let traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let frame = UMessageBuilder::publish(topic.clone())
            .with_message_id(message_id.clone())
            .with_priority(UPriority::CS2)
            .with_ttl(5_000)
            .with_traceparent(traceparent)
            .build_with_raw_payload(vec![0x01, 0x02])
            .unwrap();

        let attributes = frame.metadata().attributes();
        assert_eq!(attributes.id(), &message_id);
        assert_eq!(attributes.message_type(), UMessageType::Publish);
        assert_eq!(attributes.priority(), UPriority::CS2);
        assert_eq!(attributes.source(), &topic);
        assert_eq!(attributes.sink(), None);
        assert_eq!(attributes.ttl(), Some(5_000));
        assert_eq!(attributes.traceparent(), Some(traceparent));
        assert_eq!(frame.metadata().encoding(), &RawBytes::encoding());
        assert_eq!(frame.payload_bytes(), &[0x01, 0x02]);
    }

    #[test]
    fn message_builder_builds_response_from_request_attributes() {
        let method = UUri::try_from("//vehicle/4210/1/0001").unwrap();
        let reply_to = UUri::try_from("//client/ABCD/1/0000").unwrap();
        let request = UMessageBuilder::request(method.clone(), reply_to.clone(), 5_000)
            .with_priority(UPriority::CS5)
            .build()
            .unwrap();
        let response_id = UUID::build();
        let response = UMessageBuilder::response_for_request(request.metadata().attributes())
            .with_message_id(response_id.clone())
            .with_comm_status(UCode::DEADLINE_EXCEEDED)
            .build()
            .unwrap();

        let attributes = response.metadata().attributes();
        assert_eq!(attributes.id(), &response_id);
        assert_eq!(attributes.message_type(), UMessageType::Response);
        assert_eq!(attributes.priority(), UPriority::CS5);
        assert_eq!(attributes.source(), &method);
        assert_eq!(attributes.sink(), Some(&reply_to));
        assert_eq!(
            attributes.request_id(),
            Some(request.metadata().attributes().id())
        );
        assert_eq!(attributes.commstatus(), Some(UCode::DEADLINE_EXCEEDED));
        assert_eq!(attributes.ttl(), Some(5_000));
    }

    #[test]
    fn message_builder_rejects_low_rpc_priority() {
        let method = UUri::try_from("//vehicle/4210/1/0001").unwrap();
        let reply_to = UUri::try_from("//client/ABCD/1/0000").unwrap();
        let result = UMessageBuilder::request(method, reply_to, 5_000)
            .with_priority(UPriority::CS3)
            .build();

        assert!(matches!(
            result,
            Err(UMessageBuilderError::AttributesValidationError(_))
        ));
    }

    #[test]
    fn message_builder_uses_selected_wire_format_for_typed_payload() {
        let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
        let frame = UMessageBuilder::publish(topic)
            .build_with_serializable::<RawBytes, _>(&&[0x0a_u8, 0x0b_u8][..])
            .unwrap();

        assert_eq!(frame.metadata().encoding(), &RawBytes::encoding());
        assert_eq!(frame.payload_bytes(), &[0x0a_u8, 0x0b_u8]);
    }
}
