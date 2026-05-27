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

use std::{io::Cursor, mem, sync::Mutex};

use super::*;
use crate::{
    test_util::{zero_copy_conformance, InMemoryZeroCopyTransport},
    zero_copy::{UUninitTxBuffer, UVecUninitTxBuffer, UZeroCopyTransport, UZeroCopyTransportExt},
    PlacementDefault, UCode, UStatus, UUri, UZeroCopyUninitTransportExt, UUID,
};

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    PlacementDefault,
    crate::StablePayload,
    crate::ByteBackedStablePayload,
)]
#[stable_payload(type_name = "example.vehicle.VehiclePose")]
struct VehiclePose {
    x: u32,
    y: u32,
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, crate::StablePayload, crate::ByteBackedStablePayload)]
#[stable_payload(type_name = "example.vehicle.NoCopyMarker")]
struct NoCopyMarker {
    value: u8,
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, crate::StablePayload, crate::ByteBackedStablePayload)]
#[stable_payload(type_name = "example.vehicle.NonCopyPose")]
struct NonCopyPose {
    x: u32,
    y: u32,
    marker: NoCopyMarker,
    _pad: [u8; 3],
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, crate::StablePayload)]
#[stable_payload(type_name = "example.vehicle.PaddedPose")]
struct PaddedPose {
    small: u8,
    large: u32,
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq)]
struct ManualByteBackedPose {
    x: u32,
    y: u32,
}

// SAFETY:
// - `ManualByteBackedPose` is `#[repr(C)]`, has no drop glue, and contains only
//   two initialized integer fields.
// - The fixed stable type name is used only by these tests.
unsafe impl ZeroCopySend for ManualByteBackedPose {
    unsafe fn type_name() -> &'static str {
        "example.vehicle.ManualByteBackedPose"
    }

    fn __is_zero_copy_send(&self) {}
}

// SAFETY:
// - The test type is fixed-size, `#[repr(C)]`, and all valid test payload bytes
//   are checked by stable-container metadata before borrowed as `Self`.
unsafe impl StablePayload for ManualByteBackedPose {
    const SUPPORTS_BYTE_BACKED_UNINIT: bool = true;

    fn stable_type_name() -> &'static str {
        "example.vehicle.ManualByteBackedPose"
    }
}

// SAFETY:
// - `ManualByteBackedPose` has no implicit padding and every byte in
//   `size_of::<Self>()` is covered by initialized `u32` fields.
unsafe impl ByteBackedStablePayload for ManualByteBackedPose {}

struct BorrowedContiguousFrame<'a> {
    metadata: UFrameMetadata,
    payload: &'a [u8],
}

impl UZeroCopyRxFrame for BorrowedContiguousFrame<'_> {
    type PayloadReader<'a>
        = Cursor<&'a [u8]>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = std::iter::Once<&'a [u8]>
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload_len(&self) -> usize {
        self.payload.len()
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        Cursor::new(self.payload)
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        std::iter::once(self.payload)
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        Some(self.payload)
    }
}

impl UContiguousZeroCopyRxFrame for BorrowedContiguousFrame<'_> {
    fn contiguous_payload(&self) -> &[u8] {
        self.payload
    }
}

impl ULoanedContiguousZeroCopyRxFrame for BorrowedContiguousFrame<'_> {
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError> {
        // SAFETY:
        // - This test frame deliberately models a transport-backed borrowed
        //   payload, and `self.payload` remains valid for the lifetime of `&self`.
        Ok(unsafe { LoanedPayload::new_unchecked(self.payload, PayloadLoanKind::TransportLoan) })
    }
}

struct CopiedContiguousFrame<'a> {
    metadata: UFrameMetadata,
    payload: &'a [u8],
}

impl UZeroCopyRxFrame for CopiedContiguousFrame<'_> {
    type PayloadReader<'a>
        = Cursor<&'a [u8]>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = std::iter::Once<&'a [u8]>
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload_len(&self) -> usize {
        self.payload.len()
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        Cursor::new(self.payload)
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        std::iter::once(self.payload)
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        Some(self.payload)
    }
}

impl UContiguousZeroCopyRxFrame for CopiedContiguousFrame<'_> {
    fn contiguous_payload(&self) -> &[u8] {
        self.payload
    }
}

impl ULoanedContiguousZeroCopyRxFrame for CopiedContiguousFrame<'_> {
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError> {
        Err(UWireError::NotLoanBacked)
    }
}

#[repr(C, align(4))]
struct AlignedVehiclePoseBytes([u8; mem::size_of::<VehiclePose>()]);

struct VehiclePoseTxBuffer {
    metadata: UFrameMetadata,
    storage: AlignedVehiclePoseBytes,
}

impl UTxBuffer for VehiclePoseTxBuffer {
    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload(&self) -> &[u8] {
        self.storage.0.as_slice()
    }

    fn payload_mut(&mut self) -> &mut [u8] {
        self.storage.0.as_mut_slice()
    }
}

#[derive(Default)]
struct VehiclePoseTransport {
    sent: Mutex<Option<(UFrameMetadata, VehiclePose)>>,
}

