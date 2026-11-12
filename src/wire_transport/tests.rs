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

//! Selected-wire adapter behavior tests.

use crate::test_support::{RawRx, RecordingCore};
#[cfg(feature = "zero-copy-transport")]
use std::io::Cursor;
use std::sync::Mutex as StdMutex;

use protobuf::well_known_types::wrappers::StringValue;

use super::*;
use crate::wire::{UProtocolNativeWire, UWireMetadataCodec};
#[cfg(feature = "zero-copy-transport")]
use crate::{
    BorrowPayload, PayloadLoanProvenance, StableContainerPayload, StablePayloadField, UVecRxLease,
};
#[cfg(feature = "protobuf-support")]
use crate::{EncodePayload, ProtobufPayload};
use crate::{PayloadEncoding, UMessageBuilder};
use test_case::test_case;

#[cfg(feature = "zero-copy-transport")]
#[repr(C)]
#[derive(Debug, Eq, PartialEq, crate::StablePayload, crate::StablePayloadInit)]
#[stable_payload(type_name = "uprotocol.test.WireStableBytes")]
struct WireStableBytes {
    bytes: [u8; 4],
}

#[cfg(feature = "zero-copy-transport")]
#[repr(C)]
#[derive(Debug, Eq, PartialEq, crate::StablePayload)]
#[stable_payload(type_name = "uprotocol.test.WireBool")]
struct WireBool {
    value: bool,
}

#[cfg(feature = "zero-copy-transport")]
#[repr(C)]
#[derive(Debug, Eq, PartialEq, crate::StablePayload)]
#[stable_payload(type_name = "uprotocol.test.WireNestedInner")]
struct WireNestedInner {
    value: bool,
}

#[cfg(feature = "zero-copy-transport")]
#[repr(C)]
#[derive(Debug, Eq, PartialEq, crate::StablePayload)]
#[stable_payload(type_name = "uprotocol.test.WireNested")]
struct WireNested {
    inner: WireNestedInner,
    padding: [u8; 3],
    letter: char,
}

#[cfg(feature = "zero-copy-transport")]
#[repr(C)]
#[derive(Debug, Eq, PartialEq, crate::StablePayload)]
#[stable_payload(type_name = "uprotocol.test.WireAligned")]
struct WireAligned {
    value: u32,
}

#[cfg(feature = "zero-copy-transport")]
#[derive(Debug)]
struct CheckedDefaultCodec;

#[cfg(feature = "zero-copy-transport")]
impl crate::PayloadCodecIdentity for CheckedDefaultCodec {
    fn name() -> &'static str {
        "checked-default-test"
    }

    fn encoding() -> PayloadEncoding {
        PayloadEncoding::RAW
    }
}

#[cfg(feature = "zero-copy-transport")]
// SAFETY: Both lanes check exact length and the bool bit pattern before the
// only cast; the unchecked lane is a test double that deliberately keeps
// the checks.
unsafe impl BorrowPayload<WireBool> for CheckedDefaultCodec {
    fn borrow_payload(src: &[u8]) -> Result<&WireBool, UWireError> {
        if src.len() != core::mem::size_of::<WireBool>() || !WireBool::validate_field_bytes(src) {
            return Err(UWireError::invalid_payload("invalid checked-default bool"));
        }
        // SAFETY: WireBool has alignment one and its bool byte was checked.
        Ok(unsafe { &*src.as_ptr().cast::<WireBool>() })
    }
}

#[cfg(feature = "zero-copy-transport")]
#[repr(align(8))]
struct AlignedPayloadStorage([u8; 5]);

#[cfg(feature = "zero-copy-transport")]
struct MisalignedRawRx {
    encoded_metadata: Vec<u8>,
    storage: AlignedPayloadStorage,
}

#[cfg(feature = "zero-copy-transport")]
impl MisalignedRawRx {
    fn payload(&self) -> &[u8] {
        &self.storage.0[1..]
    }
}

