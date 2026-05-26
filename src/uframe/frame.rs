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

use super::payload::{
    BorrowPayload, BytePayloadCodec, DecodePayload, EncodePayload, EncodedPayload, PayloadCodec,
    PayloadFormat, RawBytes, UDeserializer, USerializer, UWireError,
};

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

/// Standard payload formats defined by the uProtocol data model.
///
/// These variants mirror upstream `UPayloadFormat`. Native frames can carry one
/// of these standard formats or a transport/native-only custom encoding through
/// [`PayloadEncoding`].
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum UPayloadFormat {
    /// Payload format is unknown and must be understood out of band.
    #[default]
    Unspecified,
    /// Payload is a serialized `google.protobuf.Any`.
    ProtobufWrappedInAny,
    /// Payload is a serialized protobuf message.
    Protobuf,
    /// Payload is UTF-8 JSON.
    Json,
    /// Payload is SOME/IP payload data.
    SomeIp,
    /// Payload is SOME/IP TLV payload data.
    SomeIpTlv,
    /// Payload is uninterpreted bytes.
    Raw,
    /// Payload is UTF-8 text.
    Text,
    /// Payload is a shared-memory reference.
    Shm,
}

impl UPayloadFormat {
    pub const UPAYLOAD_FORMAT_UNSPECIFIED: Self = Self::Unspecified;
    pub const UPAYLOAD_FORMAT_PROTOBUF_WRAPPED_IN_ANY: Self = Self::ProtobufWrappedInAny;
    pub const UPAYLOAD_FORMAT_PROTOBUF: Self = Self::Protobuf;
    pub const UPAYLOAD_FORMAT_JSON: Self = Self::Json;
    pub const UPAYLOAD_FORMAT_SOMEIP: Self = Self::SomeIp;
    pub const UPAYLOAD_FORMAT_SOMEIP_TLV: Self = Self::SomeIpTlv;
    pub const UPAYLOAD_FORMAT_RAW: Self = Self::Raw;
    pub const UPAYLOAD_FORMAT_TEXT: Self = Self::Text;
    pub const UPAYLOAD_FORMAT_SHM: Self = Self::Shm;

    /// Returns the upstream enum numeric value.
    pub fn value(self) -> u8 {
        match self {
            Self::Unspecified => 0,
            Self::ProtobufWrappedInAny => 1,
            Self::Protobuf => 2,
            Self::Json => 3,
            Self::SomeIp => 4,
            Self::SomeIpTlv => 5,
            Self::Raw => 6,
            Self::Text => 7,
            Self::Shm => 8,
        }
    }

    /// Returns the standard payload format for an upstream enum numeric value.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Unspecified),
            1 => Some(Self::ProtobufWrappedInAny),
            2 => Some(Self::Protobuf),
            3 => Some(Self::Json),
            4 => Some(Self::SomeIp),
            5 => Some(Self::SomeIpTlv),
            6 => Some(Self::Raw),
            7 => Some(Self::Text),
            8 => Some(Self::Shm),
            _ => None,
        }
    }

    /// Returns the standard media type, if this format has one.
    pub fn content_type(self) -> Option<&'static str> {
        match self {
            Self::Unspecified => None,
            Self::ProtobufWrappedInAny => Some("application/x-protobuf"),
            Self::Protobuf => Some("application/protobuf"),
            Self::Json => Some("application/json"),
            Self::SomeIp => Some("application/x-someip"),
            Self::SomeIpTlv => Some("application/x-someip_tlv"),
            Self::Raw => Some("application/octet-stream"),
            Self::Text => Some("text/plain"),
            Self::Shm => Some("application/x-shm"),
        }
    }

    /// Finds a standard payload format for a media type, ignoring parameters.
    pub fn from_content_type(content_type: &str) -> Option<Self> {
        let requested = mediatype::MediaType::parse(content_type).ok()?;
        [
            Self::ProtobufWrappedInAny,
            Self::Protobuf,
            Self::Json,
            Self::SomeIp,
            Self::SomeIpTlv,
            Self::Raw,
            Self::Text,
            Self::Shm,
        ]
        .into_iter()
        .find(|format| {
            format.content_type().is_some_and(|candidate| {
                mediatype::MediaType::parse(candidate).is_ok_and(|candidate| {
                    candidate.ty == requested.ty && candidate.subty == requested.subty
                })
            })
        })
    }
}

/// Native-only payload encoding identity for byte-compatible payload layouts.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CustomPayloadEncoding {
    id: String,
    content_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PayloadEncodingError {
    /// The custom encoding identifier was empty.
    EmptyCustomId,
    /// The custom encoding media type was empty.
    EmptyContentType,
    /// The custom encoding media type was not valid.
    InvalidContentType(String),
}

impl Display for PayloadEncodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyCustomId => f.write_str("custom payload encoding id must not be empty"),
            Self::EmptyContentType => {
                f.write_str("custom payload encoding content_type must not be empty")
            }
            Self::InvalidContentType(error) => f.write_fmt(format_args!(
                "custom payload encoding content_type is not valid: {error}"
            )),
        }
    }
}

