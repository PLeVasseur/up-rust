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

use crate::{
    DecodePayload, EncodePayload, PayloadEncoding, ProtobufMappable, ReadDecodePayload,
    SerializationError, UAttributes, UCode, UFrameMetadata, UFrameMetadataError, UPayloadFormat,
    UStatus,
};

const MAGIC: &[u8; 4] = b"UPWM";
const ID_REF_COMPACT: u8 = 0x00;
const ID_REF_LITERAL: u8 = 0x01;
const ENCODING_NONE: u8 = 0;
const ENCODING_STANDARD: u8 = 1;
const ENCODING_CUSTOM: u8 = 2;

/// First supported native-prefix metadata byte-format version.
pub const FORMAT_VERSION: u16 = 1;

/// Compact ID reserved for invalid-ID fixtures.
pub const INVALID_ID_FIXTURE_COMPACT_ID: u16 = 0xFFFF;

/// First compact ID reserved for local or experimental future tools.
pub const LOCAL_EXPERIMENTAL_COMPACT_ID_START: u16 = 0x8000;

/// Last compact ID reserved for local or experimental future tools.
pub const LOCAL_EXPERIMENTAL_COMPACT_ID_END: u16 = 0xFFFE;

/// Identity for the first-wave uProtocol native selected wire.
pub const UPROTOCOL_NATIVE_WIRE_ID: WireIdentity =
    WireIdentity::new("org.eclipse.uprotocol.wire.native", 0x0001);

/// Payload-family identity for explicit native payload bytes.
pub const NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID: WireIdentity =
    WireIdentity::new("native-explicit", 0x0001);

/// Identity for the first-wave native-prefix metadata layout.
pub const NATIVE_PREFIX_METADATA_LAYOUT_ID: WireIdentity =
    WireIdentity::new("org.eclipse.uprotocol.metadata.native-prefix", 0x0001);

/// Stable wire, payload-family, or metadata-layout identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WireIdentity {
    literal_id: &'static str,
    compact_id: u16,
}

impl WireIdentity {
    /// Creates an identity from its literal and compact representations.
    #[must_use]
    pub const fn new(literal_id: &'static str, compact_id: u16) -> Self {
        Self {
            literal_id,
            compact_id,
        }
    }

    /// Returns the language-neutral literal identity.
    #[must_use]
    pub fn literal_id(self) -> &'static str {
        self.literal_id
    }

    /// Returns the compact first-wave identity.
    #[must_use]
    pub fn compact_id(self) -> u16 {
        self.compact_id
    }
}

/// Identity reference decoded from native-prefix metadata bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WireIdentityRef {
    /// Compact `u16` identity reference.
    Compact(u16),
    /// Literal UTF-8 identity reference.
    Literal(String),
}

impl WireIdentityRef {
    /// Returns whether this reference identifies `expected`.
    #[must_use]
    pub fn matches(&self, expected: WireIdentity) -> bool {
        match self {
            Self::Compact(actual) => *actual == expected.compact_id(),
            Self::Literal(actual) => actual == expected.literal_id(),
        }
    }
}

/// Compatibility decision between decoded metadata and a selected wire.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireCompatibility {
    /// Decoded metadata is compatible with the selected wire.
    Compatible,
    /// Decoded metadata is incompatible with the selected wire.
    Incompatible,
}

impl WireCompatibility {
    /// Returns true when the compatibility result is compatible.
    #[must_use]
    pub fn is_compatible(self) -> bool {
        matches!(self, Self::Compatible)
    }
}

/// Static selected-wire contract.
pub trait UWire {
    /// Stable identity for the full selected wire.
    const WIRE_ID: WireIdentity;
    /// Payload operation family supported by this selected wire.
    const PAYLOAD_FAMILY_ID: WireIdentity;
    /// Metadata byte layout used by this selected wire.
    const METADATA_LAYOUT_ID: WireIdentity;
    /// Metadata byte-format version supported by this selected wire.
    const FORMAT_VERSION: u16;

