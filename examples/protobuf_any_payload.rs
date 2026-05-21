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

use protobuf::well_known_types::{any::Any, wrappers::StringValue};
use up_rust::{payload::USerializer, ProtobufAnyPayload, UFrameMetadata, UOwnedFrame, UUri};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let topic = UUri::try_from("//vehicle/4210/1/8001")?;
    let mut message = StringValue::new();
    message.value = "protobuf Any payload".to_string();
    let any = Any::pack(&message)?;

    let payload = <Any as USerializer<ProtobufAnyPayload>>::serialize_owned(&any)?;
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(topic).with_encoding(ProtobufAnyPayload::encoding()),
        payload,
    );

    let decoded_any: Any = frame.deserialize::<ProtobufAnyPayload, _>()?;
    let decoded = decoded_any
        .unpack::<StringValue>()?
        .expect("unexpected protobuf Any type");

    assert_eq!(
        frame.metadata().encoding(),
        Some(&ProtobufAnyPayload::encoding())
    );
    assert_eq!(decoded.value, message.value);
    println!("decoded protobuf Any payload: {}", decoded.value);
    Ok(())
}
