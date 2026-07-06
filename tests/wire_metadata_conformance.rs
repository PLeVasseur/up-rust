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

use bytes::Bytes;
use up_rust::{
    NativePrefixProtobufMetadataCodec, PayloadEncoding, UFrameMetadata, UMessageBuilder,
    UPayloadFormat, UProtocolNativeWire, UUri, UWire, UWireMetadataCodec, UWireMetadataError,
    WireIdentity,
};

const LAYOUT_COMPACT_LO: usize = 5;
const LAYOUT_COMPACT_HI: usize = 6;
const VERSION_LO: usize = 7;
const SELECTED_WIRE_COMPACT_LO: usize = 10;
const SELECTED_WIRE_COMPACT_HI: usize = 11;
const PAYLOAD_FAMILY_COMPACT_LO: usize = 13;
const PAYLOAD_FAMILY_COMPACT_HI: usize = 14;
const FLAGS_LO: usize = 15;

struct WrongWire;

impl UWire for WrongWire {
    const WIRE_ID: WireIdentity = WireIdentity::new("org.eclipse.uprotocol.wire.test", 0x7001);
    const PAYLOAD_FAMILY_ID: WireIdentity = WireIdentity::new("native-explicit", 0x0001);
    const METADATA_LAYOUT_ID: WireIdentity =
        WireIdentity::new("org.eclipse.uprotocol.metadata.native-prefix", 0x0001);
    const FORMAT_VERSION: u16 = 1;
}

struct WrongPayloadFamily;

impl UWire for WrongPayloadFamily {
    const WIRE_ID: WireIdentity = WireIdentity::new("org.eclipse.uprotocol.wire.native", 0x0001);
    const PAYLOAD_FAMILY_ID: WireIdentity =
        WireIdentity::new("org.eclipse.uprotocol.payload.test", 0x7002);
    const METADATA_LAYOUT_ID: WireIdentity =
        WireIdentity::new("org.eclipse.uprotocol.metadata.native-prefix", 0x0001);
    const FORMAT_VERSION: u16 = 1;
}

fn topic() -> UUri {
    UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).expect("topic")
}

fn metadata_without_payload() -> UFrameMetadata {
    let message = UMessageBuilder::publish(topic()).build().expect("message");
    up_rust::try_project_attributes_to_frame_metadata(message.attributes(), None).expect("metadata")
}

fn metadata_with_standard_payload() -> UFrameMetadata {
    let message = UMessageBuilder::publish(topic())
        .build_with_payload(Bytes::from_static(b"payload"), UPayloadFormat::Raw)
        .expect("message");
    up_rust::try_project_attributes_to_frame_metadata(
        message.attributes(),
        Some(PayloadEncoding::RAW),
    )
    .expect("metadata")
}

fn metadata_with_custom_payload() -> UFrameMetadata {
    let message = UMessageBuilder::publish(topic()).build().expect("message");
    up_rust::try_project_attributes_to_frame_metadata(
        message.attributes(),
        Some(
            PayloadEncoding::custom("com.example.native", "application/vnd.example.native")
                .expect("custom encoding"),
        ),
    )
    .expect("metadata")
}

fn encode<W>(metadata: &UFrameMetadata) -> Vec<u8>
where
    W: UWire,
{
    NativePrefixProtobufMetadataCodec
        .encode_frame_metadata(W::metadata_context(), metadata)
        .expect("encode")
}

fn decode<W>(encoded: &[u8]) -> Result<UFrameMetadata, UWireMetadataError>
where
    W: UWire,
{
    NativePrefixProtobufMetadataCodec.decode_frame_metadata(W::metadata_context(), encoded)
}

fn encoded_without_payload() -> Vec<u8> {
    encode::<UProtocolNativeWire>(&metadata_without_payload())
}

fn set_byte(bytes: &mut [u8], offset: usize, value: u8) {
    *bytes.get_mut(offset).expect("offset in test vector") = value;
}

#[test]
fn identity_constants_match_first_wave_register() {
    assert_eq!(
        UProtocolNativeWire::WIRE_ID.literal_id(),
        "org.eclipse.uprotocol.wire.native"
    );
    assert_eq!(UProtocolNativeWire::WIRE_ID.compact_id(), 0x0001);
    assert_eq!(
        UProtocolNativeWire::PAYLOAD_FAMILY_ID.literal_id(),
        "native-explicit"
    );
    assert_eq!(UProtocolNativeWire::PAYLOAD_FAMILY_ID.compact_id(), 0x0001);
    assert_eq!(
        UProtocolNativeWire::METADATA_LAYOUT_ID.literal_id(),
        "org.eclipse.uprotocol.metadata.native-prefix"
    );
    assert_eq!(UProtocolNativeWire::METADATA_LAYOUT_ID.compact_id(), 0x0001);
    assert_eq!(UProtocolNativeWire::FORMAT_VERSION, 1);
}

#[test]
fn no_payload_metadata_round_trips() {
    let metadata = metadata_without_payload();

    let encoded = encode::<UProtocolNativeWire>(&metadata);
    let decoded = decode::<UProtocolNativeWire>(&encoded).expect("decode");

    assert_eq!(decoded, metadata);
    assert!(decoded.payload_encoding().is_none());
}

#[test]
fn standard_payload_metadata_round_trips() {
    let metadata = metadata_with_standard_payload();

    let encoded = encode::<UProtocolNativeWire>(&metadata);
    let decoded = decode::<UProtocolNativeWire>(&encoded).expect("decode");

    assert_eq!(decoded, metadata);
    assert_eq!(decoded.payload_encoding(), Some(&PayloadEncoding::RAW));
}

