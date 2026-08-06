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

//! The unified payload-encoding identity.
//!
//! A payload's encoding is described by exactly one mechanism: an entry
//! identifier in the uProtocol payload-encoding registry
//! ([spec][spec-registry]). This module provides the [`PayloadEncoding`]
//! newtype over that identifier, constants for the permanently assigned
//! entries `1..=8` (the formats historically expressed by the retired
//! `UPayloadFormat` enum, with unchanged numeric values), and the
//! registry metadata for those entries.
//!
//! A message with no payload carries no encoding: absence is expressed as
//! `Option<PayloadEncoding>` everywhere in this crate, never as a zero or
//! "unspecified" value. Identifier `0` is reserved and rejected.
//!
//! [spec-registry]: https://github.com/eclipse-uprotocol/up-spec/blob/main/basics/uattributes.adoc#payload-encoding-registry

use crate::uattributes::UAttributesError;

/// A payload-encoding registry entry identifier.
///
/// See the module documentation for the registry model. The type
/// is a transparent identifier: two values are the same encoding exactly
/// when their identifiers are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PayloadEncoding(u32);

/// First identifier of the private-use range.
///
/// Identifiers at or above this value are valid on the wire, are never
/// registered, and carry meaning only by deployment-level agreement.
pub const PAYLOAD_ENCODING_PRIVATE_USE_MIN: u32 = 0x1000_0000;

macro_rules! well_known {
    ($(#[$doc:meta] $name:ident = $id:expr, $media:expr;)+) => {
        impl PayloadEncoding {
            $(
                #[$doc]
                pub const $name: PayloadEncoding = PayloadEncoding($id);
            )+

            /// The registered media type for this encoding, if it is one of
            /// the permanently assigned entries `1..=8`.
            pub fn media_type(self) -> Option<&'static str> {
                match self.0 {
                    $($id => Some($media),)+
                    _ => None,
                }
            }
        }
    };
}

well_known! {
    /// Protobuf, wrapped in `google.protobuf.Any` (registry entry 1).
    PROTOBUF_WRAPPED_IN_ANY = 1, "application/x-protobuf";
    /// Protobuf (registry entry 2).
    PROTOBUF = 2, "application/protobuf";
    /// JSON (registry entry 3).
    JSON = 3, "application/json";
    /// SOME/IP (registry entry 4).
    SOMEIP = 4, "application/x-someip";
    /// SOME/IP TLV (registry entry 5).
    SOMEIP_TLV = 5, "application/x-someip_tlv";
    /// Raw bytes (registry entry 6).
    RAW = 6, "application/octet-stream";
    /// UTF-8 text (registry entry 7).
    TEXT = 7, "text/plain";
    /// Shared-memory reference (registry entry 8).
    SHM = 8, "application/x-shm";
}

impl PayloadEncoding {
    /// Creates an encoding identity from a registry entry identifier.
    ///
    /// # Errors
    ///
    /// Returns [`UAttributesError::ValidationError`] if `id` is `0`, which
    /// the registry reserves: a message without a payload carries no
    /// encoding at all rather than a zero identifier.
    pub fn from_id(id: u32) -> Result<Self, UAttributesError> {
        if id == 0 {
            return Err(UAttributesError::validation_error(
                "payload-encoding identifier 0 is reserved",
            ));
        }
        Ok(PayloadEncoding(id))
    }

    /// Creates an encoding identity from a registry entry identifier known at
    /// compile time, for declaring constants (e.g. a wire crate declaring its
    /// registered encoding).
    ///
    /// # Panics
    ///
    /// At compile time (in const contexts) if `id` is `0`, which the registry
    /// reserves.
    #[must_use]
    pub const fn from_registry_entry(id: u32) -> Self {
        assert!(id != 0, "payload-encoding identifier 0 is reserved");
        PayloadEncoding(id)
    }

    /// The registry entry identifier.
    pub const fn id(self) -> u32 {
        self.0
    }

    /// Whether this identifier lies in the private-use range.
    ///
    /// Private-use encodings are valid on the wire but never registered;
    /// their meaning is fixed by deployment-level agreement only.
    pub fn is_private_use(self) -> bool {
        self.0 >= PAYLOAD_ENCODING_PRIVATE_USE_MIN
    }

