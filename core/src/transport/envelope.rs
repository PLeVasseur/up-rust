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

//! Envelopes in runtime mode: heap-free encode into a slot, borrowed parse.
//!
//! The envelope stays with the transport and cannot vary per message (R10).
//! Every envelope produces the same bytes as its setup-time codec in `up-rust`,
//! which the golden-vector and differential tests hold them to.

use core::fmt;
use core::mem::MaybeUninit;

use super::binding::PayloadBytes;
use crate::frame::header::{
    header_fields_len, parse_field_block, parse_header, write_field_block_uninit,
    write_header_uninit, HeaderLimits, RxHeader, TxHeaderParts, UFrameHeaderError,
};
use crate::frame::native::NativeTypeToken;
use crate::memory::StartupMemory;
use crate::uattributes::PayloadEncoding;
use crate::wire::{exact_prefix_len, render_selected_prefix, selected_prefix_len, WireIdentity};

/// The native whole-frame envelope magic.
pub const NATIVE_ENVELOPE_MAGIC: [u8; 4] = *b"UPFE";
/// The native whole-frame envelope version.
pub const NATIVE_ENVELOPE_VERSION: u8 = 1;
/// Size of the native whole-frame envelope's fixed header.
pub const NATIVE_ENVELOPE_HEADER_LEN: usize = 20;

/// A transport-defined envelope, for bindings whose profile is not built in.
///
/// SOME/IP's `uPF1` profile fits here: the binding supplies the profile's head
/// layout, and the SDK writes the head and payload straight into a slot.
pub trait CustomEnvelope: Send + Sync + 'static {
    /// Name, for reports.
    fn name(&self) -> &'static str;

    /// The largest head this envelope produces for a route with this token limit.
    fn max_head_len(&self, max_token_len: usize) -> usize;

    /// The exact length of the head for these parts. A lending route needs it
    /// before it can loan; `write_head` must then write exactly this many bytes.
    ///
    /// # Errors
    /// Reports a frame the envelope cannot represent.
    fn head_len(
        &self,
        parts: &TxHeaderParts<'_>,
        payload_len: Option<usize>,
    ) -> Result<usize, UFrameHeaderError>;

    /// Writes everything before the payload and returns its length.
    ///
    /// # Errors
    /// Reports a frame the envelope cannot represent, or insufficient storage.
    fn write_head(
        &self,
        parts: &TxHeaderParts<'_>,
        payload_len: Option<usize>,
        out: &mut [u8],
    ) -> Result<usize, UFrameHeaderError>;

    /// Parses a received frame, borrowing from its bytes.
    ///
    /// # Errors
    /// Reports malformed input.
    fn parse<'a>(
        &self,
        metadata: &'a [u8],
        payload: PayloadBytes<'a>,
        limits: &HeaderLimits,
    ) -> Result<(RxHeader<'a>, PayloadBytes<'a>), UFrameHeaderError>;
}

/// What a transport puts around a frame on one route.
///
/// `ProtobufWholeFrame` is ported only if a Stage 6 binding declares it.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub enum Envelope {
    /// A selected-wire prefix and field block, with the payload carried separately.
    UFrameFields,
    /// The native whole-frame envelope, `UPFE`.
    NativeWholeFrame,
    /// A transport-defined envelope.
    Custom(&'static dyn CustomEnvelope),
}

impl fmt::Debug for Envelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl PartialEq for Envelope {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::UFrameFields, Self::UFrameFields)
            | (Self::NativeWholeFrame, Self::NativeWholeFrame) => true,
            (Self::Custom(left), Self::Custom(right)) => left.name() == right.name(),
            (Self::UFrameFields | Self::NativeWholeFrame | Self::Custom(_), _) => false,
        }
    }
}

impl Eq for Envelope {}

/// One wire admitted on a route. Built once in initialization.
#[doc(hidden)]
pub struct AdmittedWire {
    pub(crate) encoding: PayloadEncoding,
    pub(crate) native_type_token: Option<NativeTypeToken>,
    pub(crate) wire_id: WireIdentity,
    pub(crate) payload_family: WireIdentity,
    /// Pre-rendered selected-wire prefix; empty for other envelopes.
    pub(crate) prefix: &'static [u8],
}