#[cfg(feature = "zero-copy-transport")]
impl UEncodedRxFrame for MisalignedRawRx {
    type PayloadReader<'a>
        = Cursor<&'a [u8]>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = std::iter::Once<&'a [u8]>
    where
        Self: 'a;

    fn encoded_metadata(&self) -> &[u8] {
        &self.encoded_metadata
    }

    fn payload_len(&self) -> usize {
        self.payload().len()
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        Cursor::new(self.payload())
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        std::iter::once(self.payload())
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        Some(self.payload())
    }
}

#[cfg(feature = "zero-copy-transport")]
impl UEncodedLoanedRxFrame for MisalignedRawRx {
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError> {
        // The payload is borrowed directly from this raw receive object.
        Ok(LoanedPayload::new(
            self.payload(),
            PayloadLoanProvenance::OpaqueTransportLoan,
        ))
    }
}

#[cfg(feature = "owned-frame-transport")]
#[derive(Default)]
struct RecordingOwnedListener(StdMutex<Vec<UOwnedFrame>>);

#[cfg(feature = "owned-frame-transport")]
#[async_trait]
impl UOwnedListener for RecordingOwnedListener {
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        self.0.lock().unwrap().push(frame);
    }
}

#[cfg(feature = "owned-frame-transport")]
#[test_case(5, 1, "unknown metadata layout"; "layout")]
#[test_case(10, 2, "wrong selected wire"; "wire")]
#[test_case(13, 2, "payload family mismatch"; "family")]
#[tokio::test]
async fn owned_receive_and_listener_reject_identity_mismatch(
    offset: usize,
    value: u8,
    message: &str,
) {
    let mut raw = raw_frame_for_topic(0x9000, b"must-not-be-exposed");
    *raw.encoded_metadata.get_mut(offset).unwrap() = value;
    let core = RecordingCore::default();
    core.received.lock().unwrap().push_back(raw.clone());
    let transport = core.into_native_prefix_wire_transport(UProtocolNativeWire);
    let topic = crate::test_support::topic(0x9000);
    let error = transport
        .receive_validated_owned(&topic, None)
        .await
        .unwrap_err();
    assert_eq!(error.code(), UCode::InvalidArgument);
    assert!(error
        .message()
        .is_some_and(|detail| detail.contains(message)));
    let listener = Arc::new(RecordingOwnedListener::default());
    transport
        .register_validated_owned_listener(&topic, None, listener.clone())
        .await
        .unwrap();
    let registered = transport
        .core()
        .owned_listeners
        .lock()
        .unwrap()
        .first()
        .unwrap()
        .clone();
    registered
        .on_receive_encoded_owned(EncodedOwnedFrame::new(
            raw.encoded_metadata,
            Some(raw.payload.into()),
        ))
        .await;
    assert!(listener.0.lock().unwrap().is_empty());
}

#[cfg(feature = "owned-frame-transport")]
#[tokio::test]
async fn sinkless_owned_listener_does_not_duplicate_notifications() {
    use crate::UOwnedTransport;
    let transport = RecordingCore::default().into_protobuf_transport();
    let source = UUri::try_from_parts("source", 0x0001_4210, 1, 0x8001).unwrap();
    let sink = UUri::try_from_parts("sink", 0x0001_4210, 1, 0).unwrap();
    let publishes = Arc::new(RecordingOwnedListener::default());
    let notifications = Arc::new(RecordingOwnedListener::default());
    transport
        .register_owned_listener(&source, None, publishes.clone())
        .await
        .unwrap();
    transport
        .register_owned_listener(&source, Some(&sink), notifications.clone())
        .await
        .unwrap();
    let metadata = UFrameMetadata::notification(source, sink)
        .with_payload_encoding(PayloadEncoding::PROTOBUF)
        .build()
        .unwrap();
    let encoded = UFrameFieldsMetadataCodec
        .encode_frame_metadata(
            UWireMetadataContext::from_wire::<ProtobufWire, UFrameFieldsMetadataCodec>(),
            &metadata,
        )
        .unwrap();
    let listeners = transport.core().owned_listeners.lock().unwrap().clone();
    for listener in listeners {
        listener
            .on_receive_encoded_owned(EncodedOwnedFrame::new(
                encoded.clone(),
                Some(Bytes::from_static(b"notify")),
            ))
            .await;
    }
    assert!(
        publishes.0.lock().unwrap().is_empty(),
        "None means no sink, not any sink"
    );
    assert_eq!(notifications.0.lock().unwrap().len(), 1);
}