#[async_trait::async_trait]
impl UZeroCopyTransport for VehiclePoseTransport {
    type Tx = VehiclePoseTxBuffer;
    type Rx = UOwnedFrame;

    async fn reserve(
        &self,
        metadata: UFrameMetadata,
        payload_len: usize,
        alignment: usize,
    ) -> Result<Self::Tx, UStatus> {
        if payload_len != mem::size_of::<VehiclePose>() {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "unexpected payload length",
            ));
        }
        if alignment > mem::align_of::<AlignedVehiclePoseBytes>() {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "unsupported payload alignment",
            ));
        }
        Ok(VehiclePoseTxBuffer {
            metadata,
            storage: AlignedVehiclePoseBytes([0; mem::size_of::<VehiclePose>()]),
        })
    }

    async fn send_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
        let pose =
            *<StableContainerPayload<VehiclePose> as BorrowPayload<VehiclePose>>::borrow_payload(
                buffer.payload(),
            )
            .map_err(UStatus::from)?;
        self.sent
            .lock()
            .expect("sent lock poisoned")
            .replace((buffer.metadata().clone(), pose));
        Ok(())
    }
}

fn bytes_of_pose(pose: &VehiclePose) -> &[u8] {
    // SAFETY:
    // - `pose` is a valid shared reference to one `VehiclePose` and is therefore
    //   non-null, aligned, and valid for reads of `size_of::<VehiclePose>()`
    //   bytes.
    // - Per https://doc.rust-lang.org/stable/std/slice/fn.from_raw_parts.html#safety:
    //
    //   "data must be non-null, valid for reads for `len * size_of::<T>()` many
    //   bytes, and it must be properly aligned."
    unsafe {
        std::slice::from_raw_parts(
            (pose as *const VehiclePose).cast::<u8>(),
            mem::size_of::<VehiclePose>(),
        )
    }
}

fn stable_container_encoding(
    type_name: &str,
    variant: &str,
    size: usize,
    align: usize,
) -> PayloadEncoding {
    PayloadEncoding::custom(
        StableContainerPayload::<VehiclePose>::ENCODING_ID,
        format!(
            "application/vnd.uprotocol.stable-container;type=\"{type_name}\";variant={variant};size={size};align={align}"
        ),
    )
}

#[test]
fn raw_bytes_serialize_and_deserialize_without_copying_on_read() {
    let input: &[u8] = &[1, 2, 3, 4];

    let payload = input.serialize_owned().unwrap();
    let decoded = <&[u8] as UDeserializer<RawBytes>>::deserialize_from(&payload).unwrap();

    assert_eq!(decoded, input);
}

struct ShortWriteSerializer;

impl USerializer<RawBytes> for ShortWriteSerializer {
    fn encoded_len(&self) -> usize {
        2
    }

    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        let actual = dst.len();
        let out = dst
            .get_mut(..1)
            .ok_or_else(|| UWireError::buffer_too_small(1, actual))?;
        *out.first_mut()
            .ok_or_else(|| UWireError::buffer_too_small(1, actual))? = 0x01;
        Ok(1)
    }
}

#[test]
fn owned_serializer_rejects_mismatched_written_length() {
    let error = ShortWriteSerializer.serialize_owned().unwrap_err();

    assert!(
        matches!(error, UWireError::InvalidPayload(message) if message.contains("encoded_len returned 2"))
    );
}

#[test]
fn owned_frame_distinguishes_absent_payload_from_empty_payload() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let absent = UFrameBuilder::publish(topic.clone()).build().unwrap();
    let empty = UFrameBuilder::publish(topic)
        .build_with_raw_payload(Vec::<u8>::new())
        .unwrap();

    assert!(!absent.has_payload());
    assert_eq!(absent.metadata().encoding(), None);
    assert_eq!(absent.payload(), None);
    assert_eq!(absent.payload_bytes(), b"");
    assert!(empty.has_payload());
    assert_eq!(empty.metadata().encoding(), Some(&RawBytes::encoding()));
    assert_eq!(empty.payload_bytes(), b"");
}

#[test]
fn owned_frame_deserialize_rejects_absent_payload() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UFrameBuilder::publish(topic).build().unwrap();

    assert!(matches!(
        frame.deserialize::<RawBytes, &[u8]>(),
        Err(UWireError::MissingPayload)
    ));
}

#[test]
fn encoding_rejects_invalid_content_type() {
    let error = PayloadEncoding::try_custom("json", "not a media type").unwrap_err();

    assert!(matches!(error, PayloadEncodingError::InvalidContentType(_)));
}

#[test]
fn standard_encoding_maps_known_content_type() {
    assert_eq!(
        PayloadEncoding::from_content_type("application/json; charset=utf-8"),
        PayloadEncoding::standard(UPayloadFormat::Json)
    );
}