    /// Looks up the permanently assigned entry for a media type, covering
    /// registry entries `1..=8`.
    ///
    /// # Errors
    ///
    /// Returns [`UAttributesError::ValidationError`] if the media type is
    /// not one of the eight permanently assigned entries' types; encodings
    /// beyond those are identified by registry id, not media type.
    pub fn from_media_type(media_type: &str) -> Result<Self, UAttributesError> {
        let base = media_type.split(';').next().unwrap_or("").trim();
        match base {
            "application/x-protobuf" => Ok(Self::PROTOBUF_WRAPPED_IN_ANY),
            "application/protobuf" => Ok(Self::PROTOBUF),
            "application/json" => Ok(Self::JSON),
            "application/x-someip" => Ok(Self::SOMEIP),
            "application/x-someip_tlv" => Ok(Self::SOMEIP_TLV),
            "application/octet-stream" => Ok(Self::RAW),
            "text/plain" => Ok(Self::TEXT),
            "application/x-shm" => Ok(Self::SHM),
            other => Err(UAttributesError::validation_error(format!(
                "no permanently assigned payload encoding for media type [{other}]"
            ))),
        }
    }
}

impl core::fmt::Display for PayloadEncoding {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.media_type() {
            Some(m) => write!(f, "{} ({m})", self.0),
            None if self.is_private_use() => write!(f, "{} (private use)", self.0),
            None => write!(f, "{}", self.0),
        }
    }
}

impl TryFrom<u32> for PayloadEncoding {
    type Error = UAttributesError;

    fn try_from(id: u32) -> Result<Self, Self::Error> {
        Self::from_id(id)
    }
}

impl From<PayloadEncoding> for u32 {
    fn from(encoding: PayloadEncoding) -> u32 {
        encoding.id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_ids_match_the_registry() {
        for (e, id, media) in [
            (
                PayloadEncoding::PROTOBUF_WRAPPED_IN_ANY,
                1,
                "application/x-protobuf",
            ),
            (PayloadEncoding::PROTOBUF, 2, "application/protobuf"),
            (PayloadEncoding::JSON, 3, "application/json"),
            (PayloadEncoding::SOMEIP, 4, "application/x-someip"),
            (PayloadEncoding::SOMEIP_TLV, 5, "application/x-someip_tlv"),
            (PayloadEncoding::RAW, 6, "application/octet-stream"),
            (PayloadEncoding::TEXT, 7, "text/plain"),
            (PayloadEncoding::SHM, 8, "application/x-shm"),
        ] {
            assert_eq!(e.id(), id);
            assert_eq!(e.media_type(), Some(media));
            assert_eq!(PayloadEncoding::from_media_type(media).unwrap(), e);
            assert!(!e.is_private_use());
        }
    }

    #[test]
    fn zero_is_reserved() {
        assert!(PayloadEncoding::from_id(0).is_err());
        assert!(PayloadEncoding::try_from(0u32).is_err());
    }

    #[test]
    fn private_use_boundary() {
        assert!(
            !PayloadEncoding::from_id(PAYLOAD_ENCODING_PRIVATE_USE_MIN - 1)
                .unwrap()
                .is_private_use()
        );
        assert!(PayloadEncoding::from_id(PAYLOAD_ENCODING_PRIVATE_USE_MIN)
            .unwrap()
            .is_private_use());
        assert_eq!(
            PayloadEncoding::from_id(PAYLOAD_ENCODING_PRIVATE_USE_MIN)
                .unwrap()
                .media_type(),
            None
        );
    }

    #[test]
    fn well_known_media_types_round_trip() {
        for encoding in [
            PayloadEncoding::PROTOBUF_WRAPPED_IN_ANY,
            PayloadEncoding::PROTOBUF,
            PayloadEncoding::JSON,
            PayloadEncoding::SOMEIP,
            PayloadEncoding::SOMEIP_TLV,
            PayloadEncoding::RAW,
            PayloadEncoding::TEXT,
            PayloadEncoding::SHM,
        ] {
            let media_type = encoding.media_type().expect("well-known media type");
            assert_eq!(
                PayloadEncoding::from_media_type(media_type).expect("resolvable"),
                encoding
            );
            let shown = encoding.to_string();
            assert!(shown.starts_with(&encoding.id().to_string()));
            assert!(shown.contains(media_type), "display names the media type");
        }
        assert!(PayloadEncoding::from_id(0x1000_0BEE)
            .expect("private-use id")
            .media_type()
            .is_none());
    }

    #[test]
    fn media_type_parameters_are_ignored() {
        assert_eq!(
            PayloadEncoding::from_media_type("text/plain; charset=utf-8").unwrap(),
            PayloadEncoding::TEXT
        );
    }
}
