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

use std::{hint::black_box, io::Cursor, mem, time::Instant};

use bytes::Bytes;
use up_rust::{
    payload::{PlacementDefault, RawBytes, StableContainerPayload},
    zero_copy::{
        LoanedPayload, PayloadLoanProvenance, UFrameView, ULoanedContiguousZeroCopyRxFrame,
        UZeroCopyRxLease,
    },
    McapPayload, UFrameMetadata, UOwnedFrame, UUri,
};

const ITERATIONS: usize = 100_000;

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
    up_rust::ByteBackedStablePayload,
)]
#[stable_payload(type_name = "example.bench.VehiclePose")]
struct VehiclePose {
    x: u32,
    y: u32,
}

#[repr(C, align(4))]
struct AlignedPoseBytes([u8; mem::size_of::<VehiclePose>()]);

struct LoanedPoseFrame<'a> {
    metadata: UFrameMetadata,
    payload: &'a [u8],
}

impl UFrameView for LoanedPoseFrame<'_> {
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

impl UZeroCopyRxLease for LoanedPoseFrame<'_> {}

impl ULoanedContiguousZeroCopyRxFrame for LoanedPoseFrame<'_> {
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, up_rust::UWireError> {
        // SAFETY: The benchmark frame keeps `payload` alive for `&self` and
        // exposes it as the exact visible loan-backed payload slice.
        Ok(unsafe {
            LoanedPayload::new_unchecked(self.payload, PayloadLoanProvenance::OpaqueTransportLoan)
        })
    }
}

fn bench(name: &str, mut f: impl FnMut()) {
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        f();
    }
    let elapsed = start.elapsed();
    let ns_per_iter = elapsed.as_nanos() / ITERATIONS as u128;
    println!("{name}: {ns_per_iter} ns/iter over {ITERATIONS} iterations");
}

fn main() {
    let topic = UUri::try_from("//bench/4210/1/9000").expect("valid benchmark topic");
    let payload = Bytes::from(vec![0x5a_u8; 4096]);
    let mcap = Bytes::from_static(b"\x89MCAP\r\nbenchmark-fixture");
    let pose = VehiclePose { x: 1, y: 2 };
    let mut pose_bytes = AlignedPoseBytes([0_u8; mem::size_of::<VehiclePose>()]);
    <StableContainerPayload<VehiclePose> as up_rust::payload::EncodePayload<VehiclePose>>::encode_payload(
        &pose,
        pose_bytes.0.as_mut_slice(),
    )
    .expect("pose bytes should encode");

    bench("raw from_bytes_as moves Bytes handle", || {
        let frame = UOwnedFrame::from_bytes_as::<RawBytes>(
            UFrameMetadata::publish_unchecked(topic.clone()),
            payload.clone(),
        );
        black_box(frame.payload_bytes().len());
    });

    bench("raw from_payload_as copies bytes", || {
        let frame = UOwnedFrame::from_payload_as::<RawBytes, [u8]>(
            UFrameMetadata::publish_unchecked(topic.clone()),
            payload.as_ref(),
        )
        .expect("raw payload should encode");
        black_box(frame.payload_bytes().len());
    });

    bench("mcap from_bytes_as moves Bytes handle", || {
        let frame = UOwnedFrame::from_bytes_as::<McapPayload>(
            UFrameMetadata::publish_unchecked(topic.clone()),
            mcap.clone(),
        );
        black_box(frame.payload_bytes().len());
    });

    bench("stable container owned encode", || {
        let frame =
            UOwnedFrame::from_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>(
                UFrameMetadata::publish_unchecked(topic.clone()),
                &pose,
            )
            .expect("stable payload should encode");
        black_box(frame.payload_bytes().len());
    });

    bench("stable container typed borrow", || {
        let frame = LoanedPoseFrame {
            metadata: UFrameMetadata::publish_unchecked(topic.clone())
                .with_encoding(StableContainerPayload::<VehiclePose>::encoding()),
            payload: pose_bytes.0.as_slice(),
        };
        let borrowed = frame
            .borrow_stable_payload::<VehiclePose>()
            .expect("stable payload should borrow");
        black_box(borrowed.x);
    });
}