struct CompileCore;

#[cfg(feature = "owned-frame-transport")]
#[async_trait]
impl UOwnedTransportCore for CompileCore {
    async fn send_prepared_owned(&self, _frame: PreparedOwnedFrame) -> Result<(), UStatus> {
        Ok(())
    }
}

fn metadata_with_payload() -> UFrameMetadata {
    metadata_with_payload_encoding(PayloadEncoding::RAW)
}

fn metadata_with_payload_encoding(payload_encoding: PayloadEncoding) -> UFrameMetadata {
    metadata_with_topic_and_payload_encoding(0x9000, payload_encoding)
}

#[cfg(feature = "zero-copy-transport")]
fn stable_metadata<T: crate::StablePayload>() -> UFrameMetadata {
    let identity = StableContainerPayload::<T>::identity(&stable_profile()).unwrap();
    metadata_with_payload_encoding(identity.encoding())
        .with_native_payload_identity(identity)
        .unwrap()
}

#[cfg(feature = "zero-copy-transport")]
fn stable_profile() -> crate::NativeProfileAgreement {
    use crate::{NativeProfile, StablePayload};
    static AGREEMENT: std::sync::OnceLock<crate::NativeProfileAgreement> =
        std::sync::OnceLock::new();
    AGREEMENT
        .get_or_init(|| {
            let representations = [
                WireStableBytes::native_representation(),
                WireBool::native_representation(),
                WireNested::native_representation(),
                WireAligned::native_representation(),
            ];
            let profile = NativeProfile::new(
                "wire-tests",
                1,
                PayloadEncoding::private_use(0xF100),
                representations,
            )
            .unwrap();
            crate::test_support::self_agreement(profile)
        })
        .clone()
}

#[cfg(feature = "zero-copy-transport")]
fn stable_rx<R: UEncodedRxFrame>(
    raw: R,
) -> UWireRx<R, StableContainerWire, UFrameFieldsMetadataCodec> {
    UWireRx::try_from_encoded_with_native_agreement(
        raw,
        &UFrameFieldsMetadataCodec,
        stable_profile(),
    )
    .unwrap()
}

fn metadata_with_topic_and_payload_encoding(
    resource_id: u16,
    payload_encoding: PayloadEncoding,
) -> UFrameMetadata {
    let topic = crate::test_support::topic(resource_id);
    let message = UMessageBuilder::publish(topic)
        .build_with_payload(Vec::new(), PayloadEncoding::RAW)
        .expect("message")
        .with_declared_payload_encoding(payload_encoding)
        .expect("carried encoding");
    crate::frame::metadata::try_project_umessage_to_frame_metadata(&message).expect("metadata")
}

fn raw_frame_for_topic(resource_id: u16, payload: &[u8]) -> RawRx {
    let metadata = metadata_with_topic_and_payload_encoding(resource_id, PayloadEncoding::RAW);
    RawRx {
        encoded_metadata: encode_metadata::<UProtocolNativeWire>(&metadata),
        payload: payload.to_vec(),
    }
}

fn encode_metadata<W>(metadata: &UFrameMetadata) -> Vec<u8>
where
    W: UWire,
{
    UFrameFieldsMetadataCodec
        .encode_frame_metadata(
            UWireMetadataContext::from_wire::<W, UFrameFieldsMetadataCodec>(),
            metadata,
        )
        .unwrap()
}

