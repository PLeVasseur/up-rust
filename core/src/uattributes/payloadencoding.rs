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

//! Compact payload-representation identity, independent of transport and schema.
//!
//! Every 16-bit value is structurally representable. Registry allocation policy
//! and application decoding support are separate from numeric validity. Unknown
//! and reserved values can therefore be preserved by older intermediaries.
//!
//! Absence of an encoding is represented by `None`, never by zero. A present
//! [`PayloadEncoding::CONTRACT_DEFINED`] means a binding or service contract defines the
//! representation. Neither that contract nor a native layout is inferred from
//! payload bytes or a media type.
//!
//! See the [registry](https://github.com/eclipse-uprotocol/up-spec/blob/main/basics/payload_encoding_registry.adoc).

/// A structurally valid 16-bit payload-encoding identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PayloadEncoding(u16);

/// First private-use identifier; allocations require deployment agreement.
pub const PAYLOAD_ENCODING_PRIVATE_USE_MIN: u32 = 0xF000;

/// Last private-use identifier; `0xFFFF` remains reserved for future use.
pub const PAYLOAD_ENCODING_PRIVATE_USE_MAX: u32 = 0xFFFE;

/// First identifier of the registry's reserved range (`0xE000..=0xEFFF`).
///
/// Reserved identifiers are structurally valid and are preserved by every
/// carrying component, but a producer must not originate one until the
/// registry assigns it a meaning. Builders enforce that rule at origination;
/// message validation never does.
pub const PAYLOAD_ENCODING_RESERVED_MIN: u32 = 0xE000;

/// A refused payload-encoding conversion, as a fixed-size value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PayloadEncodingError {
    /// An identifier is outside the 16-bit field.
    IdentifierTooLarge,
    /// A producer tried to originate a reserved identifier.
    ReservedIdentifier(u16),
    /// A `text/plain` media type declares a charset other than UTF-8.
    TextEncodingNotUtf8,
    /// A media type does not identify an unambiguous entry.
    AmbiguousMediaType,
}

impl core::fmt::Display for PayloadEncodingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::IdentifierTooLarge => f.write_str("payload-encoding identifier exceeds 65535"),
            Self::ReservedIdentifier(id) => write!(
                f,
                "payload-encoding identifier {id} is reserved and must not be originated"
            ),
            Self::TextEncodingNotUtf8 => f.write_str("text encoding requires UTF-8"),
            Self::AmbiguousMediaType => {
                f.write_str("media type does not identify an unambiguous payload encoding")
            }
        }
    }
}

impl core::error::Error for PayloadEncodingError {}

impl PayloadEncoding {
    /// An applicable binding/service contract supplies the encoding.
    pub const CONTRACT_DEFINED: Self = Self(0);
    /// Serialized `google.protobuf.Any` (registry entry 1).
    pub const PROTOBUF_WRAPPED_IN_ANY: Self = Self(1);
    /// Unwrapped Protobuf message (registry entry 2).
    pub const PROTOBUF: Self = Self(2);
    /// JSON (registry entry 3).
    pub const JSON: Self = Self(3);
    /// SOME/IP payload serialization (registry entry 4).
    pub const SOMEIP: Self = Self(4);
    /// SOME/IP TLV payload serialization (registry entry 5).
    pub const SOMEIP_TLV: Self = Self(5);
    /// Arbitrary bytes (registry entry 6).
    pub const RAW: Self = Self(6);
    /// UTF-8 text (registry entry 7).
    pub const TEXT: Self = Self(7);

    /// Accepts any identifier within the 16-bit field's structural range.
    ///
    /// This does not authorize assigning an unallocated identifier or prove
    /// that a particular endpoint understands its payload. The classic
    /// `from_id`, with its `UAttributesError`, is an extension method in
    /// `up_rust::prelude`.
    ///
    /// # Errors
    /// Returns [`PayloadEncodingError::IdentifierTooLarge`] when `id` is greater
    /// than `65535`.
    pub const fn try_from_wire_id(id: u32) -> Result<Self, PayloadEncodingError> {
        if id > u16::MAX as u32 {
            return Err(PayloadEncodingError::IdentifierTooLarge);
        }
        Ok(Self(id as u16))
    }

    /// Declares an identifier in the registry-assignment range.
    ///
    /// The caller is responsible for its registry assignment; this checks the
    /// range, not current registration or decoder support. Use [`Self::private_use`]
    /// for a private contract and [`Self::try_from_wire_id`] for structural carriage.
    ///
    /// # Panics
    /// Panics when `id` is outside `0x0001..=0xDFFF`.
    #[must_use]
    pub const fn from_registry_entry(id: u32) -> Self {
        assert!(
            id >= 1 && id <= 0xDFFF,
            "payload-encoding identifier is outside the registry-assignment range"
        );
        Self(id as u16)
    }

