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
//! In-place native headers using the canonical metadata field block.
//!
//! The caller supplies its agreed selected-wire prefix. Headers encode that
//! prefix, a little-endian field-block length and the existing versioned fields.
//! Payload placement and storage ownership remain the binding's responsibility.
//! Receive token/traceparent values borrow the input; errors carry no heap data.

use std::{mem::MaybeUninit, time::Duration};

use super::codec::*;
use crate::{
    FrameMessageKind, FramePriority, NativeTypeToken, PayloadEncoding, UCode, UFrameMetadata, UUri,
    UUriPartsError, UUID,
};

/// Maximum traceparent length admitted by the zero-copy header profile.
pub const HEADER_TRACEPARENT_CAPACITY: usize = 63;

/// A fixed-size failure while encoding or validating a native header.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum UFrameHeaderError {
    /// A declared field extends past the available bytes.
    #[error("truncated header")]
    Truncated,
    /// The selected-wire prefix differs from the agreed prefix.
    #[error("wrong native prefix")]
    WrongPrefix,
    /// A structural field is invalid.
    #[error("malformed header: {0}")]
    Malformed(&'static str),
    /// Message metadata violates its role's invariants.
    #[error("invalid metadata: {0}")]
    Invalid(&'static str),
    /// A URI authority is invalid.
    #[error("invalid URI: {0}")]
    Uri(UUriPartsError),
    /// A value cannot fit its wire length field or arithmetic bound.
    #[error("header field is too long: {0}")]
    ValueTooLong(&'static str),
    /// The destination cannot hold the complete header.
    #[error("header destination is too small")]
    OutputTooShort,
    /// A valid token exceeds this receiver's configured admission limit.
    #[error("token exceeds receiver admission limit")]
    TokenTooLong,
}

/// Per-message metadata borrowed from setup state and the current message.
///
/// Debug output reports token length, never its contents.
#[derive(Clone, Copy)]
pub struct TxHeaderParts<'a> {
    /// Message kind.
    pub kind: FrameMessageKind,
    /// Priority, when present.
    pub priority: Option<FramePriority>,
    /// Message identifier.
    pub id: &'a UUID,
    /// Correlated request identifier.
    pub reqid: Option<&'a UUID>,
    /// Time to live.
    pub ttl: Option<Duration>,
    /// Communication status.
    pub comm_status: Option<UCode>,
    /// Permission level.
    pub permission_level: Option<u32>,
    /// Source address.
    pub source: &'a UUri,
    /// Sink address.
    pub sink: Option<&'a UUri>,
    /// Access token, including an explicitly present empty token.
    pub token: Option<&'a str>,
    /// Traceparent, when present.
    pub traceparent: Option<&'a str>,
    /// Payload encoding identity; independent of the native type token.
    pub payload_encoding: Option<PayloadEncoding>,
    /// Opaque structural native type token.
    pub native_type_token: Option<NativeTypeToken>,
}

impl std::fmt::Debug for TxHeaderParts<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TxHeaderParts")
            .field("kind", &self.kind)
            .field("source", self.source)
            .field("sink", &self.sink)
            .field("token_len", &self.token.map(str::len))
            .finish_non_exhaustive()
    }
}

impl<'a> From<&'a UFrameMetadata> for TxHeaderParts<'a> {
    fn from(metadata: &'a UFrameMetadata) -> Self {
        Self {
            kind: metadata.kind(),
            priority: metadata.priority(),
            id: metadata.id(),
            reqid: metadata.reqid(),
            ttl: metadata.ttl(),
            comm_status: metadata.comm_status(),
            permission_level: metadata.permission_level(),
            source: metadata.source(),
            sink: metadata.sink(),
            token: metadata.token(),
            traceparent: metadata.traceparent(),
            payload_encoding: metadata.payload_encoding().copied(),
            native_type_token: metadata.native_type_token(),
        }
    }
}

