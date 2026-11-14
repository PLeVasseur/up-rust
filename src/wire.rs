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

#![cfg_attr(not(feature = "wire-implementer-api"), allow(dead_code))]

//! Selected-wire identities, payload codecs, and metadata profiles.
//!
//! Wire authors define a [`UWire`] marker and stable [`WireIdentity`], associate
//! payload types through [`UWirePayload`], and implement the codec traits the
//! payload family requires. Physical carriage does not belong here. The
//! adapter in `wire_transport` composes a wire and metadata codec over any
//! compatible encoded core when that adapter capability is enabled.
//!
//! Receive checks identity before profile decoding. A mismatch is an explicit
//! unknown-identity error, never fallback to another decoder. These namespace
//! conventions are provisional SDK policy, requiring explicit peer agreement;
//! they do not create a normative wire/native-profile registry assignment.

#[cfg(any(feature = "protobuf-support", feature = "up-core-api"))]
use std::io::Read;
#[cfg(feature = "selected-wire-codec-core")]
use std::{error::Error, fmt::Display};

use crate::payload::codec::{DecodePayload, EncodePayload, PayloadCodec, ReadDecodePayload};
#[cfg(feature = "protobuf-support")]
use crate::payload::codec::{
    PayloadCodecIdentity, PayloadDecodeLimit, PayloadLayout, ProtobufPayload,
};
#[cfg(feature = "protobuf-support")]
use crate::payload::UWireError;
#[cfg(any(feature = "protobuf-support", feature = "up-core-api"))]
use crate::PayloadEncoding;
#[cfg(feature = "protobuf-support")]
use crate::ProtobufMappable;
#[cfg(feature = "selected-wire-codec-core")]
use crate::{UCode, UFrameMetadata, UFrameMetadataError, UStatus};

#[cfg(feature = "selected-wire-codec-core")]
const NATIVE_PREFIX_MAGIC: &[u8; 4] = b"UPWM";
#[cfg(feature = "selected-wire-codec-core")]
const ID_REF_COMPACT: u8 = 0x00;
#[cfg(feature = "selected-wire-codec-core")]
const ID_REF_LITERAL: u8 = 0x01;

pub use up_rust_core::wire::FORMAT_VERSION;

/// First provisional compact alias for deployment-private wire identities.
///
/// Private identities require their agreed literal on the wire. This band is
/// separate from the payload-encoding registry's private-use range.
pub const PRIVATE_COMPACT_ID_START: u16 = 0x8000;

/// Last provisional compact alias for deployment-private wire identities.
///
/// An alias alone does not identify a private wire, payload family or layout.
pub const PRIVATE_COMPACT_ID_END: u16 = 0xFFFE;

/// Identity for the first-wave uProtocol native selected wire.
pub use up_rust_core::wire::UPROTOCOL_NATIVE_WIRE_ID;

/// Payload-family identity for explicit native payload bytes.
pub use up_rust_core::wire::NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID;

/// Identity for the first-wave Protocol Buffers selected wire.
pub use up_rust_core::wire::PROTOBUF_WIRE_ID;

/// Payload-family identity for Protocol Buffers application payload bytes.
pub use up_rust_core::wire::PROTOBUF_PAYLOAD_FAMILY_ID;

/// Identity reserved for the external first-wave XCDRv2 selected wire.
///
/// The production `XcdrV2Wire` type is intentionally owned by the external
/// `up-wire-xcdrv2-rust` crate. `up-rust` exposes only the shared identity.
#[cfg_attr(not(feature = "wire-implementer-api"), allow(unused_imports))]
pub use up_rust_core::wire::XCDR_V2_WIRE_ID;

/// Payload-family identity reserved for external XCDRv2 payload bytes.
#[cfg_attr(not(feature = "wire-implementer-api"), allow(unused_imports))]
pub use up_rust_core::wire::XCDR_V2_PAYLOAD_FAMILY_ID;

/// Deployment-private stable-container selected-wire identity.
#[cfg(feature = "zero-copy-transport")]
pub use up_rust_core::wire::STABLE_CONTAINER_WIRE_ID;

/// Deployment-private stable-container payload-family identity.
#[cfg(feature = "zero-copy-transport")]
pub use up_rust_core::wire::STABLE_CONTAINER_PAYLOAD_FAMILY_ID;

// Compact metadata-layout value 1 is reserved by this SDK format policy and
// has no decoder. This reservation does not apply to the other namespaces.
const RESERVED_METADATA_LAYOUT_COMPACT_ID: u16 = 0x0001;

/// Metadata layout identity for the canonical UFrame metadata field block
/// carried behind native-prefix framing.
///
/// The layout identity is owned by the metadata codec
/// (`UWireMetadataCodec::METADATA_LAYOUT_ID`), not by the selected wire:
/// the same wire can be carried with different metadata profiles, and the
/// profile on the wire must always name the codec that produced it. Decoding
/// bytes of another profile with this codec fails as
/// `UWireMetadataError::UnknownMetadataLayoutId`, never as generic malformed
/// metadata.
#[cfg_attr(not(feature = "selected-wire-codec-core"), allow(unused_imports))]
pub use up_rust_core::wire::UFRAME_FIELDS_METADATA_LAYOUT_ID;

