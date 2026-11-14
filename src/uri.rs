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

/*!
Implementation of the [uProtocol URI (UUri) data model](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/basics/uri.adoc).
A UUri represents a uProtocol resource identifier and is used in various places across the uProtocol specification, e.g., to identify the source and destination of messages or to identify resources that are being accessed via RPC calls.
*/

// [impl->dsn~uri-data-model-naming~2]
// [impl->req~uri-data-model-proto~1]

use thiserror::Error;

#[allow(unused_imports)] // used only by the feature-gated RPC server paths.
pub(crate) use up_rust_core::uri::RESOURCE_ID_RESPONSE;
pub use up_rust_core::uri::{
    ExactUUri, UEntityId, UUri, UUriDisplay, UUriPartsError, UUriRefusal, UUriRefusalDetail,
};

/// An error indicating a problem with creating or parsing a UUri.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum UUriError {
    /// Indicates that a given URI string cannot be parsed into a UUri due to invalid formatting or content.
    #[error("Serialization error: {0}")]
    SerializationError(String),
    /// Indicates that a given URI does not comply with the UUri specification.
    #[error("Validation error: {0}")]
    ValidationError(String),
}

impl UUriError {
    /// Creates a serialization error with the given message.
    pub fn serialization_error<T>(message: T) -> UUriError
    where
        T: Into<String>,
    {
        Self::SerializationError(message.into())
    }

    /// Creates a validation error with the given message.
    pub fn validation_error<T>(message: T) -> UUriError
    where
        T: Into<String>,
    {
        Self::ValidationError(message.into())
    }
}

impl From<UUriPartsError> for UUriError {
    fn from(error: UUriPartsError) -> Self {
        Self::validation_error(error.to_string())
    }
}

impl From<UUriRefusal> for UUriError {
    fn from(refusal: UUriRefusal) -> Self {
        match refusal {
            UUriRefusal::Serialization(detail) => Self::serialization_error(detail.to_string()),
            UUriRefusal::Validation(detail) => Self::validation_error(detail.to_string()),
            _ => Self::serialization_error("invalid URI"),
        }
    }
}

/// The classic [`UUri`] names, with their classic error type.
///
/// The value type and its allocation-free checks live in `up-rust-core`, which
/// has only fixed-error names. These extension methods live here, in
/// `up_rust::prelude`, so `UUri::try_from_parts`, the `verify_*` checks and
/// `to_uri` keep their spelling and their `UUriError`.
///
/// Note that `impl From<&UUri> for String`, `impl TryFrom<String> for UUri` and
/// `impl PartialEq<String> for UUri` no longer exist, because of the orphan rule.
/// Use `uri.to_uri(false)` (not `to_string()`, which adds the `up:` scheme),
/// `UUri::try_from(text.as_str())` and `uri == text.as_str()`.
pub trait UUriExt: Sized {
    /// Creates a new UUri from its parts.
    ///
    /// # Errors
    ///
    /// Returns a [`UUriError::ValidationError`] if the authority or packed entity
    /// instance does not comply with the UUri specification.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use up_rust::{prelude::*, UUri};
    ///
    /// assert!(UUri::try_from_parts("vin", 0x0001_5A6B, 0x01, 0x0001).is_ok());
    /// ```
    // [impl->dsn~uri-authority-name-length~1]
    // [impl->dsn~uri-host-only~2]
    fn try_from_parts(
        authority: &str,
        entity_id: u32,
        entity_version: u8,
        resource_id: u16,
    ) -> Result<Self, UUriError>;

    /// Creates a new UUri for an entity named by its separate type and instance.
    ///
    /// Prefer this over [`Self::try_from_parts`]: a packed entity ID makes it easy to
    /// write a bare 16-bit type ID, which means instance 0 and is always rejected.
    ///
    /// # Errors
    ///
    /// Returns a [`UUriError::ValidationError`] for an invalid authority.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use up_rust::{prelude::*, UEntityId, UUri};
    ///
    /// let uri = UUri::for_entity("vin", UEntityId::first_instance(0x5A6B), 1, 0x0001).unwrap();
    /// assert_eq!(uri.ue_type_id(), 0x5A6B);
    /// assert_eq!(uri.ue_instance_id(), 1);
    /// ```
    fn for_entity(
        authority: &str,
        entity: UEntityId,
        entity_version: u8,
        resource_id: u16,
    ) -> Result<Self, UUriError>;

