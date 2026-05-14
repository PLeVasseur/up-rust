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

use crate::{UCode, UStatus, UUri, UUID};

/// Native uProtocol message kind carried in a frame header.
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct UEncoding {
    format_id: String,
    content_type: String,
    schema_ref: Option<String>,
}

impl UEncoding {
    pub fn new(
        format_id: impl Into<String>,
        content_type: impl Into<String>,
        schema_ref: Option<impl Into<String>>,
    ) -> Self {
        Self {
            format_id: format_id.into(),
            content_type: content_type.into(),
            schema_ref: schema_ref.map(Into::into),
        }
    }

    pub fn from_content_type(content_type: impl Into<String>) -> Self {
        let content_type = content_type.into();
        Self {
            format_id: content_type.clone(),
            content_type,
            schema_ref: None,
        }
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
}

impl Default for UEncoding {
    fn default() -> Self {
        RawBytes::encoding()
    }
}

/// Error type used by serialization-neutral frame helpers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UWireError {
    BufferTooSmall { expected: usize, actual: usize },
    InvalidPayload(String),
    UnsupportedEncoding(UEncoding),
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
            Self::UnsupportedEncoding(encoding) => f.write_fmt(format_args!(
                "unsupported encoding: format_id={}, content_type={}",
                encoding.format_id(),
                encoding.content_type()
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

/// Native uProtocol metadata plus serialization-neutral encoding metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UFrameHeader {
    attributes: UAttributes,
    encoding: UEncoding,
}

impl UFrameHeader {
    pub fn new(attributes: UAttributes, encoding: UEncoding) -> Self {
        Self {
            attributes,
            encoding,
        }
    }

    pub fn publish(topic: UUri) -> Self {
        Self::new(
            UAttributes::new(UUID::build(), topic, None, UMessageType::Publish),
            UEncoding::default(),
        )
    }

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
#[derive(Clone, Debug, PartialEq)]
pub struct UOwnedFrame {
    header: UFrameHeader,
    payload: Bytes,
}

impl UOwnedFrame {
    pub fn new(header: UFrameHeader, payload: impl Into<Bytes>) -> Self {
        Self {
            header,
            payload: payload.into(),
        }
    }

    pub fn from_serializable<F, T>(header: UFrameHeader, value: &T) -> Result<Self, UWireError>
    where
        F: WireFormat,
        T: USerializer<F>,
    {
        let payload = value.serialize_owned()?;
        Ok(Self::new(header.with_encoding(F::encoding()), payload))
    }

    pub fn header(&self) -> &UFrameHeader {
        &self.header
    }

    pub fn header_mut(&mut self) -> &mut UFrameHeader {
        &mut self.header
    }

    pub fn payload(&self) -> &Bytes {
        &self.payload
    }

    pub fn payload_bytes(&self) -> &[u8] {
        self.payload.as_ref()
    }

    pub fn into_header(self) -> UFrameHeader {
        self.header
    }

    pub fn into_payload(self) -> Bytes {
        self.payload
    }

    pub fn deserialize<'a, F, T>(&'a self) -> Result<T, UWireError>
    where
        F: WireFormat,
        T: UDeserializer<'a, F>,
    {
        T::deserialize_from(self.payload_bytes())
    }
}

impl UZeroCopyRxFrame for UOwnedFrame {
    fn header(&self) -> &UFrameHeader {
        &self.header
    }

    fn payload(&self) -> &[u8] {
        self.payload_bytes()
    }
}

/// Compile-time identity for a payload wire representation.
pub trait WireFormat {
    fn name() -> &'static str;
    fn encoding() -> UEncoding;
}

/// Serializes a value into caller-provided storage.
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
        UEncoding::new("raw-bytes", "application/octet-stream", None::<String>)
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
    fn header(&self) -> &UFrameHeader;
    fn header_mut(&mut self) -> &mut UFrameHeader;
    fn payload(&self) -> &[u8];
    fn payload_mut(&mut self) -> &mut [u8];
}

/// Receive-side zero-copy frame lease.
pub trait UZeroCopyRxFrame {
    fn header(&self) -> &UFrameHeader;
    fn payload(&self) -> &[u8];

    fn deserialize_borrowed<'a, F, T>(&'a self) -> Result<T, UWireError>
    where
        F: WireFormat,
        T: UDeserializer<'a, F>,
    {
        T::deserialize_from(self.payload())
    }
}

/// Owned buffer useful for tests, examples, and adapters that emulate a transmit loan.
#[derive(Clone, Debug, PartialEq)]
pub struct UVecTxBuffer {
    header: UFrameHeader,
    payload: Vec<u8>,
}

impl UVecTxBuffer {
    pub fn new(header: UFrameHeader, payload_len: usize) -> Self {
        Self {
            header,
            payload: vec![0_u8; payload_len],
        }
    }

    pub fn into_frame(self) -> UOwnedFrame {
        UOwnedFrame::new(self.header, self.payload)
    }
}

impl UTxBuffer for UVecTxBuffer {
    fn header(&self) -> &UFrameHeader {
        &self.header
    }

    fn header_mut(&mut self) -> &mut UFrameHeader {
        &mut self.header
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
    fn owned_frame_uses_selected_wire_format() {
        let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
        let frame = UOwnedFrame::from_serializable::<RawBytes, _>(
            UFrameHeader::publish(topic),
            &&[0x0a_u8, 0x0b_u8][..],
        )
        .unwrap();

        assert_eq!(frame.header().encoding(), &RawBytes::encoding());
        assert_eq!(frame.payload_bytes(), &[0x0a_u8, 0x0b_u8]);
    }
}
