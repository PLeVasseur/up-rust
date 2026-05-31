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

use std::{collections::VecDeque, io::Cursor, mem, sync::Mutex};

use async_trait::async_trait;
use up_rust::{
    payload::{PlacementDefault, StableContainerPayload},
    transport::ValidatedTxLoanSpec,
    zero_copy::{
        LoanedPayload, PayloadLoanProvenance, UFrameView, ULoanedContiguousZeroCopyRxFrame,
        UVecTxBuffer, UVecUninitTxBuffer, UZeroCopyRxLease, UZeroCopyTransport,
        UZeroCopyTransportExt, UZeroCopyTransportImpl, UZeroCopyUninitTransportExt,
        UZeroCopyUninitTransportImpl,
    },
    ByteBackedStablePayload, UFrameMetadata, UOwnedFrame, UStatus, UUri,
};

struct LoanBackedRxLease {
    frame: UOwnedFrame,
}

impl UFrameView for LoanBackedRxLease {
    type PayloadReader<'a>
        = Cursor<&'a [u8]>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = std::iter::Once<&'a [u8]>
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata {
        self.frame.metadata()
    }

    fn payload_len(&self) -> usize {
        self.frame.payload_bytes().len()
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        Cursor::new(self.frame.payload_bytes())
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        std::iter::once(self.frame.payload_bytes())
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        Some(self.frame.payload_bytes())
    }
}

impl UZeroCopyRxLease for LoanBackedRxLease {}

impl ULoanedContiguousZeroCopyRxFrame for LoanBackedRxLease {
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, up_rust::UWireError> {
        // SAFETY: This test lease owns the frame bytes for `&self` and exposes
        // them as the exact visible loan-backed payload range.
        Ok(unsafe {
            LoanedPayload::new_unchecked(
                self.frame.payload_bytes(),
                PayloadLoanProvenance::OpaqueTransportLoan,
            )
        })
    }
}

#[derive(Default)]
struct RecordingUninitTransport {
    sent: Mutex<Vec<UOwnedFrame>>,
    queue: Mutex<VecDeque<UOwnedFrame>>,
}

impl RecordingUninitTransport {
    fn sent_len(&self) -> usize {
        self.sent.lock().expect("sent lock poisoned").len()
    }
}

#[async_trait]
impl UZeroCopyTransportImpl for RecordingUninitTransport {
    type Tx = UVecTxBuffer;
    type Rx = LoanBackedRxLease;

    async fn loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus> {
        UVecTxBuffer::with_alignment(
            spec.metadata().clone(),
            spec.payload_len(),
            spec.payload_alignment(),
        )
        .map_err(UStatus::from)
    }

    async fn send_validated_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
        let frame = buffer.into_frame();
        self.sent
            .lock()
            .expect("sent lock poisoned")
            .push(frame.clone());
        self.queue
            .lock()
            .expect("queue lock poisoned")
            .push_back(frame);
        Ok(())
    }

    async fn receive_validated_zero_copy(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus> {
        self.queue
            .lock()
            .expect("queue lock poisoned")
            .pop_front()
            .map(|frame| LoanBackedRxLease { frame })
            .ok_or_else(|| up_rust::UStatus::fail_with_code(up_rust::UCode::NOT_FOUND, "empty"))
    }
}

#[async_trait]
impl UZeroCopyUninitTransportImpl for RecordingUninitTransport {
    type UninitTx = UVecUninitTxBuffer;

    async fn loan_validated_uninit_tx(
        &self,
        spec: ValidatedTxLoanSpec,
    ) -> Result<Self::UninitTx, UStatus> {
        UVecUninitTxBuffer::with_alignment(
            spec.metadata().clone(),
            spec.payload_len(),
            spec.payload_alignment(),
        )
        .map_err(UStatus::from)
    }
}

fn topic(resource_id: u16) -> UUri {
    UUri::try_from_parts("vehicle", 0x4210, 1, resource_id).expect("valid test topic")
}

