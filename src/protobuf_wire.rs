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

//! Optional Protocol Buffers payload codec support.
//!
//! This module deliberately models Protocol Buffers as a payload [`WireFormat`].
//! It does not reintroduce generated transport envelopes.

use std::io::Read;

use protobuf::{CodedOutputStream, Message};

use crate::{
    wire::{UDeserializer, UReadDeserializer, USerializer, UWireError, WireFormat},
    UEncoding,
};

pub struct ProtobufWire;

impl WireFormat for ProtobufWire {
    fn name() -> &'static str {
        "protobuf"
    }

    fn encoding() -> UEncoding {
        UEncoding::without_schema_ref("protobuf", "application/x-protobuf")
    }
}

impl<T> USerializer<ProtobufWire> for T
where
    T: Message,
{
    fn encoded_len(&self) -> usize {
        self.compute_size() as usize
    }

    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        let expected = self.encoded_len();
        let actual = dst.len();
        let out = dst
            .get_mut(..expected)
            .ok_or_else(|| UWireError::buffer_too_small(expected, actual))?;
        let mut output = CodedOutputStream::bytes(out);
        self.write_to(&mut output)
            .map_err(|error| UWireError::serialization_error(error.to_string()))?;
        output
            .flush()
            .map_err(|error| UWireError::serialization_error(error.to_string()))?;
        let written = usize::try_from(output.total_bytes_written())
            .map_err(|error| UWireError::serialization_error(error.to_string()))?;
        if written != expected {
            return Err(UWireError::invalid_payload(format!(
                "protobuf writer wrote {written} bytes but encoded_len returned {expected} bytes"
            )));
        }
        Ok(written)
    }
}

impl<'a, T> UDeserializer<'a, ProtobufWire> for T
where
    T: Message,
{
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
        protobuf::Message::parse_from_bytes(src)
            .map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

impl<T> UReadDeserializer<ProtobufWire> for T
where
    T: Message,
{
    fn deserialize_from_reader<R: Read>(
        mut reader: R,
        _payload_len: usize,
    ) -> Result<Self, UWireError> {
        protobuf::Message::parse_from_reader(&mut reader)
            .map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use protobuf::well_known_types::wrappers::StringValue;

    use crate::{
        wire::{UDeserializer, USerializer},
        zero_copy::{UContiguousZeroCopyRxFrame, UTxBuffer, UVecTxBuffer, UZeroCopyRxFrame},
        UFrameMetadata, UOwnedFrame, UUri,
    };

    use super::*;

    fn message(value: &str) -> StringValue {
        let mut message = StringValue::new();
        message.value = value.to_string();
        message
    }

    #[test]
    fn owned_frame_round_trips_protobuf_payload() {
        let topic = UUri::try_from("//vehicle/4210/1/8001").unwrap();
        let input = message("owned protobuf payload");

        let frame = UOwnedFrame::from_serializable::<ProtobufWire, _>(
            UFrameMetadata::publish(topic),
            &input,
        )
        .unwrap();
        let decoded: StringValue = frame.deserialize::<ProtobufWire, _>().unwrap();

        assert_eq!(frame.metadata().encoding(), Some(&ProtobufWire::encoding()));
        assert_eq!(decoded.value, input.value);
    }

    #[test]
    fn zero_copy_buffer_round_trips_protobuf_payload() {
        let topic = UUri::try_from("//vehicle/4210/1/8001").unwrap();
        let input = message("zero-copy protobuf payload");
        let payload_len = <StringValue as USerializer<ProtobufWire>>::encoded_len(&input);
        let mut buffer = UVecTxBuffer::new(
            UFrameMetadata::publish(topic).with_encoding(ProtobufWire::encoding()),
            payload_len,
        );

        let written = input.serialize_into(buffer.payload_mut()).unwrap();
        assert_eq!(written, payload_len);

        let frame = buffer.into_frame();
        let decoded: StringValue = frame.deserialize_borrowed::<ProtobufWire, _>().unwrap();
        let decoded_from_reader: StringValue =
            frame.deserialize_from_reader::<ProtobufWire, _>().unwrap();

        assert_eq!(frame.metadata().encoding(), Some(&ProtobufWire::encoding()));
        assert_eq!(decoded.value, input.value);
        assert_eq!(decoded_from_reader.value, input.value);
    }

    #[test]
    fn protobuf_deserializer_rejects_invalid_payload() {
        let result = StringValue::deserialize_from(&[0x0a]);

        assert!(matches!(
            result,
            Err(crate::wire::UWireError::InvalidPayload(_))
        ));
    }
}