    /// Declares a private-use encoding under an explicit contract.
    ///
    /// A fixed library profile can define that contract; interpreting peers must
    /// agree to it. This neither creates a public registry assignment nor changes
    /// structural acceptance of unknown identifiers.
    ///
    /// # Panics
    /// Panics when `id` is outside `0xF000..=0xFFFE`.
    #[must_use]
    pub const fn private_use(id: u32) -> Self {
        assert!(
            id >= PAYLOAD_ENCODING_PRIVATE_USE_MIN && id <= PAYLOAD_ENCODING_PRIVATE_USE_MAX,
            "payload-encoding identifier is outside the private-use range"
        );
        Self(id as u16)
    }

    /// Returns the identifier in the protobuf/native metadata's u32 storage form.
    ///
    /// The value remains limited to 16 bits even where a metadata field uses u32.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.0 as u32
    }

    /// Whether the identifier is in the deployment-private allocation range.
    #[must_use]
    pub const fn is_private_use(self) -> bool {
        self.id() >= PAYLOAD_ENCODING_PRIVATE_USE_MIN
            && self.id() <= PAYLOAD_ENCODING_PRIVATE_USE_MAX
    }

    /// Whether the identifier is in the registry's reserved range.
    ///
    /// Reserved identifiers may be carried and preserved but not originated.
    #[must_use]
    pub const fn is_reserved(self) -> bool {
        (self.id() >= PAYLOAD_ENCODING_RESERVED_MIN && self.id() < PAYLOAD_ENCODING_PRIVATE_USE_MIN)
            || self.id() == 0xFFFF
    }

    /// Checks that a producer may put this identifier on the wire.
    ///
    /// This is the one origination rule a library can enforce without registry
    /// knowledge: the reserved range is never originable. Unassigned
    /// registry-range values cannot be detected here and remain the producer's
    /// responsibility. The classic `check_originable` is an extension method.
    ///
    /// # Errors
    /// Returns [`PayloadEncodingError::ReservedIdentifier`] for an identifier in
    /// `0xE000..=0xEFFF` or `0xFFFF`.
    pub const fn check_wire_originable(self) -> Result<(), PayloadEncodingError> {
        if self.is_reserved() {
            return Err(PayloadEncodingError::ReservedIdentifier(self.0));
        }
        Ok(())
    }

    /// Registered media metadata, where one accurately describes the encoding.
    ///
    /// Both protobuf identities have the same media type. This metadata is not
    /// a replacement for the encoding ID or permission to interpret the bytes.
    #[must_use]
    pub const fn media_type(self) -> Option<&'static str> {
        match self.0 {
            1 | 2 => Some("application/protobuf"),
            3 => Some("application/json"),
            6 => Some("application/octet-stream"),
            7 => Some("text/plain; charset=utf-8"),
            _ => None,
        }
    }

    /// Resolves media metadata only where it identifies an unambiguous entry.
    ///
    /// Protobuf media types cannot distinguish a serialized Any from an unwrapped
    /// message and are rejected. Unregistered or unknown media types and a
    /// conflicting text charset are also rejected. The classic `from_media_type`
    /// extension method adds the rejected media type to its error message.
    ///
    /// # Errors
    /// See above. Transport bindings must preserve the explicit encoding
    /// identifier rather than use this lookup.
    pub fn from_media_type_str(media_type: &str) -> Result<Self, PayloadEncodingError> {
        // One-element arrays, not `char`: the `char` searcher keeps a bounds check on
        // Rust 1.88, which the panic-link gate correctly refuses. The match is the same.
        let mut parts = media_type.split([';']);
        let base = parts.next().unwrap_or("").trim();
        if base.eq_ignore_ascii_case("application/json") {
            return Ok(Self::JSON);
        }
        if base.eq_ignore_ascii_case("application/octet-stream") {
            return Ok(Self::RAW);
        }
        if base.eq_ignore_ascii_case("text/plain") {
            for parameter in parts {
                if let Some((name, value)) = parameter.split_once(['=']) {
                    if name.trim().eq_ignore_ascii_case("charset")
                        && !value
                            .trim()
                            .trim_matches(['"'])
                            .eq_ignore_ascii_case("utf-8")
                    {
                        return Err(PayloadEncodingError::TextEncodingNotUtf8);
                    }
                }
            }
            return Ok(Self::TEXT);
        }
        Err(PayloadEncodingError::AmbiguousMediaType)
    }
}

impl core::fmt::Display for PayloadEncoding {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.media_type() {
            Some(media) => write!(f, "{} ({media})", self.0),
            None if *self == Self::CONTRACT_DEFINED => write!(f, "0 (contract-defined)"),
            None if self.is_private_use() => write!(f, "{} (private use)", self.0),
            None => write!(f, "{}", self.0),
        }
    }
}

impl TryFrom<u32> for PayloadEncoding {
    type Error = PayloadEncodingError;

    fn try_from(id: u32) -> Result<Self, Self::Error> {
        Self::try_from_wire_id(id)
    }
}

impl From<PayloadEncoding> for u32 {
    fn from(value: PayloadEncoding) -> Self {
        value.id()
    }
}
