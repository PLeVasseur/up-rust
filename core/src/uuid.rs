/********************************************************************************
 * Copyright (c) 2023 Contributors to the Eclipse Foundation
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

//! The uProtocol UUIDv7 message identifier, as a plain value.
//!
//! Generating an identifier needs a clock and randomness, which belong to the
//! `std` layer. This crate has only fixed-error names (`try_from_wire_pair`,
//! `try_from_wire_bytes`, [`UuidRefusal`]). `UUID::build()`, `UUID::from_u64_pair`
//! and the other classic names, with their classic error types, are extension
//! traits in `up_rust::prelude`.

use core::fmt;
use core::hash::{Hash, Hasher};
use core::str::FromStr;

const BITMASK_VERSION: u64 = 0b1111 << 12;
const VERSION_7: u64 = 0b0111 << 12;
const BITMASK_VARIANT: u64 = 0b11 << 62;
const VARIANT_RFC4122: u64 = 0b10 << 62;

const fn is_correct_version(msb: u64) -> bool {
    msb & BITMASK_VERSION == VERSION_7
}

const fn is_correct_variant(lsb: u64) -> bool {
    lsb & BITMASK_VARIANT == VARIANT_RFC4122
}

/// Why a byte or string value is not a valid uProtocol UUID.
///
/// `up-rust` keeps the classic `UuidConversionError`, with an owned string, and
/// converts from this type. Formatting produces the same text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UuidRefusal {
    message: &'static str,
}

impl UuidRefusal {
    /// Creates a refusal with the given fixed explanation.
    #[must_use]
    pub const fn new(message: &'static str) -> Self {
        Self { message }
    }

    /// The fixed explanation of this refusal.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
}

impl fmt::Display for UuidRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error converting UUID: {}", self.message)
    }
}

impl core::error::Error for UuidRefusal {}

/// A uProtocol UUIDv7 message identifier (RFC 9562 layout).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct UUID {
    msb: u64,
    lsb: u64,
}

impl fmt::Display for UUID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.hyphenated_ascii();
        match core::str::from_utf8(&bytes) {
            Ok(text) => f.write_str(text),
            Err(_) => Err(fmt::Error),
        }
    }
}

impl Eq for UUID {}

impl Hash for UUID {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let bytes = (self.msb, self.lsb);
        bytes.hash(state)
    }
}

impl UUID {
    /// Checks the UUIDv7 version and RFC variant bits without creating an error string.
    ///
    /// This applies the same bit constraints as [`UUID::try_from_wire_pair`]. It does
    /// not impose a timestamp freshness or origin policy.
    #[must_use]
    pub const fn is_valid_u64_pair(msb: u64, lsb: u64) -> bool {
        is_correct_version(msb) && is_correct_variant(lsb)
    }

    /// Creates a new UUID from a byte array.
    ///
    /// # Errors
    ///
    /// Returns a refusal if the given bytes contain an invalid version and/or
    /// variant identifier.
    pub const fn try_from_wire_bytes(bytes: &[u8; 16]) -> Result<Self, UuidRefusal> {
        let bytes = u128::from_be_bytes(*bytes);
        Self::try_from_wire_pair((bytes >> 64) as u64, bytes as u64)
    }

    /// Creates a new UUID from a high/low value pair without checking its bits.
    ///
    /// Only for callers that have already validated the bytes.
    #[doc(hidden)]
    #[must_use]
    pub const fn from_bytes_unchecked(msb: [u8; 8], lsb: [u8; 8]) -> Self {
        UUID {
            msb: u64::from_be_bytes(msb),
            lsb: u64::from_be_bytes(lsb),
        }
    }

    /// Creates a new UUID from a high/low value pair.
    ///
    /// # Errors
    ///
    /// Returns a refusal if the given bytes contain an invalid version and/or
    /// variant identifier.
    // [impl->dsn~uuid-spec~1]
    pub const fn try_from_wire_pair(msb: u64, lsb: u64) -> Result<Self, UuidRefusal> {
        if !is_correct_version(msb) {
            return Err(UuidRefusal::new("not a v7 UUID"));
        }
        if !is_correct_variant(lsb) {
            return Err(UuidRefusal::new("not an RFC4122 UUID"));
        }
        Ok(UUID { msb, lsb })
    }

    /// Gets this UUID as a pair of 64 bit integers.
    ///
    /// # Returns
    ///
    /// A tuple of the most significant and least significant 8 bytes,
    /// i.e. the inverse of [`UUID::try_from_wire_pair`].
    #[must_use]
    pub const fn as_u64_pair(&self) -> (u64, u64) {
        (self.msb, self.lsb)
    }

    /// Returns this UUID's 16 bytes in big-endian order.
    #[must_use]
    pub const fn to_be_bytes(&self) -> [u8; 16] {
        (((self.msb as u128) << 64) | self.lsb as u128).to_be_bytes()
    }

    /// Returns the point in time that this UUID has been created at.
    ///
    /// # Returns
    ///
    /// The number of milliseconds since UNIX EPOCH.
    // [impl->dsn~uuid-spec~1]
    // [utest->dsn~uuid-spec~1]
    #[must_use]
    pub const fn time(&self) -> u64 {
        // the timestamp is contained in the 48 most significant bits
        self.msb >> 16
    }

    /// Whether this UUID has expired at `reference_ms`, for a time-to-live of
    /// `ttl_ms` milliseconds. A TTL of zero never expires.
    #[must_use]
    pub const fn is_expired_at(&self, ttl_ms: u32, reference_ms: u128) -> bool {
        if ttl_ms == 0 {
            return false;
        }
        (self.time() as u128).saturating_add(ttl_ms as u128) < reference_ms
    }

    /// Serializes this UUID to 36 lower-case hyphenated ASCII bytes as defined by
    /// [RFC 4122, Section 3](https://www.rfc-editor.org/rfc/rfc4122.html#section-3).
    #[must_use]
    pub fn hyphenated_ascii(&self) -> [u8; 36] {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let bytes = self.to_be_bytes();
        let mut out = [b'-'; 36];
        let mut position = 0_usize;
        for (index, byte) in bytes.iter().enumerate() {
            if matches!(index, 4 | 6 | 8 | 10) {
                position = position.wrapping_add(1);
            }
            let high = HEX.get(usize::from(byte >> 4)).copied().unwrap_or(b'0');
            let low = HEX.get(usize::from(byte & 0x0F)).copied().unwrap_or(b'0');
            if let Some(slot) = out.get_mut(position) {
                *slot = high;
            }
            if let Some(slot) = out.get_mut(position.wrapping_add(1)) {
                *slot = low;
            }
            position = position.wrapping_add(2);
        }
        out
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Parses the 36-byte hyphenated form, accepting upper and lower case digits.
fn parse_hyphenated(src: &[u8]) -> Option<[u8; 16]> {
    if src.len() != 36
        || src.get(8) != Some(&b'-')
        || src.get(13) != Some(&b'-')
        || src.get(18) != Some(&b'-')
        || src.get(23) != Some(&b'-')
    {
        return None;
    }
    const GROUP_START: [usize; 8] = [0, 4, 9, 14, 19, 24, 28, 32];
    let mut out = [0_u8; 16];
    let mut index = 0_usize;
    for start in GROUP_START {
        for offset in [0_usize, 2] {
            let hi = hex_digit(*src.get(start.wrapping_add(offset))?)?;
            let lo = hex_digit(*src.get(start.wrapping_add(offset).wrapping_add(1))?)?;
            *out.get_mut(index)? = (hi << 4) | lo;
            index = index.wrapping_add(1);
        }
    }
    Some(out)
}

impl FromStr for UUID {
    type Err = UuidRefusal;

    /// Parses a string into a UUID.
    ///
    /// # Errors
    ///
    /// Returns an error
    /// * if the given string does not represent a UUID as defined by
    ///   [RFC 4122, Section 3](https://www.rfc-editor.org/rfc/rfc4122.html#section-3), or
    /// * if the bytes encoded in the string contain an invalid version and/or variant
    ///   identifier.
    // [impl->req~uuid-hex-and-dash~1]
    fn from_str(uuid_str: &str) -> Result<Self, Self::Err> {
        match parse_hyphenated(uuid_str.as_bytes()) {
            Some(bytes) => UUID::try_from_wire_bytes(&bytes),
            None => Err(UuidRefusal::new("UUIDError")),
        }
    }
}