#[test]
fn wire_rx_decodes_metadata_and_delegates_payload() {
    let metadata = metadata_with_payload();
    let encoded_metadata = encode_metadata::<UProtocolNativeWire>(&metadata);
    let raw = RawRx {
        encoded_metadata,
        payload: b"abc".to_vec(),
    };

    let rx = UWireRx::<RawRx, UProtocolNativeWire, UFrameFieldsMetadataCodec>::try_from_encoded(
        raw,
        &UFrameFieldsMetadataCodec,
    )
    .unwrap();

    assert_eq!(rx.metadata(), &metadata);
    assert_eq!(rx.payload_len(), 3);
    assert_eq!(rx.try_contiguous_payload(), Some(&b"abc"[..]));
}

#[test]
fn wire_rx_decodes_payload_with_selected_wire() {
    let value = StringValue {
        value: "selected-wire".to_string(),
        special_fields: Default::default(),
    };
    let payload = ProtobufWire::encode_payload_owned(&value).unwrap();
    let metadata = metadata_with_payload_encoding(ProtobufPayload::encoding());
    let encoded_metadata = encode_metadata::<ProtobufWire>(&metadata);
    let raw = RawRx {
        encoded_metadata,
        payload: payload.to_vec(),
    };

    let rx = UWireRx::<RawRx, ProtobufWire, UFrameFieldsMetadataCodec>::try_from_encoded(
        raw,
        &UFrameFieldsMetadataCodec,
    )
    .unwrap();
    let decoded: StringValue = rx.decode_payload(PayloadDecodeLimit::new(1024)).unwrap();

    assert_eq!(decoded.value, "selected-wire");
}

#[cfg(feature = "zero-copy-transport")]
#[test]
fn selected_stable_wire_safe_borrow_validates_encoding_and_bits() {
    let value = WireBool { value: true };
    let metadata = stable_metadata::<WireBool>();
    let valid = RawRx {
        encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
        payload: vec![1],
    };
    let valid = stable_rx(valid);
    assert_eq!(valid.borrow_payload::<WireBool>().unwrap(), &value);

    let invalid = RawRx {
        encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
        payload: vec![2],
    };
    let invalid = stable_rx(invalid);
    assert!(
        matches!(invalid.borrow_payload::<WireBool>(), Err(UWireError::InvalidPayload(reason)) if reason.contains("not valid"))
    );

    let wrong_metadata = metadata_with_payload_encoding(PayloadEncoding::RAW);
    let wrong = RawRx {
        encoded_metadata: encode_metadata::<StableContainerWire>(&wrong_metadata),
        payload: vec![1],
    };
    let wrong = stable_rx(wrong);
    assert!(matches!(
        wrong.borrow_payload::<WireBool>(),
        Err(UWireError::UnsupportedEncoding { .. })
    ));
}

#[cfg(feature = "zero-copy-transport")]
#[test]
fn selected_stable_wire_rejects_nested_bool_and_char_bits() {
    let metadata = stable_metadata::<WireNested>();
    let mut nested_bool = vec![0; core::mem::size_of::<WireNested>()];
    let bool_offset =
        core::mem::offset_of!(WireNested, inner) + core::mem::offset_of!(WireNestedInner, value);
    *nested_bool.get_mut(bool_offset).unwrap() = 2;
    let raw = crate::test_support::AlignedRawFrame::new(
        metadata.clone(),
        encode_metadata::<StableContainerWire>(&metadata),
        &nested_bool,
        core::mem::align_of::<WireNested>(),
    );
    let rx = stable_rx(raw);
    assert!(
        matches!(rx.borrow_payload::<WireNested>(), Err(UWireError::InvalidPayload(reason)) if reason.contains("not valid"))
    );

    let mut invalid_char = vec![0; core::mem::size_of::<WireNested>()];
    *invalid_char.get_mut(bool_offset).unwrap() = 1;
    let char_offset = core::mem::offset_of!(WireNested, letter);
    invalid_char
        .get_mut(char_offset..char_offset + core::mem::size_of::<char>())
        .unwrap()
        .copy_from_slice(&0x0011_0000_u32.to_ne_bytes());
    let raw = crate::test_support::AlignedRawFrame::new(
        metadata.clone(),
        encode_metadata::<StableContainerWire>(&metadata),
        &invalid_char,
        core::mem::align_of::<WireNested>(),
    );
    let rx = stable_rx(raw);
    assert!(
        matches!(rx.borrow_payload::<WireNested>(), Err(UWireError::InvalidPayload(reason)) if reason.contains("not valid"))
    );
}

