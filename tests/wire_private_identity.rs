// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Production-private identities must not alias through compact references.
#![cfg(feature = "wire-implementer-api")]

use up_rust::{
    UFrameFieldsMetadataCodec, UFrameMetadata, UProtocolNativeWire, UUri, UWire,
    UWireMetadataCodec, UWireMetadataContext, UWireMetadataError, WireCompatibility, WireIdentity,
    WireIdentityRef, NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID, UFRAME_FIELDS_METADATA_LAYOUT_ID,
};

const VENDOR_A: WireIdentity = WireIdentity::new("vendor-a.production.frame", 0x8A00);
const VENDOR_B: WireIdentity = WireIdentity::new("vendor-b.production.frame", 0x8A00);

fn metadata() -> UFrameMetadata {
    UFrameMetadata::publish(UUri::try_from_parts("vehicle", 0x4210, 1, 0x9000).unwrap())
        .build()
        .unwrap()
}

fn context(wire: WireIdentity, family: WireIdentity) -> UWireMetadataContext {
    UWireMetadataContext::new_exact(wire, family, UFRAME_FIELDS_METADATA_LAYOUT_ID, 1)
}

fn compact_prefix(wire: u16, family: u16) -> Vec<u8> {
    let mut bytes = b"UPWM".to_vec();
    bytes.extend_from_slice(&[0, 2, 0, 1, 0]); // Registered layout 2, format version 1.
    for identity in [wire, family] {
        bytes.push(0);
        bytes.extend_from_slice(&identity.to_le_bytes());
    }
    bytes.extend_from_slice(&[0, 0]); // Reserved flags.
    bytes.extend_from_slice(&u32::MAX.to_le_bytes()); // Deliberately absent body.
    bytes
}

#[test]
fn reserved_metadata_compact_value_is_not_a_wire_or_payload_reservation() {
    let mut bytes = b"UPWM".to_vec();
    bytes.extend_from_slice(&[0, 1, 0]);
    assert!(matches!(
        UFrameFieldsMetadataCodec.decode_frame_metadata(
            context(
                UProtocolNativeWire::WIRE_ID,
                NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID
            ),
            &bytes,
        ),
        Err(UWireMetadataError::UnknownMetadataLayoutId {
            actual: WireIdentityRef::Compact(1)
        })
    ));
    assert!(WireIdentityRef::Compact(1).matches(UProtocolNativeWire::WIRE_ID));
    assert_eq!(
        up_rust::PayloadEncoding::from_id(1).unwrap(),
        up_rust::PayloadEncoding::PROTOBUF_WRAPPED_IN_ANY
    );
}

#[test]
fn two_vendors_with_one_compact_alias_are_rejected_before_exposure() {
    let codec = UFrameFieldsMetadataCodec;
    let a = context(VENDOR_A, NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID);
    let b = context(VENDOR_B, NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID);
    let encoded = codec.encode_frame_metadata(b, &metadata()).unwrap();
    assert!(matches!(
        codec.decode_frame_metadata(a, &encoded),
        Err(UWireMetadataError::WrongWireMetadata { .. })
    ));
}

#[test]
fn private_payload_families_cannot_alias_through_their_compact_value() {
    let codec = UFrameFieldsMetadataCodec;
    let a = context(UProtocolNativeWire::WIRE_ID, VENDOR_A);
    let b = context(UProtocolNativeWire::WIRE_ID, VENDOR_B);
    let encoded = codec.encode_frame_metadata(b, &metadata()).unwrap();
    assert!(matches!(
        codec.decode_frame_metadata(a, &encoded),
        Err(UWireMetadataError::PayloadFamilyMismatch { .. })
    ));
}

#[test]
fn private_wire_compact_only_is_rejected_before_malformed_body() {
    assert!(matches!(
        UFrameFieldsMetadataCodec.decode_frame_metadata(
            context(VENDOR_A, NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID),
            &compact_prefix(0x8A00, 1),
        ),
        Err(UWireMetadataError::WrongWireMetadata { .. })
    ));
}

#[test]
fn private_family_compact_only_is_rejected_before_malformed_body() {
    assert!(matches!(
        UFrameFieldsMetadataCodec.decode_frame_metadata(
            context(UProtocolNativeWire::WIRE_ID, VENDOR_A),
            &compact_prefix(1, 0x8A00),
        ),
        Err(UWireMetadataError::PayloadFamilyMismatch { .. })
    ));
}

struct PermissivePrivateWire;

impl UWire for PermissivePrivateWire {
    const WIRE_ID: WireIdentity = VENDOR_A;
    const PAYLOAD_FAMILY_ID: WireIdentity = NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID;
    const FORMAT_VERSION: u16 = 1;

    fn wire_compatibility(_: &WireIdentityRef) -> WireCompatibility {
        WireCompatibility::Compatible
    }
}

#[test]
fn compatibility_callback_cannot_accept_compact_only_private_identity() {
    assert!(matches!(
        UFrameFieldsMetadataCodec.decode_frame_metadata(
            UWireMetadataContext::from_wire::<PermissivePrivateWire, UFrameFieldsMetadataCodec>(),
            &compact_prefix(0x8A00, 1),
        ),
        Err(UWireMetadataError::WrongWireMetadata { .. })
    ));
}

struct PermissivePrivateFamily;

impl UWire for PermissivePrivateFamily {
    const WIRE_ID: WireIdentity = UProtocolNativeWire::WIRE_ID;
    const PAYLOAD_FAMILY_ID: WireIdentity = VENDOR_A;
    const FORMAT_VERSION: u16 = 1;

    fn payload_family_compatibility(_: &WireIdentityRef) -> WireCompatibility {
        WireCompatibility::Compatible
    }
}

#[test]
fn family_callback_cannot_accept_compact_only_private_identity() {
    assert!(matches!(
        UFrameFieldsMetadataCodec.decode_frame_metadata(
            UWireMetadataContext::from_wire::<PermissivePrivateFamily, UFrameFieldsMetadataCodec>(),
            &compact_prefix(1, 0x8A00),
        ),
        Err(UWireMetadataError::PayloadFamilyMismatch { .. })
    ));
}

#[test]
fn family_registration_does_not_register_the_same_literal_as_a_wire() {
    assert!(matches!(
        UFrameFieldsMetadataCodec.decode_frame_metadata(
            context(
                NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID,
                NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID
            ),
            &compact_prefix(1, 1),
        ),
        Err(UWireMetadataError::WrongWireMetadata { .. })
    ));
}

#[test]
fn unregistered_low_compact_value_is_not_a_registration() {
    let unregistered = WireIdentity::new("vendor-a.unregistered", 1);
    assert!(!WireIdentityRef::Compact(1).matches(unregistered));
}

#[test]
fn explicit_private_literal_round_trip_preserves_metadata() {
    let codec = UFrameFieldsMetadataCodec;
    let context = context(VENDOR_A, VENDOR_A);
    let metadata = metadata();
    let encoded = codec.encode_frame_metadata(context, &metadata).unwrap();
    assert!(encoded
        .windows(VENDOR_A.literal_id().len())
        .any(|bytes| bytes == VENDOR_A.literal_id().as_bytes()));
    assert_eq!(
        codec.decode_frame_metadata(context, &encoded).unwrap(),
        metadata
    );
}