pub use up_rust_core::wire::{WireIdentity, WireIdentityNamespace};

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
    ///
    /// Compact-only equality is valid only for a registered first-wave pair.
    /// Private/unregistered identities require literal equality, including when
    /// two vendors choose the same compact alias. Metadata codecs additionally
    /// enforce registration in the particular wire/family/layout namespace.
    #[must_use]
    pub fn matches(&self, expected: WireIdentity) -> bool {
        match self {
            Self::Compact(actual) => {
                *actual == expected.compact_id()
                    && [
                        WireIdentityNamespace::Wire,
                        WireIdentityNamespace::PayloadFamily,
                        WireIdentityNamespace::MetadataLayout,
                    ]
                    .iter()
                    .any(|namespace| namespace.is_registered(expected))
            }
            Self::Literal(actual) => actual == expected.literal_id(),
        }
    }

    fn matches_in(&self, expected: WireIdentity, namespace: WireIdentityNamespace) -> bool {
        self.reference_policy_allows(expected, namespace) && self.matches(expected)
    }

    fn reference_policy_allows(
        &self,
        expected: WireIdentity,
        namespace: WireIdentityNamespace,
    ) -> bool {
        match self {
            Self::Literal(_) => true,
            Self::Compact(actual) => {
                namespace.is_registered(expected)
                    && namespace
                        .registered_identities()
                        .iter()
                        .any(|identity| identity.compact_id() == *actual)
            }
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
    /// Metadata byte-format version supported by this selected wire.
    ///
    /// The metadata byte layout itself is declared by the metadata codec
    /// ([`UWireMetadataCodec::METADATA_LAYOUT_ID`]), not by the wire.
    const FORMAT_VERSION: u16;
    /// Whether this selected wire can only operate under a native route agreement.
    ///
    /// Wires whose payload codecs resolve identities through a native profile
    /// agreement set this to `true`; an adapter for such a wire can then only
    /// be built through a constructor that supplies the agreement, and building
    /// one without it is a compile-time error instead of a per-message runtime
    /// error.
    const REQUIRES_NATIVE_AGREEMENT: bool = false;

    /// Checks selected-wire compatibility before public frame exposure.
    fn wire_compatibility(actual: &WireIdentityRef) -> WireCompatibility {
        if actual.matches_in(Self::WIRE_ID, WireIdentityNamespace::Wire) {
            WireCompatibility::Compatible
        } else {
            WireCompatibility::Incompatible
        }
    }

    /// Checks payload-family compatibility before public frame exposure.
    fn payload_family_compatibility(actual: &WireIdentityRef) -> WireCompatibility {
        if actual.matches_in(
            Self::PAYLOAD_FAMILY_ID,
            WireIdentityNamespace::PayloadFamily,
        ) {
            WireCompatibility::Compatible
        } else {
            WireCompatibility::Incompatible
        }
    }
}

/// Selected-wire metadata context consumed by metadata codecs.
#[cfg(feature = "selected-wire-codec-core")]
#[derive(Clone, Copy, Debug)]
pub struct UWireMetadataContext {
    /// Stable identity for the full selected wire.
    pub wire_id: WireIdentity,
    /// Payload operation family supported by this selected wire.
    pub payload_family_id: WireIdentity,
    /// Metadata byte layout used by this selected wire.
    pub metadata_layout_id: WireIdentity,
    /// Metadata byte-format version supported by this selected wire.
    pub format_version: u16,
    wire_compatibility: Option<fn(&WireIdentityRef) -> WireCompatibility>,
    payload_family_compatibility: Option<fn(&WireIdentityRef) -> WireCompatibility>,
}

#[cfg(feature = "selected-wire-codec-core")]
impl UWireMetadataContext {
    /// Builds a metadata context for a selected wire carried by metadata codec `C`.
    ///
    /// The wire contributes its wire and payload-family identities, version and
    /// compatibility rules; the codec contributes the metadata layout identity.
    #[must_use]
    pub fn from_wire<W, C>() -> Self
    where
        W: UWire,
        C: UWireMetadataCodec,
    {
        Self {
            wire_id: W::WIRE_ID,
            payload_family_id: W::PAYLOAD_FAMILY_ID,
            metadata_layout_id: C::METADATA_LAYOUT_ID,
            format_version: W::FORMAT_VERSION,
            wire_compatibility: Some(W::wire_compatibility),
            payload_family_compatibility: Some(W::payload_family_compatibility),
        }
    }

    /// Builds a metadata context with exact identity matching.
    #[must_use]
    pub fn new_exact(
        wire_id: WireIdentity,
        payload_family_id: WireIdentity,
        metadata_layout_id: WireIdentity,
        format_version: u16,
    ) -> Self {
        Self {
            wire_id,
            payload_family_id,
            metadata_layout_id,
            format_version,
            wire_compatibility: None,
            payload_family_compatibility: None,
        }
    }

    /// Returns this context with the metadata layout identity replaced.
    ///
    /// [`Self::from_wire`] already takes the layout from the codec; codecs
    /// still overlay their identity before encoding and decoding so that a
    /// context built with [`Self::new_exact`] cannot name a profile the codec
    /// does not produce.
    #[must_use]
    pub fn with_metadata_layout(mut self, metadata_layout_id: WireIdentity) -> Self {
        self.metadata_layout_id = metadata_layout_id;
        self
    }

    /// Checks selected-wire compatibility before public frame exposure.
    ///
    /// Compact-only private or unregistered references are rejected before the
    /// configured callback. Literal compatibility rules must implement the
    /// deployment's explicit identity agreement; no numeric alias is evidence
    /// that a different private literal is compatible.
    #[must_use]
    pub fn wire_compatibility(&self, actual: &WireIdentityRef) -> WireCompatibility {
        if !actual.reference_policy_allows(self.wire_id, WireIdentityNamespace::Wire) {
            return WireCompatibility::Incompatible;
        }
        if let Some(wire_compatibility) = self.wire_compatibility {
            return wire_compatibility(actual);
        }
        if actual.matches_in(self.wire_id, WireIdentityNamespace::Wire) {
            WireCompatibility::Compatible
        } else {
            WireCompatibility::Incompatible
        }
    }

    /// Checks payload-family compatibility before public frame exposure.
    ///
    /// The mandatory literal-reference rule applies before a custom family
    /// callback, which must check its explicitly agreed literal identities.
    #[must_use]
    pub fn payload_family_compatibility(&self, actual: &WireIdentityRef) -> WireCompatibility {
        if !actual
            .reference_policy_allows(self.payload_family_id, WireIdentityNamespace::PayloadFamily)
        {
            return WireCompatibility::Incompatible;
        }
        if let Some(payload_family_compatibility) = self.payload_family_compatibility {
            return payload_family_compatibility(actual);
        }
        if actual.matches_in(self.payload_family_id, WireIdentityNamespace::PayloadFamily) {
            WireCompatibility::Compatible
        } else {
            WireCompatibility::Incompatible
        }
    }
}

/// Default first-wave native wire for explicit native payload paths.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UProtocolNativeWire;

impl UWire for UProtocolNativeWire {
    const WIRE_ID: WireIdentity = UPROTOCOL_NATIVE_WIRE_ID;
    const PAYLOAD_FAMILY_ID: WireIdentity = NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID;
    const FORMAT_VERSION: u16 = FORMAT_VERSION;
}

/// Selected wire for Protocol Buffers application payloads.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProtobufWire;

impl UWire for ProtobufWire {
    const WIRE_ID: WireIdentity = PROTOBUF_WIRE_ID;
    const PAYLOAD_FAMILY_ID: WireIdentity = PROTOBUF_PAYLOAD_FAMILY_ID;
    const FORMAT_VERSION: u16 = FORMAT_VERSION;
}

/// Selected wire for deployment-private stable in-memory payloads.
#[cfg(feature = "zero-copy-transport")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StableContainerWire;

#[cfg(feature = "zero-copy-transport")]
impl UWire for StableContainerWire {
    const WIRE_ID: WireIdentity = STABLE_CONTAINER_WIRE_ID;
    const PAYLOAD_FAMILY_ID: WireIdentity = STABLE_CONTAINER_PAYLOAD_FAMILY_ID;
    const FORMAT_VERSION: u16 = FORMAT_VERSION;
    const REQUIRES_NATIVE_AGREEMENT: bool = true;
}

#[cfg(feature = "zero-copy-transport")]
impl<T: crate::StablePayload> UWirePayload<T> for StableContainerWire {
    type Codec = crate::StableContainerPayload<T>;
}

#[cfg(feature = "protobuf-support")]
impl PayloadCodecIdentity for ProtobufWire {
    fn payload_family() -> Option<WireIdentity> {
        Some(PROTOBUF_PAYLOAD_FAMILY_ID)
    }
    fn name() -> &'static str {
        ProtobufPayload::name()
    }

    fn encoding() -> PayloadEncoding {
        ProtobufPayload::encoding()
    }
}