    /// Checks selected-wire compatibility before public frame exposure.
    fn wire_compatibility(actual: &WireIdentityRef) -> WireCompatibility {
        if actual.matches(Self::WIRE_ID) {
            WireCompatibility::Compatible
        } else {
            WireCompatibility::Incompatible
        }
    }

    /// Checks payload-family compatibility before public frame exposure.
    fn payload_family_compatibility(actual: &WireIdentityRef) -> WireCompatibility {
        if actual.matches(Self::PAYLOAD_FAMILY_ID) {
            WireCompatibility::Compatible
        } else {
            WireCompatibility::Incompatible
        }
    }
}

/// Default first-wave native wire for explicit native payload paths.
pub struct UProtocolNativeWire;

impl UWire for UProtocolNativeWire {
    const WIRE_ID: WireIdentity = UPROTOCOL_NATIVE_WIRE_ID;
    const PAYLOAD_FAMILY_ID: WireIdentity = NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID;
    const METADATA_LAYOUT_ID: WireIdentity = NATIVE_PREFIX_METADATA_LAYOUT_ID;
    const FORMAT_VERSION: u16 = FORMAT_VERSION;
}

/// Metadata-prefix encode/decode helper trait for selected wires.
pub trait UWireMetadata: UWire {
    /// Encodes native frame metadata for this selected wire.
    ///
    /// # Errors
    ///
    /// Returns an error when metadata is invalid or cannot be serialized.
    fn encode_frame_metadata(metadata: &UFrameMetadata) -> Result<Vec<u8>, UWireMetadataError>
    where
        Self: Sized,
    {
        encode_frame_metadata::<Self>(metadata)
    }

    /// Decodes and validates native frame metadata for this selected wire.
    ///
    /// # Errors
    ///
    /// Returns an error when bytes are malformed, use an unsupported layout or
    /// version, or declare an incompatible wire or payload family.
    fn decode_frame_metadata(src: &[u8]) -> Result<UFrameMetadata, UWireMetadataError>
    where
        Self: Sized,
    {
        decode_frame_metadata::<Self>(src)
    }
}

impl<W> UWireMetadata for W where W: UWire {}

/// Wire-level encode helper alias for existing payload codecs.
pub trait UWireEncode<T: ?Sized>: EncodePayload<T> {}

impl<C, T: ?Sized> UWireEncode<T> for C where C: EncodePayload<T> {}

/// Wire-level borrowed decode helper alias for existing payload codecs.
pub trait UWireDecode<'a, T>: DecodePayload<'a, T> {}

impl<'a, C, T> UWireDecode<'a, T> for C where C: DecodePayload<'a, T> {}

/// Wire-level owned decode helper alias for existing payload codecs.
pub trait UWireDecodeOwned<T>: for<'a> DecodePayload<'a, T> {}

impl<C, T> UWireDecodeOwned<T> for C where C: for<'a> DecodePayload<'a, T> {}

/// Wire-level reader decode helper alias for existing payload codecs.
pub trait UWireReadDecode<T>: ReadDecodePayload<T> {}

impl<C, T> UWireReadDecode<T> for C where C: ReadDecodePayload<T> {}

/// Errors returned by native-prefix selected-wire metadata handling.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UWireMetadataError {
    /// The input did not start with the native-prefix metadata magic bytes.
    WrongMagic,
    /// The metadata layout id was not native-prefix.
    UnknownMetadataLayoutId { actual: WireIdentityRef },
    /// The metadata version is unsupported by the selected wire.
    UnsupportedVersion { expected: u16, actual: u16 },
    /// The selected-wire id is incompatible with `W`.
    WrongWireMetadata {
        expected: WireIdentity,
        actual: WireIdentityRef,
    },
    /// The payload-family id is incompatible with `W`.
    PayloadFamilyMismatch {
        expected: WireIdentity,
        actual: WireIdentityRef,
    },
    /// The reserved flags field contained unsupported bits.
    UnsupportedReservedFlags(u16),
    /// The payload encoding block is unsupported or malformed.
    UnsupportedPayloadEncoding(String),
    /// The metadata bytes are malformed.
    MalformedMetadata(String),
    /// Frame metadata validation failed.
    FrameMetadata(String),
    /// Metadata serialization or parsing failed.
    SerializationError(String),
}