#[cfg(feature = "zero-copy-transport")]
#[test]
fn selected_stable_wire_rejects_truncated_and_misaligned_payloads() {
    let metadata = stable_metadata::<WireAligned>();
    let truncated = RawRx {
        encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
        payload: vec![0; core::mem::size_of::<WireAligned>() - 1],
    };
    let truncated = stable_rx(truncated);
    assert!(
        matches!(truncated.borrow_payload::<WireAligned>(), Err(UWireError::InvalidPayload(reason)) if reason.contains("length"))
    );

    let misaligned = MisalignedRawRx {
        encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
        storage: AlignedPayloadStorage([0; 5]),
    };
    assert_ne!(
        (misaligned.payload().as_ptr() as usize) % core::mem::align_of::<WireAligned>(),
        0
    );
    let misaligned = stable_rx(misaligned);
    assert!(
        matches!(misaligned.borrow_payload::<WireAligned>(), Err(UWireError::InvalidPayload(reason)) if reason.contains("not aligned"))
    );
}

#[cfg(feature = "zero-copy-transport")]
#[test]
fn stable_loan_bridge_rejects_malformed_and_wrong_encoding() {
    let metadata = stable_metadata::<WireBool>();
    let malformed = RawRx {
        encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
        payload: vec![2],
    };
    let malformed = stable_rx(malformed);
    assert!(
        matches!(malformed.borrow_stable_payload::<WireBool>(&stable_profile()),
            Err(UWireError::InvalidPayload(reason)) if reason.contains("not valid"))
    );

    let wrong_metadata = metadata_with_payload_encoding(PayloadEncoding::RAW);
    let wrong = RawRx {
        encoded_metadata: encode_metadata::<StableContainerWire>(&wrong_metadata),
        payload: vec![1],
    };
    let wrong = stable_rx(wrong);
    assert!(matches!(
        wrong.borrow_stable_payload::<WireBool>(&stable_profile()),
        Err(UWireError::UnsupportedEncoding { .. })
    ));
}

#[cfg(feature = "zero-copy-transport")]
#[test]
fn inherited_unchecked_default_remains_checked() {
    // SAFETY: The test codec's unchecked lane is the inherited checked
    // path; the invalid bool pattern below must be rejected, not cast.
    let result =
        unsafe { <CheckedDefaultCodec as BorrowPayload<WireBool>>::borrow_payload_unchecked(&[2]) };
    assert!(matches!(result, Err(UWireError::InvalidPayload(_))));
}

#[cfg(feature = "zero-copy-transport")]
#[test]
fn selected_stable_wire_unchecked_lane_accepts_caller_proven_payload() {
    let metadata = stable_metadata::<WireBool>();
    let raw = RawRx {
        encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
        payload: vec![1],
    };
    let rx = stable_rx(raw);

    // SAFETY: This test constructed the single payload byte itself as a
    // valid `bool`, so the caller-proven-validity obligation holds.
    let borrowed = unsafe { rx.borrow_payload_unchecked::<WireBool>() }.unwrap();
    assert!(borrowed.value);
}