#[cfg(feature = "protobuf-support")]
impl<T> EncodePayload<T> for ProtobufWire
where
    T: ProtobufMappable,
{
    fn payload_layout(value: &T) -> Result<PayloadLayout, UWireError> {
        ProtobufPayload::payload_layout(value)
    }

    fn encode_payload(value: &T, dst: &mut [u8]) -> Result<(), UWireError> {
        ProtobufPayload::encode_payload(value, dst)
    }
}

#[cfg(feature = "protobuf-support")]
impl<'a, T> DecodePayload<'a, T> for ProtobufWire
where
    T: ProtobufMappable,
{
    fn decode_payload(src: &'a [u8]) -> Result<T, UWireError> {
        ProtobufPayload::decode_payload(src)
    }
}

#[cfg(feature = "protobuf-support")]
impl<T> ReadDecodePayload<T> for ProtobufWire
where
    T: ProtobufMappable,
{
    fn decode_payload_from_reader<R: Read>(
        reader: R,
        payload_len: usize,
        limit: PayloadDecodeLimit,
    ) -> Result<T, UWireError> {
        ProtobufPayload::decode_payload_from_reader(reader, payload_len, limit)
    }
}

/// Explicit metadata codec used by selected-wire transport adapters.
#[cfg(feature = "selected-wire-codec-core")]
pub trait UWireMetadataCodec {
    /// Metadata byte-profile identity this codec writes and accepts.
    ///
    /// The codec, not the selected wire, owns the layout identity. Codecs MUST
    /// overlay this identity onto the context they encode/decode with so that
    /// cross-profile decode fails as
    /// [`UWireMetadataError::UnknownMetadataLayoutId`].
    const METADATA_LAYOUT_ID: WireIdentity;

    /// Encodes native frame metadata for the selected-wire context.
    ///
    /// # Errors
    ///
    /// Returns an error when metadata is invalid or cannot be serialized.
    fn encode_frame_metadata(
        &self,
        context: UWireMetadataContext,
        metadata: &UFrameMetadata,
    ) -> Result<Vec<u8>, UWireMetadataError> {
        let mut out = vec![0; self.encoded_frame_metadata_len(context, metadata)?];
        let written = self.encode_frame_metadata_into(context, metadata, &mut out)?;
        out.truncate(written);
        Ok(out)
    }

    /// Computes the exact encoded metadata length before obtaining caller storage.
    ///
    /// # Errors
    /// Rejects invalid metadata, identities or unrepresentable lengths.
    fn encoded_frame_metadata_len(
        &self,
        context: UWireMetadataContext,
        metadata: &UFrameMetadata,
    ) -> Result<usize, UWireMetadataError>;

    /// Encodes metadata directly into caller-owned bytes and returns its extent.
    ///
    /// No output allocation or resize is performed. An undersized destination is
    /// rejected before writing; bytes beyond the returned extent are unchanged.
    ///
    /// # Errors
    /// Rejects invalid metadata, identities, lengths or insufficient output.
    fn encode_frame_metadata_into(
        &self,
        context: UWireMetadataContext,
        metadata: &UFrameMetadata,
        out: &mut [u8],
    ) -> Result<usize, UWireMetadataError>;

    /// Decodes and validates native frame metadata for the selected-wire context.
    ///
    /// # Errors
    ///
    /// Returns an error when bytes are malformed, use an unsupported layout or
    /// version, or declare an incompatible wire or payload family.
    fn decode_frame_metadata(
        &self,
        context: UWireMetadataContext,
        src: &[u8],
    ) -> Result<UFrameMetadata, UWireMetadataError>;
}

/// Metadata codec that is statically compatible with selected wire `W`.
///
/// This marker keeps the ordinary metadata codec contract reusable while
/// allowing adapter constructors to reject invalid wire/codec pairings at
/// compile time.
#[cfg(feature = "selected-wire-codec-core")]
pub trait UWireMetadataCodecFor<W>: UWireMetadataCodec
where
    W: UWire,
{
}

/// Metadata codec for the UFrame field block, carried in a selected-wire prefix.
///
/// This codec owns [`UFRAME_FIELDS_METADATA_LAYOUT_ID`] and serializes
/// [`UFrameMetadata`] with [`crate::frame::codec`]. It preserves numeric payload
/// identities and checks the layout, wire, family and version before decoding
/// metadata. Bindings own physical placement of the resulting bytes.
#[cfg(feature = "selected-wire-codec-core")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UFrameFieldsMetadataCodec;

