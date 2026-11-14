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

use core::fmt;
use core::mem::MaybeUninit;
use core::time::Duration;

use super::codec::{
    FIELD_COMM_STATUS, FIELD_MASK_V1, FIELD_NATIVE_TYPE_TOKEN, FIELD_PAYLOAD_ENCODING,
    FIELD_PERMISSION_LEVEL, FIELD_REQID, FIELD_SINK, FIELD_TOKEN, FIELD_TRACEPARENT, FIELD_TTL,
    FRAME_FIELDS_VERSION,
};
use super::native::NativeTypeToken;
use crate::uattributes::{PayloadEncoding, UMessageType, UPriority};
use crate::ucode::UCode;
use crate::uri::{UUri, UUriPartsError};
use crate::uuid::UUID;

/// Maximum traceparent length admitted by the zero-copy header profile.
pub const HEADER_TRACEPARENT_CAPACITY: usize = 63;

/// A fixed-size failure while encoding or validating a native header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum UFrameHeaderError {
    /// A declared field extends past the available bytes.
    Truncated,
    /// The selected-wire prefix differs from the agreed prefix.
    WrongPrefix,
    /// A structural field is invalid.
    Malformed(&'static str),
    /// Message metadata violates its role's invariants.
    Invalid(&'static str),
    /// A URI authority is invalid.
    Uri(UUriPartsError),
    /// A value cannot fit its wire length field or arithmetic bound.
    ValueTooLong(&'static str),
    /// The destination cannot hold the complete header.
    OutputTooShort,
    /// A valid token exceeds this receiver's configured admission limit.
    TokenTooLong,
}

impl fmt::Display for UFrameHeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated header"),
            Self::WrongPrefix => f.write_str("wrong native prefix"),
            Self::Malformed(reason) => write!(f, "malformed header: {reason}"),
            Self::Invalid(reason) => write!(f, "invalid metadata: {reason}"),
            Self::Uri(error) => write!(f, "invalid URI: {error}"),
            Self::ValueTooLong(field) => write!(f, "header field is too long: {field}"),
            Self::OutputTooShort => f.write_str("header destination is too small"),
            Self::TokenTooLong => f.write_str("token exceeds receiver admission limit"),
        }
    }
}

impl core::error::Error for UFrameHeaderError {}

/// Per-message metadata borrowed from setup state and the current message.
///
/// Debug output reports token length, never its contents.
#[derive(Clone, Copy)]
pub struct TxHeaderParts<'a> {
    /// Message kind.
    pub kind: UMessageType,
    /// Priority, when present.
    pub priority: Option<UPriority>,
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

impl fmt::Debug for TxHeaderParts<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TxHeaderParts")
            .field("kind", &self.kind)
            .field("source", self.source)
            .field("sink", &self.sink)
            .field("token_len", &self.token.map(str::len))
            .finish_non_exhaustive()
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
    pub kind: UMessageType,
    /// Priority, if present.
    pub priority: Option<UPriority>,
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

impl fmt::Debug for RxHeader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

    /// Assembles a header parsed by a whole-frame envelope. Callers validate it.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn assemble(
        kind: UMessageType,
        priority: Option<UPriority>,
        id: UUID,
        reqid: Option<UUID>,
        ttl: Option<Duration>,
        comm_status: Option<UCode>,
        permission_level: Option<u32>,
        source: UUri,
        sink: Option<UUri>,
        payload_encoding: Option<PayloadEncoding>,
        native_type_token: Option<NativeTypeToken>,
        header_len: usize,
        token: Option<&'a str>,
        traceparent: Option<&'a str>,
    ) -> Self {
        Self {
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
        }
    }

    /// Replaces the borrowed text, keeping every other field.
    #[must_use]
    pub fn with_text<'b>(
        self,
        token: Option<&'b str>,
        traceparent: Option<&'b str>,
    ) -> RxHeader<'b> {
        RxHeader {
            kind: self.kind,
            priority: self.priority,
            id: self.id,
            reqid: self.reqid,
            ttl: self.ttl,
            comm_status: self.comm_status,
            permission_level: self.permission_level,
            source: self.source,
            sink: self.sink,
            payload_encoding: self.payload_encoding,
            native_type_token: self.native_type_token,
            header_len: self.header_len,
            token,
            traceparent,
        }
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
    // Safety invariant: uniquely borrowed storage; raw cells never escape to
    // caller code, and every write preserves existing byte initialization.
    bytes: &'a mut [MaybeUninit<u8>],
    // Safety invariant: position<=bytes.len(), prefix0..position initialized.
    position: usize,
}
impl Writer<'_> {
    /// # Safety-usable invariant
    /// Writes initialized bytes only to the checked range, advances position
    /// afterwards, and preserves initialization on every error/normal exit.
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
        let entity = (u32::from(value.ue_instance_id()) << 16) | u32::from(value.ue_type_id());
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
/// # Safety-usable invariant
/// Preserves all bytes initialized on entry on every exit, including errors and
/// unwind: only Writer's checked put operations write initialized u8 values;
/// no caller-supplied callback receives raw MaybeUninit storage. On success the
/// reported prefix is initialized; later bytes are untouched.
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
    let mut writer = Writer {
        bytes: output,
        position: 0,
    };
    writer.put(prefix)?;
    writer.put(&(fields as u32).to_le_bytes())?;
    let token = write_field_block(&mut writer, parts)?;
    debug_assert_eq!(writer.position, total);
    Ok(WrittenHeader { len: total, token })
}