fn metadata(resource_id: u16) -> UFrameMetadata {
    UFrameMetadata::publish_unchecked(topic(resource_id))
}

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    PlacementDefault,
    up_rust::StablePayload,
    ByteBackedStablePayload,
    up_rust::StablePayloadInit,
)]
#[stable_payload(type_name = "example.init.SensorHeader")]
struct SensorHeader {
    case_id: u32,
    sequence: u32,
    logical_payload_len: u32,
}

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    PlacementDefault,
    up_rust::StablePayload,
    ByteBackedStablePayload,
    up_rust::StablePayloadInit,
)]
#[stable_payload(type_name = "example.init.SensorFrame")]
struct SensorFrame {
    header: SensorHeader,
    checksum: u32,
    payload: [u8; 4096],
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, up_rust::StablePayload, up_rust::StablePayloadInit)]
#[stable_payload(type_name = "example.init.PaddedRuntime")]
struct PaddedRuntime {
    small: u8,
    large: u32,
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, up_rust::StablePayload, up_rust::StablePayloadInit)]
#[stable_payload(type_name = "example.init.TypedArrayRuntime")]
struct TypedArrayRuntime {
    values: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, up_rust::StablePayload, up_rust::StablePayloadInit)]
#[stable_payload(type_name = "example.init.NestedRuntime")]
struct NestedRuntime {
    header: SensorHeader,
    checksum: u32,
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, up_rust::StablePayload, up_rust::StablePayloadInit)]
#[stable_payload(type_name = "example.init.LargeRuntime")]
struct LargeRuntime {
    header: SensorHeader,
    checksum: u32,
    payload: [u8; 65_536],
}

#[tokio::test]
async fn no_zero_stable_init_round_trips_and_borrows() {
    let transport = RecordingUninitTransport::default();

    transport
        .send_uninit_stable_payload_as::<SensorFrame>(metadata(0x9000), |frame| {
            frame
                .header(|header| {
                    header
                        .logical_payload_len(4096)
                        .case_id(7)
                        .sequence(11)
                        .finish()
                })?
                .checksum(0xfeed_beef)
                .payload_fill(0x5a)
                .finish()
        })
        .await
        .expect("send succeeds");

    let rx = transport
        .receive_zero_copy(&topic(0x9000), None)
        .await
        .expect("receive succeeds");
    let frame = rx
        .borrow_stable_payload::<SensorFrame>()
        .expect("stable borrow succeeds");

    assert_eq!(frame.header.case_id, 7);
    assert_eq!(frame.header.sequence, 11);
    assert_eq!(frame.header.logical_payload_len, 4096);
    assert_eq!(frame.checksum, 0xfeed_beef);
    assert_eq!(frame.payload.first().copied(), Some(0x5a));
    assert_eq!(frame.payload.last().copied(), Some(0x5a));
}

#[tokio::test]
async fn slice_length_mismatch_does_not_commit() {
    let transport = RecordingUninitTransport::default();

    let err = transport
        .send_uninit_stable_payload_as::<SensorFrame>(metadata(0x9001), |frame| {
            frame
                .header(|header| {
                    header
                        .case_id(1)
                        .sequence(2)
                        .logical_payload_len(4096)
                        .finish()
                })?
                .checksum(3)
                .payload_from_slice(&[0x5a; 3])?
                .finish()
        })
        .await
        .expect_err("wrong slice length fails");

    assert_eq!(err.get_code(), up_rust::UCode::INVALID_ARGUMENT);
    assert_eq!(transport.sent_len(), 0);
}

#[tokio::test]
async fn padded_payload_initializes_padding_gap() {
    let transport = RecordingUninitTransport::default();

    transport
        .send_uninit_stable_payload_as::<PaddedRuntime>(metadata(0x9002), |payload| {
            payload.large(0x1122_3344).small(0xaa).finish()
        })
        .await
        .expect("send succeeds");

    let rx = transport
        .receive_zero_copy(&topic(0x9002), None)
        .await
        .expect("receive succeeds");
    let payload = rx
        .borrow_stable_payload::<PaddedRuntime>()
        .expect("stable borrow succeeds");
    assert_eq!(payload.small, 0xaa);
    assert_eq!(payload.large, 0x1122_3344);

    let bytes = rx
        .loaned_contiguous_payload()
        .expect("loaned payload")
        .as_bytes();
    let gap = &bytes[1..mem::offset_of!(PaddedRuntime, large)];
    assert!(gap.iter().all(|byte| *byte == 0));
}

