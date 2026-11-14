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

//! The value types and fixed-size errors, with no heap on any path.
#![allow(missing_docs)]

mod support;

use std::str::FromStr;

use support::{assert_no_heap, CountingAllocator};
use up_rust_core::frame::header::{
    header_len, parse_header, write_header, HeaderLimits, TxHeaderParts, UFrameHeaderError,
};
use up_rust_core::frame::native::NativeTypeToken;
use up_rust_core::transport::{
    AdmissionError, LoanError, PayloadLoanProvenance, SendError, SetupError,
};
use up_rust_core::uattributes::{PayloadEncoding, PayloadEncodingError, UMessageType, UPriority};
use up_rust_core::ucode::UCode;
use up_rust_core::uri::{UEntityId, UUri, UUriPartsError, UUriRefusal};
use up_rust_core::uuid::{UuidRefusal, UUID};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const VALID_UUID: &str = "00000000-0001-7000-8010-101010101a1a";

#[test]
fn a_uuid_round_trips_through_text_with_no_heap() {
    let uuid: UUID = assert_no_heap("parse", || VALID_UUID.parse().unwrap());
    let text = assert_no_heap("format", || uuid.hyphenated_ascii());
    assert_eq!(std::str::from_utf8(&text).unwrap(), VALID_UUID);
    assert_eq!(uuid.to_string(), VALID_UUID);
    assert_eq!(
        uuid.as_u64_pair(),
        (0x0000_0000_0001_7000, 0x8010_1010_1010_1a1a)
    );
    assert_eq!(uuid.time(), 1);
}

#[test]
fn uuid_rejections_are_fixed_values() {
    for text in [
        "",
        "not-a-uuid",
        "a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8",
        "00000000-0001-7000-8010-101010101a1g",
        "00000000-0001-7000-8010_101010101a1a",
    ] {
        let refusal: UuidRefusal = assert_no_heap("reject", || text.parse::<UUID>().unwrap_err());
        assert!(!refusal.to_string().is_empty());
    }
    assert_eq!(
        UUID::try_from_wire_pair(0, 0).unwrap_err().to_string(),
        "Error converting UUID: not a v7 UUID"
    );
    assert_eq!(
        UUID::try_from_wire_pair(0x7000, 0).unwrap_err().to_string(),
        "Error converting UUID: not an RFC4122 UUID"
    );
}

#[test]
fn uuid_time_to_live_matches_the_classic_rule() {
    let uuid = UUID::from_str(VALID_UUID).unwrap();
    assert!(!uuid.is_expired_at(0, u128::MAX));
    assert!(!uuid.is_expired_at(500, 501));
    assert!(uuid.is_expired_at(500, 502));
}

#[test]
fn identifier_codes_round_trip() {
    assert_eq!(UCode::from_i32(16), Some(UCode::Unauthenticated));
    assert_eq!(UCode::from_i32(17), None);
    assert_eq!(UCode::Unauthenticated.value(), 16);
    assert_eq!(UPriority::from_wire_code(7), Some(UPriority::CS6));
    assert_eq!(UPriority::from_priority_code("CS4"), Some(UPriority::CS4));
    assert_eq!(UPriority::from_priority_code("CS9"), None);
    assert_eq!(UPriority::CS4.to_priority_code(), "CS4");
    assert_eq!(
        UMessageType::from_wire_code(4),
        Some(UMessageType::Notification)
    );
    assert_eq!(UMessageType::Publish.cloudevent_type(), "up-pub.v1");
    assert_eq!(
        UMessageType::from_cloudevent_type("up-not.v1"),
        Some(UMessageType::Notification)
    );
    assert_eq!(UMessageType::from_cloudevent_type("up-x"), None);
}

#[test]
fn payload_encoding_rules_match_the_registry() {
    assert_eq!(
        PayloadEncoding::try_from_wire_id(7),
        Ok(PayloadEncoding::TEXT)
    );
    assert_eq!(
        PayloadEncoding::try_from_wire_id(0x1_0000),
        Err(PayloadEncodingError::IdentifierTooLarge)
    );
    assert_eq!(
        PayloadEncoding::try_from(0xE000_u32)
            .unwrap()
            .check_wire_originable(),
        Err(PayloadEncodingError::ReservedIdentifier(0xE000))
    );
    assert_eq!(
        PayloadEncoding::from_media_type_str("text/plain; charset=utf-8"),
        Ok(PayloadEncoding::TEXT)
    );
    assert_eq!(
        PayloadEncoding::from_media_type_str("text/plain; charset=latin1"),
        Err(PayloadEncodingError::TextEncodingNotUtf8)
    );
    assert_eq!(
        PayloadEncoding::from_media_type_str("application/protobuf"),
        Err(PayloadEncodingError::AmbiguousMediaType)
    );
    assert_eq!(
        PayloadEncoding::private_use(0xF001).to_string(),
        "61441 (private use)"
    );
}