#[test]
fn owned_frame_uses_selected_payload_codec() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UOwnedFrame::from_serializable::<RawBytes, _>(
        UFrameMetadata::publish(topic),
        &&[0x0a_u8, 0x0b_u8][..],
    )
    .unwrap();

    assert_eq!(frame.metadata().encoding(), Some(&RawBytes::encoding()));
    assert_eq!(frame.payload_bytes(), &[0x0a_u8, 0x0b_u8]);
}

#[test]
fn owned_frame_uses_payload_codec_adapter_for_raw_bytes() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UOwnedFrame::from_payload_as::<RawBytes, [u8]>(
        UFrameMetadata::publish(topic),
        &[0x0a_u8, 0x0b_u8],
    )
    .unwrap();

    let decoded = frame.decode_payload_as::<RawBytes, &[u8]>().unwrap();
    assert_eq!(frame.metadata().encoding(), Some(&RawBytes::encoding()));
    assert_eq!(decoded, &[0x0a_u8, 0x0b_u8]);
}

#[test]
fn mcap_payload_borrows_archive_bytes() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let archive: &[u8] = b"\x89MCAP\r\nfixture";
    let frame =
        UOwnedFrame::from_payload_as::<McapPayload, [u8]>(UFrameMetadata::publish(topic), archive)
            .unwrap();

    let borrowed = frame.borrow_payload_as::<McapPayload, [u8]>().unwrap();
    assert_eq!(frame.metadata().encoding(), Some(&McapPayload::encoding()));
    assert_eq!(borrowed, archive);
}

#[test]
fn stable_container_borrow_rejects_wrong_encoding() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let pose = VehiclePose { x: 10, y: 20 };
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(topic).with_encoding(RawBytes::encoding()),
        bytes_of_pose(&pose).to_vec(),
    );

    let error = frame
        .borrow_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>()
        .unwrap_err();

    assert!(matches!(error, UWireError::UnsupportedEncoding { .. }));
}

#[test]
fn stable_container_borrow_rejects_wrong_stable_type_name() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let pose = VehiclePose { x: 10, y: 20 };
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(topic).with_encoding(stable_container_encoding(
            "example.vehicle.OtherPose",
            "fixed",
            mem::size_of::<VehiclePose>(),
            mem::align_of::<VehiclePose>(),
        )),
        bytes_of_pose(&pose).to_vec(),
    );

    let error = frame
        .borrow_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>()
        .unwrap_err();

    assert!(matches!(
        error,
        UWireError::IncompatibleStablePayload { expected, actual }
            if expected.contains("type=example.vehicle.VehiclePose") && actual.contains("OtherPose")
    ));
}

#[test]
fn stable_container_borrow_rejects_wrong_variant() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let pose = VehiclePose { x: 10, y: 20 };
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(topic).with_encoding(stable_container_encoding(
            "example.vehicle.VehiclePose",
            "dynamic",
            mem::size_of::<VehiclePose>(),
            mem::align_of::<VehiclePose>(),
        )),
        bytes_of_pose(&pose).to_vec(),
    );

    let error = frame
        .borrow_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>()
        .unwrap_err();

    assert!(matches!(
        error,
        UWireError::IncompatibleStablePayload { expected, actual }
            if expected.contains("variant=fixed") && actual.contains("variant parameter")
    ));
}

#[test]
fn stable_container_borrow_rejects_wrong_advertised_size() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let pose = VehiclePose { x: 10, y: 20 };
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(topic).with_encoding(stable_container_encoding(
            "example.vehicle.VehiclePose",
            "fixed",
            mem::size_of::<VehiclePose>() - 1,
            mem::align_of::<VehiclePose>(),
        )),
        bytes_of_pose(&pose).to_vec(),
    );

    let error = frame
        .borrow_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>()
        .unwrap_err();

    assert!(matches!(
        error,
        UWireError::IncompatibleStablePayload { expected, actual }
            if expected.contains("size=8") && actual.contains("size parameter")
    ));
}

#[test]
fn stable_container_borrow_rejects_insufficient_advertised_alignment() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let pose = VehiclePose { x: 10, y: 20 };
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(topic).with_encoding(stable_container_encoding(
            "example.vehicle.VehiclePose",
            "fixed",
            mem::size_of::<VehiclePose>(),
            mem::align_of::<VehiclePose>() - 1,
        )),
        bytes_of_pose(&pose).to_vec(),
    );

    let error = frame
        .borrow_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>()
        .unwrap_err();

    assert!(matches!(
        error,
        UWireError::IncompatibleStablePayload { expected, actual }
            if expected.contains("align=4") && actual.contains("align parameter")
    ));
}

#[test]
fn stable_container_borrow_rejects_wrong_payload_length() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(topic)
            .with_encoding(StableContainerPayload::<VehiclePose>::encoding()),
        vec![0_u8; mem::size_of::<VehiclePose>() - 1],
    );

    let error = frame
        .borrow_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>()
        .unwrap_err();

    assert!(matches!(
        error,
        UWireError::InvalidPayloadLength { expected, actual }
            if expected == mem::size_of::<VehiclePose>() && actual == mem::size_of::<VehiclePose>() - 1
    ));
}