#[tokio::test]
async fn typed_array_and_nested_payloads_round_trip() {
    let transport = RecordingUninitTransport::default();

    transport
        .send_uninit_stable_payload_as::<TypedArrayRuntime>(metadata(0x9003), |payload| {
            payload.values_from_slice(&[1, 2, 3, 4])?.finish()
        })
        .await
        .expect("typed array send succeeds");
    let rx = transport
        .receive_zero_copy(&topic(0x9003), None)
        .await
        .expect("receive succeeds");
    assert_eq!(
        rx.borrow_stable_payload::<TypedArrayRuntime>()
            .unwrap()
            .values,
        [1, 2, 3, 4]
    );

    transport
        .send_uninit_stable_payload_as::<NestedRuntime>(metadata(0x9004), |payload| {
            payload
                .checksum(5)
                .header(|header| {
                    header
                        .sequence(4)
                        .logical_payload_len(0)
                        .case_id(3)
                        .finish()
                })?
                .finish()
        })
        .await
        .expect("nested send succeeds");
    let rx = transport
        .receive_zero_copy(&topic(0x9004), None)
        .await
        .expect("receive succeeds");
    let nested = rx.borrow_stable_payload::<NestedRuntime>().unwrap();
    assert_eq!(nested.header.case_id, 3);
    assert_eq!(nested.header.sequence, 4);
    assert_eq!(nested.checksum, 5);
}

#[tokio::test]
async fn large_payload_fill_smoke() {
    let transport = RecordingUninitTransport::default();

    transport
        .send_uninit_stable_payload_as::<LargeRuntime>(metadata(0x9005), |payload| {
            payload
                .header(|header| {
                    header
                        .case_id(1)
                        .sequence(2)
                        .logical_payload_len(65_536)
                        .finish()
                })?
                .checksum(3)
                .payload_fill_with(|index| index as u8)
                .finish()
        })
        .await
        .expect("large payload send succeeds");

    let rx = transport
        .receive_zero_copy(&topic(0x9005), None)
        .await
        .expect("receive succeeds");
    let payload = rx.borrow_stable_payload::<LargeRuntime>().unwrap();
    assert_eq!(payload.header.logical_payload_len, 65_536);
    assert_eq!(payload.payload[0], 0);
    assert_eq!(payload.payload[65_535], 255);
}

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    PlacementDefault,
    up_rust::StablePayload,
    ByteBackedStablePayload,
    up_rust::StablePayloadInit,
)]
#[stable_payload(type_name = "example.init.SmallPose")]
struct SmallPose {
    x: u32,
    y: u32,
}

#[tokio::test]
async fn stable_receive_api_is_shared_by_all_stable_send_constructors() {
    let transport = RecordingUninitTransport::default();

    transport
        .send_loaned_payload_as::<StableContainerPayload<SmallPose>, SmallPose>(
            metadata(0x9006),
            |pose| {
                pose.x = 1;
                pose.y = 2;
            },
        )
        .await
        .expect("initialized loan send succeeds");
    let rx = transport
        .receive_zero_copy(&topic(0x9006), None)
        .await
        .unwrap();
    assert_eq!(
        rx.borrow_stable_payload::<SmallPose>().unwrap(),
        &SmallPose { x: 1, y: 2 }
    );

    transport
        .send_uninit_loaned_payload_as::<StableContainerPayload<SmallPose>, SmallPose>(
            metadata(0x9007),
            |slot| Ok(slot.write(SmallPose { x: 3, y: 4 })),
        )
        .await
        .expect("whole-value uninit send succeeds");
    let rx = transport
        .receive_zero_copy(&topic(0x9007), None)
        .await
        .unwrap();
    assert_eq!(
        rx.borrow_stable_payload::<SmallPose>().unwrap(),
        &SmallPose { x: 3, y: 4 }
    );

    transport
        .send_uninit_stable_payload_as::<SmallPose>(metadata(0x9008), |pose| {
            pose.y(6).x(5).finish()
        })
        .await
        .expect("no-zero stable init send succeeds");
    let rx = transport
        .receive_zero_copy(&topic(0x9008), None)
        .await
        .unwrap();
    assert_eq!(
        rx.borrow_stable_payload::<SmallPose>().unwrap(),
        &SmallPose { x: 5, y: 6 }
    );
}