    /// Verifies that the given authority name complies with the UUri specification.
    ///
    /// # Arguments
    ///
    /// * `authority` - The authority name to verify.
    ///
    /// # Errors
    ///
    /// Returns an error if the authority name is invalid.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use up_rust::{prelude::*, UUri};
    ///
    /// assert!(UUri::verify_authority("my.vin").is_ok());
    /// ```
    // [impl->dsn~uri-authority-name-length~1]
    // [impl->dsn~uri-host-only~2]
    fn verify_authority(authority: &str) -> Result<(), UUriError>;

    /// Serializes this UUri to a URI string.
    ///
    /// # Arguments
    ///
    /// * `include_scheme` - Indicates whether to include the uProtocol scheme (`up`) in the URI.
    ///
    /// # Returns
    ///
    /// The URI as defined by the [uProtocol Specification](https://github.com/eclipse-uprotocol/up-spec).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use up_rust::{prelude::*, UUri};
    ///
    /// let uuri = UUri::try_from_parts("vin.vehicles", 0x0001_800A, 0x02, 0x1a50).unwrap();
    /// let uri_string = uuri.to_uri(true);
    /// assert_eq!(uri_string, "up://vin.vehicles/800A/1/2/1A50");
    /// ````
    // [impl->dsn~uri-authority-mapping~1]
    // [impl->dsn~uri-path-mapping~3]
    // [impl->req~uri-serialization~1]
    #[must_use]
    fn to_uri(&self, include_scheme: bool) -> String;

    /// Verifies that this UUri does not contain any wildcards.
    ///
    /// # Errors
    ///
    /// Returns an error if any of this UUri's properties contain a wildcard value.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use up_rust::{prelude::*, UUri};
    ///
    /// let uri = UUri::try_from_parts("vin.vehicles", 0x0001_2310, 0x03, 0xa000).unwrap();
    /// assert!(uri.verify_no_wildcards().is_ok());
    /// ```
    fn verify_no_wildcards(&self) -> Result<(), UUriError>;

    /// Verifies that this UUri refers to a service method.
    ///
    /// # Errors
    ///
    /// Returns an error if [`UUri::is_rpc_method`] fails or
    /// the UUri [contains any wildcards](Self::verify_no_wildcards).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use up_rust::{prelude::*, UUri};
    ///
    /// let uri = UUri::try_from_parts("", 0x0001_ABCD, 0x01, 0x8000).unwrap();
    /// assert!(uri.verify_rpc_method().is_err());
    ///
    /// let uri = UUri::try_from_parts("", 0x0001_ABCD, 0x01, 0x0000).unwrap();
    /// assert!(uri.verify_rpc_method().is_err());
    /// ```
    fn verify_rpc_method(&self) -> Result<(), UUriError>;

    /// Verifies that this UUri represents an RPC response address.
    ///
    /// # Errors
    ///
    /// Returns an error if [`UUri::is_rpc_response`] fails or
    /// the UUri [contains any wildcards](Self::verify_no_wildcards).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use up_rust::{prelude::*, UUri};
    ///
    /// let uri = UUri::try_from_parts("", 0x0001_ABCD, 0x01, 0x4001).unwrap();
    /// assert!(uri.verify_rpc_response().is_err());
    /// ```
    fn verify_rpc_response(&self) -> Result<(), UUriError>;

    /// Verifies that this UUri can be used as the source of an event or notification.
    ///
    /// # Errors
    ///
    /// Returns an error if [`UUri::is_event`] fails or
    /// the UUri [contains any wildcards](Self::verify_no_wildcards).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use up_rust::{prelude::*, UUri};
    ///
    /// let uri = UUri::try_from_parts("", 0x0001_ABCD, 0x01, 0x7FFF).unwrap();
    /// assert!(uri.verify_event().is_err());
    /// ```
    fn verify_event(&self) -> Result<(), UUriError>;
}

