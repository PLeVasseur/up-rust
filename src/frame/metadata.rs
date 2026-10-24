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

//! Semantic metadata shared by frame carriers.
//!
//! [`UFrameMetadata`] is the canonical metadata model for native uProtocol
//! frames (owned frames, zero-copy frame views, and selected-wire paths).
//! It carries semantic fields directly and contains no `UAttributes` or
//! protobuf concepts. Payload identity uses the unified [`PayloadEncoding`].
//!
//! Message-model compatibility lives at the edges of this module as *fallible
//! projections*:
//!
//! * [`try_project_umessage_to_frame_metadata`] / [`try_project_frame_to_umessage`]
//!   convert between `UMessage` and native frames.
//! * [`UAttributes::try_project_to_frame_metadata`] /
//!   [`UFrameMetadata::try_project_to_attributes`] convert between
//!   `UAttributes` and native frame metadata.
//!
//! Projections fail rather than truncate or silently drop information
//! when the target model cannot represent the source (for example a TTL with
//! sub-millisecond precision).
//!
//! Fixed-layout representations of this metadata (for C/C++ shared-memory
//! interop) are *derived profiles*, not this type: see the `frame::abi`
//! module. Byte serializations for transports are produced by selected-wire
//! metadata codecs such as [`crate::frame::codec`].

use std::time::Duration;

use bytes::Bytes;

#[cfg(feature = "native-identity")]
use super::native::{NativePayloadIdentity, NativeProfileAgreement};
use crate::uattributes::PayloadEncoding;
use crate::{UAttributes, UCode, UMessage, UMessageError, UMessageType, UPriority, UUri, UUID};

/// An opaque structural native type token, independent of payload encoding IDs.
///
/// All 32-bit values, including zero, can be carried without `native-identity`.
/// Equality alone does not authorize typed access: full representation agreement,
/// layout, bit validity and backing lifetime must also be established. Descriptor
/// construction and token derivation require the `native-identity` feature.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct NativeTypeToken(u32);

impl NativeTypeToken {
    /// Preserves a received token without interpreting its payload.
    pub const fn from_u32(value: u32) -> Self {
        Self(value)
    }

    /// Returns the token's complete 32-bit value.
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

// ---------------------------------------------------------------------------
// PayloadEncoding: the unified registry-id identity, defined in
// `crate::uattributes::payloadencoding` and re-exported at the crate root.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors returned by native frame metadata operations and projections.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum UFrameMetadataError {
    /// Payload bytes require a payload encoding.
    PayloadWithoutEncoding,
    /// A payload encoding requires payload bytes.
    EncodingWithoutPayload,
    /// A semantic field cannot be represented by legacy types.
    FieldNotRepresentable {
        /// Name of the offending field.
        field: &'static str,
        /// Reason the value is not representable.
        reason: String,
    },
    /// The metadata violates the validity rules for its frame message kind.
    InvalidMetadata(String),
    /// Building a `UMessage` from projected metadata failed.
    MessageBuildError(String),
}

impl std::fmt::Display for UFrameMetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PayloadWithoutEncoding => f.write_str("payload bytes require a payload encoding"),
            Self::EncodingWithoutPayload => f.write_str("payload encoding requires payload bytes"),
            Self::FieldNotRepresentable { field, reason } => f.write_fmt(format_args!(
                "frame metadata field `{field}` cannot be represented by legacy types: {reason}"
            )),
            Self::InvalidMetadata(error) => {
                f.write_fmt(format_args!("invalid frame metadata: {error}"))
            }
            Self::MessageBuildError(error) => f.write_fmt(format_args!(
                "failed to build UMessage from frame metadata: {error}"
            )),
        }
    }
}

impl std::error::Error for UFrameMetadataError {}

impl From<UMessageError> for UFrameMetadataError {
    fn from(value: UMessageError) -> Self {
        Self::MessageBuildError(value.to_string())
    }
}

// ---------------------------------------------------------------------------
// UFrameMetadata
// ---------------------------------------------------------------------------

/// Canonical semantic metadata of a native uProtocol frame.
///
/// This is the model that owned frames (`UOwnedFrame`), zero-copy
/// frame views, and selected-wire metadata codecs reason about. It owns its
/// fields ergonomically (strings, options, [`Duration`]) because it is never
/// reinterpreted as raw bytes across processes or languages; fixed-layout
/// and byte-serialized representations are derived from it.
///
/// The frame's payload presence is a property of the frame carrier
/// (`UOwnedFrame`, `UFrameView`, TX loan specs), not of the
/// metadata; the v1 invariant "payload present if and only if
/// `payload_encoding` is present" is enforced at those boundaries.
#[derive(Clone, Debug, PartialEq)]
pub struct UFrameMetadata {
    kind: UMessageType,
    id: UUID,
    source: UUri,
    sink: Option<UUri>,
    reqid: Option<UUID>,
    priority: Option<UPriority>,
    ttl: Option<Duration>,
    comm_status: Option<UCode>,
    permission_level: Option<u32>,
    token: Option<String>,
    traceparent: Option<String>,
    payload_encoding: Option<PayloadEncoding>,
    native_type_token: Option<NativeTypeToken>,
}

impl UFrameMetadata {
    /// Starts building metadata for a publish frame on `topic`.
    #[must_use]
    pub fn publish(topic: UUri) -> UFrameMetadataBuilder {
        UFrameMetadataBuilder::new(UMessageType::Publish, topic, None)
    }