impl<'a> From<&'a RxHeader<'_>> for TxHeaderParts<'a> {
    fn from(header: &'a RxHeader<'_>) -> Self {
        Self {
            kind: header.kind,
            priority: header.priority,
            id: &header.id,
            reqid: header.reqid.as_ref(),
            ttl: header.ttl,
            comm_status: header.comm_status,
            permission_level: header.permission_level,
            source: &header.source,
            sink: header.sink.as_ref(),
            token: header.token(),
            traceparent: header.traceparent(),
            payload_encoding: header.payload_encoding,
            native_type_token: header.native_type_token,
        }
    }
}

/// Written header extent and the token range that an owning buffer can wipe.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WrittenHeader {
    /// Total initialized header bytes, including the prefix and length word.
    pub len: usize,
    /// Token offset and byte length in the header, if a token is present.
    pub token: Option<(usize, usize)>,
}

/// Receive admission limits; these are chosen when the registration is created.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeaderLimits {
    /// Longest access token accepted; zero admits only absent or empty tokens.
    pub max_token_len: usize,
}

impl Default for HeaderLimits {
    fn default() -> Self {
        Self {
            max_token_len: 2048,
        }
    }
}

/// A validated header whose token and traceparent borrow the received bytes.
///
/// The borrow prevents those bytes being safely mutated or released while this
/// view is used. This checks metadata, not payload presence, type or field bits;
/// the frame carrier and typed admission layer perform those remaining checks.
#[derive(Clone)]
pub struct RxHeader<'a> {
    /// Message kind.
    pub kind: FrameMessageKind,
    /// Priority, if present.
    pub priority: Option<FramePriority>,
    /// Message identifier.
    pub id: UUID,
    /// Correlated request identifier.
    pub reqid: Option<UUID>,
    /// Time to live.
    pub ttl: Option<Duration>,
    /// Communication status.
    pub comm_status: Option<UCode>,
    /// Permission level.
    pub permission_level: Option<u32>,
    /// Inline source address.
    pub source: UUri,
    /// Inline sink address.
    pub sink: Option<UUri>,
    /// Payload encoding identity.
    pub payload_encoding: Option<PayloadEncoding>,
    /// Opaque structural native type token.
    pub native_type_token: Option<NativeTypeToken>,
    /// Header extent within the provided frame bytes.
    pub header_len: usize,
    token: Option<&'a str>,
    traceparent: Option<&'a str>,
}

impl std::fmt::Debug for RxHeader<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RxHeader")
            .field("kind", &self.kind)
            .field("source", &self.source)
            .field("sink", &self.sink)
            .field("token_len", &self.token.map(str::len))
            .finish_non_exhaustive()
    }
}

impl<'a> RxHeader<'a> {
    /// Borrows the token from the same immutable bytes validated by the parser.
    #[must_use]
    pub fn token(&self) -> Option<&'a str> {
        self.token
    }

    /// Borrows the traceparent from the validated input.
    #[must_use]
    pub fn traceparent(&self) -> Option<&'a str> {
        self.traceparent
    }
}

fn add(total: &mut usize, amount: usize) -> Result<(), UFrameHeaderError> {
    *total = total
        .checked_add(amount)
        .ok_or(UFrameHeaderError::ValueTooLong("header"))?;
    Ok(())
}

/// Computes the canonical field-block length without allocating.
///
/// # Errors
/// Returns a fixed-size error if a value exceeds its wire bound.
pub fn header_fields_len(parts: &TxHeaderParts<'_>) -> Result<usize, UFrameHeaderError> {
    let mut len = 24;
    if parts.reqid.is_some() {
        add(&mut len, 16)?;
    }
    if let Some(ttl) = parts.ttl {
        u64::try_from(ttl.as_nanos()).map_err(|_| UFrameHeaderError::ValueTooLong("ttl"))?;
        add(&mut len, 8)?;
    }
    if parts.comm_status.is_some() {
        add(&mut len, 4)?;
    }
    if parts.permission_level.is_some() {
        add(&mut len, 4)?;
    }
    for uri in [Some(parts.source), parts.sink].into_iter().flatten() {
        let bytes = uri.authority_name().len();
        u8::try_from(bytes).map_err(|_| UFrameHeaderError::ValueTooLong("authority"))?;
        add(&mut len, 8)?;
        add(&mut len, bytes)?;
    }
    if let Some(token) = parts.token {
        u16::try_from(token.len()).map_err(|_| UFrameHeaderError::ValueTooLong("token"))?;
        add(&mut len, 2)?;
        add(&mut len, token.len())?;
    }
    if let Some(traceparent) = parts.traceparent {
        if traceparent.len() > HEADER_TRACEPARENT_CAPACITY {
            return Err(UFrameHeaderError::ValueTooLong("traceparent"));
        }
        add(&mut len, 1)?;
        add(&mut len, traceparent.len())?;
    }
    if parts.payload_encoding.is_some() {
        add(&mut len, 4)?;
    }
    if parts.native_type_token.is_some() {
        add(&mut len, 4)?;
    }
    Ok(len)
}

