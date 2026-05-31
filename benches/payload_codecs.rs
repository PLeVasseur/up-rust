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
    payload::{LoanPayload, PlacementDefault, RawBytes, StableContainerPayload, StablePayloadInit},
    zero_copy::{
        LoanedPayload, PayloadLoanProvenance, UFrameView, ULoanedContiguousZeroCopyRxFrame,
        UTxBuffer, UUninitTxBuffer, UVecTxBuffer, UVecUninitTxBuffer, UZeroCopyRxLease,
    },
    ByteBackedStablePayload, McapPayload, UFrameMetadata, UOwnedFrame, UUri,
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

fn bench(name: &str, f: impl FnMut()) {
    bench_with_iterations(name, ITERATIONS, f);
}

fn bench_with_iterations(name: &str, iterations: usize, mut f: impl FnMut()) {
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let elapsed = start.elapsed();
    let ns_per_iter = elapsed.as_nanos() / iterations as u128;
    println!("{name}: {ns_per_iter} ns/iter over {iterations} iterations");
}

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Eq,
    PartialEq,
    PlacementDefault,
    up_rust::StablePayload,
    ByteBackedStablePayload,
    up_rust::StablePayloadInit,
)]
#[stable_payload(type_name = "example.bench.StableInitHeader")]
struct StableInitHeader {
    case_id: u32,
    sequence: u32,
    logical_payload_len: u32,
}

macro_rules! stable_init_payload {
    ($name:ident, $type_name:literal, $len:expr) => {
        #[repr(C)]
        #[derive(
            Clone,
            Copy,
            Eq,
            PartialEq,
            PlacementDefault,
            up_rust::StablePayload,
            ByteBackedStablePayload,
            up_rust::StablePayloadInit,
        )]
        #[stable_payload(type_name = $type_name)]
        struct $name {
            header: StableInitHeader,
            checksum: u32,
            payload: [u8; $len],
        }
    };
}

stable_init_payload!(
    StableInitPayload4096,
    "example.bench.StableInitPayload4096",
    4096
);
stable_init_payload!(
    StableInitPayload65536,
    "example.bench.StableInitPayload65536",
    65_536
);
stable_init_payload!(
    StableInitPayloadCamera,
    "example.bench.StableInitPayloadCamera",
    12_441_600
);

fn stable_current_zeroing_init_4096(topic: &UUri) {
    let mut buffer = stable_init_tx_buffer::<StableInitPayload4096>(topic);
    let payload = <StableContainerPayload<StableInitPayload4096> as LoanPayload<
        StableInitPayload4096,
    >>::loan_payload(buffer.payload_mut())
    .expect("stable loan");
    payload.header = stable_init_header(4096);
    payload.checksum = 0x5eed_cafe;
    payload.payload.fill(0x5a);
    black_box(payload.payload[0]);
}

fn stable_current_zeroing_init_65536(topic: &UUri) {
    let mut buffer = stable_init_tx_buffer::<StableInitPayload65536>(topic);
    let payload = <StableContainerPayload<StableInitPayload65536> as LoanPayload<
        StableInitPayload65536,
    >>::loan_payload(buffer.payload_mut())
    .expect("stable loan");
    payload.header = stable_init_header(65_536);
    payload.checksum = 0x5eed_cafe;
    payload.payload.fill(0x5a);
    black_box(payload.payload[0]);
}

fn stable_current_zeroing_init_camera(topic: &UUri) {
    let mut buffer = stable_init_tx_buffer::<StableInitPayloadCamera>(topic);
    let payload = <StableContainerPayload<StableInitPayloadCamera> as LoanPayload<
        StableInitPayloadCamera,
    >>::loan_payload(buffer.payload_mut())
    .expect("stable loan");
    payload.header = stable_init_header(12_441_600);
    payload.checksum = 0x5eed_cafe;
    payload.payload.fill(0x5a);
    black_box(payload.payload[0]);
}

fn stable_init_header(logical_len: u32) -> StableInitHeader {
    StableInitHeader {
        case_id: 1,
        sequence: 2,
        logical_payload_len: logical_len,
    }
}

fn stable_init_tx_buffer<T>(topic: &UUri) -> UVecTxBuffer
where
    T: up_rust::payload::ByteBackedStablePayload + PlacementDefault,
{
    UVecTxBuffer::with_alignment(
        UFrameMetadata::publish_unchecked(topic.clone())
            .with_encoding(StableContainerPayload::<T>::encoding()),
        mem::size_of::<T>(),
        mem::align_of::<T>(),
    )
    .expect("aligned buffer")
}