#[test]
fn stable_container_borrow_rejects_wrong_alignment() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let align = mem::align_of::<VehiclePose>();
    let storage = vec![0_u8; mem::size_of::<VehiclePose>() + align];
    let base = storage.as_ptr() as usize;
    let offset = (1..align)
        .find(|offset| !(base + offset).is_multiple_of(align))
        .unwrap();
    let payload = storage
        .get(offset..offset + mem::size_of::<VehiclePose>())
        .unwrap();
    let frame = BorrowedContiguousFrame {
        metadata: UFrameMetadata::publish(topic)
            .with_encoding(StableContainerPayload::<VehiclePose>::encoding()),
        payload,
    };

    let error = frame
        .borrow_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>()
        .unwrap_err();

    assert!(matches!(
        error,
        UWireError::InvalidPayloadAlignment { expected, .. } if expected == mem::align_of::<VehiclePose>()
    ));
}

#[test]
fn stable_container_borrows_typed_payload() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let pose = VehiclePose { x: 10, y: 20 };
    let frame = BorrowedContiguousFrame {
        metadata: UFrameMetadata::publish(topic)
            .with_encoding(StableContainerPayload::<VehiclePose>::encoding()),
        payload: bytes_of_pose(&pose),
    };

    let borrowed = frame
        .borrow_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>()
        .unwrap();

    assert_eq!(borrowed, &pose);
}

#[test]
fn stable_container_borrows_broad_padded_payload_from_initialized_bytes() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    #[repr(C, align(4))]
    struct AlignedPaddedPoseBytes([u8; mem::size_of::<PaddedPose>()]);
    let mut storage = AlignedPaddedPoseBytes([0; mem::size_of::<PaddedPose>()]);
    storage.0[0] = 1;
    storage.0[4..8].copy_from_slice(&2_u32.to_ne_bytes());
    let frame = BorrowedContiguousFrame {
        metadata: UFrameMetadata::publish(topic)
            .with_encoding(StableContainerPayload::<PaddedPose>::encoding()),
        payload: storage.0.as_slice(),
    };

    let borrowed = frame
        .borrow_payload_as::<StableContainerPayload<PaddedPose>, PaddedPose>()
        .unwrap();

    assert_eq!(borrowed.small, 1);
    assert_eq!(borrowed.large, 2);
}

#[test]
fn stable_container_borrow_accepts_larger_advertised_alignment() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let pose = VehiclePose { x: 10, y: 20 };
    let frame = BorrowedContiguousFrame {
        metadata: UFrameMetadata::publish(topic).with_encoding(stable_container_encoding(
            "example.vehicle.VehiclePose",
            "fixed",
            mem::size_of::<VehiclePose>(),
            mem::align_of::<VehiclePose>() * 2,
        )),
        payload: bytes_of_pose(&pose),
    };

    let borrowed = frame
        .borrow_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>()
        .unwrap();

    assert_eq!(borrowed, &pose);
}

#[test]
fn stable_container_borrows_typed_payload_from_loaned_rx() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let pose = VehiclePose { x: 10, y: 20 };
    let frame = BorrowedContiguousFrame {
        metadata: UFrameMetadata::publish(topic)
            .with_encoding(StableContainerPayload::<VehiclePose>::encoding()),
        payload: bytes_of_pose(&pose),
    };

    verify_loaned_rx_payload_layout(
        &frame,
        mem::size_of::<VehiclePose>(),
        mem::align_of::<VehiclePose>(),
    )
    .unwrap();
    let borrowed = frame
        .borrow_loaned_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>()
        .unwrap();

    assert_eq!(
        frame.payload_loan_kind().unwrap(),
        PayloadLoanKind::TransportLoan
    );
    assert_eq!(borrowed, &pose);
}

#[test]
fn stable_payload_type_detail_is_used_in_encoding() {
    let detail = VehiclePose::stable_type_detail();

    assert_eq!(detail.variant, StablePayloadVariant::FixedSize);
    assert_eq!(detail.type_name, "example.vehicle.VehiclePose");
    assert_eq!(detail.size, mem::size_of::<VehiclePose>());
    assert_eq!(detail.alignment, mem::align_of::<VehiclePose>());
    assert_eq!(
        StableContainerPayload::<VehiclePose>::encoding().content_type(),
        Some("application/vnd.uprotocol.stable-container;type=\"example.vehicle.VehiclePose\";variant=fixed;size=8;align=4")
    );
}

#[test]
fn derived_byte_backed_stable_payload_impl_is_available() {
    assert_stable_payload_byte_backed_uninit::<VehiclePose>();
    assert_stable_payload_byte_backed_uninit::<NonCopyPose>();
}

#[test]
fn zero_copy_conformance_helpers_cover_stable_container_failures() {
    zero_copy_conformance::verify_stable_container_encoding::<VehiclePose>().unwrap();
    zero_copy_conformance::verify_stable_container_rejects_wrong_type_name::<VehiclePose>(
        "example.vehicle.OtherPose",
    )
    .unwrap();
    zero_copy_conformance::verify_stable_container_rejects_wrong_variant::<VehiclePose>("dynamic")
        .unwrap();
    zero_copy_conformance::verify_stable_container_rejects_wrong_size::<VehiclePose>().unwrap();
    zero_copy_conformance::verify_stable_container_rejects_insufficient_alignment::<VehiclePose>()
        .unwrap();
    zero_copy_conformance::verify_stable_container_rejects_actual_misalignment::<VehiclePose>()
        .unwrap();
}

