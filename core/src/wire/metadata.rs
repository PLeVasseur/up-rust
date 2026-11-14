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

//! The selected-wire metadata prefix, rendered at setup and matched at receive.
//!
//! Byte for byte the prefix `UFrameFieldsMetadataCodec::native_prefix` emits in
//! `up-rust`; a differential test holds them together. Identities are stable
//! values; no registry lookup happens at runtime.

use core::mem::{self, MaybeUninit};

use crate::frame::header::UFrameHeaderError;

use super::WireIdentity;

/// The native metadata prefix magic.
pub const NATIVE_PREFIX_MAGIC: &[u8; 4] = b"UPWM";
/// Identity reference tag: registered compact `u16`.
pub const ID_REF_COMPACT: u8 = 0x00;
/// Identity reference tag: literal `u32` length and UTF-8 bytes.
pub const ID_REF_LITERAL: u8 = 0x01;
/// The selected-wire metadata format version.
pub const FORMAT_VERSION: u16 = 1;

/// Native selected-wire identity.
pub const UPROTOCOL_NATIVE_WIRE_ID: WireIdentity =
    WireIdentity::new("org.eclipse.uprotocol.wire.native", 0x0001);
/// Native explicit payload family identity.
pub const NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID: WireIdentity =
    WireIdentity::new("native-explicit", 0x0001);
/// Protobuf selected-wire identity.
pub const PROTOBUF_WIRE_ID: WireIdentity =
    WireIdentity::new("org.eclipse.uprotocol.wire.protobuf", 0x0002);
/// Protobuf payload family identity.
pub const PROTOBUF_PAYLOAD_FAMILY_ID: WireIdentity = WireIdentity::new("protobuf", 0x0002);
/// XCDR version 2 selected-wire identity.
pub const XCDR_V2_WIRE_ID: WireIdentity =
    WireIdentity::new("org.eclipse.uprotocol.wire.xcdr-v2", 0x0003);
/// XCDR version 2 payload family identity.
pub const XCDR_V2_PAYLOAD_FAMILY_ID: WireIdentity = WireIdentity::new("xcdr-v2", 0x0003);
/// Stable-container selected-wire identity.
pub const STABLE_CONTAINER_WIRE_ID: WireIdentity =
    WireIdentity::new("org.eclipse.uprotocol.wire.stable-container", 0x8000);
/// Stable-container payload family identity.
pub const STABLE_CONTAINER_PAYLOAD_FAMILY_ID: WireIdentity =
    WireIdentity::new("stable-container", 0x8001);
/// The field-block metadata layout identity.
pub const UFRAME_FIELDS_METADATA_LAYOUT_ID: WireIdentity =
    WireIdentity::new("org.eclipse.uprotocol.metadata.uframe-fields", 0x0002);

/// Separate identity namespaces used by selected-wire metadata and agreements.
///
/// Payload encoding identifiers are a different namespace and are not listed here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireIdentityNamespace {
    /// Complete selected-wire representation.
    Wire,
    /// Payload representation family.
    PayloadFamily,
    /// Frame metadata representation/layout.
    MetadataLayout,
}

impl WireIdentityNamespace {
    /// Returns the provisional first-wave literal/compact pairs for this namespace.
    ///
    /// Agreement validators use the same list as metadata admission. A private
    /// declaration cannot become registered by claiming a known compact number.
    #[must_use]
    pub const fn registered_identities(self) -> &'static [WireIdentity] {
        match self {
            Self::Wire => &[UPROTOCOL_NATIVE_WIRE_ID, PROTOBUF_WIRE_ID, XCDR_V2_WIRE_ID],
            Self::PayloadFamily => &[
                NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID,
                PROTOBUF_PAYLOAD_FAMILY_ID,
                XCDR_V2_PAYLOAD_FAMILY_ID,
            ],
            Self::MetadataLayout => &[UFRAME_FIELDS_METADATA_LAYOUT_ID],
        }
    }

    /// Whether `identity` is one of this namespace's registered pairs.
    #[must_use]
    pub fn is_registered(self, identity: WireIdentity) -> bool {
        self.registered_identities().contains(&identity)
    }
}

fn identity_ref_len(
    identity: WireIdentity,
    namespace: WireIdentityNamespace,
) -> Result<usize, UFrameHeaderError> {
    if namespace.is_registered(identity) {
        return Ok(1 + mem::size_of::<u16>());
    }
    u32::try_from(identity.literal_id().len())
        .map_err(|_| UFrameHeaderError::ValueTooLong("identity literal"))?;
    identity
        .literal_id()
        .len()
        .checked_add(1 + mem::size_of::<u32>())
        .ok_or(UFrameHeaderError::ValueTooLong("identity reference"))
}