fn stable_nozero_init_4096(topic: &UUri) {
    let mut buffer = stable_init_uninit_tx_buffer::<StableInitPayload4096>(topic);
    let init = StableInitPayload4096::init_from_uninit_payload(buffer.payload_uninit_mut())
        .expect("init builder");
    let _initialized = init
        .header(|header| {
            header
                .case_id(1)
                .sequence(2)
                .logical_payload_len(4096)
                .finish()
        })
        .expect("header init")
        .checksum(0x5eed_cafe)
        .payload_fill(0x5a)
        .finish()
        .expect("finish");
    // SAFETY: The generated builder returned its completion proof.
    let buffer = unsafe { buffer.assume_payload_init() };
    black_box(buffer.payload()[0]);
}

fn stable_nozero_init_65536(topic: &UUri) {
    let mut buffer = stable_init_uninit_tx_buffer::<StableInitPayload65536>(topic);
    let init = StableInitPayload65536::init_from_uninit_payload(buffer.payload_uninit_mut())
        .expect("init builder");
    let _initialized = init
        .header(|header| {
            header
                .case_id(1)
                .sequence(2)
                .logical_payload_len(65_536)
                .finish()
        })
        .expect("header init")
        .checksum(0x5eed_cafe)
        .payload_fill(0x5a)
        .finish()
        .expect("finish");
    // SAFETY: The generated builder returned its completion proof.
    let buffer = unsafe { buffer.assume_payload_init() };
    black_box(buffer.payload()[0]);
}

fn stable_nozero_init_camera(topic: &UUri) {
    let mut buffer = stable_init_uninit_tx_buffer::<StableInitPayloadCamera>(topic);
    let init = StableInitPayloadCamera::init_from_uninit_payload(buffer.payload_uninit_mut())
        .expect("init builder");
    let _initialized = init
        .header(|header| {
            header
                .case_id(1)
                .sequence(2)
                .logical_payload_len(12_441_600)
                .finish()
        })
        .expect("header init")
        .checksum(0x5eed_cafe)
        .payload_fill(0x5a)
        .finish()
        .expect("finish");
    // SAFETY: The generated builder returned its completion proof.
    let buffer = unsafe { buffer.assume_payload_init() };
    black_box(buffer.payload()[0]);
}

fn stable_init_uninit_tx_buffer<T>(topic: &UUri) -> UVecUninitTxBuffer
where
    T: StablePayloadInit,
{
    UVecUninitTxBuffer::with_alignment(
        UFrameMetadata::publish_unchecked(topic.clone())
            .with_encoding(StableContainerPayload::<T>::encoding()),
        mem::size_of::<T>(),
        mem::align_of::<T>(),
    )
    .expect("aligned uninit buffer")
}

fn raw_fill_once(topic: &UUri, len: usize) {
    let mut buffer = UVecUninitTxBuffer::with_alignment(
        UFrameMetadata::publish_unchecked(topic.clone()),
        len,
        1,
    )
    .expect("aligned uninit buffer");
    let mut writer = buffer.payload_uninit_mut().into_writer();
    let chunk = [0x5a_u8; 4096];
    let mut remaining = len;
    while remaining > 0 {
        let take = remaining.min(chunk.len());
        writer.write_all(&chunk[..take]).expect("write chunk");
        remaining -= take;
    }
    let _initialized = writer.finish().expect("raw fill finished");
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

    bench_with_iterations("stable_current_zeroing_init/4096", 10_000, || {
        stable_current_zeroing_init_4096(&topic);
    });
    bench_with_iterations("stable_nozero_init/4096", 10_000, || {
        stable_nozero_init_4096(&topic);
    });
    bench_with_iterations("raw_fill_once/4096", 10_000, || {
        raw_fill_once(&topic, 4096);
    });
    bench_with_iterations("stable_current_zeroing_init/65536", 1_000, || {
        stable_current_zeroing_init_65536(&topic);
    });
    bench_with_iterations("stable_nozero_init/65536", 1_000, || {
        stable_nozero_init_65536(&topic);
    });
    bench_with_iterations("raw_fill_once/65536", 1_000, || {
        raw_fill_once(&topic, 65_536);
    });
    bench_with_iterations("stable_current_zeroing_init/12441600", 10, || {
        stable_current_zeroing_init_camera(&topic);
    });
    bench_with_iterations("stable_nozero_init/12441600", 10, || {
        stable_nozero_init_camera(&topic);
    });
    bench_with_iterations("raw_fill_once/12441600", 10, || {
        raw_fill_once(&topic, 12_441_600);
    });
}