/// Computes the complete prefix, length word and metadata extent.
///
/// # Errors
/// Returns an error if a length is unrepresentable or arithmetic overflows.
pub fn header_len(prefix: &[u8], parts: &TxHeaderParts<'_>) -> Result<usize, UFrameHeaderError> {
    let fields = header_fields_len(parts)?;
    u32::try_from(fields).map_err(|_| UFrameHeaderError::ValueTooLong("fields"))?;
    let mut total = prefix.len();
    add(&mut total, 4)?;
    add(&mut total, fields)?;
    Ok(total)
}

struct Writer<'a> {
    bytes: &'a mut [MaybeUninit<u8>],
    position: usize,
}
impl Writer<'_> {
    fn put(&mut self, value: &[u8]) -> Result<(), UFrameHeaderError> {
        let end = self
            .position
            .checked_add(value.len())
            .ok_or(UFrameHeaderError::OutputTooShort)?;
        let target = self
            .bytes
            .get_mut(self.position..end)
            .ok_or(UFrameHeaderError::OutputTooShort)?;
        for (slot, byte) in target.iter_mut().zip(value) {
            slot.write(*byte);
        }
        self.position = end;
        Ok(())
    }
    fn uuid(&mut self, value: &UUID) -> Result<(), UFrameHeaderError> {
        let (high, low) = value.as_u64_pair();
        self.put(&high.to_le_bytes())?;
        self.put(&low.to_le_bytes())
    }
    fn uri(&mut self, value: &UUri) -> Result<(), UFrameHeaderError> {
        let entity =
            (u32::from(value.uentity_instance_id()) << 16) | u32::from(value.uentity_type_id());
        self.put(&entity.to_le_bytes())?;
        self.put(&value.resource_id().to_le_bytes())?;
        self.put(&[
            value.uentity_major_version(),
            value.authority_name().len() as u8,
        ])?;
        self.put(value.authority_name().as_bytes())
    }
}

