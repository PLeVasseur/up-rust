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

use crate::uattributes::UAttributesError;

pub use up_rust_core::uattributes::{
    PayloadEncoding, PayloadEncodingError, PAYLOAD_ENCODING_PRIVATE_USE_MAX,
    PAYLOAD_ENCODING_PRIVATE_USE_MIN, PAYLOAD_ENCODING_RESERVED_MIN,
};

impl From<PayloadEncodingError> for UAttributesError {
    fn from(error: PayloadEncodingError) -> Self {
        UAttributesError::validation_error(error.to_string())
    }
}

/// The classic [`PayloadEncoding`] names, with their classic error type.
pub trait PayloadEncodingExt: Sized {
    /// Accepts any identifier within the 16-bit field's structural range.
    ///
    /// This does not authorize assigning an unallocated identifier or prove
    /// that a particular endpoint understands its payload.
    ///
    /// # Errors
    /// Returns an error when `id` is greater than `65535`.
    fn from_id(id: u32) -> Result<Self, UAttributesError>;

    /// Checks that a producer may put this identifier on the wire.
    ///
    /// This is the one origination rule a library can enforce without registry
    /// knowledge: the reserved range is never originable. Unassigned
    /// registry-range values cannot be detected here and remain the producer's
    /// responsibility.
    ///
    /// # Errors
    ///
    /// Returns an error for an identifier in `0xE000..=0xEFFF` or `0xFFFF`.
    fn check_originable(self) -> Result<(), UAttributesError>;

    /// Resolves media metadata only where it identifies an unambiguous entry.
    ///
    /// # Errors
    /// Protobuf media types cannot distinguish a serialized Any from an
    /// unwrapped message and are rejected. Unregistered/unknown media types and
    /// a conflicting text charset are also rejected. Transport bindings must
    /// preserve the explicit encoding identifier rather than use this lookup.
    fn from_media_type(media_type: &str) -> Result<Self, UAttributesError>;
}

impl PayloadEncodingExt for PayloadEncoding {
    fn from_id(id: u32) -> Result<Self, UAttributesError> {
        PayloadEncoding::try_from_wire_id(id).map_err(UAttributesError::from)
    }

    fn check_originable(self) -> Result<(), UAttributesError> {
        self.check_wire_originable().map_err(UAttributesError::from)
    }