impl fmt::Debug for AdmittedWire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdmittedWire")
            .field("encoding", &self.encoding)
            .finish_non_exhaustive()
    }
}

impl AdmittedWire {
    /// Admits one (wire, family) pair for this envelope. Renders the prefix in `memory`.
    pub(crate) fn admit(
        memory: &StartupMemory,
        envelope: Envelope,
        wire_id: WireIdentity,
        payload_family: WireIdentity,
        identity: crate::wire::WirePayloadIdentity,
    ) -> Result<Self, UFrameHeaderError> {
        let prefix: &'static [u8] = if matches!(envelope, Envelope::UFrameFields) {
            let len = selected_prefix_len(wire_id, payload_family)?;
            let bytes = memory
                .bytes(len)
                .map_err(|_| UFrameHeaderError::Invalid("no startup memory for the prefix"))?;
            let written = render_selected_prefix(wire_id, payload_family, bytes)?;
            let initialized = bytes.get(..written).unwrap_or_default();
            // SAFETY: `render_selected_prefix` initialized exactly the first
            // `written` bytes of this `MaybeUninit<u8>` slice, whose layout is `u8`.
            unsafe { &*(core::ptr::from_ref(initialized) as *const [u8]) }
        } else {
            &[]
        };
        Ok(Self {
            encoding: identity.encoding,
            native_type_token: identity.native_type_token,
            wire_id,
            payload_family,
            prefix,
        })
    }
}

/// The payload size known when a head is written.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PayloadSize {
    /// No payload.
    Absent,
    /// Exactly this size.
    Exact(usize),
    /// Only a bound; the head is patched when the payload has been written.
    AtMost(usize),
}

/// What [`Envelope::write_head`] wrote.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WrittenHead {
    pub(crate) len: usize,
    /// Token extent in the slot, wiped after the send.
    pub(crate) token: Option<(usize, usize)>,
}

/// A parsed frame, borrowing the transport's or slot's bytes.
#[derive(Debug)]
pub(crate) struct ParsedFrame<'a> {
    pub(crate) header: RxHeader<'a>,
    pub(crate) payload: PayloadBytes<'a>,
    /// For a selected-wire prefix, which admitted wire matched.
    pub(crate) matched: Option<usize>,
}