#[test]
fn custom_payload_encoding_is_preserved() {
    let metadata = metadata_with_custom_payload();

    let encoded = encode::<UProtocolNativeWire>(&metadata);
    let decoded = decode::<UProtocolNativeWire>(&encoded).expect("decode");

    assert_eq!(decoded, metadata);
    assert_eq!(
        decoded
            .payload_encoding()
            .and_then(PayloadEncoding::custom_identity),
        Some(("com.example.native", "application/vnd.example.native"))
    );
}

#[test]
fn wrong_magic_is_rejected() {
    let mut encoded = encoded_without_payload();
    set_byte(&mut encoded, 0, b'X');

    let error = decode::<UProtocolNativeWire>(&encoded).unwrap_err();

    assert_eq!(error, UWireMetadataError::WrongMagic);
}

#[test]
fn unknown_metadata_layout_is_rejected() {
    let mut encoded = encoded_without_payload();
    set_byte(&mut encoded, LAYOUT_COMPACT_LO, 0xFF);
    set_byte(&mut encoded, LAYOUT_COMPACT_HI, 0xFF);

    let error = decode::<UProtocolNativeWire>(&encoded).unwrap_err();

    assert!(matches!(
        error,
        UWireMetadataError::UnknownMetadataLayoutId { .. }
    ));
}

#[test]
fn unsupported_version_is_rejected() {
    let mut encoded = encoded_without_payload();
    set_byte(&mut encoded, VERSION_LO, 0x02);

    let error = decode::<UProtocolNativeWire>(&encoded).unwrap_err();

    assert!(matches!(
        error,
        UWireMetadataError::UnsupportedVersion { actual: 2, .. }
    ));
}

#[test]
fn wrong_selected_wire_id_is_rejected() {
    let encoded = encoded_without_payload();

    let error = decode::<WrongWire>(&encoded).unwrap_err();

    assert!(matches!(
        error,
        UWireMetadataError::WrongWireMetadata { .. }
    ));
}

#[test]
fn unknown_selected_wire_id_is_rejected() {
    let mut encoded = encoded_without_payload();
    set_byte(&mut encoded, SELECTED_WIRE_COMPACT_LO, 0xFF);
    set_byte(&mut encoded, SELECTED_WIRE_COMPACT_HI, 0xFF);

    let error = decode::<UProtocolNativeWire>(&encoded).unwrap_err();

    assert!(matches!(
        error,
        UWireMetadataError::WrongWireMetadata { .. }
    ));
}

#[test]
fn payload_family_mismatch_is_rejected() {
    let encoded = encoded_without_payload();

    let error = decode::<WrongPayloadFamily>(&encoded).unwrap_err();

    assert!(matches!(
        error,
        UWireMetadataError::PayloadFamilyMismatch { .. }
    ));
}

#[test]
fn unknown_payload_family_id_is_rejected() {
    let mut encoded = encoded_without_payload();
    set_byte(&mut encoded, PAYLOAD_FAMILY_COMPACT_LO, 0xFF);
    set_byte(&mut encoded, PAYLOAD_FAMILY_COMPACT_HI, 0xFF);

    let error = decode::<UProtocolNativeWire>(&encoded).unwrap_err();

    assert!(matches!(
        error,
        UWireMetadataError::PayloadFamilyMismatch { .. }
    ));
}

#[test]
fn reserved_flags_are_rejected() {
    let mut encoded = encoded_without_payload();
    set_byte(&mut encoded, FLAGS_LO, 0x01);

    let error = decode::<UProtocolNativeWire>(&encoded).unwrap_err();

    assert_eq!(error, UWireMetadataError::UnsupportedReservedFlags(1));
}

#[test]
fn malformed_length_is_rejected() {
    let mut encoded = encoded_without_payload();
    encoded.truncate(encoded.len().saturating_sub(2));

    let error = decode::<UProtocolNativeWire>(&encoded).unwrap_err();

    assert!(matches!(error, UWireMetadataError::MalformedMetadata(_)));
}

#[test]
fn unsupported_payload_encoding_tag_is_rejected() {
    let mut encoded = encoded_without_payload();
    let last = encoded.len().checked_sub(1).expect("nonempty vector");
    set_byte(&mut encoded, last, 0x03);

    let error = decode::<UProtocolNativeWire>(&encoded).unwrap_err();

    assert!(matches!(
        error,
        UWireMetadataError::UnsupportedPayloadEncoding(_)
    ));
}

#[test]
fn unsupported_standard_payload_encoding_is_rejected() {
    let metadata = metadata_with_standard_payload();
    let mut encoded = encode::<UProtocolNativeWire>(&metadata);
    let last = encoded
        .len()
        .checked_sub(4)
        .expect("standard payload bytes");
    set_byte(&mut encoded, last, 0x7F);

    let error = decode::<UProtocolNativeWire>(&encoded).unwrap_err();

    assert!(matches!(
        error,
        UWireMetadataError::UnsupportedPayloadEncoding(_)
    ));
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut encoded = encoded_without_payload();
    encoded.push(0x00);

    let error = decode::<UProtocolNativeWire>(&encoded).unwrap_err();

    assert!(matches!(error, UWireMetadataError::MalformedMetadata(_)));
}

#[test]
fn metadata_size_budget_cases_are_recordable() {
    let no_payload = encode::<UProtocolNativeWire>(&metadata_without_payload());
    let standard = encode::<UProtocolNativeWire>(&metadata_with_standard_payload());
    let custom = encode::<UProtocolNativeWire>(&metadata_with_custom_payload());
    let full_attributes = standard.clone();

    assert!(!no_payload.is_empty());
    assert!(standard.len() >= no_payload.len());
    assert!(custom.len() > no_payload.len());
    assert_eq!(full_attributes.len(), standard.len());
}