    /// Starts building metadata for a notification frame from `origin` to
    /// `destination`.
    #[must_use]
    pub fn notification(origin: UUri, destination: UUri) -> UFrameMetadataBuilder {
        UFrameMetadataBuilder::new(UMessageType::Notification, origin, Some(destination))
    }

    /// Starts building metadata for an RPC request frame invoking `method`,
    /// with responses directed to `reply_to`.
    ///
    /// Request frames require a TTL and default to [`UPriority::CS4`].
    #[must_use]
    pub fn request(method: UUri, reply_to: UUri, ttl: Duration) -> UFrameMetadataBuilder {
        let mut builder = UFrameMetadataBuilder::new(UMessageType::Request, reply_to, Some(method));
        builder.priority = Some(UPriority::CS4);
        builder.ttl = Some(ttl);
        builder
    }

    /// Starts building metadata for an RPC response frame from `invoked_method`
    /// to `reply_to`, correlated to the request with id `request_id`.
    ///
    /// Response frames default to [`UPriority::CS4`].
    #[must_use]
    pub fn response(
        invoked_method: UUri,
        reply_to: UUri,
        request_id: UUID,
    ) -> UFrameMetadataBuilder {
        let mut builder =
            UFrameMetadataBuilder::new(UMessageType::Response, invoked_method, Some(reply_to));
        builder.priority = Some(UPriority::CS4);
        builder.reqid = Some(request_id);
        builder
    }

