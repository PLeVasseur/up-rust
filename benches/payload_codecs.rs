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

use std::{hint::black_box, mem, time::Instant};

use bytes::Bytes;
use up_rust::{
    payload::{BorrowPayload, PlacementDefault, RawBytes, StableContainerPayload},
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
            UFrameMetadata::publish(topic.clone()),
            payload.clone(),
        );
        black_box(frame.payload_bytes().len());
    });

    bench("raw from_payload_as copies bytes", || {
        let frame = UOwnedFrame::from_payload_as::<RawBytes, [u8]>(
            UFrameMetadata::publish(topic.clone()),
            payload.as_ref(),
        )
        .expect("raw payload should encode");
        black_box(frame.payload_bytes().len());
    });

    bench("mcap from_bytes_as moves Bytes handle", || {
        let frame = UOwnedFrame::from_bytes_as::<McapPayload>(
            UFrameMetadata::publish(topic.clone()),
            mcap.clone(),
        );
        black_box(frame.payload_bytes().len());
    });

    bench("stable container owned encode", || {
        let frame =
            UOwnedFrame::from_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>(
                UFrameMetadata::publish(topic.clone()),
                &pose,
            )
            .expect("stable payload should encode");
        black_box(frame.payload_bytes().len());
    });

    bench("stable container typed borrow", || {
        let borrowed =
            <StableContainerPayload<VehiclePose> as BorrowPayload<VehiclePose>>::borrow_payload(
                pose_bytes.0.as_slice(),
            )
            .expect("stable payload should borrow");
        black_box(borrowed.x);
    });
}