/// The length of the selected-wire prefix for this wire and family.
///
/// # Errors
/// Refuses an identity that cannot be written.
pub fn selected_prefix_len(
    wire_id: WireIdentity,
    payload_family: WireIdentity,
) -> Result<usize, UFrameHeaderError> {
    let parts = [
        (
            UFRAME_FIELDS_METADATA_LAYOUT_ID,
            WireIdentityNamespace::MetadataLayout,
        ),
        (wire_id, WireIdentityNamespace::Wire),
        (payload_family, WireIdentityNamespace::PayloadFamily),
    ];
    let mut len = NATIVE_PREFIX_MAGIC.len() + 2 * mem::size_of::<u16>();
    for (identity, namespace) in parts {
        len = len
            .checked_add(identity_ref_len(identity, namespace)?)
            .ok_or(UFrameHeaderError::ValueTooLong("identity prefix"))?;
    }
    Ok(len)
}

struct UninitWriter<'a> {
    out: &'a mut [MaybeUninit<u8>],
    position: usize,
}

impl UninitWriter<'_> {
    fn put(&mut self, bytes: &[u8]) -> Result<(), UFrameHeaderError> {
        let end = self
            .position
            .checked_add(bytes.len())
            .ok_or(UFrameHeaderError::OutputTooShort)?;
        let target = self
            .out
            .get_mut(self.position..end)
            .ok_or(UFrameHeaderError::OutputTooShort)?;
        for (slot, byte) in target.iter_mut().zip(bytes) {
            slot.write(*byte);
        }
        self.position = end;
        Ok(())
    }

    fn identity(
        &mut self,
        identity: WireIdentity,
        namespace: WireIdentityNamespace,
    ) -> Result<(), UFrameHeaderError> {
        if namespace.is_registered(identity) {
            self.put(&[ID_REF_COMPACT])?;
            self.put(&identity.compact_id().to_le_bytes())
        } else {
            let length = u32::try_from(identity.literal_id().len())
                .map_err(|_| UFrameHeaderError::ValueTooLong("identity literal"))?;
            self.put(&[ID_REF_LITERAL])?;
            self.put(&length.to_le_bytes())?;
            self.put(identity.literal_id().as_bytes())
        }
    }
}

/// Renders the selected-wire prefix into `out`, which must be at least
/// [`selected_prefix_len`] long. Returns the bytes written.
///
/// # Errors
/// Refuses an identity that cannot be written, or storage that is too small.
pub fn render_selected_prefix(
    wire_id: WireIdentity,
    payload_family: WireIdentity,
    out: &mut [MaybeUninit<u8>],
) -> Result<usize, UFrameHeaderError> {
    let mut writer = UninitWriter { out, position: 0 };
    writer.put(NATIVE_PREFIX_MAGIC)?;
    writer.identity(
        UFRAME_FIELDS_METADATA_LAYOUT_ID,
        WireIdentityNamespace::MetadataLayout,
    )?;
    writer.put(&FORMAT_VERSION.to_le_bytes())?;
    writer.identity(wire_id, WireIdentityNamespace::Wire)?;
    writer.identity(payload_family, WireIdentityNamespace::PayloadFamily)?;
    writer.put(&0_u16.to_le_bytes())?;
    Ok(writer.position)
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

    fn u16(&mut self) -> Result<u16, UFrameHeaderError> {
        let mut bytes = [0; 2];
        bytes.copy_from_slice(self.take(2)?);
        Ok(u16::from_le_bytes(bytes))
    }

    fn identity(
        &mut self,
        expected: WireIdentity,
        namespace: WireIdentityNamespace,
    ) -> Result<(), UFrameHeaderError> {
        let tag = self
            .take(1)?
            .first()
            .copied()
            .ok_or(UFrameHeaderError::Truncated)?;
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
                let actual = core::str::from_utf8(self.take(len)?)
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

/// Matches `bytes` against the selected-wire prefix of this wire and family.
/// Returns the prefix length.
///
/// # Errors
/// Reports a prefix that does not match, or malformed bytes.
pub fn exact_prefix_len(
    wire_id: WireIdentity,
    payload_family: WireIdentity,
    bytes: &[u8],
) -> Result<usize, UFrameHeaderError> {
    let mut reader = Reader { bytes, position: 0 };
    if reader.take(4)? != NATIVE_PREFIX_MAGIC {
        return Err(UFrameHeaderError::WrongPrefix);
    }
    reader.identity(
        UFRAME_FIELDS_METADATA_LAYOUT_ID,
        WireIdentityNamespace::MetadataLayout,
    )?;
    if reader.u16()? != FORMAT_VERSION {
        return Err(UFrameHeaderError::WrongPrefix);
    }
    reader.identity(wire_id, WireIdentityNamespace::Wire)?;
    reader.identity(payload_family, WireIdentityNamespace::PayloadFamily)?;
    if reader.u16()? != 0 {
        return Err(UFrameHeaderError::Malformed("native reserved flags"));
    }
    Ok(reader.position)
}
