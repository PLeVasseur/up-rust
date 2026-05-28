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
use up_rust::{ProtobufAnyPayload, UFrameBuilder, UUri};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let topic = UUri::try_from("//vehicle/4210/1/8001")?;
    let mut message = StringValue::new();
    message.value = "protobuf Any payload".to_string();
    let frame = UFrameBuilder::publish(topic).build_with_protobuf_any_payload(&message)?;

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