#[test]
fn zero_copy_conformance_helper_rejects_copied_fallback_as_loaned_rx() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let pose = VehiclePose { x: 10, y: 20 };
    let frame = CopiedContiguousFrame {
        metadata: UFrameMetadata::publish(topic)
            .with_encoding(StableContainerPayload::<VehiclePose>::encoding()),
        payload: bytes_of_pose(&pose),
    };

    assert_eq!(
        frame
            .borrow_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>()
            .unwrap(),
        &pose
    );
    zero_copy_conformance::verify_loaned_rx_rejects_copied_fallback_as::<
        StableContainerPayload<VehiclePose>,
        VehiclePose,
    >(&frame)
    .unwrap();
}

#[test]
fn stable_container_owned_frame_encodes_payload_bytes() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let pose = VehiclePose { x: 10, y: 20 };
    let frame = UOwnedFrame::from_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>(
        UFrameMetadata::publish(topic),
        &pose,
    )
    .unwrap();

    assert_eq!(
        frame.metadata().encoding(),
        Some(&StableContainerPayload::<VehiclePose>::encoding())
    );
    assert_eq!(frame.payload_bytes(), bytes_of_pose(&pose));
}

#[test]
fn owned_frame_from_bytes_as_moves_bytes_without_payload_copy() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let payload = bytes::Bytes::from_static(b"\x89MCAP\r\nfixture");
    let payload_ptr = payload.as_ptr();

    let frame = UOwnedFrame::from_bytes_as::<McapPayload>(UFrameMetadata::publish(topic), payload);

    assert_eq!(frame.metadata().encoding(), Some(&McapPayload::encoding()));
    assert_eq!(frame.payload().unwrap().as_ptr(), payload_ptr);
}

#[test]
fn owned_frame_from_encoded_payload_moves_bytes_without_payload_copy() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let payload = bytes::Bytes::from_static(b"payload");
    let payload_ptr = payload.as_ptr();
    let encoded = EncodedPayload::<RawBytes>::from_bytes(payload);

    let frame = UOwnedFrame::from_encoded_payload(UFrameMetadata::publish(topic), encoded);
    let (metadata, payload) = frame.into_parts();

    assert_eq!(metadata.encoding(), Some(&RawBytes::encoding()));
    assert_eq!(payload.unwrap().as_ptr(), payload_ptr);
}

#[test]
fn zero_copy_rx_decodes_payload_from_reader_as_new_codec_api() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = BorrowedContiguousFrame {
        metadata: UFrameMetadata::publish(topic).with_encoding(RawBytes::encoding()),
        payload: b"payload",
    };

    let decoded = frame
        .decode_payload_from_reader_as::<RawBytes, Vec<u8>>()
        .unwrap();

    assert_eq!(decoded, b"payload");
}

#[test]
fn dynamic_payload_codec_registry_encodes_and_decodes_owned_values() {
    let mut registry = PayloadCodecRegistry::new();
    registry.register::<RawBytes, Vec<u8>>();
    let input = b"payload".to_vec();

    let capabilities = registry.capabilities(&RawBytes::encoding()).unwrap();
    assert!(capabilities.encode_owned);
    assert!(capabilities.decode_owned);

    let encoded = registry.encode_as(&RawBytes::encoding(), &input).unwrap();
    let decoded: Vec<u8> = registry.decode_as(&RawBytes::encoding(), &encoded).unwrap();

    assert_eq!(decoded, input);
}

#[test]
fn tx_buffer_layout_conformance_checks_length_and_alignment() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let mut buffer = UVecTxBuffer::with_alignment(
        UFrameMetadata::publish(topic).with_encoding(RawBytes::encoding()),
        8,
        4,
    )
    .unwrap();

    verify_tx_buffer_payload_layout(&mut buffer, 8, 4).unwrap();
    let mut loaned = buffer.loaned_payload_mut();
    assert_eq!(loaned.kind(), PayloadLoanKind::TransportLoan);
    *loaned.as_mut_bytes().first_mut().unwrap() = 0x42;
    assert_eq!(buffer.payload().first(), Some(&0x42));
}

#[test]
fn uninit_tx_buffer_layout_conformance_checks_length_and_alignment() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let mut buffer = UVecUninitTxBuffer::with_alignment(
        UFrameMetadata::publish(topic).with_encoding(RawBytes::encoding()),
        8,
        4,
    )
    .unwrap();

    verify_uninit_tx_buffer_payload_layout(&mut buffer, 8, 4).unwrap();
    let writer = buffer.payload_uninit_mut().into_writer();
    assert_eq!(writer.len(), 8);
    assert_eq!(writer.remaining(), 8);
}