impl Display for UWireMetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongMagic => f.write_str("wrong native-prefix metadata magic"),
            Self::UnknownMetadataLayoutId { actual } => {
                write!(f, "unknown metadata layout id: {actual:?}")
            }
            Self::UnsupportedVersion { expected, actual } => write!(
                f,
                "unsupported metadata version: expected {expected}, got {actual}"
            ),
            Self::WrongWireMetadata { expected, actual } => write!(
                f,
                "wrong selected wire metadata: expected {expected:?}, got {actual:?}"
            ),
            Self::PayloadFamilyMismatch { expected, actual } => write!(
                f,
                "payload family mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::UnsupportedReservedFlags(flags) => {
                write!(f, "unsupported native-prefix reserved flags: {flags:#06x}")
            }
            Self::UnsupportedPayloadEncoding(message) => {
                write!(f, "unsupported payload encoding: {message}")
            }
            Self::MalformedMetadata(message) => write!(f, "malformed wire metadata: {message}"),
            Self::FrameMetadata(message) => write!(f, "invalid frame metadata: {message}"),
            Self::SerializationError(message) => {
                write!(f, "metadata serialization error: {message}")
            }
        }
    }
}

impl Error for UWireMetadataError {}

impl From<UFrameMetadataError> for UWireMetadataError {
    fn from(value: UFrameMetadataError) -> Self {
        Self::FrameMetadata(value.to_string())
    }
}

impl From<SerializationError> for UWireMetadataError {
    fn from(value: SerializationError) -> Self {
        Self::SerializationError(value.to_string())
    }
}

impl From<UWireMetadataError> for UStatus {
    fn from(value: UWireMetadataError) -> Self {
        UStatus::fail_with_code(UCode::InvalidArgument, value.to_string())
    }
}

/// Encodes native frame metadata using the first-wave native-prefix layout.
///
/// # Errors
///
/// Returns an error when metadata is invalid or cannot be serialized.
pub fn encode_frame_metadata<W>(metadata: &UFrameMetadata) -> Result<Vec<u8>, UWireMetadataError>
where
    W: UWire,
{
    metadata.validate()?;

    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    write_identity_ref(&mut out, W::METADATA_LAYOUT_ID);
    write_u16(&mut out, W::FORMAT_VERSION);
    write_identity_ref(&mut out, W::WIRE_ID);
    write_identity_ref(&mut out, W::PAYLOAD_FAMILY_ID);
    write_u16(&mut out, 0);
    write_len_prefixed_bytes(&mut out, &metadata.attributes().write_to_protobuf_bytes()?)?;
    write_payload_encoding(&mut out, metadata.payload_encoding())?;
    Ok(out)
}