    fn from_media_type(media_type: &str) -> Result<Self, UAttributesError> {
        match PayloadEncoding::from_media_type_str(media_type) {
            Err(PayloadEncodingError::AmbiguousMediaType) => {
                Err(UAttributesError::validation_error(format!(
                    "media type [{media_type}] does not identify an unambiguous payload encoding"
                )))
            }
            other => other.map_err(UAttributesError::from),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(0; "contract defined zero")]
    #[test_case(1; "first registered")]
    #[test_case(7; "last currently registered")]
    #[test_case(8; "retired label remains unassigned")]
    #[test_case(9; "unassigned")]
    #[test_case(0xDFFF; "assignment range upper bound")]
    #[test_case(0xE000; "reserved lower bound")]
    #[test_case(0xEFFF; "reserved upper bound")]
    #[test_case(0xF000; "private lower bound")]
    #[test_case(0xFFFE; "private upper bound")]
    #[test_case(0xFFFF; "future reserved value remains structurally valid")]
    fn identifier_is_structurally_valid(id: u32) {
        assert_eq!(PayloadEncoding::from_id(id).unwrap().id(), id);
    }

    #[test_case(1; "first registry value")]
    #[test_case(0xDFFF; "last registry value")]
    fn registry_constructor_checks_the_assignment_range(id: u32) {
        assert_eq!(PayloadEncoding::from_registry_entry(id).id(), id);
    }

    #[test_case(0; "contract-defined is not registry-assigned")]
    #[test_case(0xE000; "reserved is not registry-assigned")]
    #[test_case(0xF001; "private is not registry-assigned")]
    #[test_case(65536; "oversized is not registry-assigned")]
    #[should_panic(expected = "outside the registry-assignment range")]
    fn registry_constructor_rejects_other_allocation_classes(id: u32) {
        let _ = PayloadEncoding::from_registry_entry(id);
    }

    #[test_case(0xF000; "first private value")]
    #[test_case(0xFFFE; "last private value")]
    fn private_constructor_accepts_contract_identifiers(id: u32) {
        assert_eq!(PayloadEncoding::private_use(id).id(), id);
    }

    #[test_case(0; "contract-defined is not private")]
    #[test_case(2; "registered is not private")]
    #[test_case(0xEFFF; "reserved is not private")]
    #[test_case(0xFFFF; "reserved future value is not private")]
    #[test_case(65536; "oversized is not private")]
    #[should_panic(expected = "outside the private-use range")]
    fn private_constructor_rejects_other_allocation_classes(id: u32) {
        let _ = PayloadEncoding::private_use(id);
    }

    #[test_case(65536; "just above the field range")]
    #[test_case(u32::MAX; "maximum protobuf integer")]
    fn identifier_outside_field_range_is_rejected(id: u32) {
        assert!(matches!(
            PayloadEncoding::from_id(id),
            Err(UAttributesError::ValidationError(_))
        ));
    }

    #[test]
    fn explicit_contract_encoding_is_not_absence() {
        assert_ne!(Some(PayloadEncoding::CONTRACT_DEFINED), None);
        assert_eq!(PayloadEncoding::CONTRACT_DEFINED.id(), 0);
        assert_eq!(
            PayloadEncoding::CONTRACT_DEFINED.to_string(),
            "0 (contract-defined)"
        );
    }

    #[test_case(0xEFFF => false; "reserved is not private")]
    #[test_case(0xF000 => true; "private lower bound")]
    #[test_case(0xFFFE => true; "private upper bound")]
    #[test_case(0xFFFF => false; "future reserved value")]
    fn private_range_classification(id: u32) -> bool {
        PayloadEncoding::from_id(id).unwrap().is_private_use()
    }

    #[test_case(0xDFFF => false; "last registry value is not reserved")]
    #[test_case(0xE000 => true; "reserved lower bound")]
    #[test_case(0xEFFF => true; "reserved upper bound")]
    #[test_case(0xF000 => false; "private is not reserved")]
    #[test_case(0xFFFF => true; "future reserved value")]
    fn reserved_range_classification(id: u32) -> bool {
        PayloadEncoding::from_id(id).unwrap().is_reserved()
    }

    #[test_case(0; "contract-defined is originable")]
    #[test_case(7; "registered is originable")]
    #[test_case(8; "unassigned cannot be detected without the registry")]
    #[test_case(0xF001; "private is originable under agreement")]
    fn originable_identifiers_pass_the_origination_check(id: u32) {
        assert!(PayloadEncoding::from_id(id)
            .unwrap()
            .check_originable()
            .is_ok());
    }

    #[test_case(0xE000; "reserved lower bound")]
    #[test_case(0xEFFF; "reserved upper bound")]
    #[test_case(0xFFFF; "future reserved value")]
    fn reserved_identifiers_fail_the_origination_check(id: u32) {
        assert!(matches!(
            PayloadEncoding::from_id(id).unwrap().check_originable(),
            Err(UAttributesError::ValidationError(_))
        ));
    }

    #[test_case(PayloadEncoding::from_registry_entry(8); "unassigned encoding")]
    #[test_case(PayloadEncoding::SOMEIP; "encoding without registered media type")]
    fn missing_media_metadata_does_not_make_an_identifier_invalid(encoding: PayloadEncoding) {
        assert_eq!(PayloadEncoding::from_id(encoding.id()).unwrap(), encoding);
        assert_eq!(encoding.media_type(), None);
    }

    #[test]
    fn media_type_cannot_recover_protobuf_wrapper_identity() {
        assert_eq!(
            PayloadEncoding::PROTOBUF.media_type(),
            PayloadEncoding::PROTOBUF_WRAPPED_IN_ANY.media_type()
        );
    }

    #[test_case("application/protobuf"; "ambiguous protobuf representation")]
    #[test_case("application/x-protobuf"; "retired protobuf alias")]
    #[test_case("text/plain; charset=iso-8859-1"; "non UTF8 charset")]
    fn ambiguous_or_incompatible_media_type_is_rejected(media_type: &str) {
        assert!(matches!(
            PayloadEncoding::from_media_type(media_type),
            Err(UAttributesError::ValidationError(_))
        ));
    }

    #[test_case("text/plain; charset=UTF-8", PayloadEncoding::TEXT; "explicit UTF8 text")]
    #[test_case("application/json", PayloadEncoding::JSON; "JSON")]
    fn unambiguous_media_type_resolves(media_type: &str, expected: PayloadEncoding) {
        assert_eq!(
            PayloadEncoding::from_media_type(media_type).unwrap(),
            expected
        );
    }
}