    /// Assembles metadata from decoded parts without validation.
    ///
    /// Callers (frame codecs, ABI profile conversions) must run
    /// [`UFrameMetadata::validate`] on the result.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_decoded_parts(
        kind: UMessageType,
        id: UUID,
        source: UUri,
        sink: Option<UUri>,
        reqid: Option<UUID>,
        priority: Option<UPriority>,
        ttl: Option<Duration>,
        comm_status: Option<UCode>,
        permission_level: Option<u32>,
        token: Option<String>,
        traceparent: Option<String>,
        payload_encoding: Option<PayloadEncoding>,
        native_type_token: Option<NativeTypeToken>,
    ) -> Self {
        Self {
            kind,
            id,
            source,
            sink,
            reqid,
            priority,
            ttl,
            comm_status,
            permission_level,
            token,
            traceparent,
            payload_encoding,
            native_type_token,
        }
    }

    /// Returns the frame message kind.
    #[must_use]
    pub fn kind(&self) -> UMessageType {
        self.kind
    }

    /// Returns the unique frame id.
    #[must_use]
    pub fn id(&self) -> &UUID {
        &self.id
    }

    /// Returns the source address.
    #[must_use]
    pub fn source(&self) -> &UUri {
        &self.source
    }

    /// Returns the sink address, if present.
    #[must_use]
    pub fn sink(&self) -> Option<&UUri> {
        self.sink.as_ref()
    }

    /// Returns the correlated request id, if present.
    #[must_use]
    pub fn reqid(&self) -> Option<&UUID> {
        self.reqid.as_ref()
    }

    /// Returns the frame priority, if present.
    #[must_use]
    pub fn priority(&self) -> Option<UPriority> {
        self.priority
    }

    /// Returns the time-to-live, if present.
    #[must_use]
    pub fn ttl(&self) -> Option<Duration> {
        self.ttl
    }

    /// Returns the communication status, if present.
    #[must_use]
    pub fn comm_status(&self) -> Option<UCode> {
        self.comm_status
    }

    /// Returns the permission level, if present.
    #[must_use]
    pub fn permission_level(&self) -> Option<u32> {
        self.permission_level
    }

    /// Returns the access token, if present.
    #[must_use]
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// Returns the W3C traceparent, if present.
    #[must_use]
    pub fn traceparent(&self) -> Option<&str> {
        self.traceparent.as_deref()
    }

    /// Returns the native payload encoding, if one is present.
    #[must_use]
    pub fn payload_encoding(&self) -> Option<&PayloadEncoding> {
        self.payload_encoding.as_ref()
    }

    /// Returns the independent native type token, if carried. It is an opaque
    /// claim until checked against an agreed profile and complete representation.
    pub fn native_type_token(&self) -> Option<NativeTypeToken> {
        self.native_type_token
    }

    /// Sets an opaque native token without interpreting the payload.
    ///
    /// # Errors
    ///
    /// Rejects a token without payload encoding or invalid frame metadata.
    pub fn with_native_type_token(
        mut self,
        token: NativeTypeToken,
    ) -> Result<Self, UFrameMetadataError> {
        self.native_type_token = Some(token);
        self.validate()?;
        Ok(self)
    }

    /// Sets both parts of an identity resolved under an explicit native agreement.
    ///
    /// # Errors
    ///
    /// Returns an error if the resulting metadata violates message-kind rules.
    #[cfg(feature = "native-identity")]
    pub fn with_native_payload_identity(
        mut self,
        identity: NativePayloadIdentity,
    ) -> Result<Self, UFrameMetadataError> {
        self.payload_encoding = Some(identity.encoding());
        self.native_type_token = Some(identity.token());
        self.validate()?;
        Ok(self)
    }

    /// Consumes this metadata and returns its native payload encoding.
    #[must_use]
    pub fn into_payload_encoding(self) -> Option<PayloadEncoding> {
        self.payload_encoding
    }

    /// Returns metadata equal to `self` but carrying `payload_encoding`.
    ///
    /// # Errors
    ///
    /// Returns an error if the encoding or the resulting metadata is invalid.
    pub fn with_payload_encoding(
        mut self,
        payload_encoding: PayloadEncoding,
    ) -> Result<Self, UFrameMetadataError> {
        if self.native_type_token.is_some() && self.payload_encoding != Some(payload_encoding) {
            return Err(UFrameMetadataError::InvalidMetadata(
                "cannot change encoding independently of a carried native token".to_string(),
            ));
        }
        self.payload_encoding = Some(payload_encoding);
        self.validate()?;
        Ok(self)
    }

    /// Removes payload identity, including any native token, for a payload-free frame.
    #[must_use]
    pub fn without_payload_encoding(mut self) -> Self {
        self.payload_encoding = None;
        self.native_type_token = None;
        self
    }

    /// Validates this metadata against the rules of its frame message kind.
    ///
    /// # Errors
    ///
    /// Returns an error if the metadata violates the rules for its kind, for
    /// example a publish frame carrying a sink, a request frame without TTL,
    /// or an RPC frame with a priority below [`UPriority::CS4`].
    pub fn validate(&self) -> Result<(), UFrameMetadataError> {
        let mut errors: Vec<String> = Vec::new();
        if self.native_type_token.is_some() && self.payload_encoding.is_none() {
            errors.push("native type token requires payload encoding".to_string());
        }
        match self.kind {
            UMessageType::Publish => {
                if let Err(e) = self.source.verify_event() {
                    errors.push(format!("invalid source URI: {e}"));
                }
                if self.sink.is_some() {
                    errors.push("publish frame must not have a sink".to_string());
                }
            }
            UMessageType::Notification => {
                if let Err(e) = self.source.verify_event() {
                    errors.push(format!("invalid source URI: {e}"));
                }
                match &self.sink {
                    Some(sink) => {
                        if !sink.is_notification_destination() {
                            errors.push("sink is not a valid notification destination".to_string());
                        } else if let Err(e) = sink.verify_no_wildcards() {
                            errors.push(format!("invalid sink URI: {e}"));
                        }
                    }
                    None => errors.push("notification frame must have a sink".to_string()),
                }
            }
            UMessageType::Request => {
                if let Err(e) = self.source.verify_rpc_response() {
                    errors.push(format!("invalid source URI: {e}"));
                }
                match &self.sink {
                    Some(sink) => {
                        if let Err(e) = sink.verify_rpc_method() {
                            errors.push(format!("invalid sink URI: {e}"));
                        }
                    }
                    None => errors
                        .push("request frame must have a method-to-invoke in the sink".to_string()),
                }
                match self.ttl {
                    Some(ttl) if !ttl.is_zero() => {}
                    Some(_) => {
                        errors.push("request frame TTL must be greater than zero".to_string())
                    }
                    None => errors.push("request frame must have a TTL".to_string()),
                }
                if let Err(e) = self.validate_rpc_priority() {
                    errors.push(e);
                }
            }
            UMessageType::Response => {
                if let Err(e) = self.source.verify_rpc_method() {
                    errors.push(format!("invalid source URI: {e}"));
                }
                match &self.sink {
                    Some(sink) => {
                        if let Err(e) = sink.verify_rpc_response() {
                            errors.push(format!("invalid sink URI: {e}"));
                        }
                    }
                    None => errors.push("response frame must have a sink".to_string()),
                }
                if self.reqid.is_none() {
                    errors.push("response frame must have a request id".to_string());
                }
                if let Err(e) = self.validate_rpc_priority() {
                    errors.push(e);
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(UFrameMetadataError::InvalidMetadata(errors.join("; ")))
        }
    }

    fn validate_rpc_priority(&self) -> Result<(), String> {
        match self.priority {
            Some(priority) if priority >= UPriority::CS4 => Ok(()),
            Some(_) => Err("RPC frame must have a priority of at least CS4".to_string()),
            None => Err("RPC frame must have a priority".to_string()),
        }
    }

    /// Projects this metadata to the `UAttributes` representation.
    ///
    /// # Errors
    ///
    /// Returns an error if the metadata is invalid or not representable.
    pub fn try_project_to_attributes(&self) -> Result<UAttributes, UFrameMetadataError> {
        self.validate()?;
        if self.native_type_token.is_some() {
            return Err(UFrameMetadataError::FieldNotRepresentable {
                field: "native_type_token",
                reason: "classic attributes require a native agreement before token removal"
                    .to_string(),
            });
        }
        Ok(UAttributes {
            type_: self.kind,
            id: self.id.clone(),
            source: self.source.clone(),
            sink: self.sink.clone(),
            priority: self.priority,
            commstatus: self.comm_status,
            ttl: self
                .ttl
                .filter(|ttl| !ttl.is_zero())
                .map(project_ttl_to_legacy_millis)
                .transpose()?,
            permission_level: self.permission_level,
            token: self.token.clone(),
            traceparent: self.traceparent.clone(),
            reqid: self.reqid.clone(),
            payload_encoding_id: self.payload_encoding,
        })
    }

    /// Projects attributes through a confirmed native table/operation agreement.
    /// This checks the carried pair before removing the native-only token.
    ///
    /// # Errors
    ///
    /// Rejects wrong or missing tokens for agreed native IDs, unknown token-bearing
    /// IDs and ordinary attribute projection failures. No payload codec is run.
    #[cfg(feature = "native-identity")]
    pub fn try_project_to_attributes_with_native_agreement(
        &self,
        agreement: &NativeProfileAgreement,
    ) -> Result<UAttributes, UFrameMetadataError> {
        self.validate()?;
        if let Some(encoding) = self.payload_encoding {
            match self.native_type_token {
                Some(token) => agreement.verify_carried(encoding, token).map_err(|error| {
                    UFrameMetadataError::FieldNotRepresentable {
                        field: "native_type_token",
                        reason: error.to_string(),
                    }
                })?,
                None if agreement.identity_for_encoding(encoding).is_some() => {
                    return Err(UFrameMetadataError::FieldNotRepresentable {
                        field: "native_type_token",
                        reason: "agreed native representation has no carried token".to_string(),
                    });
                }
                None => {}
            }
        }
        let mut projected = self.clone();
        projected.native_type_token = None;
        projected.try_project_to_attributes()
    }
}

fn project_ttl_to_legacy_millis(ttl: Duration) -> Result<u32, UFrameMetadataError> {
    if !ttl.subsec_nanos().is_multiple_of(1_000_000) {
        return Err(UFrameMetadataError::FieldNotRepresentable {
            field: "ttl",
            reason: format!(
                "{ttl:?} has sub-millisecond precision; legacy TTL is in whole milliseconds"
            ),
        });
    }
    u32::try_from(ttl.as_millis()).map_err(|_| UFrameMetadataError::FieldNotRepresentable {
        field: "ttl",
        reason: format!("{ttl:?} exceeds the legacy 32-bit millisecond TTL range"),
    })
}

// ---------------------------------------------------------------------------
// UFrameMetadataBuilder
// ---------------------------------------------------------------------------

/// Builder for native frame metadata.
///
/// Obtained from [`UFrameMetadata::publish`], [`UFrameMetadata::notification`],
/// [`UFrameMetadata::request`], or [`UFrameMetadata::response`]. Native
/// selected-wire code should construct metadata through this builder instead
/// of going through `UMessageBuilder` and projecting.
#[derive(Clone, Debug)]
pub struct UFrameMetadataBuilder {
    kind: UMessageType,
    id: Option<UUID>,
    source: UUri,
    sink: Option<UUri>,
    reqid: Option<UUID>,
    priority: Option<UPriority>,
    ttl: Option<Duration>,
    comm_status: Option<UCode>,
    permission_level: Option<u32>,
    token: Option<String>,
    traceparent: Option<String>,
    payload_encoding: Option<PayloadEncoding>,
    native_type_token: Option<NativeTypeToken>,
}

impl UFrameMetadataBuilder {
    fn new(kind: UMessageType, source: UUri, sink: Option<UUri>) -> Self {
        Self {
            kind,
            id: None,
            source,
            sink,
            reqid: None,
            priority: None,
            ttl: None,
            comm_status: None,
            permission_level: None,
            token: None,
            traceparent: None,
            payload_encoding: None,
            native_type_token: None,
        }
    }

    /// Sets an explicit frame id instead of a generated one.
    #[must_use]
    pub fn with_id(mut self, id: UUID) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the frame priority.
    #[must_use]
    pub fn with_priority(mut self, priority: UPriority) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Sets the time-to-live.
    #[must_use]
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Sets the communication status.
    #[must_use]
    pub fn with_comm_status(mut self, comm_status: UCode) -> Self {
        self.comm_status = Some(comm_status);
        self
    }

    /// Sets the permission level.
    #[must_use]
    pub fn with_permission_level(mut self, permission_level: u32) -> Self {
        self.permission_level = Some(permission_level);
        self
    }

    /// Sets the access token.
    #[must_use]
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Sets the W3C traceparent.
    #[must_use]
    pub fn with_traceparent(mut self, traceparent: impl Into<String>) -> Self {
        self.traceparent = Some(traceparent.into());
        self
    }

    /// Sets the payload encoding of the frame's payload bytes.
    #[must_use]
    pub fn with_payload_encoding(mut self, payload_encoding: PayloadEncoding) -> Self {
        self.payload_encoding = Some(payload_encoding);
        self
    }

    /// Sets an opaque native token; building still requires a payload encoding.
    pub fn with_native_type_token(mut self, token: NativeTypeToken) -> Self {
        self.native_type_token = Some(token);
        self
    }

    /// Sets the pair resolved by an explicit deployment agreement.
    #[cfg(feature = "native-identity")]
    pub fn with_native_payload_identity(mut self, identity: NativePayloadIdentity) -> Self {
        self.payload_encoding = Some(identity.encoding());
        self.native_type_token = Some(identity.token());
        self
    }

    /// Builds and validates the frame metadata.
    ///
    /// A fresh UUIDv7 frame id is generated unless one was supplied via
    /// [`Self::with_id`].
    ///
    /// # Errors
    ///
    /// Returns an error if the metadata violates the rules of its frame
    /// message kind.
    pub fn build(self) -> Result<UFrameMetadata, UFrameMetadataError> {
        if let Some(encoding) = self.payload_encoding {
            encoding
                .check_originable()
                .map_err(|error| UFrameMetadataError::InvalidMetadata(error.to_string()))?;
        }
        let metadata = UFrameMetadata {
            kind: self.kind,
            id: self.id.unwrap_or_else(UUID::build),
            source: self.source,
            sink: self.sink,
            reqid: self.reqid,
            priority: self.priority,
            ttl: self.ttl,
            comm_status: self.comm_status,
            permission_level: self.permission_level,
            token: self.token,
            traceparent: self.traceparent,
            payload_encoding: self.payload_encoding,
            native_type_token: self.native_type_token,
        };
        metadata.validate()?;
        Ok(metadata)
    }
}

// ---------------------------------------------------------------------------
// Message-model projections
// ---------------------------------------------------------------------------

impl crate::UAttributes {
    /// Projects these attributes using their own optional payload encoding.
    ///
    /// This attribute-only projection has no payload storage to check. When a
    /// complete message is available, use [`try_project_umessage_to_frame_metadata`]
    /// to check payload presence as well. Native token reconstruction additionally
    /// requires `native-identity` and
    /// `try_project_umessage_to_frame_metadata_with_native_agreement`.
    ///
    /// # Errors
    ///
    /// Returns an error if the resulting frame metadata is invalid.
    pub fn try_project_to_frame_metadata(&self) -> Result<UFrameMetadata, UFrameMetadataError> {
        try_project_attributes_to_frame_metadata(self)
    }
}

pub(crate) fn try_project_attributes_to_frame_metadata(
    attributes: &UAttributes,
) -> Result<UFrameMetadata, UFrameMetadataError> {
    let metadata = UFrameMetadata {
        kind: attributes.type_,
        id: attributes.id.clone(),
        source: attributes.source.clone(),
        sink: attributes.sink.clone(),
        reqid: attributes.reqid.clone(),
        priority: attributes.priority,
        ttl: attributes
            .ttl
            .filter(|ttl| *ttl != 0)
            .map(u64::from)
            .map(Duration::from_millis),
        comm_status: attributes.commstatus,
        permission_level: attributes.permission_level,
        token: attributes.token.clone(),
        traceparent: attributes.traceparent.clone(),
        payload_encoding: attributes.payload_encoding(),
        native_type_token: None,
    };
    metadata.validate()?;
    Ok(metadata)
}

/// Projects a `UMessage` into native frame metadata.
///
/// # Errors
///
/// Returns an error when a message carries payload bytes without a concrete
/// payload encoding, or a concrete payload encoding without payload bytes.
pub fn try_project_umessage_to_frame_metadata(
    message: &UMessage,
) -> Result<UFrameMetadata, UFrameMetadataError> {
    let encoding_present = message.attributes().payload_encoding().is_some();
    match (message.payload().is_some(), encoding_present) {
        (true, false) => {
            return Err(UFrameMetadataError::PayloadWithoutEncoding);
        }
        (false, true) => {
            return Err(UFrameMetadataError::EncodingWithoutPayload);
        }
        _ => {}
    }
    try_project_attributes_to_frame_metadata(message.attributes())
}

/// Projects native frame metadata and optional payload bytes into a
/// `UMessage`.
///
/// # Errors
///
/// Returns an error if payload bytes and payload encoding are not both
/// present or both absent, if the encoding is not representable, or if
/// metadata invariants are violated.
pub fn try_project_frame_to_umessage(
    metadata: UFrameMetadata,
    payload: Option<Bytes>,
) -> Result<UMessage, UFrameMetadataError> {
    match (payload.is_some(), metadata.payload_encoding().is_some()) {
        (true, true) | (false, false) => {}
        (true, false) => return Err(UFrameMetadataError::PayloadWithoutEncoding),
        (false, true) => return Err(UFrameMetadataError::EncodingWithoutPayload),
    }
    let attributes = metadata.try_project_to_attributes()?;
    UMessage::new(attributes, payload).map_err(UFrameMetadataError::from)
}

/// Reconstructs native identity from the carried classic encoding and an agreed
/// deployment mapping. Unknown IDs remain opaque, with no invented token.
///
/// # Errors
///
/// Returns an error for inconsistent payload presence or invalid metadata.
#[cfg(feature = "native-identity")]
pub fn try_project_umessage_to_frame_metadata_with_native_agreement(
    message: &UMessage,
    agreement: &NativeProfileAgreement,
) -> Result<UFrameMetadata, UFrameMetadataError> {
    let mut metadata = try_project_umessage_to_frame_metadata(message)?;
    if let Some(identity) = metadata
        .payload_encoding()
        .and_then(|encoding| agreement.identity_for_encoding(*encoding))
    {
        metadata = metadata.with_native_payload_identity(identity)?;
    }
    Ok(metadata)
}

/// Projects a complete native frame through a confirmed deployment agreement,
/// checking identity before dropping the native-only token. Bytes are unchanged.
///
/// # Errors
///
/// Rejects inconsistent presence, missing/wrong native identity or unrepresentable
/// metadata. In particular positive sub-millisecond TTLs are never truncated.
#[cfg(feature = "native-identity")]
pub fn try_project_frame_to_umessage_with_native_agreement(
    metadata: UFrameMetadata,
    payload: Option<Bytes>,
    agreement: &NativeProfileAgreement,
) -> Result<UMessage, UFrameMetadataError> {
    match (payload.is_some(), metadata.payload_encoding().is_some()) {
        (true, true) | (false, false) => {}
        (true, false) => return Err(UFrameMetadataError::PayloadWithoutEncoding),
        (false, true) => return Err(UFrameMetadataError::EncodingWithoutPayload),
    }
    let attributes = metadata.try_project_to_attributes_with_native_agreement(agreement)?;
    UMessage::new(attributes, payload).map_err(UFrameMetadataError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UCode, UMessageBuilder, UUri};
    use test_case::test_case;

    #[cfg(feature = "native-identity")]
    mod native_agreements {
        use super::*;
        use crate::frame::native::{
            NativeByteOrder, NativeProfile, NativeProfileMode, NativeProfileTable,
            NativeRepresentation, NativeSingleType,
        };
        use ::test_case::test_case;
        use std::sync::Arc;

        fn native_agreement(id: u32) -> (NativeProfileAgreement, NativePayloadIdentity) {
            let representation = NativeRepresentation::new(
                "test.u32",
                1,
                4,
                4,
                NativeByteOrder::LittleEndian,
                vec![],
            )
            .unwrap();
            let mode = if id == 0 {
                NativeProfileMode::SingleType(NativeSingleType::new(representation.clone()))
            } else {
                NativeProfileMode::Table(
                    NativeProfileTable::new([(
                        PayloadEncoding::from_id(id).unwrap(),
                        representation.clone(),
                    )])
                    .unwrap(),
                )
            };
            let profile = NativeProfile::new("test-domain", 1, mode).unwrap();
            let agreement =
                NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
            let identity = agreement.identity_for(&representation).unwrap();
            (agreement, identity)
        }

        #[test_case(0, b""; "single type empty payload")]
        #[test_case(0, b"\xff\0\xfe"; "single type opaque bytes")]
        #[test_case(0xF001, b""; "private empty payload")]
        #[test_case(0xF001, b"\xff\0\xfe"; "private opaque bytes")]
        fn native_classic_projection_requires_agreement_and_preserves_bytes(
            id: u32,
            payload: &'static [u8],
        ) {
            let (agreement, identity) = native_agreement(id);
            let metadata = UFrameMetadata::publish(topic())
                .with_native_payload_identity(identity)
                .build()
                .unwrap();
            let bytes = Bytes::from_static(payload);
            assert!(matches!(
                try_project_frame_to_umessage(metadata.clone(), Some(bytes.clone())),
                Err(UFrameMetadataError::FieldNotRepresentable {
                    field: "native_type_token",
                    ..
                })
            ));
            let message = try_project_frame_to_umessage_with_native_agreement(
                metadata.clone(),
                Some(bytes.clone()),
                &agreement,
            )
            .unwrap();
            assert_eq!(message.payload(), Some(bytes));
            assert_eq!(
                message.attributes().payload_encoding(),
                Some(identity.encoding())
            );
            let restored =
                try_project_umessage_to_frame_metadata_with_native_agreement(&message, &agreement)
                    .unwrap();
            assert_eq!(restored, metadata);
        }

        #[test_case(0xF001, None; "missing native token")]
        #[test_case(0xF001, Some(0); "wrong native token")]
        #[test_case(0xF002, Some(0); "unknown ID with token")]
        fn native_projection_rejects_missing_or_wrong_identity(id: u32, token: Option<u32>) {
            let (agreement, _) = native_agreement(0xF001);
            let mut builder = UFrameMetadata::publish(topic())
                .with_payload_encoding(PayloadEncoding::from_id(id).unwrap());
            if let Some(token) = token {
                builder = builder.with_native_type_token(NativeTypeToken::from_u32(token));
            }
            let metadata = builder.build().unwrap();
            assert!(matches!(
                try_project_frame_to_umessage_with_native_agreement(
                    metadata,
                    Some(Bytes::new()),
                    &agreement
                ),
                Err(UFrameMetadataError::FieldNotRepresentable {
                    field: "native_type_token",
                    ..
                })
            ));
        }

        #[test_case(8; "unassigned classic ID")]
        #[test_case(0xE000; "reserved classic ID")]
        #[test_case(0xF123; "private ID outside agreed table")]
        fn unknown_classic_encoding_does_not_invent_native_identity(id: u32) {
            let (agreement, _) = native_agreement(0xF001);
            let encoding = PayloadEncoding::from_id(id).unwrap();
            // Carriage test: re-stamped as a carrier would, since builders refuse
            // to originate reserved identifiers.
            let message = UMessageBuilder::publish(topic())
                .build_with_payload(Bytes::new(), PayloadEncoding::RAW)
                .unwrap()
                .with_declared_payload_encoding(encoding)
                .unwrap();
            let metadata =
                try_project_umessage_to_frame_metadata_with_native_agreement(&message, &agreement)
                    .unwrap();
            assert_eq!(metadata.payload_encoding(), Some(&encoding));
            assert_eq!(metadata.native_type_token(), None);
        }

        #[test]
        fn native_identity_presence_and_pair_are_not_silently_changed() {
            let (_, identity) = native_agreement(0xF001);
            assert!(matches!(
                UFrameMetadata::publish(topic())
                    .with_native_type_token(identity.token())
                    .build(),
                Err(UFrameMetadataError::InvalidMetadata(_))
            ));
            let metadata = UFrameMetadata::publish(topic())
                .with_native_payload_identity(identity)
                .build()
                .unwrap();
            assert!(matches!(
                metadata.clone().with_payload_encoding(PayloadEncoding::RAW),
                Err(UFrameMetadataError::InvalidMetadata(_))
            ));
            let absent = metadata.without_payload_encoding();
            assert_eq!(absent.native_type_token(), None);
            assert!(absent.validate().is_ok());
        }
    }

    #[test_case(0xE000; "reserved lower bound")]
    #[test_case(0xEFFF; "reserved upper bound")]
    fn frame_builder_refuses_reserved_origination(id: u32) {
        assert!(matches!(
            UFrameMetadata::publish(topic())
                .with_payload_encoding(PayloadEncoding::from_id(id).unwrap())
                .build(),
            Err(UFrameMetadataError::InvalidMetadata(_))
        ));
    }

    #[test]
    fn native_zero_ttl_projects_to_canonical_no_expiry() {
        let metadata = UFrameMetadata::publish(topic())
            .with_ttl(Duration::ZERO)
            .build()
            .unwrap();
        assert_eq!(metadata.ttl(), Some(Duration::ZERO));
        let attributes = metadata.try_project_to_attributes().unwrap();
        assert_eq!(attributes.ttl(), None);
        assert_eq!(
            attributes.try_project_to_frame_metadata().unwrap().ttl(),
            None
        );
    }

    #[test_case("vehicle", 0, false; "response resource is not an event")]
    #[test_case("vehicle", 1, false; "RPC method is not an event")]
    #[test_case("vehicle", 0x7FFF, false; "last method resource")]
    #[test_case("vehicle", 0x8000, true; "first event resource")]
    #[test_case("vehicle", 0xFFFE, true; "last concrete event resource")]
    #[test_case("vehicle", 0xFFFF, false; "wildcard resource")]
    #[test_case("*", 0x8000, false; "wildcard authority")]
    fn native_and_classic_notification_source_rules_agree(
        authority: &str,
        resource: u16,
        valid: bool,
    ) {
        let source = UUri::try_from_parts(authority, 0x4210, 1, resource).unwrap();
        let native = UFrameMetadata::notification(source.clone(), reply_to()).build();
        let classic = UMessageBuilder::notification(source, reply_to()).build();
        assert_eq!(native.is_ok(), valid);
        assert_eq!(classic.is_ok(), valid);
    }

    fn topic() -> UUri {
        UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).expect("failed to create test URI")
    }

    fn method() -> UUri {
        UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x00b1).expect("failed to create method URI")
    }

    fn reply_to() -> UUri {
        UUri::try_from_parts("cloud", 0x10ab, 0x02, 0x0000).expect("failed to create reply URI")
    }

    #[test]
    fn builder_constructs_native_publish_metadata() {
        let metadata = UFrameMetadata::publish(topic())
            .with_priority(UPriority::CS1)
            .with_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
            .with_payload_encoding(PayloadEncoding::JSON)
            .build()
            .expect("metadata");

        assert_eq!(metadata.kind(), UMessageType::Publish);
        assert_eq!(metadata.source(), &topic());
        assert!(metadata.sink().is_none());
        assert_eq!(metadata.priority(), Some(UPriority::CS1));
        assert_eq!(metadata.payload_encoding(), Some(&PayloadEncoding::JSON));
    }

    #[test]
    fn builder_rejects_publish_with_low_rpc_shape() {
        // request without ttl
        let error = UFrameMetadata::request(method(), reply_to(), Duration::ZERO)
            .build()
            .unwrap_err();
        assert!(matches!(error, UFrameMetadataError::InvalidMetadata(_)));

        // rpc priority below CS4
        let error = UFrameMetadata::request(method(), reply_to(), Duration::from_secs(5))
            .with_priority(UPriority::CS2)
            .build()
            .unwrap_err();
        assert!(matches!(error, UFrameMetadataError::InvalidMetadata(_)));
    }

    #[test]
    fn builder_constructs_request_and_response_metadata() {
        let request = UFrameMetadata::request(method(), reply_to(), Duration::from_secs(5))
            .build()
            .expect("request metadata");
        assert_eq!(request.kind(), UMessageType::Request);
        assert_eq!(request.priority(), Some(UPriority::CS4));
        assert_eq!(request.ttl(), Some(Duration::from_secs(5)));

        let response = UFrameMetadata::response(method(), reply_to(), request.id().clone())
            .build()
            .expect("response metadata");
        assert_eq!(response.kind(), UMessageType::Response);
        assert_eq!(response.reqid(), Some(request.id()));
    }

    #[test]
    fn message_without_payload_projects_to_metadata_without_encoding() {
        let message = UMessageBuilder::publish(topic()).build().expect("message");

        let metadata = try_project_umessage_to_frame_metadata(&message).expect("metadata");

        assert!(message.payload().is_none());
        assert!(metadata.payload_encoding().is_none());
        assert_eq!(metadata.id(), message.id());
        assert_eq!(metadata.kind(), UMessageType::Publish);
    }

    #[test]
    fn attribute_projection_without_payload_encoding_keeps_it_absent() {
        let message = UMessageBuilder::publish(topic()).build().expect("message");

        let metadata = message
            .attributes()
            .try_project_to_frame_metadata()
            .expect("metadata");

        assert!(metadata.payload_encoding().is_none());
    }

    #[test]
    fn frame_projection_rejects_encoding_without_payload() {
        let metadata = UFrameMetadata::publish(topic())
            .with_payload_encoding(PayloadEncoding::RAW)
            .build()
            .unwrap();

        assert_eq!(
            try_project_frame_to_umessage(metadata, None).unwrap_err(),
            UFrameMetadataError::EncodingWithoutPayload
        );
    }

    #[test]
    fn frame_projection_rejects_payload_without_encoding() {
        let metadata = UFrameMetadata::publish(topic()).build().unwrap();

        assert_eq!(
            try_project_frame_to_umessage(metadata, Some(Bytes::new())).unwrap_err(),
            UFrameMetadataError::PayloadWithoutEncoding
        );
    }

    #[test]
    fn empty_present_payload_keeps_encoding() {
        let message = UMessageBuilder::publish(topic())
            .build_with_payload(Bytes::new(), PayloadEncoding::RAW)
            .expect("message");

        let metadata = try_project_umessage_to_frame_metadata(&message).expect("metadata");

        assert_eq!(message.payload(), Some(Bytes::new()));
        assert_eq!(metadata.payload_encoding(), Some(&PayloadEncoding::RAW));

        let projected =
            try_project_frame_to_umessage(metadata, Some(Bytes::new())).expect("projected message");
        assert_eq!(projected.payload(), Some(Bytes::new()));
        assert_eq!(projected.payload_encoding(), Some(PayloadEncoding::RAW));
    }

    #[test_case::test_case("*", 1, 1; "wildcard authority")]
    #[test_case::test_case("vehicle", 0xFFFF0001, 1; "wildcard instance")]
    #[test_case::test_case("vehicle", 1, 0xFF; "wildcard version")]
    fn message_source_wildcards_are_rejected(authority: &str, entity: u32, version: u8) {
        let source = UUri::try_from_parts(authority, entity, version, 0x8001).unwrap();
        assert!(matches!(
            UFrameMetadata::publish(source).build(),
            Err(UFrameMetadataError::InvalidMetadata(_))
        ));
    }

    #[test_case::test_case(0; "contract defined")]
    #[test_case::test_case(1; "Any")]
    #[test_case::test_case(2; "protobuf")]
    #[test_case::test_case(3; "JSON")]
    #[test_case::test_case(4; "SOMEIP")]
    #[test_case::test_case(5; "SOMEIP TLV")]
    #[test_case::test_case(6; "raw bytes")]
    #[test_case::test_case(7; "text")]
    #[test_case::test_case(8; "unassigned")]
    #[test_case::test_case(0xE000; "reserved")]
    #[test_case::test_case(0xFFFF; "future reserved")]
    fn payload_encoding_is_preserved_without_lookup(id: u32) {
        let expected_encoding = PayloadEncoding::from_id(id).unwrap();
        // Carriage test: re-stamped as a carrier would.
        let message = UMessageBuilder::publish(topic())
            .build_with_payload(Bytes::from_static(b"payload"), PayloadEncoding::RAW)
            .expect("message")
            .with_declared_payload_encoding(expected_encoding)
            .expect("carried encoding");
        let metadata = try_project_umessage_to_frame_metadata(&message).expect("metadata");
        assert_eq!(metadata.payload_encoding(), Some(&expected_encoding));
        let projected =
            try_project_frame_to_umessage(metadata, Some(Bytes::from_static(b"payload")))
                .expect("projected message");
        assert_eq!(projected.payload(), Some(Bytes::from_static(b"payload")));
        assert_eq!(projected.payload_encoding(), Some(expected_encoding));
    }

    #[test]
    fn attribute_projection_uses_the_declared_encoding_without_an_override() {
        let message = UMessageBuilder::publish(topic())
            .build_with_payload(Bytes::from_static(b"payload"), PayloadEncoding::RAW)
            .expect("message");

        let metadata = message
            .attributes()
            .try_project_to_frame_metadata()
            .unwrap();
        assert_eq!(metadata.payload_encoding(), Some(&PayloadEncoding::RAW));
        assert_eq!(
            metadata,
            try_project_umessage_to_frame_metadata(&message).unwrap()
        );
    }

    #[test]
    fn sub_millisecond_ttl_is_rejected_by_legacy_projection_not_truncated() {
        let metadata = UFrameMetadata::request(
            method(),
            reply_to(),
            Duration::from_micros(1_500), // 1.5 ms
        )
        .build()
        .expect("metadata");

        let error = metadata.try_project_to_attributes().unwrap_err();
        assert!(matches!(
            error,
            UFrameMetadataError::FieldNotRepresentable { field: "ttl", .. }
        ));
    }

    #[test]
    fn pr328_enum_names_compile() {
        assert_eq!(UCode::InvalidArgument as i32, 3);
        assert_eq!(UCode::Unimplemented as i32, 12);
        assert_eq!(PayloadEncoding::PROTOBUF_WRAPPED_IN_ANY.id() as i32, 1);
        assert_eq!(PayloadEncoding::SOMEIP.id() as i32, 4);
        assert_eq!(PayloadEncoding::SOMEIP_TLV.id() as i32, 5);
    }
}