/// Decodes native frame metadata using the first-wave native-prefix layout.
///
/// # Errors
///
/// Returns an error when bytes are malformed, use an unsupported layout or
/// version, or declare an incompatible wire or payload family.
pub fn decode_frame_metadata<W>(src: &[u8]) -> Result<UFrameMetadata, UWireMetadataError>
where
    W: UWire,
{
    let mut reader = MetadataReader::new(src);
    if reader.take(MAGIC.len())? != MAGIC.as_slice() {
        return Err(UWireMetadataError::WrongMagic);
    }

    let layout_id = reader.read_identity_ref()?;
    if !layout_id.matches(W::METADATA_LAYOUT_ID) {
        return Err(UWireMetadataError::UnknownMetadataLayoutId { actual: layout_id });
    }

    let version = reader.read_u16()?;
    if version != W::FORMAT_VERSION {
        return Err(UWireMetadataError::UnsupportedVersion {
            expected: W::FORMAT_VERSION,
            actual: version,
        });
    }

    let wire_id = reader.read_identity_ref()?;
    if !W::wire_compatibility(&wire_id).is_compatible() {
        return Err(UWireMetadataError::WrongWireMetadata {
            expected: W::WIRE_ID,
            actual: wire_id,
        });
    }

    let payload_family_id = reader.read_identity_ref()?;
    if !W::payload_family_compatibility(&payload_family_id).is_compatible() {
        return Err(UWireMetadataError::PayloadFamilyMismatch {
            expected: W::PAYLOAD_FAMILY_ID,
            actual: payload_family_id,
        });
    }

    let flags = reader.read_u16()?;
    if flags != 0 {
        return Err(UWireMetadataError::UnsupportedReservedFlags(flags));
    }

    let attributes = UAttributes::parse_from_protobuf_bytes(reader.read_len_prefixed_bytes()?)?;
    let payload_encoding = reader.read_payload_encoding()?;
    reader.finish()?;

    UFrameMetadata::new(attributes, payload_encoding).map_err(UWireMetadataError::from)
}

fn write_identity_ref(out: &mut Vec<u8>, identity: WireIdentity) {
    out.push(ID_REF_COMPACT);
    write_u16(out, identity.compact_id());
}

fn write_payload_encoding(
    out: &mut Vec<u8>,
    payload_encoding: Option<&PayloadEncoding>,
) -> Result<(), UWireMetadataError> {
    match payload_encoding {
        None => out.push(ENCODING_NONE),
        Some(PayloadEncoding::Standard(format)) => {
            if *format == UPayloadFormat::Unspecified {
                return Err(UWireMetadataError::UnsupportedPayloadEncoding(
                    "UPayloadFormat::Unspecified is not a concrete payload encoding".to_string(),
                ));
            }
            out.push(ENCODING_STANDARD);
            write_i32(out, format.as_i32());
        }
        Some(PayloadEncoding::Custom { id, content_type }) => {
            out.push(ENCODING_CUSTOM);
            write_len_prefixed_str(out, id)?;
            write_len_prefixed_str(out, content_type)?;
        }
    }
    Ok(())
}

fn write_len_prefixed_str(out: &mut Vec<u8>, value: &str) -> Result<(), UWireMetadataError> {
    write_len_prefixed_bytes(out, value.as_bytes())
}