impl Envelope {
    /// Name, for reports.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::UFrameFields => "uframe-fields",
            Self::NativeWholeFrame => "native-whole-frame",
            Self::Custom(custom) => custom.name(),
        }
    }

    /// An upper bound on the head of a route with this prefix and token limit.
    pub(crate) fn max_head_len(self, prefix_len: usize, max_token_len: usize) -> usize {
        // Field block: 24 fixed bytes, optional reqid, ttl, status, level and
        // encoding, two URIs of at most 128-byte authorities, token, traceparent.
        let fields = (24_usize + 16 + 8 + 4 + 4 + 2 * (8 + 128) + 2 + 1 + 63 + 4 + 4)
            .saturating_add(max_token_len);
        match self {
            Self::UFrameFields => prefix_len.saturating_add(4).saturating_add(fields),
            Self::NativeWholeFrame => NATIVE_ENVELOPE_HEADER_LEN.saturating_add(fields),
            Self::Custom(custom) => custom.max_head_len(max_token_len),
        }
    }

    /// The exact head length for these parts.
    pub(crate) fn head_len(
        self,
        wire: Option<&AdmittedWire>,
        parts: &TxHeaderParts<'_>,
        payload_len: Option<usize>,
    ) -> Result<usize, UFrameHeaderError> {
        match self {
            Self::UFrameFields => {
                let prefix = wire.map_or(&[][..], |wire| wire.prefix);
                crate::frame::header::header_len(prefix, parts)
            }
            Self::NativeWholeFrame => header_fields_len(parts)?
                .checked_add(NATIVE_ENVELOPE_HEADER_LEN)
                .ok_or(UFrameHeaderError::ValueTooLong("fields")),
            Self::Custom(custom) => custom.head_len(parts, payload_len),
        }
    }

    /// Writes everything before the payload into `out`.
    /// # Safety-usable invariant
    /// Every byte initialized on entry remains initialized on success, error and
    /// unwind. Built-in writers' contracts preserve initialization; custom code
    /// receives only &mut[u8] after zero initialization, never MaybeUninit storage.
    /// On success, the returned prefix `out[..len]` is fully initialized by
    /// crate-controlled writes. A custom callback cannot enlarge that prefix:
    /// its returned length must equal the initialized view it was given.
    /// Callers certifying an entire loan must also require `len == out.len()`.
    pub(crate) fn write_head(
        self,
        wire: Option<&AdmittedWire>,
        parts: &TxHeaderParts<'_>,
        payload: PayloadSize,
        out: &mut [MaybeUninit<u8>],
    ) -> Result<WrittenHead, UFrameHeaderError> {
        match self {
            Self::UFrameFields => {
                let prefix = wire.map_or(&[][..], |wire| wire.prefix);
                let written = write_header_uninit(prefix, parts, out)?;
                Ok(WrittenHead {
                    len: written.len,
                    token: written.token,
                })
            }
            Self::NativeWholeFrame => {
                let fields = header_fields_len(parts)?;
                let metadata_len =
                    u32::try_from(fields).map_err(|_| UFrameHeaderError::ValueTooLong("fields"))?;
                let (header, rest) = out
                    .split_at_mut_checked(NATIVE_ENVELOPE_HEADER_LEN)
                    .ok_or(UFrameHeaderError::OutputTooShort)?;
                write_native_header(header, metadata_len, payload);
                let written = write_field_block_uninit(parts, rest)?;
                Ok(WrittenHead {
                    len: NATIVE_ENVELOPE_HEADER_LEN.saturating_add(written.len),
                    token: written.token.map(|(offset, len)| {
                        (offset.saturating_add(NATIVE_ENVELOPE_HEADER_LEN), len)
                    }),
                })
            }
            Self::Custom(custom) => {
                let payload_len = match payload {
                    PayloadSize::Absent => None,
                    PayloadSize::Exact(len) | PayloadSize::AtMost(len) => Some(len),
                };
                let head_len = custom.head_len(parts, payload_len)?;
                let head = out
                    .get_mut(..head_len)
                    .ok_or(UFrameHeaderError::OutputTooShort)?;
                for byte in head.iter_mut() {
                    byte.write(0);
                }
                // SAFETY: the loop initialized every byte. MaybeUninit<u8>
                // has u8's size/alignment, and this wide-pointer cast preserves
                // the checked slice length, including an empty slice. The range
                // is inside the original live exclusive out borrow; this is a
                // reborrow with no extended lifetime. head is not accessed while
                // initialized is live. The safe callback receives initialized
                // u8 cells and cannot safely deinitialize them.
                let initialized = unsafe { &mut *(core::ptr::from_mut(head) as *mut [u8]) };
                let written = custom.write_head(parts, payload_len, initialized)?;
                if written != head_len {
                    return Err(UFrameHeaderError::Invalid("custom envelope header length"));
                }
                Ok(WrittenHead {
                    len: written,
                    token: None,
                })
            }
        }
    }

    /// Completes the frame once the payload length is known.
    /// # Safety-usable invariant
    /// Preserves every initialized byte on all exits. Only checked byte cells are
    /// written with initialized integers; no callback receives the raw storage.
    pub(crate) fn finish(
        self,
        payload_len: Option<usize>,
        head: &mut [MaybeUninit<u8>],
    ) -> Result<(), UFrameHeaderError> {
        if !matches!(self, Self::NativeWholeFrame) {
            return Ok(());
        }
        let presence = head.get_mut(5).ok_or(UFrameHeaderError::OutputTooShort)?;
        presence.write(u8::from(payload_len.is_some()));
        let len = u64::try_from(payload_len.unwrap_or(0)).unwrap_or(u64::MAX);
        let bytes = head
            .get_mut(12..20)
            .ok_or(UFrameHeaderError::OutputTooShort)?;
        for (slot, byte) in bytes.iter_mut().zip(len.to_le_bytes()) {
            slot.write(byte);
        }
        Ok(())
    }

    /// Parses a received frame, borrowing its bytes and heap-free.
    ///
    /// The payload and its encoding must be both present or both absent, the
    /// same rule the owned frame type enforces.
    pub(crate) fn parse<'a>(
        self,
        admitted: &[AdmittedWire],
        metadata: &'a [u8],
        payload: PayloadBytes<'a>,
        limits: &HeaderLimits,
    ) -> Result<ParsedFrame<'a>, UFrameHeaderError> {
        let parsed = self.parse_frame(admitted, metadata, payload, limits)?;
        if parsed.payload.is_present() != parsed.header.payload_encoding.is_some() {
            return Err(UFrameHeaderError::Invalid("payload presence and encoding"));
        }
        Ok(parsed)
    }

    fn parse_frame<'a>(
        self,
        admitted: &[AdmittedWire],
        metadata: &'a [u8],
        payload: PayloadBytes<'a>,
        limits: &HeaderLimits,
    ) -> Result<ParsedFrame<'a>, UFrameHeaderError> {
        match self {
            Self::UFrameFields => {
                for (index, wire) in admitted.iter().enumerate() {
                    let Ok(prefix_len) =
                        exact_prefix_len(wire.wire_id, wire.payload_family, metadata)
                    else {
                        continue;
                    };
                    let prefix = metadata
                        .get(..prefix_len)
                        .ok_or(UFrameHeaderError::Truncated)?;
                    let header = parse_header(prefix, metadata, limits)?;
                    if header.header_len != metadata.len() {
                        return Err(UFrameHeaderError::Malformed("trailing metadata bytes"));
                    }
                    return Ok(ParsedFrame {
                        header,
                        payload,
                        matched: Some(index),
                    });
                }
                Err(UFrameHeaderError::WrongPrefix)
            }
            Self::NativeWholeFrame => {
                let frame = payload.as_contiguous().ok_or(UFrameHeaderError::Malformed(
                    "whole-frame envelope requires a contiguous head",
                ))?;
                let (header, body) = parse_native_frame(frame, limits)?;
                Ok(ParsedFrame {
                    header,
                    payload: body.map_or(PayloadBytes::Absent, PayloadBytes::one),
                    matched: None,
                })
            }
            Self::Custom(custom) => {
                let (header, body) = custom.parse(metadata, payload, limits)?;
                Ok(ParsedFrame {
                    header,
                    payload: body,
                    matched: None,
                })
            }
        }
    }
}