#[cfg(feature = "zero-copy-transport")]
#[test_case::test_case(false, None; "checked missing token")]
#[test_case::test_case(false, Some(1); "checked wrong token")]
#[test_case::test_case(true, None; "unchecked missing token")]
#[test_case::test_case(true, Some(1); "unchecked wrong token")]
fn native_identity_is_checked_in_both_borrow_lanes(unchecked: bool, delta: Option<u32>) {
    let original = stable_metadata::<WireBool>();
    let encoding = *original.payload_encoding().unwrap();
    let mut metadata = original
        .clone()
        .without_payload_encoding()
        .with_payload_encoding(encoding)
        .unwrap();
    if let Some(delta) = delta {
        metadata = metadata
            .with_native_type_token(crate::NativeTypeToken::from_u32(
                original
                    .native_type_token()
                    .unwrap()
                    .as_u32()
                    .wrapping_add(delta),
            ))
            .unwrap();
    }
    let frame = stable_rx(RawRx {
        encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
        payload: vec![1],
    });
    let result = if unchecked {
        // SAFETY: This producer supplies a valid WireBool byte and alignment.
        // The intentionally wrong metadata is a checked precondition.
        unsafe { frame.borrow_payload_unchecked::<WireBool>() }
    } else {
        frame.borrow_payload::<WireBool>()
    };
    assert!(matches!(
        result,
        Err(UWireError::UnsupportedNativeTypeToken { .. })
    ));
}

#[cfg(feature = "zero-copy-transport")]
#[test_case::test_case(false; "checked missing profile")]
#[test_case::test_case(true; "unchecked missing profile")]
fn native_bytes_without_route_context_remain_opaque(unchecked: bool) {
    let metadata = stable_metadata::<WireBool>();
    let frame = UWireRx::<RawRx, StableContainerWire, UFrameFieldsMetadataCodec>::try_from_encoded(
        RawRx {
            encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
            payload: vec![1],
        },
        &UFrameFieldsMetadataCodec,
    )
    .unwrap();
    assert_eq!(frame.try_contiguous_payload(), Some(&[1][..]));
    let result = if unchecked {
        // SAFETY: Known typed producer, exact WireBool representation and bits.
        unsafe { frame.borrow_payload_unchecked::<WireBool>() }
    } else {
        frame.borrow_payload::<WireBool>()
    };
    assert!(matches!(result, Err(UWireError::NativeProfile(_))));
}

#[cfg(feature = "zero-copy-transport")]
#[test]
fn received_frame_keeps_its_generation_when_a_route_is_replaced() {
    let metadata = stable_metadata::<WireBool>();
    let frame = stable_rx(RawRx {
        encoded_metadata: encode_metadata::<StableContainerWire>(&metadata),
        payload: vec![1],
    });
    let old = stable_profile();
    let next = crate::NativeProfile::new(
        "wire-tests",
        2,
        old.profile().encoding(),
        old.profile().representations().cloned(),
    )
    .unwrap();
    let next = old.successor(Arc::new(next.clone()), &next).unwrap();
    assert!(frame.borrow_payload::<WireBool>().unwrap().value);
    assert!(matches!(
        frame.borrow_stable_payload::<WireBool>(&next),
        Err(UWireError::NativeProfile(
            crate::NativeProfileError::ProfileMismatch
        ))
    ));
}

#[cfg(feature = "owned-frame-transport")]
#[tokio::test]
async fn owned_receive_uses_exact_core_filter_for_exact_source_filter() {
    let core = RecordingCore::default();
    core.received
        .lock()
        .unwrap()
        .push_back(raw_frame_for_topic(0x9000, b"owned"));
    let transport = core.into_native_prefix_wire_transport(UProtocolNativeWire);
    let source_filter = UUri::try_from_parts("vehicle", 0x0001_4210, 0x01, 0x9000).unwrap();

    let frame = transport
        .receive_validated_owned(&source_filter, None)
        .await
        .expect("matching owned frame");

    assert_eq!(frame.payload_bytes(), b"owned");
    let filters = transport.core().receive_filters.lock().unwrap();
    assert_eq!(filters.len(), 1);
    let filter = filters.first().expect("one receive filter");
    assert_eq!(filter.0, source_filter);
    assert_eq!(filter.1, None);
}