#[tokio::test]
async fn stable_container_send_loaned_payload_initializes_and_sends() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let transport = VehiclePoseTransport::default();

    transport
        .send_loaned_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>(
            UFrameMetadata::publish(topic),
            |payload| {
                payload.x = 7;
                payload.y = 9;
            },
        )
        .await
        .unwrap();

    let (metadata, pose) = transport
        .sent
        .lock()
        .expect("sent lock poisoned")
        .clone()
        .unwrap();
    assert_eq!(
        metadata.encoding(),
        Some(&StableContainerPayload::<VehiclePose>::encoding())
    );
    assert_eq!(pose, VehiclePose { x: 7, y: 9 });
}

#[tokio::test]
async fn stable_container_send_uninit_payload_initializes_and_sends_non_copy() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let transport = InMemoryZeroCopyTransport::default();

    transport
        .send_uninit_loaned_payload_as::<StableContainerPayload<NonCopyPose>, NonCopyPose>(
            UFrameMetadata::publish(topic),
            |slot| {
                Ok(slot.write(NonCopyPose {
                    x: 7,
                    y: 9,
                    marker: NoCopyMarker { value: 1 },
                    _pad: [0; 3],
                }))
            },
        )
        .await
        .unwrap();

    let frame = transport.sent_frames().pop().unwrap();
    assert_eq!(
        frame.metadata().encoding(),
        Some(&StableContainerPayload::<NonCopyPose>::encoding())
    );
    let pose = <StableContainerPayload<NonCopyPose> as BorrowPayload<NonCopyPose>>::borrow_payload(
        frame.payload_bytes(),
    )
    .unwrap();
    assert_eq!(
        pose,
        &NonCopyPose {
            x: 7,
            y: 9,
            marker: NoCopyMarker { value: 1 },
            _pad: [0; 3],
        }
    );
}

#[cfg(any(
    feature = "unsafe-stable-payload-init",
    feature = "expert-unsafe-payloads"
))]
#[tokio::test]
async fn stable_container_send_uninit_payload_supports_unsafe_field_initialization() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let transport = InMemoryZeroCopyTransport::default();

    transport
        .send_uninit_loaned_payload_as::<StableContainerPayload<NonCopyPose>, NonCopyPose>(
            UFrameMetadata::publish(topic),
            |mut slot| {
                // SAFETY: This feature-gated test writes every field of
                // `NonCopyPose`, including explicit padding, before commit.
                let ptr = unsafe { slot.as_mut_ptr() };
                // SAFETY: `ptr` came from the loaned slot and points to enough
                // storage for `NonCopyPose`; this only forms a raw field pointer.
                let x = unsafe { std::ptr::addr_of_mut!((*ptr).x) };
                // SAFETY: Same slot/provenance proof as for `x` above.
                let y = unsafe { std::ptr::addr_of_mut!((*ptr).y) };
                // SAFETY: Same slot/provenance proof as for `x` above.
                let marker = unsafe { std::ptr::addr_of_mut!((*ptr).marker) };
                // SAFETY: Same slot/provenance proof as for `x` above.
                let pad = unsafe { std::ptr::addr_of_mut!((*ptr)._pad) };
                // SAFETY: `x` points to the uninitialized `x` field and is
                // written exactly once before `assume_init`.
                unsafe { x.write(11) };
                // SAFETY: `y` points to the uninitialized `y` field and is
                // written exactly once before `assume_init`.
                unsafe { y.write(13) };
                // SAFETY: `marker` points to the uninitialized marker field and
                // is written exactly once before `assume_init`.
                unsafe { marker.write(NoCopyMarker { value: 2 }) };
                // SAFETY: `_pad` is explicit test padding and is initialized
                // before the stable payload is committed.
                unsafe { pad.write([0; 3]) };
                // SAFETY: All fields, including explicit padding, have been
                // initialized and `NonCopyPose` has no implicit padding.
                Ok(unsafe { slot.assume_init() })
            },
        )
        .await
        .unwrap();

    let frame = transport.sent_frames().pop().unwrap();
    let pose = <StableContainerPayload<NonCopyPose> as BorrowPayload<NonCopyPose>>::borrow_payload(
        frame.payload_bytes(),
    )
    .unwrap();
    assert_eq!(pose.x, 11);
    assert_eq!(pose.y, 13);
    assert_eq!(pose.marker.value, 2);
}