#[cfg(feature = "selected-wire-codec-core")]
impl UFrameFieldsMetadataCodec {
    /// Obtains the exact selected-wire prefix during route/registration setup.
    ///
    /// This excludes the following field-block length word and metadata. It uses
    /// the same identity encoding as the ordinary codec, including required
    /// literals for private identities; callers never need to encode a probe
    /// message or search its bytes to discover the prefix.
    ///
    /// # Errors
    /// Rejects an invalid or unrepresentable wire/family/layout identity.
    pub fn native_prefix(
        &self,
        context: UWireMetadataContext,
    ) -> Result<Vec<u8>, UWireMetadataError> {
        let context = context.with_metadata_layout(Self::METADATA_LAYOUT_ID);
        let mut out = Vec::with_capacity(native_prefix_len(context)?);
        write_native_prefix(&mut out, context)?;
        Ok(out)
    }

    /// Writes the agreed native prefix into fixed caller-owned setup storage.
    ///
    /// This excludes the subsequent field-block length and metadata. Private
    /// identities retain their literal bytes; no registry assignment is inferred.
    ///
    /// # Errors
    /// Rejects invalid identities or insufficient output before writing the prefix.
    pub fn native_prefix_into(
        &self,
        context: UWireMetadataContext,
        out: &mut [u8],
    ) -> Result<usize, UWireMetadataError> {
        let context = context.with_metadata_layout(Self::METADATA_LAYOUT_ID);
        let required = native_prefix_len(context)?;
        if out.len() < required {
            return Err(UWireMetadataError::OutputTooShort {
                required,
                available: out.len(),
            });
        }
        let mut writer = MetadataSliceWriter {
            bytes: out,
            position: 0,
        };
        write_native_prefix(&mut writer, context)?;
        Ok(writer.position)
    }

    #[cfg(feature = "zero-copy-transport")]
    pub(crate) fn stable_prefix_len(
        &self,
        bytes: &[u8],
    ) -> Result<usize, crate::UFrameHeaderError> {
        self.exact_prefix_len(
            UWireMetadataContext::from_wire::<StableContainerWire, Self>(),
            bytes,
        )
    }

    #[cfg(feature = "zero-copy-transport")]
    pub(crate) fn exact_prefix_len(
        &self,
        context: UWireMetadataContext,
        bytes: &[u8],
    ) -> Result<usize, crate::UFrameHeaderError> {
        use crate::UFrameHeaderError;
        let context = context.with_metadata_layout(Self::METADATA_LAYOUT_ID);
        let mut reader = BorrowedPrefixReader { bytes, position: 0 };
        if reader.take(4)? != NATIVE_PREFIX_MAGIC {
            return Err(UFrameHeaderError::WrongPrefix);
        }
        reader.identity(
            context.metadata_layout_id,
            WireIdentityNamespace::MetadataLayout,
        )?;
        if reader.u16()? != context.format_version {
            return Err(UFrameHeaderError::WrongPrefix);
        }
        reader.identity(context.wire_id, WireIdentityNamespace::Wire)?;
        reader.identity(
            context.payload_family_id,
            WireIdentityNamespace::PayloadFamily,
        )?;
        if reader.u16()? != 0 {
            return Err(UFrameHeaderError::Malformed("native reserved flags"));
        }
        Ok(reader.position)
    }
}

// StableContainerWire uses the SDK's default identity policy. Match borrowed
// references directly, including valid literal spellings of registered pairs;
// do not allocate a WireIdentityRef::Literal merely to validate received input.
#[cfg(feature = "zero-copy-transport")]
struct BorrowedPrefixReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

#[cfg(feature = "zero-copy-transport")]
impl<'a> BorrowedPrefixReader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], crate::UFrameHeaderError> {
        let end = self
            .position
            .checked_add(len)
            .ok_or(crate::UFrameHeaderError::Truncated)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(crate::UFrameHeaderError::Truncated)?;
        self.position = end;
        Ok(value)
    }
    fn u16(&mut self) -> Result<u16, crate::UFrameHeaderError> {
        let mut bytes = [0; 2];
        bytes.copy_from_slice(self.take(2)?);
        Ok(u16::from_le_bytes(bytes))
    }
    fn identity(
        &mut self,
        expected: WireIdentity,
        namespace: WireIdentityNamespace,
    ) -> Result<(), crate::UFrameHeaderError> {
        use crate::UFrameHeaderError;
        let tag = *self.take(1)?.first().expect("one identity tag");
        let matches = match tag {
            ID_REF_COMPACT => {
                let actual = self.u16()?;
                namespace.is_registered(expected) && actual == expected.compact_id()
            }
            ID_REF_LITERAL => {
                let mut bytes = [0; 4];
                bytes.copy_from_slice(self.take(4)?);
                let len = usize::try_from(u32::from_le_bytes(bytes))
                    .map_err(|_| UFrameHeaderError::Truncated)?;
                let actual = std::str::from_utf8(self.take(len)?)
                    .map_err(|_| UFrameHeaderError::Malformed("identity UTF-8"))?;
                actual == expected.literal_id()
            }
            _ => return Err(UFrameHeaderError::Malformed("identity tag")),
        };
        if matches {
            Ok(())
        } else {
            Err(UFrameHeaderError::WrongPrefix)
        }
    }
}

#[cfg(feature = "selected-wire-codec-core")]
fn native_prefix_len(context: UWireMetadataContext) -> Result<usize, UWireMetadataError> {
    let mut len = NATIVE_PREFIX_MAGIC.len() + 2 * std::mem::size_of::<u16>();
    for (identity, namespace) in [
        (
            context.metadata_layout_id,
            WireIdentityNamespace::MetadataLayout,
        ),
        (context.wire_id, WireIdentityNamespace::Wire),
        (
            context.payload_family_id,
            WireIdentityNamespace::PayloadFamily,
        ),
    ] {
        len = len
            .checked_add(identity_ref_len(identity, namespace)?)
            .ok_or_else(|| {
                UWireMetadataError::SerializationError("identity prefix length overflow".into())
            })?;
    }
    Ok(len)
}

