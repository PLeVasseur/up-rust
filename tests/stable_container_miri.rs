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

use up_rust::{
    StableContainerPayload, UFrameMetadata, ULoanedContiguousZeroCopyRxFrame, UMessageBuilder,
    UUri, UVecRxLease, UUID,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, up_rust::StablePayload)]
#[stable_payload(type_name = "example.miri.StableBytes")]
struct StableBytes {
    bytes: [u8; 4],
}

fn topic() -> UUri {
    UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).expect("valid topic")
}

fn metadata<T: up_rust::StablePayload>() -> UFrameMetadata {
    let fixed_id = UUID::from_u64_pair(0x0000_0000_0001_7000, 0x8010_1010_1010_1a1a)
        .expect("fixed UUID should be valid");
    let message = UMessageBuilder::publish(topic())
        .with_message_id(fixed_id)
        .build()
        .expect("message");
    UFrameMetadata::new(
        message.attributes().clone(),
        Some(StableContainerPayload::<T>::encoding()),
    )
    .expect("stable metadata")
}

fn stable_bytes(value: &StableBytes) -> Vec<u8> {
    // SAFETY: `StableBytes` is `repr(C)` over `[u8; 4]`, has alignment 1, and
    // every byte pattern is valid for the test payload.
    unsafe {
        std::slice::from_raw_parts(
            std::ptr::from_ref(value).cast::<u8>(),
            std::mem::size_of::<StableBytes>(),
        )
        .to_vec()
    }
}

#[test]
fn stable_payload_derive_loan_borrow_is_miri_friendly() {
    let value = StableBytes { bytes: *b"miri" };
    let frame = UVecRxLease::new(metadata::<StableBytes>(), Some(stable_bytes(&value)))
        .expect("loan-backed stable frame");

    let borrowed = frame
        .borrow_stable_payload::<StableBytes>()
        .expect("borrow stable payload");

    assert_eq!(borrowed, &value);
}
