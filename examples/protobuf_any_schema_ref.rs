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
use up_rust::{
    payload::USerializer, ProtobufPayload, UEncoding, UFrameMetadata, UOwnedFrame, UUri,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let topic = UUri::try_from("//vehicle/4210/1/8001")?;
    let mut message = StringValue::new();
    message.value = "protobuf Any payload with schema_ref".to_string();
    let any = Any::pack(&message)?;

    let payload = <Any as USerializer<ProtobufPayload>>::serialize_owned(&any)?;
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(topic).with_encoding(UEncoding::with_schema_ref(
            "protobuf",
            "application/x-protobuf",
            any.type_url.clone(),
        )),
        payload,
    );

    let decoded_any: Any = frame.deserialize::<ProtobufPayload, _>()?;
    let decoded = decoded_any
        .unpack::<StringValue>()?
        .expect("unexpected protobuf Any type");

    assert_eq!(
        frame.metadata().encoding().and_then(UEncoding::schema_ref),
        Some(any.type_url.as_str())
    );
    assert_eq!(decoded.value, message.value);
    println!("decoded protobuf Any payload: {}", decoded.value);
    Ok(())
}
