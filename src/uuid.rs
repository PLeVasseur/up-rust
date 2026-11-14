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

use std::time::{Duration, SystemTime};

use rand::Rng;

use crate::UAttributesError;

pub use up_rust_core::uuid::{UuidRefusal, UUID};

#[derive(Debug)]
pub struct UuidConversionError {
    message: String,
}

impl UuidConversionError {
    pub fn new<T: Into<String>>(message: T) -> UuidConversionError {
        UuidConversionError {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for UuidConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error converting UUID: {}", self.message)
    }
}

impl std::error::Error for UuidConversionError {}

impl From<UuidRefusal> for UuidConversionError {
    fn from(refusal: UuidRefusal) -> Self {
        Self::new(refusal.message())
    }
}

/// The classic [`UUID`] names, with their classic error types.
///
/// The value type lives in `up-rust-core`, which has only fixed-error names and
/// no clock or randomness. These associated functions live here, in
/// `up-rust`'s prelude, so `UUID::build()` and the others keep their spelling
/// and error types.
pub trait UuidExt: Sized {
    /// Creates a new UUID from a byte array.
    ///
    /// # Arguments
    ///
    /// `bytes` - the byte array
    ///
    /// # Returns
    ///
    /// a uProtocol [`UUID`] with the given timestamp and random values.
    ///
    /// # Errors
    ///
    /// Returns an error if the given bytes contain an invalid version and/or variant identifier.
    fn from_bytes(bytes: &[u8; 16]) -> Result<Self, UuidConversionError>;

    /// Creates a new UUID from a high/low value pair.
    ///
    /// # Arguments
    ///
    /// `msb` - the most significant 8 bytes
    /// `lsb` - the least significant 8 bytes
    ///
    /// # Returns
    ///
    /// a uProtocol [`UUID`] with the given timestamp and random values.
    ///
    /// # Errors
    ///
    /// Returns an error if the given bytes contain an invalid version and/or variant identifier.
    // [impl->dsn~uuid-spec~1]
    fn from_u64_pair(msb: u64, lsb: u64) -> Result<Self, UuidConversionError>;

    /// Creates a new UUID that can be used for uProtocol messages.
    ///
    /// # Panics
    ///
    /// if the system clock is set to an instant before the UNIX Epoch.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::SystemTime;
    /// use up_rust::{prelude::*, UUID};
    ///
    /// let uuid = UUID::build();
    /// assert!(uuid.time() <= SystemTime::now()
    ///     .duration_since(SystemTime::UNIX_EPOCH)
    ///     .expect("current system time is set to a point in time before UNIX Epoch")
    ///     .as_millis() as u64);
    /// ```
    // [impl->dsn~uuid-spec~1]
    // [utest->dsn~uuid-spec~1]
    #[must_use]
    fn build() -> Self;

    /// Serializes this UUID to a hyphenated string as defined by
    /// [RFC 4122, Section 3](https://www.rfc-editor.org/rfc/rfc4122.html#section-3)
    /// using lower case characters.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use up_rust::{prelude::*, UUID};
    ///
    /// // timestamp = 1, ver = 0b0111
    /// let msb = 0x0000000000017000_u64;
    /// // variant = 0b10, random = 0x0010101010101a1a
    /// let lsb = 0x8010101010101a1a_u64;
    /// let uuid = UUID::from_u64_pair(msb, lsb).unwrap();
    /// assert_eq!(uuid.to_hyphenated_string(), "00000000-0001-7000-8010-101010101a1a");
    /// ```
    // [impl->req~uuid-hex-and-dash~1]
    #[must_use]
    fn to_hyphenated_string(&self) -> String;

    /// Checks if this UUID should be considered expired.
    ///
    /// # Arguments
    /// * `ttl` - The time-to-live value for the UUID, in milliseconds.
    ///
    /// # Errors
    ///
    /// Returns an error if the time-to-live is greater than 0, but
    /// * the current system time cannot be determined, or
    /// * the timestamp extracted from this UUID plus the time-to-live value
    ///   is less than the current system time.
    fn check_expired(&self, ttl: u32) -> Result<(), UAttributesError>;