impl UUriExt for UUri {
    fn try_from_parts(
        authority: &str,
        entity_id: u32,
        entity_version: u8,
        resource_id: u16,
    ) -> Result<Self, UUriError> {
        UUri::try_from_wire_parts(authority, entity_id, entity_version, resource_id)
            .map_err(UUriError::from)
    }

    fn for_entity(
        authority: &str,
        entity: UEntityId,
        entity_version: u8,
        resource_id: u16,
    ) -> Result<Self, UUriError> {
        Self::try_from_parts(authority, entity.packed(), entity_version, resource_id)
    }

    fn verify_authority(authority: &str) -> Result<(), UUriError> {
        UUri::check_authority(authority).map_err(UUriError::from)
    }

    fn to_uri(&self, include_scheme: bool) -> String {
        self.display_with_scheme(include_scheme).to_string()
    }

    fn verify_no_wildcards(&self) -> Result<(), UUriError> {
        self.check_no_wildcards().map_err(UUriError::from)
    }

    fn verify_rpc_method(&self) -> Result<(), UUriError> {
        self.check_rpc_method().map_err(UUriError::from)
    }

    fn verify_rpc_response(&self) -> Result<(), UUriError> {
        self.check_rpc_response().map_err(UUriError::from)
    }

    fn verify_event(&self) -> Result<(), UUriError> {
        self.check_event().map_err(UUriError::from)
    }
}

/// The classic constructor of [`ExactUUri`], with its classic error type.
pub trait ExactUUriExt: Sized {
    /// Creates an exact URI proof from a URI with no wildcard components.
    ///
    /// # Errors
    ///
    /// Returns the same validation error as [`UUriExt::verify_no_wildcards`] when
    /// any URI component contains a wildcard.
    fn try_new(uri: UUri) -> Result<Self, UUriError>;
}

impl ExactUUriExt for ExactUUri {
    fn try_new(uri: UUri) -> Result<Self, UUriError> {
        ExactUUri::check(uri).map_err(UUriError::from)
    }
}

/// `ProtobufMappable`-named mapping for [`UUri`].
///
/// `up-rust` cannot implement `ProtobufMappable` for a type defined in another
/// crate alongside its blanket implementation for protobuf-generated types
/// (E0119), so the same method names live on this extension trait, which the
/// prelude provides.
#[cfg(feature = "up-core-api")]
pub trait UUriProtobufExt: Sized {
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
    use crate::up_core_api::uri::UUri as UUriProto;
    use protobuf::{well_known_types::any::Any, Message};

    impl TryFrom<&UUriProto> for UUri {
        type Error = crate::UUriError;

        fn try_from(uuri_proto: &UUriProto) -> Result<Self, Self::Error> {
            let entity_type = u16::try_from(uuri_proto.ue_type_id).map_err(|_e| {
                UUriError::validation_error(
                    "uProtocol URI's entity type ID must be a 16 bit unsigned integer".to_string(),
                )
            })?;
            let entity_instance = u16::try_from(uuri_proto.ue_instance_id).map_err(|_e| {
                UUriError::validation_error(
                    "uProtocol URI's entity instance ID must be a 16 bit unsigned integer"
                        .to_string(),
                )
            })?;
            let version = u8::try_from(uuri_proto.ue_version_major).map_err(|_e| {
                UUriError::validation_error(
                    "uProtocol URI's major version must be an 8 bit unsigned integer".to_string(),
                )
            })?;
            let resource_id = u16::try_from(uuri_proto.resource_id).map_err(|_e| {
                UUriError::validation_error(
                    "uProtocol URI's resource ID must be a 16 bit unsigned integer".to_string(),
                )
            })?;
            UUri::try_from_parts(
                &uuri_proto.authority_name,
                (u32::from(entity_instance) << 16) | u32::from(entity_type),
                version,
                resource_id,
            )
        }
    }

    impl From<&UUri> for UUriProto {
        fn from(uuri: &UUri) -> Self {
            UUriProto {
                authority_name: uuri.authority_name().to_string(),
                ue_type_id: u32::from(uuri.ue_type_id()),
                ue_instance_id: u32::from(uuri.ue_instance_id()),
                ue_version_major: u32::from(uuri.uentity_major_version()),
                resource_id: u32::from(uuri.resource_id()),
                ..Default::default()
            }
        }
    }