#[cfg(feature = "selected-wire-codec-core")]
fn write_native_prefix(
    out: &mut impl MetadataOutput,
    context: UWireMetadataContext,
) -> Result<(), UWireMetadataError> {
    out.extend_from_slice(NATIVE_PREFIX_MAGIC)?;
    write_identity_ref(
        out,
        context.metadata_layout_id,
        WireIdentityNamespace::MetadataLayout,
    )?;
    write_u16(out, context.format_version)?;
    write_identity_ref(out, context.wire_id, WireIdentityNamespace::Wire)?;
    write_identity_ref(
        out,
        context.payload_family_id,
        WireIdentityNamespace::PayloadFamily,
    )?;
    write_u16(out, 0)?;
    Ok(())
}

#[cfg(feature = "selected-wire-codec-core")]
impl UWireMetadataCodec for UFrameFieldsMetadataCodec {
    const METADATA_LAYOUT_ID: WireIdentity = UFRAME_FIELDS_METADATA_LAYOUT_ID;

    fn encoded_frame_metadata_len(
        &self,
        context: UWireMetadataContext,
        metadata: &UFrameMetadata,
    ) -> Result<usize, UWireMetadataError> {
        let context = context.with_metadata_layout(Self::METADATA_LAYOUT_ID);
        let fields = crate::frame::codec::frame_metadata_fields_len(metadata)
            .map_err(UWireMetadataError::FieldBlock)?;
        u32::try_from(fields).map_err(|_| {
            UWireMetadataError::FieldBlock(crate::frame::codec::UFrameFieldsError::ValueTooLong {
                field: "metadata",
            })
        })?;
        native_prefix_len(context)?
            .checked_add(4)
            .and_then(|len| len.checked_add(fields))
            .ok_or(UWireMetadataError::FieldBlock(
                crate::frame::codec::UFrameFieldsError::ValueTooLong { field: "metadata" },
            ))
    }

    fn encode_frame_metadata_into(
        &self,
        context: UWireMetadataContext,
        metadata: &UFrameMetadata,
        out: &mut [u8],
    ) -> Result<usize, UWireMetadataError> {
        let context = context.with_metadata_layout(Self::METADATA_LAYOUT_ID);
        let required = self.encoded_frame_metadata_len(context, metadata)?;
        if out.len() < required {
            return Err(UWireMetadataError::OutputTooShort {
                required,
                available: out.len(),
            });
        }
        let mut writer = MetadataSliceWriter {
            bytes: out,
            position: 0,
        };
        write_native_prefix(&mut writer, context)?;
        let fields = required - writer.position - 4;
        write_u32(
            &mut writer,
            u32::try_from(fields).expect("field length checked above"),
        )?;
        let position = writer.position;
        let available = writer.bytes.len();
        let destination =
            writer
                .bytes
                .get_mut(position..required)
                .ok_or(UWireMetadataError::OutputTooShort {
                    required,
                    available,
                })?;
        let written = crate::frame::codec::encode_frame_metadata_fields_to(metadata, destination)
            .map_err(UWireMetadataError::FieldBlock)?;
        debug_assert_eq!(written, fields);
        Ok(required)
    }

    fn encode_frame_metadata(
        &self,
        context: UWireMetadataContext,
        metadata: &UFrameMetadata,
    ) -> Result<Vec<u8>, UWireMetadataError> {
        let context = context.with_metadata_layout(Self::METADATA_LAYOUT_ID);
        let fields_len = crate::frame::codec::frame_metadata_fields_len(metadata)
            .map_err(|error| UWireMetadataError::FrameMetadata(error.to_string()))?;
        let capacity = native_prefix_len(context)?
            .checked_add(std::mem::size_of::<u32>())
            .and_then(|len| len.checked_add(fields_len))
            .ok_or_else(|| UWireMetadataError::FrameMetadata("metadata length overflow".into()))?;
        let mut out = Vec::with_capacity(capacity);
        write_native_prefix(&mut out, context)?;
        let fields_len = u32::try_from(fields_len)
            .map_err(|_| UWireMetadataError::FrameMetadata("metadata is too long".into()))?;
        write_u32(&mut out, fields_len)?;
        crate::frame::codec::encode_frame_metadata_fields_into(metadata, &mut out)
            .map_err(|error| UWireMetadataError::FrameMetadata(error.to_string()))?;
        Ok(out)
    }

    fn decode_frame_metadata(
        &self,
        context: UWireMetadataContext,
        src: &[u8],
    ) -> Result<UFrameMetadata, UWireMetadataError> {
        let context = context.with_metadata_layout(Self::METADATA_LAYOUT_ID);
        let mut reader = MetadataReader::new(src);
        read_and_check_native_prefix(&mut reader, context)?;
        let fields = reader.read_len_prefixed_bytes()?;
        reader.finish()?;
        crate::frame::codec::decode_frame_metadata_fields(fields)
            .map_err(|error| UWireMetadataError::MalformedMetadata(error.to_string()))
    }
}

#[cfg(feature = "selected-wire-codec-core")]
impl<W> UWireMetadataCodecFor<W> for UFrameFieldsMetadataCodec where W: UWire {}

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

/// Selected-wire payload mapping for typed payloads.
///
/// The wire marker, not the payload type, chooses one payload codec for `T`.
/// The associated codec need not be the wire marker itself. Its implemented
/// capability traits decide which operations are available: encode/decode,
/// initialized TX loan, uninitialized TX loan, and receive-side typed borrow.
/// Implementing this mapping does not grant capabilities the codec does not
/// implement and does not make [`ReadDecodePayload`] a borrowed or in-place
/// decode API.
pub trait UWirePayload<T>: UWire {
    /// Concrete payload codec used by this selected wire for `T`.
    type Codec: PayloadCodec;
}

#[cfg(feature = "protobuf-support")]
impl<T> UWirePayload<T> for ProtobufWire
where
    T: ProtobufMappable,
{
    type Codec = ProtobufPayload;
}