    /// Checks if this UUID should be considered expired.
    ///
    /// # Arguments
    /// * `ttl` - The time-to-live value for the UUID, in milliseconds.
    /// * `reference_time` - The reference time to use for the expiration check, in milliseconds since UNIX EPOCH.
    ///
    /// # Errors
    ///
    /// Returns an error if the time-to-live is greater than 0, but
    /// * the current system time cannot be determined, or
    /// * the timestamp extracted from this UUID plus the time-to-live value
    ///   is less than the given reference time.
    fn check_expired_for_reference(
        &self,
        ttl: u32,
        reference_time: u128,
    ) -> Result<(), UAttributesError>;

    /// Creates a UUID from a byte vector, which replaces the orphan-rule-removed
    /// `TryFrom<Vec<u8>>` conversion.
    ///
    /// # Errors
    ///
    /// Returns an error
    /// * if the given vector does not have a length of 16 bytes, or
    /// * if the given bytes contain an invalid version and/or variant identifier.
    fn from_byte_vec(value: Vec<u8>) -> Result<Self, UuidConversionError>;

    /// Serializes this UUID to its 16 bytes in big-endian order, which replaces the
    /// orphan-rule-removed `From<&UUID> for Vec<u8>` conversion.
    #[must_use]
    fn to_byte_vec(&self) -> Vec<u8>;
}

// [impl->dsn~uuid-spec~1]
pub(crate) fn build_for_timestamp(duration_since_unix_epoch: Duration) -> UUID {
    let timestamp_millis = u64::try_from(duration_since_unix_epoch.as_millis())
        .expect("system time is set to a time too far in the future");
    build_for_timestamp_millis(timestamp_millis)
}

// [impl->dsn~uuid-spec~1]
pub(crate) fn build_for_timestamp_millis(timestamp_millis: u64) -> UUID {
    // fill upper 48 bits with timestamp
    let mut msb = (timestamp_millis << 16).to_be_bytes();
    // fill remaining bits with random bits
    rand::rng().fill_bytes(&mut msb[6..]);
    // set version (7)
    msb[6] = msb[6] & 0b00001111 | 0b01110000;

    let mut lsb = [0u8; 8];
    // fill lsb with random bits
    rand::rng().fill_bytes(&mut lsb);
    // set variant (RFC4122)
    lsb[0] = lsb[0] & 0b00111111 | 0b10000000;
    UUID::from_bytes_unchecked(msb, lsb)
}

impl UuidExt for UUID {
    fn from_bytes(bytes: &[u8; 16]) -> Result<Self, UuidConversionError> {
        Ok(UUID::try_from_wire_bytes(bytes)?)
    }

    // [impl->dsn~uuid-spec~1]
    fn from_u64_pair(msb: u64, lsb: u64) -> Result<Self, UuidConversionError> {
        Ok(UUID::try_from_wire_pair(msb, lsb)?)
    }

    // [impl->dsn~uuid-spec~1]
    // [utest->dsn~uuid-spec~1]
    fn build() -> Self {
        let duration_since_unix_epoch = SystemTime::UNIX_EPOCH
            .elapsed()
            .expect("current system time is set to a point in time before UNIX Epoch");
        build_for_timestamp(duration_since_unix_epoch)
    }

    // [impl->req~uuid-hex-and-dash~1]
    fn to_hyphenated_string(&self) -> String {
        self.to_string()
    }

    fn check_expired(&self, ttl: u32) -> Result<(), UAttributesError> {
        // messages with a TTL of 0 are considered to never expire, so we can skip the check in that case
        if ttl == 0 {
            return Ok(());
        }
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| {
                UAttributesError::validation_error(
                    "current system time is set to a point in time before UNIX Epoch",
                )
            })?
            .as_millis();
        self.check_expired_for_reference(ttl, now)
    }

    fn check_expired_for_reference(
        &self,
        ttl: u32,
        reference_time: u128,
    ) -> Result<(), UAttributesError> {
        if self.is_expired_at(ttl, reference_time) {
            return Err(UAttributesError::ExpiredError);
        }
        Ok(())
    }

    fn from_byte_vec(value: Vec<u8>) -> Result<Self, UuidConversionError> {
        let bytes: &[u8; 16] = value
            .as_slice()
            .try_into()
            .map_err(|_| UuidConversionError::new("byte vector length is not 16 bytes"))?;
        Self::from_bytes(bytes)
    }