/// Initializes only the canonical field block in caller-owned storage.
///
/// Whole-frame envelopes place the block after their own fixed header. The
/// returned token range is relative to `out`.
///
/// # Errors
/// As [`write_header_uninit`], before writing any byte.
/// # Safety-usable invariant
/// Preserves all bytes initialized on entry on every exit, including errors and
/// unwind: only Writer's checked put operations write initialized u8 values;
/// no caller-supplied callback receives raw MaybeUninit storage. On success the
/// reported prefix is initialized; later bytes are untouched.
pub fn write_field_block_uninit(
    parts: &TxHeaderParts<'_>,
    out: &mut [MaybeUninit<u8>],
) -> Result<WrittenHeader, UFrameHeaderError> {
    validate_parts(parts)?;
    let total = header_fields_len(parts)?;
    let output = out
        .get_mut(..total)
        .ok_or(UFrameHeaderError::OutputTooShort)?;
    let mut writer = Writer {
        bytes: output,
        position: 0,
    };
    let token = write_field_block(&mut writer, parts)?;
    debug_assert_eq!(writer.position, total);
    Ok(WrittenHeader { len: total, token })
}

fn write_field_block(
    writer: &mut Writer<'_>,
    parts: &TxHeaderParts<'_>,
) -> Result<Option<(usize, usize)>, UFrameHeaderError> {
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
    writer.put(&[
        FRAME_FIELDS_VERSION,
        parts.kind.wire_code(),
        parts.priority.map_or(0, UPriority::wire_code),
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
    Ok(token)
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
    let output = unsafe { &mut *(core::ptr::from_mut(out) as *mut [MaybeUninit<u8>]) };
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
        core::str::from_utf8(self.take(len)?)
            .map_err(|_| UFrameHeaderError::Malformed("invalid UTF-8"))
    }
    fn uuid(&mut self) -> Result<UUID, UFrameHeaderError> {
        let high = self.u64()?;
        let low = self.u64()?;
        if !UUID::is_valid_u64_pair(high, low) {
            return Err(UFrameHeaderError::Malformed("invalid UUID"));
        }
        UUID::try_from_wire_pair(high, low)
            .map_err(|_| UFrameHeaderError::Malformed("invalid UUID"))
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
    let reader = Reader {
        bytes: bytes
            .get(..header_len)
            .ok_or(UFrameHeaderError::Truncated)?,
        position: start,
    };
    parse_fields(reader, header_len, limits)
}

/// Parses and validates a bare canonical field block, for whole-frame envelopes.
///
/// `fields` must be exactly the block; token and traceparent borrow from it.
///
/// # Errors
/// As [`parse_header`], with no prefix to check.
pub fn parse_field_block<'a>(
    fields: &'a [u8],
    limits: &HeaderLimits,
) -> Result<RxHeader<'a>, UFrameHeaderError> {
    parse_fields(
        Reader {
            bytes: fields,
            position: 0,
        },
        fields.len(),
        limits,
    )
}

fn parse_fields<'a>(
    mut reader: Reader<'a>,
    header_len: usize,
    limits: &HeaderLimits,
) -> Result<RxHeader<'a>, UFrameHeaderError> {
    if reader.byte()? != FRAME_FIELDS_VERSION {
        return Err(UFrameHeaderError::Malformed("field block version"));
    }
    let kind = UMessageType::from_wire_code(reader.byte()?)
        .ok_or(UFrameHeaderError::Malformed("message kind"))?;
    let priority = match reader.byte()? {
        0 => None,
        value => {
            Some(UPriority::from_wire_code(value).ok_or(UFrameHeaderError::Malformed("priority"))?)
        }
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
            PayloadEncoding::try_from_wire_id(raw)
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

/// Checks message-kind and addressing invariants of a header.
///
/// # Errors
/// Returns the violated invariant as a fixed-size error.
pub fn validate_parts(parts: &TxHeaderParts<'_>) -> Result<(), UFrameHeaderError> {
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
        UMessageType::Publish | UMessageType::Notification => {
            if !parts.source.is_event() || !concrete(parts.source) {
                return Err(UFrameHeaderError::Invalid("source topic"));
            }
            match (parts.kind, parts.sink) {
                (UMessageType::Publish, None) => {}
                (UMessageType::Notification, Some(sink))
                    if sink.is_notification_destination() && concrete(sink) => {}
                _ => return Err(UFrameHeaderError::Invalid("sink for message kind")),
            }
        }
        UMessageType::Request | UMessageType::Response => {
            if parts.priority.is_none_or(|value| value < UPriority::CS4) {
                return Err(UFrameHeaderError::Invalid("RPC priority"));
            }
            let sink = parts.sink.ok_or(UFrameHeaderError::Invalid("RPC sink"))?;
            if !concrete(parts.source) || !concrete(sink) {
                return Err(UFrameHeaderError::Invalid("RPC wildcard"));
            }
            if parts.kind == UMessageType::Request {
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