/// Initializes a complete header directly in caller-owned uninitialized storage.
///
/// No payload bytes or bytes after the returned extent are touched. A returned
/// token range describes only this buffer; reclamation must follow its owner.
///
/// # Errors
/// Rejects invalid metadata, unrepresentable lengths and insufficient storage
/// before writing any byte. No error contains an allocated diagnostic string.
pub fn write_header_uninit(
    prefix: &[u8],
    parts: &TxHeaderParts<'_>,
    out: &mut [MaybeUninit<u8>],
) -> Result<WrittenHeader, UFrameHeaderError> {
    validate_parts(parts)?;
    let fields = header_fields_len(parts)?;
    let total = header_len(prefix, parts)?;
    let output = out
        .get_mut(..total)
        .ok_or(UFrameHeaderError::OutputTooShort)?;
    let mut presence = 0;
    for (present, bit) in [
        (parts.sink.is_some(), FIELD_SINK),
        (parts.reqid.is_some(), FIELD_REQID),
        (parts.ttl.is_some(), FIELD_TTL),
        (parts.comm_status.is_some(), FIELD_COMM_STATUS),
        (parts.permission_level.is_some(), FIELD_PERMISSION_LEVEL),
        (parts.token.is_some(), FIELD_TOKEN),
        (parts.traceparent.is_some(), FIELD_TRACEPARENT),
        (parts.payload_encoding.is_some(), FIELD_PAYLOAD_ENCODING),
        (parts.native_type_token.is_some(), FIELD_NATIVE_TYPE_TOKEN),
    ] {
        if present {
            presence |= bit;
        }
    }
    let mut writer = Writer {
        bytes: output,
        position: 0,
    };
    writer.put(prefix)?;
    writer.put(&(fields as u32).to_le_bytes())?;
    writer.put(&[
        FRAME_FIELDS_VERSION,
        parts.kind.wire_code(),
        parts.priority.map_or(0, FramePriority::wire_code),
        0,
    ])?;
    writer.put(&presence.to_le_bytes())?;
    writer.uuid(parts.id)?;
    if let Some(value) = parts.reqid {
        writer.uuid(value)?;
    }
    if let Some(value) = parts.ttl {
        writer.put(&(value.as_nanos() as u64).to_le_bytes())?;
    }
    if let Some(value) = parts.comm_status {
        writer.put(&value.value().to_le_bytes())?;
    }
    if let Some(value) = parts.permission_level {
        writer.put(&value.to_le_bytes())?;
    }
    writer.uri(parts.source)?;
    if let Some(value) = parts.sink {
        writer.uri(value)?;
    }
    let token = if let Some(value) = parts.token {
        writer.put(&(value.len() as u16).to_le_bytes())?;
        let range = (writer.position, value.len());
        writer.put(value.as_bytes())?;
        Some(range)
    } else {
        None
    };
    if let Some(value) = parts.traceparent {
        writer.put(&[value.len() as u8])?;
        writer.put(value.as_bytes())?;
    }
    if let Some(value) = parts.payload_encoding {
        writer.put(&value.id().to_le_bytes())?;
    }
    if let Some(value) = parts.native_type_token {
        writer.put(&value.as_u32().to_le_bytes())?;
    }
    debug_assert_eq!(writer.position, total);
    Ok(WrittenHeader { len: total, token })
}

/// Writes a header to initialized caller storage with the same checks as the
/// uninitialized writer.
///
/// # Errors
/// See [`write_header_uninit`]. Bytes outside the returned extent are unchanged.
pub fn write_header(
    prefix: &[u8],
    parts: &TxHeaderParts<'_>,
    out: &mut [u8],
) -> Result<WrittenHeader, UFrameHeaderError> {
    // SAFETY: MaybeUninit<u8> has u8's size/alignment. The temporary mutable view
    // cannot escape and the writer only stores initialized u8 values, so every
    // originally initialized byte remains initialized on success or error.
    let output = unsafe { &mut *(std::ptr::from_mut(out) as *mut [MaybeUninit<u8>]) };
    write_header_uninit(prefix, parts, output)
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], UFrameHeaderError> {
        let end = self
            .position
            .checked_add(len)
            .ok_or(UFrameHeaderError::Truncated)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(UFrameHeaderError::Truncated)?;
        self.position = end;
        Ok(value)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], UFrameHeaderError> {
        let mut value = [0; N];
        value.copy_from_slice(self.take(N)?);
        Ok(value)
    }
    fn byte(&mut self) -> Result<u8, UFrameHeaderError> {
        Ok(self.array::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16, UFrameHeaderError> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32, UFrameHeaderError> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64, UFrameHeaderError> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn text(&mut self, len: usize) -> Result<&'a str, UFrameHeaderError> {
        std::str::from_utf8(self.take(len)?)
            .map_err(|_| UFrameHeaderError::Malformed("invalid UTF-8"))
    }
    fn uuid(&mut self) -> Result<UUID, UFrameHeaderError> {
        let high = self.u64()?;
        let low = self.u64()?;
        if !UUID::is_valid_u64_pair(high, low) {
            return Err(UFrameHeaderError::Malformed("invalid UUID"));
        }
        UUID::from_u64_pair(high, low).map_err(|_| UFrameHeaderError::Malformed("invalid UUID"))
    }
    fn uri(&mut self) -> Result<UUri, UFrameHeaderError> {
        let entity = self.u32()?;
        let resource = self.u16()?;
        let version = self.byte()?;
        let len = usize::from(self.byte()?);
        let authority = self.text(len)?;
        UUri::try_from_wire_parts(authority, entity, version, resource)
            .map_err(UFrameHeaderError::Uri)
    }
}

