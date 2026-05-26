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

use bytes::Bytes;
use up_rust::{EncodedPayload, McapPayload, UFrameMetadata, UOwnedFrame, UUri};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let topic = UUri::try_from_parts("vehicle", 0x4210, 1, 0x8002)?;
    let archive = Bytes::from_static(b"\x89MCAP\r\nfixture-bytes");

    let frame = UOwnedFrame::from_encoded_payload(
        UFrameMetadata::publish(topic),
        EncodedPayload::<McapPayload>::from_bytes(archive.clone()),
    );
    let borrowed = frame.borrow_payload_as::<McapPayload, [u8]>()?;

    assert_eq!(frame.metadata().encoding(), Some(&McapPayload::encoding()));
    assert_eq!(borrowed, archive.as_ref());
    println!("MCAP payload carried {} archive bytes", borrowed.len());
    Ok(())
}