impl Error for PayloadEncodingError {}

impl CustomPayloadEncoding {
    /// Creates a custom encoding, panicking if static inputs are invalid.
    pub fn new(id: impl Into<String>, content_type: impl Into<String>) -> Self {
        Self::try_new(id, content_type)
            .expect("custom payload encoding requires non-empty id and valid content_type")
    }

    /// Creates a custom encoding from runtime input.
    pub fn try_new(
        id: impl Into<String>,
        content_type: impl Into<String>,
    ) -> Result<Self, PayloadEncodingError> {
        let id = id.into();
        if id.is_empty() {
            return Err(PayloadEncodingError::EmptyCustomId);
        }
        let content_type = content_type.into();
        if content_type.is_empty() {
            return Err(PayloadEncodingError::EmptyContentType);
        }
        content_type
            .parse::<mediatype::MediaTypeBuf>()
            .map_err(|error| PayloadEncodingError::InvalidContentType(error.to_string()))?;
        Ok(Self::new_unchecked(id, content_type))
    }

    /// Creates a custom encoding without validation.
    pub fn new_unchecked(id: impl Into<String>, content_type: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content_type: content_type.into(),
        }
    }

    /// Returns the native-only custom encoding identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the custom encoding media type.
    pub fn content_type(&self) -> &str {
        &self.content_type
    }
}

/// Identifies the payload representation carried by a native frame.
///
/// Standard encodings are the upstream `UPayloadFormat` values and can be
/// represented by generated `UMessage` envelopes. Custom encodings are native
/// frame metadata for byte-compatible layouts, such as shared-memory/zero-copy
/// structs that applications read without protobuf or JSON serialization. Custom
/// encodings are not representable by the legacy protobuf envelope.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PayloadEncoding {
    /// Upstream-compatible payload format.
    Standard(UPayloadFormat),
    /// Native-only custom payload representation.
    Custom(CustomPayloadEncoding),
}

impl PayloadEncoding {
    /// Creates a standard payload encoding.
    pub fn standard(format: UPayloadFormat) -> Self {
        Self::Standard(format)
    }

    /// Creates a custom payload encoding, panicking if static inputs are invalid.
    pub fn custom(id: impl Into<String>, content_type: impl Into<String>) -> Self {
        Self::Custom(CustomPayloadEncoding::new(id, content_type))
    }

    /// Creates a custom payload encoding from runtime input.
    pub fn try_custom(
        id: impl Into<String>,
        content_type: impl Into<String>,
    ) -> Result<Self, PayloadEncodingError> {
        CustomPayloadEncoding::try_new(id, content_type).map(Self::Custom)
    }

    /// Creates a custom payload encoding without validation.
    pub fn custom_unchecked(id: impl Into<String>, content_type: impl Into<String>) -> Self {
        Self::Custom(CustomPayloadEncoding::new_unchecked(id, content_type))
    }

    /// Creates a standard encoding if the content type is known, otherwise a
    /// custom encoding whose id is the content type.
    pub fn from_content_type(content_type: impl Into<String>) -> Self {
        let content_type = content_type.into();
        UPayloadFormat::from_content_type(&content_type).map_or_else(
            || Self::custom(content_type.clone(), content_type),
            Self::Standard,
        )
    }

    /// Returns the upstream payload format when this is a standard encoding.
    pub fn standard_format(&self) -> Option<UPayloadFormat> {
        match self {
            Self::Standard(format) => Some(*format),
            Self::Custom(_) => None,
        }
    }

    /// Returns the native-only custom encoding when present.
    pub fn custom_encoding(&self) -> Option<&CustomPayloadEncoding> {
        match self {
            Self::Standard(_) => None,
            Self::Custom(encoding) => Some(encoding),
        }
    }

    /// Returns the media type when one is known.
    pub fn content_type(&self) -> Option<&str> {
        match self {
            Self::Standard(format) => format.content_type(),
            Self::Custom(encoding) => Some(encoding.content_type()),
        }
    }

    /// Returns whether this actual frame encoding can be decoded by a decoder
    /// that declares `expected`.
    pub fn is_compatible_with(&self, expected: &Self) -> bool {
        self == expected
    }
}

impl Default for PayloadEncoding {
    fn default() -> Self {
        RawBytes::encoding()
    }
}

/// Deprecated compatibility alias for the native payload encoding type.
pub type UEncoding = PayloadEncoding;

/// Deprecated compatibility alias for custom payload encoding validation errors.
pub type UEncodingError = PayloadEncodingError;

/// Native frame metadata used by transport APIs.
///
/// `UFrameMetadata` groups native uProtocol [`UAttributes`] with optional
/// payload [`PayloadEncoding`]. It is not a generated protocol envelope and it is not a
/// replacement for `UAttributes`; transports should project these fields into
/// their own metadata channels where possible. Prefer [`crate::UFrameBuilder`]
/// for constructing application frames because it validates message-type-specific
/// attribute rules before producing an owned frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UFrameMetadata {
    attributes: UAttributes,
    encoding: Option<PayloadEncoding>,
}