#[test]
fn a_uri_parses_formats_and_rejects_with_no_heap() {
    let (uri, wire) = assert_no_heap("parse", || {
        let uri: UUri = "up://vin.vehicles/800A/1/2/1A50".parse().unwrap();
        let wire = UUri::try_from_wire_parts("vin.vehicles", 0x0001_800A, 2, 0x1A50).unwrap();
        (uri, wire)
    });
    assert_eq!(uri, wire);
    assert_eq!(uri.to_string(), "up://vin.vehicles/800A/1/2/1A50");
    assert_eq!(
        uri.display_with_scheme(false).to_string(),
        "//vin.vehicles/800A/1/2/1A50"
    );
    let refused = assert_no_heap("reject", || "up://vin/1/1".parse::<UUri>().unwrap_err());
    assert_eq!(
        refused.to_string(),
        "Serialization error: uProtocol URI must contain entity version"
    );
    assert_eq!(
        UUri::try_from_wire_parts("vin:80", 0x0001_0001, 1, 1),
        Err(UUriPartsError::AuthorityHasPort)
    );
    assert!(uri.check_rpc_method().is_ok());
    assert!(matches!(uri.check_event(), Err(UUriRefusal::Validation(_))));
}

#[test]
fn entity_ids_refuse_instance_zero() {
    assert_eq!(UEntityId::first_instance(0x1234).packed(), 0x0001_1234);
    assert_eq!(
        UEntityId::new(1, 0),
        Err(UUriPartsError::ZeroEntityInstance)
    );
    assert_eq!(
        UEntityId::from_packed(0x0000_1234),
        Err(UUriPartsError::ZeroEntityInstance)
    );
}

#[test]
fn a_header_round_trips_with_no_heap() {
    let id: UUID = VALID_UUID.parse().unwrap();
    let source = UUri::try_from_wire_parts("vin", 0x0001_8000, 1, 0x8001).unwrap();
    let parts = TxHeaderParts {
        kind: UMessageType::Publish,
        priority: Some(UPriority::CS1),
        id: &id,
        reqid: None,
        ttl: None,
        comm_status: None,
        permission_level: None,
        source: &source,
        sink: None,
        token: None,
        traceparent: None,
        payload_encoding: Some(PayloadEncoding::RAW),
        native_type_token: Some(NativeTypeToken::from_u32(7)),
    };
    let mut bytes = [0_u8; 256];
    assert_no_heap("write and parse", || {
        let written = write_header(b"prefix", &parts, &mut bytes).unwrap();
        assert_eq!(written.len, header_len(b"prefix", &parts).unwrap());
        let encoded = bytes.get(..written.len).unwrap();
        let received = parse_header(b"prefix", encoded, &HeaderLimits::default()).unwrap();
        assert_eq!(received.kind, UMessageType::Publish);
        assert_eq!(received.source, source);
        assert_eq!(
            received.native_type_token,
            Some(NativeTypeToken::from_u32(7))
        );
        assert_eq!(
            parse_header(b"wrong!", encoded, &HeaderLimits::default()).unwrap_err(),
            UFrameHeaderError::WrongPrefix
        );
    });
}

#[test]
fn fixed_errors_display_their_diagnostics() {
    assert_eq!(UFrameHeaderError::Truncated.to_string(), "truncated header");
    assert_eq!(SetupError::TableFull.to_string(), "reserved table is full");
    assert_eq!(
        LoanError::SlotTooSmall.to_string(),
        "transmit slot is too small"
    );
    assert_eq!(
        SendError::Loan(LoanError::PoolExhausted).to_string(),
        "transmit loan failed: transmit pool is exhausted"
    );
    assert_eq!(
        AdmissionError::PayloadLength.to_string(),
        "wrong payload length"
    );
    assert_eq!(
        PayloadLoanProvenance::OwnedReceiveLease,
        PayloadLoanProvenance::OwnedReceiveLease
    );
}