#[cfg(feature = "expert-unsafe-payloads")]
#[tokio::test]
async fn stable_container_unsafe_tx_hatch_sends_explicitly_initialized_padded_payload() {
    fn init_padded_pose<'payload>(
        slot: UnsafeStablePayloadTxSlot<'payload, PaddedPose>,
    ) -> Result<LoanedInitPayload<'payload, PaddedPose>, UWireError> {
        let mut slot = slot.zeroed();
        // SAFETY: `zeroed()` initialized all transported bytes; the raw pointer
        // is used only for field writes before commit.
        let ptr = unsafe { slot.as_mut_ptr() };
        // SAFETY: `ptr` came from the loaned slot and points to enough storage
        // for `PaddedPose`; this only forms a raw field pointer.
        let small = unsafe { std::ptr::addr_of_mut!((*ptr).small) };
        // SAFETY: Same slot/provenance proof as for `small` above.
        let large = unsafe { std::ptr::addr_of_mut!((*ptr).large) };
        // SAFETY: `small` points to the uninitialized `small` field and is
        // written before commit.
        unsafe { small.write(1) };
        // SAFETY: `large` points to the uninitialized `large` field and is
        // written before commit.
        unsafe { large.write(2) };
        // SAFETY: `zeroed()` initialized padding and both semantic fields have
        // been written with valid values.
        Ok(unsafe { slot.assume_init() })
    }

    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let transport = InMemoryZeroCopyTransport::default();

    // SAFETY: The test closure zero-initializes the full transported byte range,
    // writes all semantic fields, and only then returns an initialized marker.
    let send = unsafe {
        transport.send_uninit_stable_payload_unchecked::<PaddedPose>(
            UFrameMetadata::publish(topic),
            init_padded_pose,
        )
    };
    send.await.unwrap();

    let frame = transport.sent_frames().pop().unwrap();
    let pose = <StableContainerPayload<PaddedPose> as BorrowPayload<PaddedPose>>::borrow_payload(
        frame.payload_bytes(),
    )
    .unwrap();
    assert_eq!(pose.small, 1);
    assert_eq!(pose.large, 2);
}

#[test]
fn manual_byte_backed_stable_payload_impl_is_available_without_feature() {
    assert_stable_payload_byte_backed_uninit::<ManualByteBackedPose>();
}

#[tokio::test]
async fn stable_container_send_uninit_payload_failed_init_does_not_send() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let transport = InMemoryZeroCopyTransport::default();

    let error = transport
        .send_uninit_loaned_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>(
            UFrameMetadata::publish(topic),
            |_slot| Err(UWireError::invalid_payload("init failed")),
        )
        .await
        .expect_err("failed initialization must drop loan without send");

    assert_eq!(error.get_code(), UCode::INVALID_ARGUMENT);
    assert!(transport.sent_frames().is_empty());
}

#[test]
fn aligned_uvec_uninit_tx_buffer_initializes_hidden_padding_before_conversion() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let mut buffer = UVecUninitTxBuffer::with_alignment(
        UFrameMetadata::publish(topic).with_encoding(RawBytes::encoding()),
        3,
        4096,
    )
    .unwrap();
    let mut writer = buffer.payload_uninit_mut().into_writer();
    writer.write_all(b"abc").unwrap();
    let _initialized = writer.finish().unwrap();

    // SAFETY: The byte writer initialized exactly the visible payload range;
    // `UVecUninitTxBuffer` initializes hidden prefix/suffix bytes before commit.
    let buffer = unsafe { buffer.assume_payload_init() };

    assert_eq!(buffer.payload(), b"abc");
}

#[tokio::test]
async fn direct_uninit_byte_writer_requires_exact_length() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let transport = InMemoryZeroCopyTransport::default();

    transport
        .send_uninit_loaned_bytes_as::<RawBytes>(
            UFrameMetadata::publish(topic.clone()),
            3,
            1,
            |mut writer| {
                writer.write_all(b"abc")?;
                Ok(writer)
            },
        )
        .await
        .unwrap();
    assert_eq!(
        transport
            .sent_frames()
            .first()
            .expect("one frame should be sent")
            .payload_bytes(),
        b"abc"
    );

    let error = transport
        .send_uninit_loaned_bytes_as::<RawBytes>(
            UFrameMetadata::publish(topic),
            3,
            1,
            |mut writer| {
                writer.write_all(b"ab")?;
                Ok(writer)
            },
        )
        .await
        .expect_err("under-initialized writer should fail");
    assert_eq!(error.get_code(), UCode::INVALID_ARGUMENT);
    assert_eq!(transport.sent_frames().len(), 1);
}

struct OtherPayload;

impl PayloadFormat for OtherPayload {
    fn name() -> &'static str {
        "other"
    }

    fn encoding() -> PayloadEncoding {
        PayloadEncoding::custom(Self::name(), "application/x-other")
    }
}

impl<'a> UDeserializer<'a, OtherPayload> for &'a [u8] {
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
        Ok(src)
    }
}

struct NativeBytesPayload;

impl PayloadFormat for NativeBytesPayload {
    fn name() -> &'static str {
        "native-bytes-v1"
    }

    fn encoding() -> PayloadEncoding {
        PayloadEncoding::custom(Self::name(), "application/vnd.example.native-bytes")
    }
}

impl<'a> UDeserializer<'a, NativeBytesPayload> for &'a [u8] {
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
        Ok(src)
    }
}

struct OtherNativeBytesPayload;

impl PayloadFormat for OtherNativeBytesPayload {
    fn name() -> &'static str {
        "other-native-bytes-v1"
    }

    fn encoding() -> PayloadEncoding {
        PayloadEncoding::custom(Self::name(), "application/vnd.example.native-bytes")
    }
}