    fn to_byte_vec(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

/// `ProtobufMappable`-named mapping for [`UUID`].
///
/// `up-rust` cannot implement `ProtobufMappable` for a type defined in another
/// crate alongside its blanket implementation for protobuf-generated types
/// (E0119), so the same method names live on this extension trait, which the
/// prelude provides.
#[cfg(feature = "up-core-api")]
pub trait UuidProtobufExt: Sized {
    /// Parses an instance of this type from the given protobuf bytes.
    ///
    /// # Errors
    /// Returns an error if the given bytes cannot be parsed as a protobuf.
    fn parse_from_protobuf_bytes(proto: &[u8]) -> Result<Self, crate::SerializationError>;
    /// Parses an instance of this type from the given packed protobuf bytes.
    ///
    /// # Errors
    /// Returns an error if the given bytes cannot be parsed to a protobuf _Any_ message
    /// or if the contained message cannot be unpacked to the expected type.
    fn parse_from_packed_protobuf_bytes(proto: &[u8]) -> Result<Self, crate::SerializationError>;
    /// Serializes this instance to protobuf bytes.
    ///
    /// # Errors
    /// Returns an error if this instance cannot be serialized to protobuf bytes.
    fn write_to_protobuf_bytes(&self) -> Result<Vec<u8>, crate::SerializationError>;
    /// Serializes this instance to packed protobuf bytes.
    ///
    /// # Errors
    /// Returns an error if this instance cannot be serialized to packed protobuf bytes.
    fn write_to_packed_protobuf_bytes(&self) -> Result<Vec<u8>, crate::SerializationError>;
}

#[cfg(feature = "up-core-api")]
mod core_types_support {
    use super::*;
    use crate::up_core_api::uuid::UUID as UUIDProto;
    use protobuf::{well_known_types::any::Any, Message};

    impl TryFrom<&UUIDProto> for UUID {
        type Error = UuidConversionError;

        fn try_from(value: &UUIDProto) -> Result<Self, Self::Error> {
            UUID::from_u64_pair(value.msb, value.lsb)
        }
    }

    impl TryFrom<UUIDProto> for UUID {
        type Error = UuidConversionError;

        fn try_from(value: UUIDProto) -> Result<Self, Self::Error> {
            Self::try_from(&value)
        }
    }

    impl From<&UUID> for UUIDProto {
        fn from(value: &UUID) -> Self {
            let (msb, lsb) = value.as_u64_pair();
            UUIDProto {
                msb,
                lsb,
                ..Default::default()
            }
        }
    }

    impl super::UuidProtobufExt for UUID {
        fn parse_from_protobuf_bytes(proto: &[u8]) -> Result<Self, crate::SerializationError> {
            let uuid_proto = UUIDProto::parse_from_bytes(proto)
                .map_err(|err| crate::SerializationError::new(err.to_string()))?;
            UUID::try_from(uuid_proto)
                .map_err(|err| crate::SerializationError::new(err.to_string()))
        }

        fn parse_from_packed_protobuf_bytes(
            proto: &[u8],
        ) -> Result<Self, crate::SerializationError> {
            Any::parse_from_bytes(proto)
                .map_err(|err| crate::SerializationError::new(err.to_string()))
                .and_then(|any| match any.unpack::<UUIDProto>() {
                    Ok(Some(v)) => UUID::try_from(v)
                        .map_err(|err| crate::SerializationError::new(err.to_string())),
                    Ok(None) => Err(crate::SerializationError::new(
                        "cannot unpack UUID, type mismatch",
                    )),
                    Err(e) => Err(crate::SerializationError::from(e)),
                })
        }

        fn write_to_protobuf_bytes(&self) -> Result<Vec<u8>, crate::SerializationError> {
            UUIDProto::from(self)
                .write_to_bytes()
                .map_err(|err| crate::SerializationError::new(err.to_string()))
        }

        fn write_to_packed_protobuf_bytes(&self) -> Result<Vec<u8>, crate::SerializationError> {
            Any::pack(&UUIDProto::from(self))
                .map_err(|err| {
                    crate::SerializationError::new(format!("Failed to pack UUID: {err}"))
                })
                .and_then(|any| {
                    any.write_to_bytes()
                        .map_err(crate::SerializationError::from)
                })
        }
    }
}

#[cfg(test)]
mod wire_value_tests {
    use super::{UuidExt, UUID};

    #[test]
    fn wire_uuid_predicate_checks_every_version_and_variant() {
        for version in 0..16_u64 {
            for variant in 0..4_u64 {
                let msb = (1234 << 16) | (version << 12) | 0x345;
                let lsb = (variant << 62) | 0x1234;
                assert_eq!(
                    UUID::is_valid_u64_pair(msb, lsb),
                    version == 7 && variant == 2
                );
                assert_eq!(
                    UUID::is_valid_u64_pair(msb, lsb),
                    UUID::from_u64_pair(msb, lsb).is_ok()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use super::*;
    use test_case::test_case;

    #[test_case(500, 1_499 => matches Ok(()); "before deadline")]
    #[test_case(500, 1_500 => matches Ok(()); "exactly at deadline")]
    #[test_case(500, 1_501 => matches Err(UAttributesError::ExpiredError); "after deadline")]
    #[test_case(0, u128::MAX => matches Ok(()); "zero is unlimited")]
    fn expiration_at_reference_time(
        ttl: u32,
        reference_time: u128,
    ) -> Result<(), UAttributesError> {
        let id = build_for_timestamp(Duration::from_millis(1_000));
        id.check_expired_for_reference(ttl, reference_time)
    }

    // [utest->dsn~uuid-spec~1]
    // [utest->req~uuid-type~1]
    #[test]
    fn test_from_u64_pair() {
        // timestamp = 1, ver = 0b0111
        let msb = 0x0000000000017000_u64;
        // variant = 0b10
        let lsb = 0x8000000000000000_u64;
        let conversion_attempt = UUID::from_u64_pair(msb, lsb);
        assert!(conversion_attempt
            .is_ok_and(|uuid| { uuid.time() == 0x1_u64 && uuid.as_u64_pair() == (msb, lsb) }));

        // timestamp = 1, (invalid) ver = 0b0000
        let msb = 0x0000000000010000_u64;
        // variant= 0b10
        let lsb = 0x80000000000000ab_u64;
        assert!(UUID::from_u64_pair(msb, lsb).is_err());

        // timestamp = 1, ver = 0b0111
        let msb = 0x0000000000017000_u64;
        // (invalid) variant= 0b00
        let lsb = 0x00000000000000ab_u64;
        assert!(UUID::from_u64_pair(msb, lsb).is_err());
    }

    // [utest->dsn~uuid-spec~1]
    #[test]
    fn test_from_bytes() {
        // timestamp = 1, ver = 0b0111, variant = 0b10
        let bytes: [u8; 16] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x70, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ];
        let conversion_attempt = UUID::from_bytes(&bytes);
        assert!(conversion_attempt.is_ok());
        let uuid = conversion_attempt.unwrap();
        assert_eq!(uuid.time(), 0x1_u64);
    }

    #[test]
    fn test_try_from_vec_fails() {
        // not 16 bytes
        let bytes: Vec<u8> = vec![0x00, 0x00, 0x00, 0x00];
        assert!(UUID::from_byte_vec(bytes).is_err());
    }

    #[test]
    // [utest->req~uuid-hex-and-dash~1]
    fn test_into_string() {
        // timestamp = 1, ver = 0b0111
        let msb = 0x0000000000017000_u64;
        // variant = 0b10, random = 0x0010101010101a1a
        let lsb = 0x8010101010101a1a_u64;
        let uuid = UUID::from_u64_pair(msb, lsb).unwrap();

        assert_eq!(
            uuid.to_hyphenated_string(),
            "00000000-0001-7000-8010-101010101a1a"
        );
        assert_eq!(uuid.to_string(), "00000000-0001-7000-8010-101010101a1a");
    }

    fn build_for_time_offset(offset_millis: i64) -> UUID {
        let duration_since_unix_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("current system time is set to a point in time before UNIX Epoch");
        let now_as_millis_since_epoch: u64 = u64::try_from(duration_since_unix_epoch.as_millis())
            .expect("current system time is too far in the future");
        let creation_timestamp = now_as_millis_since_epoch
            .checked_add_signed(offset_millis)
            .unwrap();
        build_for_timestamp_millis(creation_timestamp)
    }
    #[test_case(build_for_time_offset(-1000), 0 => false; "for past message with TTL 0")]
    #[test_case(build_for_time_offset(-1000), 500 => true; "for past message with expired TTL")]
    #[test_case(build_for_time_offset(-1000), 2000 => false; "for past message with non-expired TTL")]
    #[test_case(build_for_time_offset(1000), 2000 => false; "for future message with TTL")]
    fn test_is_expired(id: UUID, ttl: u32) -> bool {
        if id.check_expired(ttl).is_ok() {
            false
        } else {
            id.check_expired(ttl)
                .is_err_and(|e| matches!(e, UAttributesError::ExpiredError))
        }
    }
}
