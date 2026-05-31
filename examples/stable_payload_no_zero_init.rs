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

use std::{collections::VecDeque, io::Cursor, sync::Mutex};

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
#[stable_payload(type_name = "example.no_zero.SensorHeader")]
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
#[stable_payload(type_name = "example.no_zero.SensorFrame")]
struct SensorFrame {
    header: SensorHeader,
    checksum: u32,
    samples: [u32; 4],
    payload: [u8; 4096],
}

struct ExampleRxLease {
    frame: UOwnedFrame,
}

impl UFrameView for ExampleRxLease {
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
}

impl UZeroCopyRxLease for ExampleRxLease {}

impl ULoanedContiguousZeroCopyRxFrame for ExampleRxLease {
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, up_rust::UWireError> {
        // SAFETY: The example lease owns the payload bytes for `&self` and marks
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
struct ExampleTransport {
    queue: Mutex<VecDeque<UOwnedFrame>>,
}

#[async_trait]
impl UZeroCopyTransportImpl for ExampleTransport {
    type Tx = UVecTxBuffer;
    type Rx = ExampleRxLease;

    async fn loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus> {
        UVecTxBuffer::with_alignment(
            spec.metadata().clone(),
            spec.payload_len(),
            spec.payload_alignment(),
        )
        .map_err(UStatus::from)
    }

    async fn send_validated_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
        self.queue
            .lock()
            .expect("queue lock poisoned")
            .push_back(buffer.into_frame());
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
            .map(|frame| ExampleRxLease { frame })
            .ok_or_else(|| up_rust::UStatus::fail_with_code(up_rust::UCode::NOT_FOUND, "empty"))
    }
}

#[async_trait]
impl UZeroCopyUninitTransportImpl for ExampleTransport {
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), UStatus> {
    let transport = ExampleTransport::default();
    let topic = UUri::try_from_parts("vehicle", 0x4210, 1, 0x9000).expect("valid topic");

    transport
        .send_uninit_stable_payload_as::<SensorFrame>(
            UFrameMetadata::publish_unchecked(topic.clone()),
            |frame| {
                frame
                    .header(|header| {
                        header
                            .case_id(1)
                            .sequence(42)
                            .logical_payload_len(4096)
                            .finish()
                    })?
                    .samples_from_slice(&[10, 20, 30, 40])?
                    .checksum(0x5eed_cafe)
                    .payload_fill(0x5a)
                    .finish()
            },
        )
        .await?;

    let rx = transport.receive_zero_copy(&topic, None).await?;
    let frame = rx
        .borrow_stable_payload::<SensorFrame>()
        .map_err(UStatus::from)?;
    assert_eq!(frame.header.sequence, 42);
    assert_eq!(frame.samples, [10, 20, 30, 40]);
    assert_eq!(frame.payload[0], 0x5a);

    // The receive API is unchanged: stable-container frames sent by the older
    // initialized helper are borrowed through the same loan-backed boundary.
    transport
        .send_loaned_payload_as::<StableContainerPayload<SensorHeader>, SensorHeader>(
            UFrameMetadata::publish_unchecked(topic.clone()),
            |header| {
                header.case_id = 2;
                header.sequence = 43;
                header.logical_payload_len = 0;
            },
        )
        .await?;
    let rx = transport.receive_zero_copy(&topic, None).await?;
    let header = rx
        .borrow_stable_payload::<SensorHeader>()
        .map_err(UStatus::from)?;
    assert_eq!(header.sequence, 43);

    Ok(())
}