/// Admits one wire for an envelope, for byte-level tests in `up-rust`. Not a stable API.
///
/// # Errors
/// As route admission.
#[doc(hidden)]
#[cfg(up_rust_test_hooks)]
pub fn __admit_for_tests(
    memory: &StartupMemory,
    envelope: Envelope,
    wire_id: WireIdentity,
    payload_family: WireIdentity,
    identity: crate::wire::WirePayloadIdentity,
) -> Result<&'static AdmittedWire, UFrameHeaderError> {
    let wire = AdmittedWire::admit(memory, envelope, wire_id, payload_family, identity)?;
    let placed: &'static mut AdmittedWire = memory
        .place(wire)
        .map_err(|_| UFrameHeaderError::Invalid("no startup memory for the wire"))?;
    Ok(&*placed)
}

/// Writes the head for these parts, for byte-level tests in `up-rust`. Not a stable API.
///
/// # Errors
/// As [`Envelope::write_head`].
#[doc(hidden)]
#[cfg(up_rust_test_hooks)]
pub fn __encode_head_for_tests(
    envelope: Envelope,
    wire: &AdmittedWire,
    parts: &TxHeaderParts<'_>,
    payload_len: Option<usize>,
    out: &mut [u8],
) -> Result<usize, UFrameHeaderError> {
    let size = payload_len.map_or(PayloadSize::Absent, PayloadSize::Exact);
    // SAFETY: write_head and finish preserve initialized bytes on every exit;
    // this view is passed only to those crate-controlled functions, never escaped.
    let out = unsafe { crate::transport::slots::as_uninit_mut(out) };
    let written = envelope.write_head(Some(wire), parts, size, &mut *out)?;
    envelope.finish(payload_len, out)?;
    Ok(written.len)
}