/// Errors returned by native-prefix selected-wire metadata handling.
#[cfg(feature = "selected-wire-codec-core")]
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum UWireMetadataError {
    /// Caller-provided metadata storage is too small.
    OutputTooShort {
        /// Minimum output length in bytes.
        required: usize,
        /// Provided output length in bytes.
        available: usize,
    },
    /// A caller-storage encoder rejected the field block without formatting it.
    FieldBlock(crate::frame::codec::UFrameFieldsError),
    /// The input did not start with the native-prefix metadata magic bytes.
    WrongMagic,
    /// The metadata layout id is not the one the configured codec produces.
    UnknownMetadataLayoutId {
        /// The metadata layout id actually present in the input.
        actual: WireIdentityRef,
    },
    /// The metadata version is unsupported by the selected wire.
    UnsupportedVersion {
        /// The version this codec supports.
        expected: u16,
        /// The version present in the input.
        actual: u16,
    },
    /// The selected-wire id is incompatible with `W`.
    WrongWireMetadata {
        /// The selected wire's identity.
        expected: WireIdentity,
        /// The wire identity present in the input.
        actual: WireIdentityRef,
    },
    /// The payload-family id is incompatible with `W`.
    PayloadFamilyMismatch {
        /// The selected wire's payload-family identity.
        expected: WireIdentity,
        /// The payload-family identity present in the input.
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

#[cfg(feature = "selected-wire-codec-core")]
impl Display for UWireMetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutputTooShort {
                required,
                available,
            } => write!(f, "metadata needs {required} output bytes, got {available}"),
            Self::FieldBlock(error) => Display::fmt(error, f),
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

#[cfg(feature = "selected-wire-codec-core")]
impl Error for UWireMetadataError {}

#[cfg(feature = "selected-wire-codec-core")]
impl From<UFrameMetadataError> for UWireMetadataError {
    fn from(value: UFrameMetadataError) -> Self {
        Self::FrameMetadata(value.to_string())
    }
}

#[cfg(feature = "selected-wire-codec-core")]
impl From<UWireMetadataError> for UStatus {
    fn from(value: UWireMetadataError) -> Self {
        UStatus::fail_with_code(UCode::InvalidArgument, value.to_string())
    }
}

/// Reads and validates the shared native-prefix framing: magic, metadata
/// layout id, format version, wire id, payload family id, and reserved flags.
#[cfg(feature = "selected-wire-codec-core")]
fn read_and_check_native_prefix(
    reader: &mut MetadataReader<'_>,
    context: UWireMetadataContext,
) -> Result<(), UWireMetadataError> {
    if reader.take(NATIVE_PREFIX_MAGIC.len())? != NATIVE_PREFIX_MAGIC.as_slice() {
        return Err(UWireMetadataError::WrongMagic);
    }

    let layout_id = reader.read_identity_ref()?;
    if layout_id == WireIdentityRef::Compact(RESERVED_METADATA_LAYOUT_COMPACT_ID) {
        return Err(UWireMetadataError::UnknownMetadataLayoutId { actual: layout_id });
    }
    if !layout_id.matches_in(
        context.metadata_layout_id,
        WireIdentityNamespace::MetadataLayout,
    ) {
        return Err(UWireMetadataError::UnknownMetadataLayoutId { actual: layout_id });
    }

    let version = reader.read_u16()?;
    if version != context.format_version {
        return Err(UWireMetadataError::UnsupportedVersion {
            expected: context.format_version,
            actual: version,
        });
    }

    let wire_id = reader.read_identity_ref()?;
    if !context.wire_compatibility(&wire_id).is_compatible() {
        return Err(UWireMetadataError::WrongWireMetadata {
            expected: context.wire_id,
            actual: wire_id,
        });
    }

    let payload_family_id = reader.read_identity_ref()?;
    if !context
        .payload_family_compatibility(&payload_family_id)
        .is_compatible()
    {
        return Err(UWireMetadataError::PayloadFamilyMismatch {
            expected: context.payload_family_id,
            actual: payload_family_id,
        });
    }

    let flags = reader.read_u16()?;
    if flags != 0 {
        return Err(UWireMetadataError::UnsupportedReservedFlags(flags));
    }
    Ok(())
}

#[cfg(feature = "selected-wire-codec-core")]
fn identity_ref_len(
    identity: WireIdentity,
    namespace: WireIdentityNamespace,
) -> Result<usize, UWireMetadataError> {
    if namespace.is_registered(identity) {
        return Ok(1 + std::mem::size_of::<u16>());
    }
    u32::try_from(identity.literal_id().len()).map_err(|_| {
        UWireMetadataError::SerializationError("identity literal is too long".into())
    })?;
    identity
        .literal_id()
        .len()
        .checked_add(1 + std::mem::size_of::<u32>())
        .ok_or_else(|| {
            UWireMetadataError::SerializationError("identity reference length overflow".into())
        })
}

#[cfg(feature = "selected-wire-codec-core")]
fn write_identity_ref(
    out: &mut impl MetadataOutput,
    identity: WireIdentity,
    namespace: WireIdentityNamespace,
) -> Result<(), UWireMetadataError> {
    if namespace.is_registered(identity) {
        out.push(ID_REF_COMPACT)?;
        write_u16(out, identity.compact_id())?;
    } else {
        let length = u32::try_from(identity.literal_id().len()).map_err(|_| {
            UWireMetadataError::SerializationError("identity literal is too long".into())
        })?;
        out.push(ID_REF_LITERAL)?;
        write_u32(out, length)?;
        out.extend_from_slice(identity.literal_id().as_bytes())?;
    }
    Ok(())
}

#[cfg(feature = "selected-wire-codec-core")]
fn write_u16(out: &mut impl MetadataOutput, value: u16) -> Result<(), UWireMetadataError> {
    out.extend_from_slice(&value.to_le_bytes())
}

#[cfg(feature = "selected-wire-codec-core")]
fn write_u32(out: &mut impl MetadataOutput, value: u32) -> Result<(), UWireMetadataError> {
    out.extend_from_slice(&value.to_le_bytes())
}

#[cfg(feature = "selected-wire-codec-core")]
trait MetadataOutput {
    fn extend_from_slice(&mut self, bytes: &[u8]) -> Result<(), UWireMetadataError>;
    fn push(&mut self, byte: u8) -> Result<(), UWireMetadataError> {
        self.extend_from_slice(&[byte])
    }
}
#[cfg(feature = "selected-wire-codec-core")]
impl MetadataOutput for Vec<u8> {
    fn extend_from_slice(&mut self, bytes: &[u8]) -> Result<(), UWireMetadataError> {
        Vec::extend_from_slice(self, bytes);
        Ok(())
    }
}
#[cfg(feature = "selected-wire-codec-core")]
struct MetadataSliceWriter<'a> {
    bytes: &'a mut [u8],
    position: usize,
}
#[cfg(feature = "selected-wire-codec-core")]
impl MetadataOutput for MetadataSliceWriter<'_> {
    fn extend_from_slice(&mut self, bytes: &[u8]) -> Result<(), UWireMetadataError> {
        let available = self.bytes.len();
        let end =
            self.position
                .checked_add(bytes.len())
                .ok_or(UWireMetadataError::OutputTooShort {
                    required: usize::MAX,
                    available,
                })?;
        self.bytes
            .get_mut(self.position..end)
            .ok_or(UWireMetadataError::OutputTooShort {
                required: end,
                available,
            })?
            .copy_from_slice(bytes);
        self.position = end;
        Ok(())
    }
}