#[cfg(feature = "owned-frame-transport")]
#[tokio::test]
async fn owned_receive_preserves_partial_core_source_filter() {
    let core = RecordingCore::default();
    core.received
        .lock()
        .unwrap()
        .push_back(raw_frame_for_topic(0x9000, b"owned"));
    let transport = core.into_native_prefix_wire_transport(UProtocolNativeWire);
    let source_filter = UUri::try_from_parts("vehicle", 0x0001_4210, 0x01, u16::MAX).unwrap();

    let frame = transport
        .receive_validated_owned(&source_filter, None)
        .await
        .expect("matching owned frame");

    assert_eq!(frame.payload_bytes(), b"owned");
    let filters = transport.core().receive_filters.lock().unwrap();
    assert_eq!(filters.len(), 1);
    let filter = filters.first().expect("one receive filter");
    assert_eq!(filter.0, source_filter);
    assert_eq!(filter.1, None);
}

#[test]
fn selected_wire_core_source_filter_preserves_exact_and_partial_patterns() {
    let exact = UUri::try_from_parts("vehicle", 0x0001_4210, 0x01, 0x9000).unwrap();
    let wildcard = UUri::try_from_parts("vehicle", 0x0001_4210, 0x01, u16::MAX).unwrap();

    assert_eq!(selected_wire_core_source_filter_for(&exact), exact);
    assert_eq!(selected_wire_core_source_filter_for(&wildcard), wildcard);
}

#[cfg(feature = "owned-frame-transport")]
#[test_case(0x9000; "exact owned listener filter remains exact")]
#[test_case(u16::MAX; "wildcard owned listener filter remains broad")]
#[tokio::test]
async fn owned_listener_registration_and_removal_use_the_same_core_filter(resource: u16) {
    let transport = RecordingCore::default().into_native_prefix_wire_transport(UProtocolNativeWire);
    let source = UUri::try_from_parts("vehicle", 0x0001_4210, 1, resource).unwrap();
    let listener = Arc::new(RecordingOwnedListener::default());
    transport
        .register_validated_owned_listener(&source, None, listener.clone())
        .await
        .unwrap();
    let encoded_listener = transport
        .core()
        .owned_listeners
        .lock()
        .unwrap()
        .first()
        .expect("registered owned listener")
        .clone();
    let raw = raw_frame_for_topic(0x9000, b"owned-after-unregister");
    encoded_listener
        .on_receive_encoded_owned(EncodedOwnedFrame::new(
            raw.encoded_metadata,
            Some(raw.payload.into()),
        ))
        .await;
    transport
        .unregister_validated_owned_listener(&source, None, listener.clone())
        .await
        .unwrap();
    let expected = selected_wire_core_source_filter_for(&source);
    assert_eq!(
        *transport.core().listener_filters.lock().unwrap(),
        vec![(true, expected.clone(), None), (false, expected, None)]
    );
    assert!(transport.core().owned_listeners.lock().unwrap().is_empty());
    drop(encoded_listener);
    drop(transport);
    assert_eq!(
        listener
            .0
            .lock()
            .unwrap()
            .first()
            .expect("delivered owned frame")
            .payload_bytes(),
        b"owned-after-unregister"
    );
}

#[cfg(feature = "owned-frame-transport")]
#[test]
fn wire_transport_fits_owned_blanket_boundary() {
    fn assert_owned_impl<T: UOwnedTransportImpl>() {}

    type Transport = UWireTransport<CompileCore, UProtocolNativeWire, UFrameFieldsMetadataCodec>;
    assert_owned_impl::<Transport>();
}

#[test]
fn with_wire_constructs_adapter() {
    let transport = CompileCore.into_native_prefix_wire_transport(UProtocolNativeWire);
    let _: &UProtocolNativeWire = transport.wire();
    let _: &CompileCore = transport.core();
}

#[cfg(feature = "zero-copy-transport")]
#[test]
fn existing_public_rx_lease_still_compiles_independently() {
    fn assert_public_rx<T: UZeroCopyRxLease>() {}

    assert_public_rx::<UVecRxLease>();
}