/// Parses one frame, for byte-level tests in `up-rust`. Not a stable API.
///
/// # Errors
/// As [`Envelope::parse`].
#[doc(hidden)]
#[cfg(up_rust_test_hooks)]
pub fn __parse_for_tests<'a>(
    envelope: Envelope,
    wire: &AdmittedWire,
    metadata: &'a [u8],
    payload: PayloadBytes<'a>,
    limits: &HeaderLimits,
) -> Result<(RxHeader<'a>, PayloadBytes<'a>), UFrameHeaderError> {
    let parsed = envelope.parse(core::slice::from_ref(wire), metadata, payload, limits)?;
    Ok((parsed.header, parsed.payload))
}

/// # Safety-usable invariant
/// Writes only initialized u8 values through MaybeUninit::write; preserves every
/// already initialized byte, including any short destination and every exit.
fn write_native_header(header: &mut [MaybeUninit<u8>], metadata_len: u32, payload: PayloadSize) {
    let (present, payload_len) = match payload {
        PayloadSize::Absent => (false, 0_u64),
        PayloadSize::Exact(len) | PayloadSize::AtMost(len) => {
            (true, u64::try_from(len).unwrap_or(u64::MAX))
        }
    };
    let mut parts = [0_u8; NATIVE_ENVELOPE_HEADER_LEN];
    let (magic, rest) = parts.split_at_mut(4);
    magic.copy_from_slice(&NATIVE_ENVELOPE_MAGIC);
    let (fixed, lengths) = rest.split_at_mut(4);
    fixed.copy_from_slice(&[NATIVE_ENVELOPE_VERSION, u8::from(present), 0, 0]);
    let (metadata, payload_bytes) = lengths.split_at_mut(4);
    metadata.copy_from_slice(&metadata_len.to_le_bytes());
    payload_bytes.copy_from_slice(&payload_len.to_le_bytes());
    for (slot, byte) in header.iter_mut().zip(parts) {
        slot.write(byte);
    }
}

fn parse_native_frame<'a>(
    frame: &'a [u8],
    limits: &HeaderLimits,
) -> Result<(RxHeader<'a>, Option<&'a [u8]>), UFrameHeaderError> {
    let bad = UFrameHeaderError::Malformed("native envelope");
    let header = frame
        .get(..NATIVE_ENVELOPE_HEADER_LEN)
        .ok_or(UFrameHeaderError::Truncated)?;
    if header.get(..4) != Some(&NATIVE_ENVELOPE_MAGIC[..]) {
        return Err(UFrameHeaderError::WrongPrefix);
    }
    if header.get(4).copied() != Some(NATIVE_ENVELOPE_VERSION) {
        return Err(bad);
    }
    let present = match header.get(5).copied() {
        Some(0) => false,
        Some(1) => true,
        _ => return Err(bad),
    };
    if header.get(6..8) != Some(&[0, 0][..]) {
        return Err(bad);
    }
    let mut metadata_len = [0; 4];
    metadata_len.copy_from_slice(header.get(8..12).ok_or(bad)?);
    let metadata_len = u32::from_le_bytes(metadata_len) as usize;
    let mut payload_len = [0; 8];
    payload_len.copy_from_slice(header.get(12..20).ok_or(bad)?);
    let payload_len = u64::from_le_bytes(payload_len);
    let body = frame
        .get(NATIVE_ENVELOPE_HEADER_LEN..)
        .ok_or(UFrameHeaderError::Truncated)?;
    let fields = body
        .get(..metadata_len)
        .ok_or(UFrameHeaderError::Truncated)?;
    let payload_bytes = body
        .get(metadata_len..)
        .ok_or(UFrameHeaderError::Truncated)?;
    if u64::try_from(payload_bytes.len()).unwrap_or(u64::MAX) != payload_len
        || (!present && payload_len != 0)
    {
        return Err(bad);
    }
    Ok((
        parse_field_block(fields, limits)?,
        present.then_some(payload_bytes),
    ))
}