#[cfg(test)]
mod projection_round_trip_properties {
    use proptest::prelude::*;

    use crate::{PayloadEncoding, UMessageBuilder, UUri};

    proptest! {
        /// Metadata projection round-trip: a publish message built from any
        /// valid parts projects to frame metadata whose canonical field-block
        /// encoding decodes back to equal metadata. Hand-written cases pin the
        /// known edges; this pins the space between them.
        #[test]
        fn projected_metadata_survives_field_block_round_trip(
            authority in "[a-z][a-z0-9]{0,11}",
            ue_id in 1u32..0xFFFF,
            version in 1u8..=0xFE,
            resource in 0x8000u16..0xFFFE,
            payload in proptest::collection::vec(any::<u8>(), 0..64),
        ) {
            let topic = UUri::try_from_parts(&authority, ue_id, version, resource)
                .expect("parts chosen from the valid range");
            let message = if payload.is_empty() {
                UMessageBuilder::publish(topic).build().expect("valid publish message")
            } else {
                UMessageBuilder::publish(topic)
                    .build_with_payload(payload, PayloadEncoding::RAW)
                    .expect("valid publish message with payload")
            };

            let metadata = super::try_project_umessage_to_frame_metadata(&message)
                .expect("valid message must project");

            let encoded = crate::frame::codec::encode_frame_metadata_fields(&metadata)
                .expect("projected metadata must encode");
            let decoded = crate::frame::codec::decode_frame_metadata_fields(&encoded)
                .expect("encoded metadata must decode");

            prop_assert_eq!(metadata, decoded);
        }
    }
}