#[cfg(feature = "selected-wire-codec-core")]
struct MetadataReader<'a> {
    src: &'a [u8],
    pos: usize,
}

#[cfg(feature = "selected-wire-codec-core")]
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
}

/// Binds payload types to a wire whose marker is its own codec
/// (`Codec = Self`), removing the most repetitive line of an all-in-one
/// wire implementation.
///
/// ```rust,no_run
/// # #[cfg(feature = "wire-implementer-api")]
/// # fn main() {
/// # use up_rust::{bind_wire_self_codec, DecodePayload, EncodePayload,
/// #     PayloadCodec, PayloadEncoding, PayloadLayout, UWire, UWireError,
/// #     WireIdentity};
/// # struct MyWire;
/// # struct A(u32); struct B(u32);
/// # impl UWire for MyWire {
/// #     const WIRE_ID: WireIdentity = WireIdentity::new("demo.self-codec", 0x8043);
/// #     const PAYLOAD_FAMILY_ID: WireIdentity = WireIdentity::new("demo.self", 0x8043);
/// #     const FORMAT_VERSION: u16 = 1;
/// # }
/// # impl PayloadCodec for MyWire {
/// #     fn codec_name() -> &'static str { "demo-self-codec" }
/// #     fn payload_identity(_agreement: Option<&up_rust::NativeProfileAgreement>) -> Result<up_rust::PayloadIdentity, up_rust::UWireError> { Ok(up_rust::PayloadIdentity::Fixed(PayloadEncoding::RAW)) }
/// # }
/// # impl EncodePayload<A> for MyWire {
/// #     fn payload_layout(_: &A) -> Result<PayloadLayout, UWireError> { PayloadLayout::new(4, 4) }
/// #     fn encode_payload(v: &A, dst: &mut [u8]) -> Result<(), UWireError> {
/// #         dst.copy_from_slice(&v.0.to_le_bytes());
/// #         Ok(())
/// #     }
/// # }
/// # impl<'a> DecodePayload<'a, A> for MyWire {
/// #     fn decode_payload(src: &'a [u8]) -> Result<A, UWireError> {
/// #         Ok(A(u32::from_le_bytes(src.try_into().map_err(|_| UWireError::invalid_payload("demo payload must be 4 bytes"))?)))
/// #     }
/// # }
/// # impl EncodePayload<B> for MyWire {
/// #     fn payload_layout(_: &B) -> Result<PayloadLayout, UWireError> { PayloadLayout::new(4, 4) }
/// #     fn encode_payload(v: &B, dst: &mut [u8]) -> Result<(), UWireError> {
/// #         dst.copy_from_slice(&v.0.to_le_bytes());
/// #         Ok(())
/// #     }
/// # }
/// # impl<'a> DecodePayload<'a, B> for MyWire {
/// #     fn decode_payload(src: &'a [u8]) -> Result<B, UWireError> {
/// #         Ok(B(u32::from_le_bytes(src.try_into().map_err(|_| UWireError::invalid_payload("demo payload must be 4 bytes"))?)))
/// #     }
/// # }
/// // One wire identity, two payload types, marker as codec for both:
/// bind_wire_self_codec!(MyWire: A, B);
/// # }
/// # #[cfg(not(feature = "wire-implementer-api"))]
/// # fn main() {}
/// ```
///
/// Why not a trait default or a blanket impl? `type Codec = Self;` as an
/// associated-type default is unstable Rust, and a blanket
/// `impl<W, T> UWirePayload<T> for W` would forbid, by coherence, exactly the
/// wire that binds a different codec for some payload type, the case the
/// three-trait split exists to support. The macro adds the convenience without
/// closing that door.
#[macro_export]
macro_rules! bind_wire_self_codec {
    ($wire:ty : $($payload:ty),+ $(,)?) => {
        $(
            impl $crate::UWirePayload<$payload> for $wire {
                type Codec = $wire;
            }
        )+
    };
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "selected-wire-codec-core")]
    #[test_case::test_case("[v1.xy]", false; "IPvFuture source")]
    #[test_case::test_case("[v1.xy]", true; "IPvFuture sink")]
    #[test_case::test_case("vin/x", false; "path source")]
    #[test_case::test_case("vin/x", true; "path sink")]
    #[test_case::test_case("vin?x", false; "query source")]
    #[test_case::test_case("vin?x", true; "query sink")]
    #[test_case::test_case("vin#x", false; "fragment source")]
    #[test_case::test_case("vin#x", true; "fragment sink")]
    fn received_wire_metadata_rejects_bad_authority(authority: &str, in_sink: bool) {
        let context =
            UWireMetadataContext::from_wire::<UProtocolNativeWire, UFrameFieldsMetadataCodec>();
        let framed = |authority| {
            let fields = crate::frame::codec::received_authority_fields(authority, in_sink);
            let mut bytes = UFrameFieldsMetadataCodec.native_prefix(context).unwrap();
            bytes.extend_from_slice(&u32::try_from(fields.len()).unwrap().to_le_bytes());
            bytes.extend_from_slice(&fields);
            bytes
        };
        assert!(UFrameFieldsMetadataCodec
            .decode_frame_metadata(context, &framed("vehicle"))
            .is_ok());
        assert!(UFrameFieldsMetadataCodec
            .decode_frame_metadata(context, &framed(authority))
            .is_err());
    }

    #[cfg(feature = "selected-wire-codec-core")]
    #[test]
    fn fixed_prefix_writer_preserves_registered_and_private_identity_bytes() {
        for context in [
            UWireMetadataContext::from_wire::<UProtocolNativeWire, UFrameFieldsMetadataCodec>(),
            UWireMetadataContext::new_exact(
                WireIdentity::new("private.wire", 0xFF00),
                WireIdentity::new("private.family", 0xFF01),
                UFRAME_FIELDS_METADATA_LAYOUT_ID,
                1,
            ),
        ] {
            let expected = UFrameFieldsMetadataCodec.native_prefix(context).unwrap();
            let mut out = vec![0xA5; expected.len() + 5];
            let written = UFrameFieldsMetadataCodec
                .native_prefix_into(context, &mut out)
                .unwrap();
            assert_eq!(out.get(..written).unwrap(), expected);
            assert!(out.get(written..).unwrap().iter().all(|byte| *byte == 0xA5));
            let mut short = vec![0xA5; expected.len() - 1];
            assert!(matches!(
                UFrameFieldsMetadataCodec.native_prefix_into(context, &mut short),
                Err(UWireMetadataError::OutputTooShort { .. })
            ));
            assert!(short.iter().all(|byte| *byte == 0xA5));
        }
    }

    #[cfg(feature = "zero-copy-transport")]
    #[test]
    fn stable_container_identity_and_payload_mapping() {
        #[repr(C)]
        #[derive(crate::StablePayload)]
        #[stable_payload(type_name = "uprotocol.test.WireMapping")]
        struct WireMapping {
            value: u32,
        }
        fn mapping<
            W: super::UWirePayload<WireMapping, Codec = crate::StableContainerPayload<WireMapping>>,
        >() {
        }
        mapping::<super::StableContainerWire>();
        assert_eq!(
            super::STABLE_CONTAINER_WIRE_ID.literal_id(),
            "org.eclipse.uprotocol.wire.stable-container"
        );
        assert_eq!(super::STABLE_CONTAINER_WIRE_ID.compact_id(), 0x8000);
        assert_eq!(
            super::STABLE_CONTAINER_PAYLOAD_FAMILY_ID.literal_id(),
            "stable-container"
        );
        assert_eq!(
            super::STABLE_CONTAINER_PAYLOAD_FAMILY_ID.compact_id(),
            0x8001
        );
    }
    #[cfg(feature = "protobuf-support")]
    use protobuf::well_known_types::wrappers::StringValue;

    use super::*;
    #[cfg(feature = "protobuf-support")]
    use crate::payload::codec::PayloadCodec;

    #[cfg(feature = "selected-wire-codec-core")]
    #[test]
    fn private_layout_identity_is_checked_before_version_or_body() {
        let expected = WireIdentity::new("vendor-a.metadata", 0x8B00);
        let context = UWireMetadataContext::new_exact(
            UPROTOCOL_NATIVE_WIRE_ID,
            NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID,
            expected,
            FORMAT_VERSION,
        );
        let mut compact = b"UPWM".to_vec();
        compact.extend_from_slice(&[0, 0, 0x8B]);
        assert!(matches!(
            read_and_check_native_prefix(&mut MetadataReader::new(&compact), context),
            Err(UWireMetadataError::UnknownMetadataLayoutId { .. })
        ));

        let mut wrong_literal = b"UPWM".to_vec();
        wrong_literal.push(1);
        wrong_literal.extend_from_slice(&17u32.to_le_bytes());
        wrong_literal.extend_from_slice(b"vendor-b.metadata");
        assert!(matches!(
            read_and_check_native_prefix(&mut MetadataReader::new(&wrong_literal), context),
            Err(UWireMetadataError::UnknownMetadataLayoutId { .. })
        ));

        let mut correct_literal = b"UPWM".to_vec();
        correct_literal.push(1);
        correct_literal.extend_from_slice(&17u32.to_le_bytes());
        correct_literal.extend_from_slice(b"vendor-a.metadata");
        correct_literal.extend_from_slice(&[1, 0, 0, 1, 0, 0, 1, 0, 0, 0]);
        assert!(
            read_and_check_native_prefix(&mut MetadataReader::new(&correct_literal), context,)
                .is_ok()
        );
    }

    #[test]
    fn first_wave_wire_identity_constants_match_register() {
        assert_eq!(
            PROTOBUF_WIRE_ID.literal_id(),
            "org.eclipse.uprotocol.wire.protobuf"
        );
        assert_eq!(PROTOBUF_WIRE_ID.compact_id(), 0x0002);
        assert_eq!(PROTOBUF_PAYLOAD_FAMILY_ID.literal_id(), "protobuf");
        assert_eq!(PROTOBUF_PAYLOAD_FAMILY_ID.compact_id(), 0x0002);
        assert_eq!(
            XCDR_V2_WIRE_ID.literal_id(),
            "org.eclipse.uprotocol.wire.xcdr-v2"
        );
        assert_eq!(XCDR_V2_WIRE_ID.compact_id(), 0x0003);
        assert_eq!(XCDR_V2_PAYLOAD_FAMILY_ID.literal_id(), "xcdr-v2");
        assert_eq!(XCDR_V2_PAYLOAD_FAMILY_ID.compact_id(), 0x0003);
    }

    #[cfg(feature = "protobuf-support")]
    #[test]
    fn protobuf_wire_delegates_application_payload_codec() {
        let value = StringValue {
            value: "wire".to_string(),
            special_fields: Default::default(),
        };

        let encoded = ProtobufWire::encode_payload_owned(&value).expect("encode protobuf wire");
        let decoded: StringValue =
            ProtobufWire::decode_payload(&encoded).expect("decode protobuf wire");
        assert_eq!(decoded.value, "wire");
        assert_eq!(
            ProtobufWire::payload_encoding(None).unwrap(),
            ProtobufPayload::payload_encoding(None).unwrap()
        );
    }
}
