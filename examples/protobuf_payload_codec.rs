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

use protobuf::well_known_types::wrappers::StringValue;
use up_rust::{ProtobufPayload, UFrameMetadata, UOwnedFrame, UUri};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut message = StringValue::new();
    message.value = "hello protobuf payload".to_string();

    let topic = UUri::try_from_parts("vehicle", 0x4210, 1, 0x8001)?;
    let frame = UOwnedFrame::from_payload_as::<ProtobufPayload, _>(
        UFrameMetadata::try_publish(topic)?,
        &message,
    )?;
    let decoded: StringValue = frame.decode_payload_as::<ProtobufPayload, _>()?;

    assert_eq!(
        frame.metadata().encoding(),
        Some(&ProtobufPayload::encoding())
    );
    assert_eq!(decoded.value, "hello protobuf payload");
    println!(
        "protobuf payload encoded as {:?}",
        frame.metadata().encoding()
    );
    Ok(())
}
