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

use crate::{UAttributesError, UCode, UUri, UUID};

use super::{
    frame::{UAttributes, UEncoding, UFrameMetadata, UMessageType, UOwnedFrame, UPriority},
    payload::{PayloadFormat, RawBytes, USerializer, UWireError},
};

/// Error type used by the native [`UFrameBuilder`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UFrameBuilderError {
    AttributesValidationError(UAttributesError),
    Payload(UWireError),
}

impl UFrameBuilderError {
    fn invalid_attributes(message: impl Into<String>) -> Self {
        Self::AttributesValidationError(UAttributesError::validation_error(message))
    }
}

impl Display for UFrameBuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AttributesValidationError(error) => {
                f.write_fmt(format_args!("invalid frame attributes: {error}"))
            }
            Self::Payload(error) => f.write_fmt(format_args!("invalid frame payload: {error}")),
        }
    }
}

impl Error for UFrameBuilderError {}

impl From<UWireError> for UFrameBuilderError {
    fn from(value: UWireError) -> Self {
        Self::Payload(value)
    }
}

impl From<UAttributesError> for UFrameBuilderError {
    fn from(value: UAttributesError) -> Self {
        Self::AttributesValidationError(value)
    }
}

/// Native builder for creating [`UOwnedFrame`]s.
///
/// This keeps the familiar publish/notification/request/response builder
/// ergonomics without reintroducing generated message envelopes.
///
/// ```
/// # use up_rust::{UFrameBuilder, UUri};
/// # fn build() -> Result<(), Box<dyn std::error::Error>> {
/// let topic = UUri::try_from("//vehicle/4210/1/B24D")?;
/// let frame = UFrameBuilder::publish(topic)
///     .build_with_raw_payload(b"reading".to_vec())?;
/// assert_eq!(frame.payload_bytes(), b"reading");
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct UFrameBuilder {
    commstatus: Option<UCode>,
    encoding: Option<UEncoding>,
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

impl UFrameBuilder {
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
        self.encoding = Some(encoding);
        self
    }

    /// Builds only the frame metadata.
    pub fn build_metadata(self) -> Result<UFrameMetadata, UFrameBuilderError> {
        Ok(UFrameMetadata::new(self.build_attributes()?, self.encoding))
    }

    /// Builds an owned frame with the currently configured payload, if any.
    pub fn build(self) -> Result<UOwnedFrame, UFrameBuilderError> {
        let attributes = self.build_attributes()?;
        let metadata = UFrameMetadata::new(attributes, self.encoding);
        Ok(match self.payload {
            Some(payload) => UOwnedFrame::with_payload(metadata, payload),
            None => UOwnedFrame::without_payload(metadata),
        })
    }

    /// Builds an owned frame with raw bytes.
    pub fn build_with_raw_payload<T: Into<Bytes>>(
        self,
        payload: T,
    ) -> Result<UOwnedFrame, UFrameBuilderError> {
        self.build_with_payload(payload, RawBytes::encoding())
    }

    /// Builds an owned frame with explicit encoding metadata.
    pub fn build_with_payload<T: Into<Bytes>>(
        mut self,
        payload: T,
        encoding: UEncoding,
    ) -> Result<UOwnedFrame, UFrameBuilderError> {
        self.payload = Some(payload.into());
        self.encoding = Some(encoding);
        self.build()
    }

    /// Serializes a typed payload and builds an owned frame using the selected payload codec.
    pub fn build_with_serializable<F, T>(
        mut self,
        value: &T,
    ) -> Result<UOwnedFrame, UFrameBuilderError>
    where
        F: PayloadFormat,
        T: USerializer<F>,
    {
        self.payload = Some(value.serialize_owned()?);
        self.encoding = Some(F::encoding());
        self.build()
    }

    /// Serializes a Protocol Buffers payload and builds an owned frame.
    #[cfg(feature = "protobuf-wire")]
    pub fn build_with_protobuf_payload<T>(
        self,
        value: &T,
    ) -> Result<UOwnedFrame, UFrameBuilderError>
    where
        T: USerializer<crate::ProtobufPayload>,
    {
        self.build_with_serializable::<crate::ProtobufPayload, _>(value)
    }

    fn build_attributes(&self) -> Result<UAttributes, UFrameBuilderError> {
        let id = self.message_id.clone().unwrap_or_else(UUID::build);
        if !id.is_uprotocol_uuid() {
            return Err(UFrameBuilderError::invalid_attributes(
                "message ID must be a valid uProtocol UUID",
            ));
        }
        let source = self
            .source
            .clone()
            .ok_or_else(|| UFrameBuilderError::invalid_attributes("source URI is required"))?;
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

impl Default for UFrameBuilder {
    fn default() -> Self {
        Self {
            commstatus: None,
            encoding: None,
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