/// Parses and validates metadata in place, borrowing token/traceparent bytes.
///
/// Extra bytes after the declared header are the carrier's payload. Structure
/// and metadata invariants are checked before the registration's token limit.
///
/// # Errors
/// Returns a fixed-size error for malformed fields, incorrect prefix, invalid
/// metadata or token admission. This zero-copy API allocates on neither path.
pub fn parse_header<'a>(
    prefix: &[u8],
    bytes: &'a [u8],
    limits: &HeaderLimits,
) -> Result<RxHeader<'a>, UFrameHeaderError> {
    if bytes.get(..prefix.len()) != Some(prefix) {
        return Err(if bytes.len() < prefix.len() {
            UFrameHeaderError::Truncated
        } else {
            UFrameHeaderError::WrongPrefix
        });
    }
    let mut outer = Reader {
        bytes,
        position: prefix.len(),
    };
    let length = usize::try_from(outer.u32()?).map_err(|_| UFrameHeaderError::Truncated)?;
    let start = outer.position;
    outer.take(length)?;
    let header_len = outer.position;
    let mut reader = Reader {
        bytes: bytes
            .get(..header_len)
            .ok_or(UFrameHeaderError::Truncated)?,
        position: start,
    };
    if reader.byte()? != FRAME_FIELDS_VERSION {
        return Err(UFrameHeaderError::Malformed("field block version"));
    }
    let kind = FrameMessageKind::from_wire_code(reader.byte()?)
        .ok_or(UFrameHeaderError::Malformed("message kind"))?;
    let priority = match reader.byte()? {
        0 => None,
        value => Some(
            FramePriority::from_wire_code(value).ok_or(UFrameHeaderError::Malformed("priority"))?,
        ),
    };
    if reader.byte()? != 0 {
        return Err(UFrameHeaderError::Malformed("reserved byte"));
    }
    let presence = reader.u32()?;
    if presence & !FIELD_MASK_V1 != 0 {
        return Err(UFrameHeaderError::Malformed("presence bits"));
    }
    let id = reader.uuid()?;
    let reqid = if presence & FIELD_REQID != 0 {
        Some(reader.uuid()?)
    } else {
        None
    };
    let ttl = if presence & FIELD_TTL != 0 {
        Some(Duration::from_nanos(reader.u64()?))
    } else {
        None
    };
    let comm_status = if presence & FIELD_COMM_STATUS != 0 {
        Some(
            UCode::from_i32(i32::from_le_bytes(reader.array()?))
                .ok_or(UFrameHeaderError::Malformed("status code"))?,
        )
    } else {
        None
    };
    let permission_level = if presence & FIELD_PERMISSION_LEVEL != 0 {
        Some(reader.u32()?)
    } else {
        None
    };
    let source = reader.uri()?;
    let sink = if presence & FIELD_SINK != 0 {
        Some(reader.uri()?)
    } else {
        None
    };
    let token = if presence & FIELD_TOKEN != 0 {
        let len = usize::from(reader.u16()?);
        Some(reader.text(len)?)
    } else {
        None
    };
    let traceparent = if presence & FIELD_TRACEPARENT != 0 {
        let len = usize::from(reader.byte()?);
        Some(reader.text(len)?)
    } else {
        None
    };
    let payload_encoding = if presence & FIELD_PAYLOAD_ENCODING != 0 {
        let raw = reader.u32()?;
        if u16::try_from(raw).is_err() {
            return Err(UFrameHeaderError::Malformed("payload encoding"));
        }
        Some(
            PayloadEncoding::from_id(raw)
                .map_err(|_| UFrameHeaderError::Malformed("payload encoding"))?,
        )
    } else {
        None
    };
    let native_type_token = if presence & FIELD_NATIVE_TYPE_TOKEN != 0 {
        Some(NativeTypeToken::from_u32(reader.u32()?))
    } else {
        None
    };
    if reader.position != header_len {
        return Err(UFrameHeaderError::Malformed("trailing field bytes"));
    }
    let parts = TxHeaderParts {
        kind,
        priority,
        id: &id,
        reqid: reqid.as_ref(),
        ttl,
        comm_status,
        permission_level,
        source: &source,
        sink: sink.as_ref(),
        token,
        traceparent,
        payload_encoding,
        native_type_token,
    };
    validate_parts(&parts)?;
    if traceparent.is_some_and(|value| value.len() > HEADER_TRACEPARENT_CAPACITY) {
        return Err(UFrameHeaderError::Malformed("traceparent capacity"));
    }
    if token.is_some_and(|value| value.len() > limits.max_token_len) {
        return Err(UFrameHeaderError::TokenTooLong);
    }
    Ok(RxHeader {
        kind,
        priority,
        id,
        reqid,
        ttl,
        comm_status,
        permission_level,
        source,
        sink,
        payload_encoding,
        native_type_token,
        header_len,
        token,
        traceparent,
    })
}