impl UFrameMetadata {
    /// Creates frame metadata from native attributes and optional payload encoding.
    ///
    /// The encoding must be `Some` exactly when the corresponding frame carries a
    /// payload. Use [`Self::without_payload_encoding`] for no-payload frames.
    pub fn new(attributes: UAttributes, encoding: impl Into<Option<PayloadEncoding>>) -> Self {
        Self {
            attributes,
            encoding: encoding.into(),
        }
    }

    /// Creates metadata for a frame with no payload bytes.
    pub fn without_payload_encoding(attributes: UAttributes) -> Self {
        Self::new(attributes, None::<PayloadEncoding>)
    }

    /// Creates unchecked Publish metadata.
    ///
    /// Prefer [`Self::try_publish`] or [`crate::UFrameBuilder::publish`] for application frames.
    pub fn publish(topic: UUri) -> Self {
        Self::new(
            UAttributes::new(UUID::build(), topic, None, UMessageType::Publish),
            None::<PayloadEncoding>,
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
            None::<PayloadEncoding>,
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
            None::<PayloadEncoding>,
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
            None::<PayloadEncoding>,
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
    pub fn encoding(&self) -> Option<&PayloadEncoding> {
        self.encoding.as_ref()
    }

    /// Returns metadata with payload encoding set to `encoding`.
    #[must_use]
    pub fn with_encoding(mut self, encoding: PayloadEncoding) -> Self {
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
/// helpers use [`PayloadFormat`] implementations to set or verify [`PayloadEncoding`]
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
        Self::from_payload_as::<F, T>(metadata, value)
    }

    /// Encodes `value` with payload codec `C` and returns an owned frame.
    ///
    /// Existing [`PayloadFormat`] codecs work here through their compatibility
    /// adapter. New codecs should implement [`PayloadCodec`] and
    /// [`EncodePayload`].
    pub fn from_payload_as<C, T>(metadata: UFrameMetadata, value: &T) -> Result<Self, UWireError>
    where
        C: PayloadCodec + EncodePayload<T>,
        T: ?Sized,
    {
        let payload = C::encode_payload_owned(value)?;
        Ok(Self::new(
            metadata.with_encoding(C::payload_encoding()),
            payload,
        ))
    }

    /// Creates a payload-bearing frame from already encoded owned bytes.
    ///
    /// This is the no-extra-copy path for byte-oriented codecs such as
    /// [`RawBytes`](crate::payload::RawBytes) and [`McapPayload`](crate::payload::McapPayload):
    /// the supplied [`Bytes`] buffer is moved into the frame after metadata is
    /// tagged with `C::payload_encoding()`.
    pub fn from_bytes_as<C>(metadata: UFrameMetadata, payload: impl Into<Bytes>) -> Self
    where
        C: PayloadCodec + BytePayloadCodec,
    {
        Self::from_encoded_payload(metadata, EncodedPayload::<C>::from_bytes(payload))
    }

    /// Creates a payload-bearing frame from already encoded typed payload bytes.
    pub fn from_encoded_payload<C>(metadata: UFrameMetadata, payload: EncodedPayload<C>) -> Self
    where
        C: PayloadCodec,
    {
        Self::new(
            metadata.with_encoding(C::payload_encoding()),
            payload.into_bytes(),
        )
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

    /// Consumes the frame and returns metadata plus optional payload bytes.
    pub fn into_parts(self) -> (UFrameMetadata, Option<Bytes>) {
        (self.metadata, self.payload)
    }

    /// Deserializes the payload with codec `F` after verifying encoding metadata.
    ///
    /// The frame must carry a payload and a compatible [`PayloadEncoding`].
    pub fn deserialize<'a, F, T>(&'a self) -> Result<T, UWireError>
    where
        F: PayloadFormat,
        T: UDeserializer<'a, F>,
    {
        self.decode_payload_as::<F, T>()
    }

    /// Decodes the payload with codec `C` after verifying encoding metadata.
    pub fn decode_payload_as<'a, C, T>(&'a self) -> Result<T, UWireError>
    where
        C: PayloadCodec + DecodePayload<'a, T>,
    {
        let payload = self.payload.as_deref().ok_or(UWireError::MissingPayload)?;
        C::verify_encoding(self.metadata.encoding())?;
        C::decode_payload(payload)
    }

    /// Borrows a typed payload view with codec `C` after verifying encoding metadata.
    pub fn borrow_payload_as<'a, C, T>(&'a self) -> Result<&'a T, UWireError>
    where
        C: PayloadCodec + BorrowPayload<T>,
        T: ?Sized,
    {
        let payload = self.payload.as_deref().ok_or(UWireError::MissingPayload)?;
        C::verify_encoding(self.metadata.encoding())?;
        C::borrow_payload(payload)
    }
}

impl AsRef<[u8]> for UOwnedFrame {
    fn as_ref(&self) -> &[u8] {
        self.payload_bytes()
    }
}