    impl super::UUriProtobufExt for UUri {
        fn parse_from_protobuf_bytes(proto: &[u8]) -> Result<Self, crate::SerializationError> {
            let uuri_proto = UUriProto::parse_from_bytes(proto).map_err(|e| {
                crate::SerializationError::new(format!("Protobuf decode error: {e}"))
            })?;
            UUri::try_from(&uuri_proto)
                .map_err(|e| crate::SerializationError::new(format!("UUri conversion error: {e}")))
        }
        fn parse_from_packed_protobuf_bytes(
            proto: &[u8],
        ) -> Result<Self, crate::SerializationError> {
            Any::parse_from_bytes(proto)
                .map_err(|err| crate::SerializationError::new(err.to_string()))
                .and_then(|any| match any.unpack::<UUriProto>() {
                    Ok(Some(uuri_proto)) => UUri::try_from(&uuri_proto)
                        .map_err(|e| crate::SerializationError::new(e.to_string())),
                    Ok(None) => Err(crate::SerializationError::new(
                        "Protobuf Any does not contain UUriProto".to_string(),
                    )),
                    Err(e) => Err(crate::SerializationError::new(format!(
                        "Protobuf Any unpack error: {e}"
                    ))),
                })
        }
        fn write_to_protobuf_bytes(&self) -> Result<Vec<u8>, crate::SerializationError> {
            UUriProto::from(self)
                .write_to_bytes()
                .map_err(|e| crate::SerializationError::new(format!("Protobuf encode error: {e}")))
        }
        fn write_to_packed_protobuf_bytes(&self) -> Result<Vec<u8>, crate::SerializationError> {
            Any::pack(&UUriProto::from(self))
                .map_err(|e| crate::SerializationError::new(format!("Failed to pack UUri: {e}")))
                .and_then(|any| {
                    any.write_to_bytes()
                        .map_err(crate::SerializationError::from)
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use test_case::test_case;

    #[test_case("up://vehicle/10000/1/1/8001"; "type exceeds 16 bits")]
    #[test_case("up://vehicle/1/10000/1/8001"; "instance exceeds 16 bits")]
    #[test_case("up://vehicle/1/0/1/8001"; "zero instance")]
    #[test_case("up://vehicle/1/1/100/8001"; "version exceeds 8 bits")]
    #[test_case("up://vehicle/1/1/1/10000"; "resource exceeds 16 bits")]
    #[test_case("up://vehicle//1/1/8001"; "empty type")]
    #[test_case("up://vehicle/1//1/8001"; "empty instance")]
    #[test_case("up://vehicle/1/1//8001"; "empty version")]
    #[test_case("up://vehicle/1/1/1/"; "empty resource")]
    #[test_case("up://vehicle/1/1/8001"; "legacy three segments")]
    #[test_case("up://vehicle/ABCD1234/1/8001"; "legacy packed instance")]
    #[test_case("up://vehicle/1/1/1/8001/0"; "extra segment")]
    #[test_case("up://vehicle/G/1/1/8001"; "non hexadecimal type")]
    fn split_uri_rejects_malformed_or_legacy_text(text: &str) {
        assert!(UUri::from_str(text).is_err());
    }

    #[test]
    fn wire_parts_reject_zero_instance_without_allocated_diagnostics() {
        assert_eq!(
            UUri::try_from_wire_parts("vehicle", 0x1234, 1, 0x8001),
            Err(UUriPartsError::ZeroEntityInstance)
        );
        assert!(UUri::try_from_parts("vehicle", 0x1234, 1, 0x8001).is_err());
    }

    #[cfg(feature = "up-core-api")]
    #[test_case(0, 1; "zero type with nonzero instance")]
    #[test_case(0x1234, 0xABCD; "separate identifiers")]
    #[test_case(0xFFFF, 1; "type wildcard")]
    #[test_case(1, 0xFFFF; "instance wildcard")]
    #[test_case(0xFFFF, 0xFFFF; "both wildcards")]
    fn protobuf_split_identifiers_round_trip(entity_type: u16, entity_instance: u16) {
        use crate::up_core_api::uri::UUri as UUriProto;

        let packed = (u32::from(entity_instance) << 16) | u32::from(entity_type);
        let uri = UUri::try_from_parts("vehicle", packed, 1, 0x8001).unwrap();
        let proto = UUriProto::from(&uri);
        assert_eq!(proto.ue_type_id, u32::from(entity_type));
        assert_eq!(proto.ue_instance_id, u32::from(entity_instance));
        assert_eq!(UUri::try_from(&proto).unwrap(), uri);
        assert_eq!(
            UUri::parse_from_protobuf_bytes(&uri.write_to_protobuf_bytes().unwrap()).unwrap(),
            uri
        );
        assert_eq!(
            UUri::parse_from_packed_protobuf_bytes(&uri.write_to_packed_protobuf_bytes().unwrap())
                .unwrap(),
            uri
        );
    }

    #[cfg(feature = "up-core-api")]
    #[test_case(0x1_0000, 1; "out of range type")]
    #[test_case(1, 0x1_0000; "out of range instance")]
    #[test_case(1, 0; "zero instance")]
    #[test_case(0x1234_ABCD, 0; "legacy packed type")]
    fn protobuf_rejects_out_of_range_identifiers(entity_type: u32, entity_instance: u32) {
        use crate::up_core_api::uri::UUri as UUriProto;

        let proto = UUriProto {
            ue_type_id: entity_type,
            ue_instance_id: entity_instance,
            ue_version_major: 1,
            resource_id: 0x8001,
            ..Default::default()
        };
        assert!(UUri::try_from(&proto).is_err());
    }

    #[cfg(feature = "up-core-api")]
    #[test_case("", 1, 1, 1, 0xA1FB, "0a0010011801200128fbc302"; "local")]
    #[test_case("my_vin", 1, 0x1000, 2, 0x1A, "0a066d795f76696e10011880202002281a"; "instance")]
    #[test_case("*", 0x101, 1, 0xA0, 0xA1FB, "0a012a108102180120a00128fbc302"; "authority wildcard")]
    #[test_case("vcu.my_vin", 0xFFFF, 0xFFFF, 1, 0xA1FB, "0a0a7663752e6d795f76696e10ffff0318ffff03200128fbc302"; "identifier wildcards")]
    fn protobuf_reads_upstream_split_vectors(
        authority: &str,
        entity_type: u16,
        entity_instance: u16,
        version: u8,
        resource: u16,
        encoded: &str,
    ) {
        let expected = UUri::try_from_parts(
            authority,
            (u32::from(entity_instance) << 16) | u32::from(entity_type),
            version,
            resource,
        )
        .unwrap();
        assert_eq!(
            UUri::parse_from_protobuf_bytes(&hex::decode(encoded).unwrap()).unwrap(),
            expected
        );
    }

    #[test_case(""; "empty authority")]
    #[test_case("vehicle"; "registered name")]
    #[test_case("*"; "wildcard")]
    #[test_case("192.168.1.10"; "IPv4")]
    #[test_case("[2001:db8::8]"; "IPv6")]
    fn wire_parts_preserve_valid_authority(authority: &str) {
        let uri = UUri::try_from_wire_parts(authority, 0x0001_1234, 1, 0x8001).unwrap();
        assert_eq!(uri.authority_name(), authority);
        assert_eq!(
            uri.clone_with_resource_id(0x8002).authority_name(),
            authority
        );
        assert_eq!(UUri::from_str(&uri.to_uri(true)).unwrap(), uri);
    }

    #[test_case("vin/", UUriPartsError::AuthorityHasTrailingComponent; "empty trailing path")]
    #[test_case("vin/x", UUriPartsError::AuthorityHasTrailingComponent; "trailing path")]
    #[test_case("vin?x", UUriPartsError::AuthorityHasTrailingComponent; "trailing query")]
    #[test_case("vin#x", UUriPartsError::AuthorityHasTrailingComponent; "trailing fragment")]
    #[test_case("[v1.xy]", UUriPartsError::UnsupportedHost; "IPvFuture")]
    #[test_case("user@vin", UUriPartsError::AuthorityHasUserinfo; "userinfo")]
    #[test_case("vin:80", UUriPartsError::AuthorityHasPort; "port")]
    #[test_case("INVALID", UUriPartsError::InvalidAuthorityCharacters; "uppercase name")]
    fn wire_parts_report_typed_rejections(authority: &str, expected: UUriPartsError) {
        assert_eq!(
            UUri::try_from_wire_parts(authority, 0x0001_0001, 1, 1),
            Err(expected)
        );
        let Err(UUriError::ValidationError(message)) =
            UUri::try_from_parts(authority, 0x0001_0001, 1, 1)
        else {
            panic!("legacy constructor must reject the same authority");
        };
        assert_eq!(message, expected.to_string());
        assert!(matches!(
            UUri::verify_authority(authority),
            Err(UUriError::ValidationError(_))
        ));
    }

    #[test]
    fn wire_parts_enforce_authority_boundary() {
        let maximum = "a".repeat(128);
        assert_eq!(
            UUri::try_from_wire_parts(&maximum, 0x0001_0001, 1, 1)
                .unwrap()
                .authority_name(),
            maximum
        );
        assert_eq!(
            UUri::try_from_wire_parts(&(maximum + "a"), 0x0001_0001, 1, 1),
            Err(UUriPartsError::AuthorityTooLong)
        );
    }

    #[test]
    fn uri_storage_matches_feature_contract() {
        assert_eq!(std::mem::size_of::<UUri>(), 136);
    }

    #[cfg(feature = "zero-copy-transport")]
    #[test]
    #[cfg(up_rust_test_hooks)]
    fn inline_uri_layout_matches_fixed_abi() {
        use crate::frame::abi::UUriAbi;
        use std::mem::{align_of, offset_of, size_of};

        let (ue_id, resource_id, version, authority_len, authority_bytes) = UUri::__abi_offsets();
        assert_eq!(size_of::<UUri>(), size_of::<UUriAbi>());
        assert_eq!(align_of::<UUri>(), align_of::<UUriAbi>());
        assert_eq!(ue_id, offset_of!(UUriAbi, ue_id));
        assert_eq!(resource_id, offset_of!(UUriAbi, resource_id));
        assert_eq!(version, offset_of!(UUriAbi, ue_version_major));
        assert_eq!(authority_len, offset_of!(UUriAbi, authority_name_len));
        assert_eq!(authority_bytes, offset_of!(UUriAbi, authority_name));
    }

    #[test_case("*", 0x0001_2345, 1, 0x8001; "authority")]
    #[test_case("vehicle", 0xFFFF_2345, 1, 0x8001; "entity instance")]
    #[test_case("vehicle", 0x0001_FFFF, 1, 0x8001; "entity type")]
    #[test_case("vehicle", 0x0001_2345, 0xFF, 0x8001; "version")]
    #[test_case("vehicle", 0x0001_2345, 1, 0xFFFF; "resource")]
    fn exact_uri_rejects_each_wildcard(authority: &str, entity: u32, version: u8, resource: u16) {
        let uri = UUri::try_from_parts(authority, entity, version, resource).unwrap();
        assert!(matches!(
            ExactUUri::try_from(&uri),
            Err(UUriRefusal::Validation(_))
        ));
        assert!(matches!(
            ExactUUri::try_from(uri),
            Err(UUriRefusal::Validation(_))
        ));
    }

    #[test]
    fn exact_uri_preserves_the_checked_value() {
        let uri = UUri::try_from_parts("vehicle", 0x0001_2345, 1, 0x8001).unwrap();
        let borrowed = ExactUUri::try_from(&uri).unwrap();
        let owned = ExactUUri::try_new(uri.clone()).unwrap();
        assert_eq!(borrowed.as_uuri(), &uri);
        assert_eq!(owned.into_inner(), uri);
    }

    #[test_case(UUri::try_from_parts("vin", 0x0001_8000, 0x01, 0x0002).unwrap(),
        "//vin/8000/1/1/2" => true;
        "succeeds for full URI with authority")]
    #[test_case(UUri::try_from_parts("", 0x0001_8000, 0x01, 0x0002).unwrap(),
        "up:/8000/1/1/2" => true;
        "succeeds for URI without authority")]
    #[test_case(UUri::try_from_parts("vin", 0x0001_8000, 0x01, 0x0002).unwrap(),
        "//other-vin/8000/1/1/2" => false;
        "fails for different authority")]
    #[test_case(UUri::try_from_parts("vin", 0x0001_8000, 0x01, 0x0002).unwrap(),
        "up://vin/8000/2/1/2" => false;
        "fails for different entity ID")]
    #[test_case(UUri::try_from_parts("vin", 0x0001_8000, 0x01, 0x0002).unwrap(),
        "//vin/8000/1/2/2" => false;
        "fails for different version")]
    #[test_case(UUri::try_from_parts("vin", 0x0001_8000, 0x01, 0x0002).unwrap(),
        "up://vin/8000/1/1/5" => false;
        "fails for different resource ID")]
    #[allow(clippy::cmp_owned)]
    fn test_eq_with_str(uuri: UUri, uri_str: &str) -> bool {
        uuri == uri_str && uuri == *uri_str && uuri == String::from(uri_str).as_str()
    }

    #[test_case("//*/A100/1/1/1"; "for any authority")]
    #[test_case("//vin/FFFF/1/1/1"; "for any entity type")]
    #[test_case("//vin/ABC/FFFF/1/1"; "for any entity instance")]
    #[test_case("//vin/A100/1/FF/1"; "for any version")]
    #[test_case("//vin/A100/1/1/FFFF"; "for any resource")]
    fn test_verify_no_wildcards_fails(uri: &str) {
        let uuri = UUri::try_from(uri).expect("should have been able to deserialize URI");
        assert!(uuri.verify_no_wildcards().is_err());
    }

    // [utest->dsn~uri-authority-name-length~1]
    #[test]
    fn test_from_str_fails_for_authority_exceeding_max_length() {
        let host_name = "a".repeat(129);
        let uri = format!("//{host_name}/A100/1/1/6501");
        assert!(UUri::from_str(&uri).is_err());
    }

    /// `Display` writes the `up:` scheme and `to_uri(false)` does not. A port that
    /// replaced `String::from(&uri)` with `to_string()` would silently change text.
    #[test]
    fn display_and_uri_without_scheme_are_different_strings() {
        let uri = UUri::try_from_parts("vin.vehicles", 0x0001_800A, 0x02, 0x1a50).unwrap();
        assert_eq!(uri.to_string(), "up://vin.vehicles/800A/1/2/1A50");
        assert_eq!(uri.to_uri(true), "up://vin.vehicles/800A/1/2/1A50");
        assert_eq!(uri.to_uri(false), "//vin.vehicles/800A/1/2/1A50");
        assert_ne!(uri.to_string(), uri.to_uri(false));
    }

    // [utest->dsn~uri-path-mapping~3]
    #[test]
    fn test_from_str_accepts_lowercase_hex_encoding() {
        let result = UUri::try_from("up://vin/abc/ffff/a1/bcd1");
        assert!(result.is_ok_and(|uuri| {
            uuri.authority_name() == "vin"
                && uuri.ue_type_id() == 0x0ABC
                && uuri.ue_instance_id() == 0xFFFF
                && uuri.uentity_major_version() == 0xA1
                && uuri.resource_id() == 0xBCD1
        }));
    }
}

#[cfg(test)]
mod round_trip_properties {
    use std::str::FromStr;

    use proptest::prelude::*;

    use super::{UUri, UUriExt};

    proptest! {
        /// Serialization round-trip: any valid URI built from parts survives
        /// `to_string` -> `from_str` unchanged. Hand-enumerated cases pin the
        /// known edges; this pins the space between them.
        #[test]
        fn uri_survives_string_round_trip(
            authority in "[a-z][a-z0-9]{0,11}",
            ue_id in 0x0001_0001u32..0x0001_FFFF,
            version in 1u8..=0xFE,
            resource in 0u16..0xFFFE,
        ) {
            let uri = UUri::try_from_parts(&authority, ue_id, version, resource)
                .expect("parts chosen from the valid, wildcard-free range");

            let round_tripped = UUri::from_str(&uri.to_uri(false))
                .expect("serialized form of a valid URI must parse");

            prop_assert_eq!(uri, round_tripped);
        }
    }
}