fn write_len_prefixed_bytes(out: &mut Vec<u8>, value: &[u8]) -> Result<(), UWireMetadataError> {
    let len = u32::try_from(value.len()).map_err(|_| {
        UWireMetadataError::MalformedMetadata(format!(
            "length {} exceeds u32 native-prefix limit",
            value.len()
        ))
    })?;
    write_u32(out, len);
    out.extend_from_slice(value);
    Ok(())
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

struct MetadataReader<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> MetadataReader<'a> {
    fn new(src: &'a [u8]) -> Self {
        Self { src, pos: 0 }
    }

    fn finish(&self) -> Result<(), UWireMetadataError> {
        if self.pos == self.src.len() {
            Ok(())
        } else {
            Err(UWireMetadataError::MalformedMetadata(format!(
                "{} trailing byte(s)",
                self.src.len() - self.pos
            )))
        }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], UWireMetadataError> {
        let end = self.pos.checked_add(len).ok_or_else(|| {
            UWireMetadataError::MalformedMetadata("metadata length overflow".to_string())
        })?;
        let chunk = self.src.get(self.pos..end).ok_or_else(|| {
            UWireMetadataError::MalformedMetadata(format!(
                "needed {len} byte(s) at offset {}, input has {} byte(s)",
                self.pos,
                self.src.len()
            ))
        })?;
        self.pos = end;
        Ok(chunk)
    }

    fn read_u8(&mut self) -> Result<u8, UWireMetadataError> {
        let bytes = self.take(1)?;
        bytes
            .first()
            .copied()
            .ok_or_else(|| UWireMetadataError::MalformedMetadata("missing u8 field".to_string()))
    }

    fn read_u16(&mut self) -> Result<u16, UWireMetadataError> {
        let bytes = self.take(2)?;
        let array = <[u8; 2]>::try_from(bytes)
            .map_err(|_| UWireMetadataError::MalformedMetadata("invalid u16 field".to_string()))?;
        Ok(u16::from_le_bytes(array))
    }

    fn read_u32(&mut self) -> Result<u32, UWireMetadataError> {
        let bytes = self.take(4)?;
        let array = <[u8; 4]>::try_from(bytes)
            .map_err(|_| UWireMetadataError::MalformedMetadata("invalid u32 field".to_string()))?;
        Ok(u32::from_le_bytes(array))
    }

    fn read_i32(&mut self) -> Result<i32, UWireMetadataError> {
        let bytes = self.take(4)?;
        let array = <[u8; 4]>::try_from(bytes)
            .map_err(|_| UWireMetadataError::MalformedMetadata("invalid i32 field".to_string()))?;
        Ok(i32::from_le_bytes(array))
    }

    fn read_identity_ref(&mut self) -> Result<WireIdentityRef, UWireMetadataError> {
        match self.read_u8()? {
            ID_REF_COMPACT => Ok(WireIdentityRef::Compact(self.read_u16()?)),
            ID_REF_LITERAL => {
                let bytes = self.read_len_prefixed_bytes()?;
                let value = std::str::from_utf8(bytes).map_err(|error| {
                    UWireMetadataError::MalformedMetadata(format!(
                        "identity literal is not UTF-8: {error}"
                    ))
                })?;
                Ok(WireIdentityRef::Literal(value.to_string()))
            }
            tag => Err(UWireMetadataError::MalformedMetadata(format!(
                "unknown identity reference tag {tag}"
            ))),
        }
    }

    fn read_len_prefixed_bytes(&mut self) -> Result<&'a [u8], UWireMetadataError> {
        let len = usize::try_from(self.read_u32()?).map_err(|_| {
            UWireMetadataError::MalformedMetadata("length prefix does not fit usize".to_string())
        })?;
        self.take(len)
    }

    fn read_len_prefixed_string(&mut self) -> Result<String, UWireMetadataError> {
        let bytes = self.read_len_prefixed_bytes()?;
        let value = std::str::from_utf8(bytes).map_err(|error| {
            UWireMetadataError::MalformedMetadata(format!(
                "payload encoding string is not UTF-8: {error}"
            ))
        })?;
        Ok(value.to_string())
    }

    fn read_payload_encoding(&mut self) -> Result<Option<PayloadEncoding>, UWireMetadataError> {
        match self.read_u8()? {
            ENCODING_NONE => Ok(None),
            ENCODING_STANDARD => {
                let raw = self.read_i32()?;
                let format = UPayloadFormat::from_i32(raw).ok_or_else(|| {
                    UWireMetadataError::UnsupportedPayloadEncoding(format!(
                        "unknown UPayloadFormat value {raw}"
                    ))
                })?;
                if format == UPayloadFormat::Unspecified {
                    return Err(UWireMetadataError::UnsupportedPayloadEncoding(
                        "UPayloadFormat::Unspecified is not a concrete payload encoding"
                            .to_string(),
                    ));
                }
                Ok(Some(PayloadEncoding::Standard(format)))
            }
            ENCODING_CUSTOM => {
                let id = self.read_len_prefixed_string()?;
                let content_type = self.read_len_prefixed_string()?;
                PayloadEncoding::custom(id, content_type)
                    .map(Some)
                    .map_err(UWireMetadataError::from)
            }
            tag => Err(UWireMetadataError::UnsupportedPayloadEncoding(format!(
                "unknown payload encoding tag {tag}"
            ))),
        }
    }
}