impl<'a> UDeserializer<'a, OtherNativeBytesPayload> for &'a [u8] {
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
        Ok(src)
    }
}

#[test]
fn owned_frame_deserialize_rejects_wrong_payload_codec() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UOwnedFrame::from_serializable::<RawBytes, _>(
        UFrameMetadata::publish(topic),
        &&[0x0a_u8, 0x0b_u8][..],
    )
    .unwrap();

    assert!(matches!(
        frame.deserialize::<OtherPayload, &[u8]>(),
        Err(UWireError::UnsupportedEncoding { .. })
    ));
}

#[test]
fn owned_frame_deserialize_accepts_matching_custom_encoding() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(topic).with_encoding(NativeBytesPayload::encoding()),
        vec![0x0a_u8, 0x0b_u8],
    );

    assert_eq!(
        frame.deserialize::<NativeBytesPayload, &[u8]>().unwrap(),
        &[0x0a_u8, 0x0b_u8]
    );
}

#[test]
fn owned_frame_deserialize_rejects_wrong_custom_encoding() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(topic).with_encoding(NativeBytesPayload::encoding()),
        vec![0x0a_u8, 0x0b_u8],
    );

    assert!(matches!(
        frame.deserialize::<OtherNativeBytesPayload, &[u8]>(),
        Err(UWireError::UnsupportedEncoding { .. })
    ));
}

#[test]
fn owned_frame_deserialize_rejects_custom_encoding_for_standard_decoder() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(topic).with_encoding(NativeBytesPayload::encoding()),
        vec![0x0a_u8, 0x0b_u8],
    );

    assert!(matches!(
        frame.deserialize::<RawBytes, &[u8]>(),
        Err(UWireError::UnsupportedEncoding { .. })
    ));
}

#[test]
fn frame_builder_builds_publish_frame_with_raw_payload() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let message_id = UUID::build();
    let traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let frame = UFrameBuilder::publish(topic.clone())
        .with_message_id(message_id.clone())
        .with_priority(UPriority::CS2)
        .with_ttl(5_000)
        .with_traceparent(traceparent)
        .build_with_raw_payload(vec![0x01, 0x02])
        .unwrap();

    let attributes = frame.metadata().attributes();
    assert_eq!(attributes.id(), &message_id);
    assert_eq!(attributes.message_type(), UMessageType::Publish);
    assert_eq!(attributes.priority(), UPriority::CS2);
    assert_eq!(attributes.source(), &topic);
    assert_eq!(attributes.sink(), None);
    assert_eq!(attributes.ttl(), Some(5_000));
    assert_eq!(attributes.traceparent(), Some(traceparent));
    assert_eq!(frame.metadata().encoding(), Some(&RawBytes::encoding()));
    assert_eq!(frame.payload_bytes(), &[0x01, 0x02]);
}

#[test]
fn frame_builder_builds_response_from_request_attributes() {
    let method = UUri::try_from("//vehicle/4210/1/0001").unwrap();
    let reply_to = UUri::try_from("//client/ABCD/1/0000").unwrap();
    let request = UFrameBuilder::request(method.clone(), reply_to.clone(), 5_000)
        .with_priority(UPriority::CS5)
        .build()
        .unwrap();
    let response_id = UUID::build();
    let response = UFrameBuilder::response_for_request(request.metadata().attributes())
        .with_message_id(response_id.clone())
        .with_comm_status(UCode::DEADLINE_EXCEEDED)
        .build()
        .unwrap();

    let attributes = response.metadata().attributes();
    assert_eq!(attributes.id(), &response_id);
    assert_eq!(attributes.message_type(), UMessageType::Response);
    assert_eq!(attributes.priority(), UPriority::CS5);
    assert_eq!(attributes.source(), &method);
    assert_eq!(attributes.sink(), Some(&reply_to));
    assert_eq!(
        attributes.request_id(),
        Some(request.metadata().attributes().id())
    );
    assert_eq!(attributes.commstatus(), Some(UCode::DEADLINE_EXCEEDED));
    assert_eq!(attributes.ttl(), Some(5_000));
}

#[test]
fn frame_builder_rejects_low_rpc_priority() {
    let method = UUri::try_from("//vehicle/4210/1/0001").unwrap();
    let reply_to = UUri::try_from("//client/ABCD/1/0000").unwrap();
    let result = UFrameBuilder::request(method, reply_to, 5_000)
        .with_priority(UPriority::CS3)
        .build();

    assert!(matches!(
        result,
        Err(UFrameBuilderError::AttributesValidationError(_))
    ));
}

#[test]
fn frame_builder_uses_selected_payload_codec_for_typed_payload() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UFrameBuilder::publish(topic)
        .build_with_serializable::<RawBytes, _>(&&[0x0a_u8, 0x0b_u8][..])
        .unwrap();

    assert_eq!(frame.metadata().encoding(), Some(&RawBytes::encoding()));
    assert_eq!(frame.payload_bytes(), &[0x0a_u8, 0x0b_u8]);
}
