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

use std::mem;

use up_rust::{
    payload::{BorrowPayload, LoanPayload, PlacementDefault, StableContainerPayload},
    zero_copy::{UTxBuffer, UVecTxBuffer},
    UFrameMetadata, UUri,
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
    up_rust::StablePayload,
    up_rust::ByteBackedStablePayload,
)]
#[stable_payload(type_name = "example.miri.VehiclePose")]
struct VehiclePose {
    x: u32,
    y: u32,
}

#[test]
fn stable_container_loan_then_borrow_is_miri_friendly() {
    let topic = UUri::try_from("//miri/4210/1/9000").unwrap();
    let mut buffer = UVecTxBuffer::with_alignment(
        UFrameMetadata::publish(topic)
            .with_encoding(StableContainerPayload::<VehiclePose>::encoding()),
        mem::size_of::<VehiclePose>(),
        mem::align_of::<VehiclePose>(),
    )
    .unwrap();

    {
        let pose = <StableContainerPayload<VehiclePose> as LoanPayload<VehiclePose>>::loan_payload(
            buffer.payload_mut(),
        )
        .unwrap();
        pose.x = 7;
        pose.y = 9;
    }

    let borrowed =
        <StableContainerPayload<VehiclePose> as BorrowPayload<VehiclePose>>::borrow_payload(
            buffer.payload(),
        )
        .unwrap();
    assert_eq!(borrowed, &VehiclePose { x: 7, y: 9 });
}