pub(crate) fn validate_parts(parts: &TxHeaderParts<'_>) -> Result<(), UFrameHeaderError> {
    let concrete = |uri: &UUri| {
        !(uri.has_wildcard_authority()
            || uri.has_wildcard_entity_instance()
            || uri.has_wildcard_entity_type()
            || uri.has_wildcard_version()
            || uri.has_wildcard_resource_id())
    };
    let valid_uuid = |id: &UUID| {
        let (high, low) = id.as_u64_pair();
        UUID::is_valid_u64_pair(high, low)
    };
    if !valid_uuid(parts.id) || parts.reqid.is_some_and(|id| !valid_uuid(id)) {
        return Err(UFrameHeaderError::Invalid("UUID"));
    }
    if parts.native_type_token.is_some() && parts.payload_encoding.is_none() {
        return Err(UFrameHeaderError::Invalid(
            "native token without payload encoding",
        ));
    }
    match parts.kind {
        FrameMessageKind::Publish | FrameMessageKind::Notification => {
            if !parts.source.is_event() || !concrete(parts.source) {
                return Err(UFrameHeaderError::Invalid("source topic"));
            }
            match (parts.kind, parts.sink) {
                (FrameMessageKind::Publish, None) => {}
                (FrameMessageKind::Notification, Some(sink))
                    if sink.is_notification_destination() && concrete(sink) => {}
                _ => return Err(UFrameHeaderError::Invalid("sink for message kind")),
            }
        }
        FrameMessageKind::Request | FrameMessageKind::Response => {
            if parts
                .priority
                .is_none_or(|value| value < FramePriority::CS4)
            {
                return Err(UFrameHeaderError::Invalid("RPC priority"));
            }
            let sink = parts.sink.ok_or(UFrameHeaderError::Invalid("RPC sink"))?;
            if !concrete(parts.source) || !concrete(sink) {
                return Err(UFrameHeaderError::Invalid("RPC wildcard"));
            }
            if parts.kind == FrameMessageKind::Request {
                if !parts.source.is_rpc_response() || !sink.is_rpc_method() {
                    return Err(UFrameHeaderError::Invalid("RPC request addressing"));
                }
                if parts.ttl.is_none_or(|ttl| ttl.is_zero()) {
                    return Err(UFrameHeaderError::Invalid("request TTL"));
                }
            } else {
                if !parts.source.is_rpc_method() || !sink.is_rpc_response() {
                    return Err(UFrameHeaderError::Invalid("RPC response addressing"));
                }
                if parts.reqid.is_none() {
                    return Err(UFrameHeaderError::Invalid("response request ID"));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PREFIX: &[u8] = b"agreed-prefix";

    fn metadata(seed: usize) -> UFrameMetadata {
        let topic = UUri::try_from_wire_parts("sensor", 0x1234, 1, 0x8001).unwrap();
        let method = UUri::try_from_wire_parts("service", 0x4567, 2, 0x42).unwrap();
        let reply = UUri::try_from_wire_parts("client", 0x6789, 3, 0).unwrap();
        let mut builder = match seed % 4 {
            0 => UFrameMetadata::publish(topic),
            1 => UFrameMetadata::notification(topic, reply),
            2 => UFrameMetadata::request(method, reply, Duration::from_millis(250)),
            _ => UFrameMetadata::response(method, reply, UUID::build()),
        };
        if seed & 4 != 0 {
            builder = builder.with_priority(FramePriority::CS5);
        }
        if seed & 8 != 0 {
            builder = builder.with_ttl(Duration::from_nanos(999));
        }
        if seed & 16 != 0 {
            builder = builder.with_comm_status(UCode::Unavailable);
        }
        if seed & 32 != 0 {
            builder = builder.with_permission_level(seed as u32);
        }
        if seed & 64 != 0 {
            builder = builder.with_token(if seed & 512 == 0 { "private-token" } else { "" });
        }
        if seed & 128 != 0 {
            builder =
                builder.with_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01");
        }
        if seed & 256 != 0 {
            builder = builder.with_payload_encoding(PayloadEncoding::CONTRACT_DEFINED);
        }
        if seed & 512 != 0 && seed & 256 != 0 {
            builder = builder.with_native_type_token(NativeTypeToken::from_u32(seed as u32));
        }
        builder.build().unwrap()
    }

    fn encode(metadata: &UFrameMetadata) -> Vec<u8> {
        let parts = TxHeaderParts::from(metadata);
        let mut bytes = vec![0; header_len(PREFIX, &parts).unwrap()];
        let written = write_header(PREFIX, &parts, &mut bytes).unwrap();
        assert_eq!(written.len, bytes.len());
        bytes
    }

    fn assert_received_fields(received: &RxHeader<'_>, metadata: &UFrameMetadata) {
        assert_eq!(received.kind, metadata.kind());
        assert_eq!(&received.id, metadata.id());
        assert_eq!(received.reqid.as_ref(), metadata.reqid());
        assert_eq!(&received.source, metadata.source());
        assert_eq!(received.sink.as_ref(), metadata.sink());
        assert_eq!(received.ttl, metadata.ttl());
        assert_eq!(received.priority, metadata.priority());
        assert_eq!(received.comm_status, metadata.comm_status());
        assert_eq!(received.permission_level, metadata.permission_level());
        assert_eq!(received.token(), metadata.token());
        assert_eq!(received.traceparent(), metadata.traceparent());
        assert_eq!(
            received.payload_encoding,
            metadata.payload_encoding().copied()
        );
        assert_eq!(received.native_type_token, metadata.native_type_token());
    }

    #[test]
    fn differential_field_blocks_and_receive_views() {
        let limits = HeaderLimits {
            max_token_len: 2048,
        };
        // Test-only fault controls demonstrate that the conformance gate fails
        // for encoding drift and lost decoded identity. No library path reads it.
        let fault = std::env::var("UP_RUST_HEADER_CONFORMANCE_FAULT").ok();
        for seed in 0..1024 {
            let metadata = metadata(seed);
            let fields = encode_frame_metadata_fields(&metadata).unwrap();
            let mut bytes = encode(&metadata);
            if fault.as_deref() == Some("token-byte-order")
                && metadata.native_type_token().is_some()
            {
                let length = bytes.len();
                bytes.get_mut(length - 4..).unwrap().reverse();
            }
            assert_eq!(bytes.get(PREFIX.len() + 4..).unwrap(), fields);
            assert_eq!(
                decode_frame_metadata_fields(bytes.get(PREFIX.len() + 4..).unwrap()).unwrap(),
                metadata
            );
            let mut received = parse_header(PREFIX, &bytes, &limits).unwrap();
            if fault.as_deref() == Some("lost-token") {
                received.native_type_token = None;
            }
            assert_received_fields(&received, &metadata);
            if let Some(token) = received.token() {
                let offset = token.as_ptr() as usize - bytes.as_ptr() as usize;
                assert_eq!(
                    bytes.get(offset..offset + token.len()).unwrap(),
                    token.as_bytes()
                );
            }
        }
        println!("header_field_differential: metadata_cases=1024 receive_comparisons=1024");
    }

    #[test]
    fn native_prefix_and_in_place_header_match_the_whole_codec() {
        use crate::wire::{
            StableContainerWire, UFrameFieldsMetadataCodec, UWireMetadataCodec,
            UWireMetadataContext,
        };
        let codec = UFrameFieldsMetadataCodec;
        let context =
            UWireMetadataContext::from_wire::<StableContainerWire, UFrameFieldsMetadataCodec>();
        let prefix = codec.native_prefix(context).unwrap();
        for seed in 0..1024 {
            let metadata = metadata(seed);
            let parts = TxHeaderParts::from(&metadata);
            let mut output = vec![0; header_len(&prefix, &parts).unwrap()];
            write_header(&prefix, &parts, &mut output).unwrap();
            assert_eq!(
                output,
                codec.encode_frame_metadata(context, &metadata).unwrap()
            );
            assert_eq!(
                codec.decode_frame_metadata(context, &output).unwrap(),
                metadata
            );
            let received = parse_header(
                &prefix,
                &output,
                &HeaderLimits {
                    max_token_len: 2048,
                },
            )
            .unwrap();
            assert_received_fields(&received, &metadata);
        }
        println!("header_whole_codec_differential: metadata_cases=1024 receive_comparisons=1024");
    }

    #[test]
    fn mutated_fields_agree_with_the_canonical_decoder() {
        let limits = HeaderLimits {
            max_token_len: 2048,
        };
        let mut accepted = 0;
        let mut rejected = 0;
        for seed in 0..1024 {
            let original = encode(&metadata(seed));
            for mutation in 0..48 {
                let mut bytes = original.clone();
                let offset = PREFIX.len()
                    + 4
                    + (seed * 13 + mutation * 7) % (bytes.len() - PREFIX.len() - 4);
                *bytes.get_mut(offset).unwrap() ^= 1 << ((seed + mutation) % 8);
                let reference =
                    decode_frame_metadata_fields(bytes.get(PREFIX.len() + 4..).unwrap());
                let expected = reference.as_ref().is_ok_and(|value| {
                    value
                        .traceparent()
                        .is_none_or(|value| value.len() <= HEADER_TRACEPARENT_CAPACITY)
                });
                let received = parse_header(PREFIX, &bytes, &limits);
                assert_eq!(
                    received.is_ok(),
                    expected,
                    "seed {seed}, mutation {mutation}, offset {offset}"
                );
                if expected {
                    assert_received_fields(&received.unwrap(), &reference.unwrap());
                    accepted += 1;
                } else {
                    rejected += 1;
                }
            }
        }
        assert_eq!(accepted + rejected, 49_152);
        assert!(accepted > 0 && rejected > 0);
        println!("header_mutation_differential: base_cases=1024 mutations=49152 accepted={accepted} rejected={rejected} accepted_fields_equal=true");
    }

    #[test]
    fn short_output_and_invalid_metadata_leave_bytes_unchanged() {
        let metadata = metadata(511);
        let mut parts = TxHeaderParts::from(&metadata);
        let mut output = [0xA5; 8];
        assert_eq!(
            write_header(PREFIX, &parts, &mut output),
            Err(UFrameHeaderError::OutputTooShort)
        );
        assert_eq!(output, [0xA5; 8]);
        parts.priority = Some(FramePriority::CS0);
        assert_eq!(
            write_header(PREFIX, &parts, &mut output),
            Err(UFrameHeaderError::Invalid("RPC priority"))
        );
        assert_eq!(output, [0xA5; 8]);
    }

    #[test]
    fn token_limit_follows_structure_and_debug_redacts_secret() {
        let metadata = metadata(64);
        let parts = TxHeaderParts::from(&metadata);
        let mut bytes = encode(&metadata);
        let limits = HeaderLimits { max_token_len: 0 };
        assert!(matches!(
            parse_header(PREFIX, &bytes, &limits),
            Err(UFrameHeaderError::TokenTooLong)
        ));
        assert!(!format!("{parts:?}").contains("private-token"));
        let received = parse_header(
            PREFIX,
            &bytes,
            &HeaderLimits {
                max_token_len: 2048,
            },
        )
        .unwrap();
        assert!(!format!("{received:?}").contains("private-token"));
        let last = bytes.len() - 1;
        *bytes.get_mut(last).unwrap() = 0xFF;
        assert!(matches!(
            parse_header(PREFIX, &bytes, &limits),
            Err(UFrameHeaderError::Malformed("invalid UTF-8"))
        ));
    }
}
