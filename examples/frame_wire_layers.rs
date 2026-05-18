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
    frame_wire::{ProtobufUMessageFrame, UFrameWireFormat},
    payload::{PayloadFormat, UDeserializer, UEncoding, USerializer, UWireError},
    UFrameBuilder, UUri,
};

struct JsonPayload;

impl PayloadFormat for JsonPayload {
    fn name() -> &'static str {
        "json"
    }

    fn encoding() -> UEncoding {
        UEncoding::without_schema_ref(Self::name(), "application/json")
    }
}

impl USerializer<JsonPayload> for String {
    fn encoded_len(&self) -> usize {
        self.len()
    }

    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        let actual = dst.len();
        let out = dst
            .get_mut(..self.len())
            .ok_or_else(|| UWireError::buffer_too_small(self.len(), actual))?;
        out.copy_from_slice(self.as_bytes());
        Ok(self.len())
    }
}

impl UDeserializer<'_, JsonPayload> for String {
    fn deserialize_from(src: &[u8]) -> Result<Self, UWireError> {
        String::from_utf8(src.to_vec())
            .map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let topic = UUri::try_from("//vehicle/4210/1/8001")?;

    let raw_frame =
        UFrameBuilder::publish(topic.clone()).build_with_raw_payload(b"raw".as_slice())?;
    let raw_envelope = ProtobufUMessageFrame::serialize_frame(&raw_frame)?;
    let raw_decoded = ProtobufUMessageFrame::deserialize_frame(&raw_envelope)?;
    assert_eq!(raw_decoded.payload_bytes(), b"raw");

    let json = String::from("{\"temperature\":21.5}");
    let json_frame =
        UFrameBuilder::publish(topic.clone()).build_with_serializable::<JsonPayload, _>(&json)?;
    let json_envelope = ProtobufUMessageFrame::serialize_frame(&json_frame)?;
    let json_decoded = ProtobufUMessageFrame::deserialize_frame(&json_envelope)?;
    let json_payload = json_decoded.deserialize::<JsonPayload, String>()?;
    assert_eq!(json_payload, json);

    let schema_specific = UFrameBuilder::publish(topic).build_with_payload(
        b"schema-specific".as_slice(),
        UEncoding::with_schema_ref("json", "application/json", "urn:example:Temperature:v1"),
    )?;
    assert!(ProtobufUMessageFrame::serialize_frame(&schema_specific).is_err());

    Ok(())
}
