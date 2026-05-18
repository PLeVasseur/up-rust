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

//! Optional Protocol Buffers frame and payload codec support.
//!
//! [`ProtobufPayload`] models Protocol Buffers as an application payload codec.
//! [`ProtobufUMessageFrame`] models the generated `UMessage` Protocol Buffers
//! envelope as a whole-frame wire format.

use std::io::Read;

use ::protobuf::{CodedOutputStream, EnumOrUnknown, Message};
use bytes::Bytes;

use crate::{
    payload::{PayloadFormat, UDeserializer, UReadDeserializer, USerializer, UWireError},
    up_core_api::{
        uattributes as proto_attributes, ucode as proto_ucode, umessage as proto_message,
    },
    UAttributes, UCode, UEncoding, UFrameMetadata, UFrameWireError, UFrameWireFormat, UMessageType,
    UOwnedFrame, UPriority, UUri, UUID,
};

pub struct ProtobufPayload;

impl ProtobufPayload {
    pub fn encoding() -> UEncoding {
        <Self as PayloadFormat>::encoding()
    }
}

impl PayloadFormat for ProtobufPayload {
    fn name() -> &'static str {
        "protobuf"
    }

    fn encoding() -> UEncoding {
        UEncoding::without_schema_ref("protobuf", "application/x-protobuf")
    }
}

impl<T> USerializer<ProtobufPayload> for T
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

impl<'a, T> UDeserializer<'a, ProtobufPayload> for T
where
    T: Message,
{
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
        ::protobuf::Message::parse_from_bytes(src)
            .map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

impl<T> UReadDeserializer<ProtobufPayload> for T
where
    T: Message,
{
    fn deserialize_from_reader<R: Read>(
        mut reader: R,
        _payload_len: usize,
    ) -> Result<Self, UWireError> {
        ::protobuf::Message::parse_from_reader(&mut reader)
            .map_err(|error| UWireError::invalid_payload(error.to_string()))
    }
}

/// Whole-frame wire format using the generated `UMessage` Protocol Buffers envelope.
///
/// This serializes the outer frame as `uprotocol.v1.UMessage`: native
/// [`UAttributes`], the legacy `payload_format` field, and optional payload
/// bytes. It is distinct from [`ProtobufPayload`], which serializes only an
/// application payload value.
pub struct ProtobufUMessageFrame;

impl UFrameWireFormat for ProtobufUMessageFrame {
    fn name() -> &'static str {
        "protobuf-umessage"
    }

    fn content_type() -> &'static str {
        "application/x-uprotocol-umessage+protobuf"
    }

    fn serialize_frame(frame: &UOwnedFrame) -> Result<Bytes, UFrameWireError> {
        let message = frame_to_umessage(frame)?;
        message
            .write_to_bytes()
            .map(Bytes::from)
            .map_err(|error| UFrameWireError::serialization_error(error.to_string()))
    }

    fn deserialize_frame(src: &[u8]) -> Result<UOwnedFrame, UFrameWireError> {
        let message = proto_message::UMessage::parse_from_bytes(src)
            .map_err(|error| UFrameWireError::invalid_frame(error.to_string()))?;
        umessage_to_frame(&message)
    }
}

fn frame_to_umessage(frame: &UOwnedFrame) -> Result<proto_message::UMessage, UFrameWireError> {
    let mut attributes = native_attributes_to_proto(frame.metadata().attributes())?;
    attributes.payload_format = match (frame.payload(), frame.metadata().encoding()) {
        (None, None) => {
            EnumOrUnknown::from(proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_UNSPECIFIED)
        }
        (Some(_), Some(encoding)) => EnumOrUnknown::from(payload_encoding_to_proto(encoding)?),
        (Some(_), None) => {
            return Err(UFrameWireError::invalid_frame(
                "payload is present but payload encoding is absent",
            ));
        }
        (None, Some(_)) => {
            return Err(UFrameWireError::invalid_frame(
                "payload encoding is present but payload is absent",
            ));
        }
    };

    Ok(proto_message::UMessage {
        attributes: Some(attributes).into(),
        payload: frame.payload().cloned(),
        ..Default::default()
    })
}

fn umessage_to_frame(message: &proto_message::UMessage) -> Result<UOwnedFrame, UFrameWireError> {
    let attributes = message
        .attributes
        .as_ref()
        .ok_or_else(|| UFrameWireError::invalid_frame("UMessage attributes are absent"))?;
    let payload_format = attributes.payload_format.enum_value().map_err(|value| {
        UFrameWireError::invalid_frame(format!("unknown payload_format {value}"))
    })?;
    let native_attributes = proto_attributes_to_native(attributes)?;
    let metadata = match (&message.payload, payload_format) {
        (None, proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_UNSPECIFIED) => {
            UFrameMetadata::without_payload_encoding(native_attributes)
        }
        (Some(_), proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_UNSPECIFIED) => {
            return Err(UFrameWireError::invalid_frame(
                "UMessage payload is present but payload_format is unspecified",
            ));
        }
        (None, _) => {
            return Err(UFrameWireError::invalid_frame(
                "UMessage payload_format is set but payload is absent",
            ));
        }
        (Some(_), format) => {
            UFrameMetadata::new(native_attributes, proto_payload_format_to_encoding(format)?)
        }
    };

    Ok(match &message.payload {
        Some(payload) => UOwnedFrame::with_payload(metadata, payload.clone()),
        None => UOwnedFrame::without_payload(metadata),
    })
}

fn native_attributes_to_proto(
    attributes: &UAttributes,
) -> Result<proto_attributes::UAttributes, UFrameWireError> {
    let mut proto = proto_attributes::UAttributes {
        id: Some(native_uuid_to_proto(attributes.id())).into(),
        type_: EnumOrUnknown::from(native_message_type_to_proto(attributes.message_type())),
        source: Some(native_uri_to_proto(attributes.source())).into(),
        priority: EnumOrUnknown::from(native_priority_to_proto(attributes.priority())),
        ttl: attributes.ttl(),
        permission_level: attributes.permission_level(),
        commstatus: attributes
            .commstatus()
            .map(|code| EnumOrUnknown::from(native_code_to_proto(code))),
        reqid: attributes.request_id().map(native_uuid_to_proto).into(),
        token: attributes.token().map(str::to_string),
        traceparent: attributes.traceparent().map(str::to_string),
        ..Default::default()
    };
    if let Some(sink) = attributes.sink() {
        proto.sink = Some(native_uri_to_proto(sink)).into();
    }
    Ok(proto)
}

fn proto_attributes_to_native(
    attributes: &proto_attributes::UAttributes,
) -> Result<UAttributes, UFrameWireError> {
    let id = attributes
        .id
        .as_ref()
        .ok_or_else(|| UFrameWireError::invalid_frame("UAttributes.id is absent"))?;
    let source = attributes
        .source
        .as_ref()
        .ok_or_else(|| UFrameWireError::invalid_frame("UAttributes.source is absent"))?;
    let message_type =
        proto_message_type_to_native(attributes.type_.enum_value().map_err(|value| {
            UFrameWireError::invalid_frame(format!("unknown message type {value}"))
        })?)?;
    let priority =
        proto_priority_to_native(attributes.priority.enum_value().map_err(|value| {
            UFrameWireError::invalid_frame(format!("unknown priority {value}"))
        })?)?;
    let mut native = UAttributes::new(
        proto_uuid_to_native(id),
        proto_uri_to_native(source),
        attributes.sink.as_ref().map(proto_uri_to_native),
        message_type,
    )
    .with_priority(priority);

    if let Some(ttl) = attributes.ttl {
        native = native.with_ttl(ttl);
    }
    if let Some(request_id) = attributes.reqid.as_ref() {
        native = native.with_request_id(proto_uuid_to_native(request_id));
    }
    if let Some(traceparent) = attributes.traceparent.as_ref() {
        native = native.with_traceparent(traceparent.clone());
    }
    if let Some(token) = attributes.token.as_ref() {
        native = native.with_token(token.clone());
    }
    if let Some(permission_level) = attributes.permission_level {
        native = native.with_permission_level(permission_level);
    }
    if let Some(commstatus) = attributes.commstatus {
        native = native.with_comm_status(proto_code_to_native(commstatus.enum_value().map_err(
            |value| UFrameWireError::invalid_frame(format!("unknown commstatus {value}")),
        )?));
    }
    native
        .validate()
        .map_err(|error| UFrameWireError::invalid_frame(error.to_string()))?;
    Ok(native)
}

fn native_uuid_to_proto(uuid: &UUID) -> crate::up_core_api::uuid::UUID {
    crate::up_core_api::uuid::UUID {
        msb: uuid.msb(),
        lsb: uuid.lsb(),
        ..Default::default()
    }
}

fn proto_uuid_to_native(uuid: &crate::up_core_api::uuid::UUID) -> UUID {
    UUID::from_u64_pair_unchecked(uuid.msb, uuid.lsb)
}

fn native_uri_to_proto(uri: &UUri) -> crate::up_core_api::uri::UUri {
    crate::up_core_api::uri::UUri {
        authority_name: uri.authority_name(),
        ue_id: uri.ue_id(),
        ue_version_major: uri.ue_version_major(),
        resource_id: uri.resource_id_raw(),
        ..Default::default()
    }
}

fn proto_uri_to_native(uri: &crate::up_core_api::uri::UUri) -> UUri {
    UUri::from_parts_unchecked(
        uri.authority_name.clone(),
        uri.ue_id,
        uri.ue_version_major,
        uri.resource_id,
    )
}

fn native_message_type_to_proto(message_type: UMessageType) -> proto_attributes::UMessageType {
    match message_type {
        UMessageType::Publish => proto_attributes::UMessageType::UMESSAGE_TYPE_PUBLISH,
        UMessageType::Notification => proto_attributes::UMessageType::UMESSAGE_TYPE_NOTIFICATION,
        UMessageType::Request => proto_attributes::UMessageType::UMESSAGE_TYPE_REQUEST,
        UMessageType::Response => proto_attributes::UMessageType::UMESSAGE_TYPE_RESPONSE,
    }
}

fn proto_message_type_to_native(
    message_type: proto_attributes::UMessageType,
) -> Result<UMessageType, UFrameWireError> {
    match message_type {
        proto_attributes::UMessageType::UMESSAGE_TYPE_PUBLISH => Ok(UMessageType::Publish),
        proto_attributes::UMessageType::UMESSAGE_TYPE_NOTIFICATION => {
            Ok(UMessageType::Notification)
        }
        proto_attributes::UMessageType::UMESSAGE_TYPE_REQUEST => Ok(UMessageType::Request),
        proto_attributes::UMessageType::UMESSAGE_TYPE_RESPONSE => Ok(UMessageType::Response),
        proto_attributes::UMessageType::UMESSAGE_TYPE_UNSPECIFIED => Err(
            UFrameWireError::invalid_frame("message type is unspecified"),
        ),
    }
}

fn native_priority_to_proto(priority: UPriority) -> proto_attributes::UPriority {
    match priority {
        UPriority::CS0 => proto_attributes::UPriority::UPRIORITY_CS0,
        UPriority::CS1 => proto_attributes::UPriority::UPRIORITY_CS1,
        UPriority::CS2 => proto_attributes::UPriority::UPRIORITY_CS2,
        UPriority::CS3 => proto_attributes::UPriority::UPRIORITY_CS3,
        UPriority::CS4 => proto_attributes::UPriority::UPRIORITY_CS4,
        UPriority::CS5 => proto_attributes::UPriority::UPRIORITY_CS5,
        UPriority::CS6 => proto_attributes::UPriority::UPRIORITY_CS6,
    }
}

fn proto_priority_to_native(
    priority: proto_attributes::UPriority,
) -> Result<UPriority, UFrameWireError> {
    match priority {
        proto_attributes::UPriority::UPRIORITY_UNSPECIFIED => Ok(UPriority::default()),
        proto_attributes::UPriority::UPRIORITY_CS0 => Ok(UPriority::CS0),
        proto_attributes::UPriority::UPRIORITY_CS1 => Ok(UPriority::CS1),
        proto_attributes::UPriority::UPRIORITY_CS2 => Ok(UPriority::CS2),
        proto_attributes::UPriority::UPRIORITY_CS3 => Ok(UPriority::CS3),
        proto_attributes::UPriority::UPRIORITY_CS4 => Ok(UPriority::CS4),
        proto_attributes::UPriority::UPRIORITY_CS5 => Ok(UPriority::CS5),
        proto_attributes::UPriority::UPRIORITY_CS6 => Ok(UPriority::CS6),
    }
}

fn native_code_to_proto(code: UCode) -> proto_ucode::UCode {
    match code {
        UCode::OK => proto_ucode::UCode::OK,
        UCode::CANCELLED => proto_ucode::UCode::CANCELLED,
        UCode::UNKNOWN => proto_ucode::UCode::UNKNOWN,
        UCode::INVALID_ARGUMENT => proto_ucode::UCode::INVALID_ARGUMENT,
        UCode::DEADLINE_EXCEEDED => proto_ucode::UCode::DEADLINE_EXCEEDED,
        UCode::NOT_FOUND => proto_ucode::UCode::NOT_FOUND,
        UCode::ALREADY_EXISTS => proto_ucode::UCode::ALREADY_EXISTS,
        UCode::PERMISSION_DENIED => proto_ucode::UCode::PERMISSION_DENIED,
        UCode::RESOURCE_EXHAUSTED => proto_ucode::UCode::RESOURCE_EXHAUSTED,
        UCode::FAILED_PRECONDITION => proto_ucode::UCode::FAILED_PRECONDITION,
        UCode::ABORTED => proto_ucode::UCode::ABORTED,
        UCode::OUT_OF_RANGE => proto_ucode::UCode::OUT_OF_RANGE,
        UCode::UNIMPLEMENTED => proto_ucode::UCode::UNIMPLEMENTED,
        UCode::INTERNAL => proto_ucode::UCode::INTERNAL,
        UCode::UNAVAILABLE => proto_ucode::UCode::UNAVAILABLE,
        UCode::DATA_LOSS => proto_ucode::UCode::DATA_LOSS,
        UCode::UNAUTHENTICATED => proto_ucode::UCode::UNAUTHENTICATED,
    }
}

fn proto_code_to_native(code: proto_ucode::UCode) -> UCode {
    UCode::from_u8(code as u8).unwrap_or(UCode::UNKNOWN)
}

fn payload_encoding_to_proto(
    encoding: &UEncoding,
) -> Result<proto_attributes::UPayloadFormat, UFrameWireError> {
    if encoding.schema_ref().is_some() {
        return Err(UFrameWireError::unsupported_payload_encoding(
            "generated UMessage payload_format cannot preserve schema_ref",
        ));
    }
    match (encoding.format_id(), encoding.content_type()) {
        ("raw-bytes", "application/octet-stream") => {
            Ok(proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_RAW)
        }
        ("protobuf", "application/x-protobuf") => {
            Ok(proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_PROTOBUF_WRAPPED_IN_ANY)
        }
        ("protobuf", "application/protobuf") => {
            Ok(proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_PROTOBUF)
        }
        ("json", "application/json") => Ok(proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_JSON),
        ("text", "text/plain") => Ok(proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_TEXT),
        ("someip", "application/x-someip") => {
            Ok(proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_SOMEIP)
        }
        ("someip-tlv", "application/x-someip_tlv") => {
            Ok(proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_SOMEIP_TLV)
        }
        ("shm", "application/x-shm") => Ok(proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_SHM),
        _ => Err(UFrameWireError::unsupported_payload_encoding(format!(
            "format_id={}, content_type={}",
            encoding.format_id(),
            encoding.content_type()
        ))),
    }
}

fn proto_payload_format_to_encoding(
    payload_format: proto_attributes::UPayloadFormat,
) -> Result<UEncoding, UFrameWireError> {
    match payload_format {
        proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_UNSPECIFIED => Err(
            UFrameWireError::invalid_frame("payload_format is unspecified"),
        ),
        proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_PROTOBUF_WRAPPED_IN_ANY => Ok(
            UEncoding::without_schema_ref("protobuf", "application/x-protobuf"),
        ),
        proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_PROTOBUF => Ok(
            UEncoding::without_schema_ref("protobuf", "application/protobuf"),
        ),
        proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_JSON => {
            Ok(UEncoding::without_schema_ref("json", "application/json"))
        }
        proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_SOMEIP => Ok(
            UEncoding::without_schema_ref("someip", "application/x-someip"),
        ),
        proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_SOMEIP_TLV => Ok(
            UEncoding::without_schema_ref("someip-tlv", "application/x-someip_tlv"),
        ),
        proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_RAW => Ok(UEncoding::without_schema_ref(
            "raw-bytes",
            "application/octet-stream",
        )),
        proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_TEXT => {
            Ok(UEncoding::without_schema_ref("text", "text/plain"))
        }
        proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_SHM => {
            Ok(UEncoding::without_schema_ref("shm", "application/x-shm"))
        }
    }
}

#[cfg(test)]
mod tests {
    use ::protobuf::well_known_types::wrappers::StringValue;

    use crate::{
        frame_wire::UFrameWireFormat,
        payload::{RawBytes, UDeserializer, USerializer, UWireError},
        zero_copy::{UContiguousZeroCopyRxFrame, UTxBuffer, UVecTxBuffer, UZeroCopyRxFrame},
        UEncoding, UFrameMetadata, UOwnedFrame, UUri,
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

        let frame = UOwnedFrame::from_serializable::<ProtobufPayload, _>(
            UFrameMetadata::publish(topic),
            &input,
        )
        .unwrap();
        let decoded: StringValue = frame.deserialize::<ProtobufPayload, _>().unwrap();

        assert_eq!(
            frame.metadata().encoding(),
            Some(&ProtobufPayload::encoding())
        );
        assert_eq!(decoded.value, input.value);
    }

    #[test]
    fn zero_copy_buffer_round_trips_protobuf_payload() {
        let topic = UUri::try_from("//vehicle/4210/1/8001").unwrap();
        let input = message("zero-copy protobuf payload");
        let payload_len = <StringValue as USerializer<ProtobufPayload>>::encoded_len(&input);
        let mut buffer = UVecTxBuffer::new(
            UFrameMetadata::publish(topic).with_encoding(ProtobufPayload::encoding()),
            payload_len,
        );

        let written = input.serialize_into(buffer.payload_mut()).unwrap();
        assert_eq!(written, payload_len);

        let frame = buffer.into_frame();
        let decoded: StringValue = frame.deserialize_borrowed::<ProtobufPayload, _>().unwrap();
        let decoded_from_reader: StringValue = frame
            .deserialize_from_reader::<ProtobufPayload, _>()
            .unwrap();

        assert_eq!(
            frame.metadata().encoding(),
            Some(&ProtobufPayload::encoding())
        );
        assert_eq!(decoded.value, input.value);
        assert_eq!(decoded_from_reader.value, input.value);
    }

    #[test]
    fn protobuf_deserializer_rejects_invalid_payload() {
        let result = StringValue::deserialize_from(&[0x0a]);

        assert!(matches!(result, Err(UWireError::InvalidPayload(_))));
    }

    #[test]
    fn protobuf_payload_bytes_match_generated_message_bytes() {
        let input = message("legacy protobuf payload bytes");

        let encoded =
            <StringValue as USerializer<ProtobufPayload>>::serialize_owned(&input).unwrap();
        let expected = input.write_to_bytes().unwrap();

        assert_eq!(encoded.as_ref(), expected.as_slice());
    }

    #[test]
    fn protobuf_umessage_frame_bytes_match_generated_umessage_bytes() {
        let id = UUID::from_u64_pair(0x0000_0000_0001_7000, 0x8000_0000_0000_0000).unwrap();
        let source = UUri::try_from_parts("vehicle", 0x4210, 1, 0x8001).unwrap();
        let payload = Bytes::from_static(b"legacy protobuf UMessage frame");
        let attributes = UAttributes::new(id.clone(), source.clone(), None, UMessageType::Publish)
            .with_priority(UPriority::CS1);
        let frame = UOwnedFrame::with_payload(
            UFrameMetadata::new(attributes, RawBytes::encoding()),
            payload.clone(),
        );
        let generated_message = proto_message::UMessage {
            attributes: Some(proto_attributes::UAttributes {
                id: Some(crate::up_core_api::uuid::UUID {
                    msb: id.msb(),
                    lsb: id.lsb(),
                    ..Default::default()
                })
                .into(),
                type_: EnumOrUnknown::from(proto_attributes::UMessageType::UMESSAGE_TYPE_PUBLISH),
                source: Some(crate::up_core_api::uri::UUri {
                    authority_name: source.authority_name(),
                    ue_id: source.ue_id(),
                    ue_version_major: source.ue_version_major(),
                    resource_id: source.resource_id_raw(),
                    ..Default::default()
                })
                .into(),
                priority: EnumOrUnknown::from(proto_attributes::UPriority::UPRIORITY_CS1),
                payload_format: EnumOrUnknown::from(
                    proto_attributes::UPayloadFormat::UPAYLOAD_FORMAT_RAW,
                ),
                ..Default::default()
            })
            .into(),
            payload: Some(payload),
            ..Default::default()
        };
        let expected = generated_message.write_to_bytes().unwrap();
        let encoded = ProtobufUMessageFrame::serialize_frame(&frame).unwrap();
        let decoded = ProtobufUMessageFrame::deserialize_frame(&expected).unwrap();

        assert_eq!(encoded.as_ref(), expected.as_slice());
        assert_eq!(decoded, frame);
    }

    #[test]
    fn protobuf_umessage_frame_round_trips_raw_payload() {
        let topic = UUri::try_from("//vehicle/4210/1/8001").unwrap();
        let frame = UOwnedFrame::new(
            UFrameMetadata::publish(topic).with_encoding(RawBytes::encoding()),
            b"raw payload".as_slice(),
        );

        let encoded = ProtobufUMessageFrame::serialize_frame(&frame).unwrap();
        let decoded = ProtobufUMessageFrame::deserialize_frame(&encoded).unwrap();

        assert_eq!(
            decoded.metadata().attributes(),
            frame.metadata().attributes()
        );
        assert_eq!(decoded.metadata().encoding(), Some(&RawBytes::encoding()));
        assert_eq!(decoded.payload_bytes(), b"raw payload");
    }

    #[test]
    fn protobuf_umessage_frame_round_trips_protobuf_payload() {
        let topic = UUri::try_from("//vehicle/4210/1/8001").unwrap();
        let input = message("protobuf payload inside protobuf UMessage frame");
        let frame = UOwnedFrame::from_serializable::<ProtobufPayload, _>(
            UFrameMetadata::publish(topic),
            &input,
        )
        .unwrap();

        let encoded = ProtobufUMessageFrame::serialize_frame(&frame).unwrap();
        let decoded = ProtobufUMessageFrame::deserialize_frame(&encoded).unwrap();
        let decoded_payload: StringValue = decoded.deserialize::<ProtobufPayload, _>().unwrap();

        assert_eq!(
            decoded.metadata().attributes(),
            frame.metadata().attributes()
        );
        assert_eq!(
            decoded.metadata().encoding(),
            Some(&ProtobufPayload::encoding())
        );
        assert_eq!(decoded_payload.value, input.value);
    }

    #[test]
    fn protobuf_umessage_frame_rejects_unrepresentable_payload_encoding() {
        let topic = UUri::try_from("//vehicle/4210/1/8001").unwrap();
        let frame = UOwnedFrame::new(
            UFrameMetadata::publish(topic).with_encoding(UEncoding::with_schema_ref(
                "raw-bytes",
                "application/octet-stream",
                "urn:example:Schema:v1",
            )),
            b"schema payload".as_slice(),
        );

        let error = ProtobufUMessageFrame::serialize_frame(&frame).unwrap_err();

        assert!(matches!(
            error,
            crate::UFrameWireError::UnsupportedPayloadEncoding(message)
                if message.contains("schema_ref")
        ));
    }

    #[test]
    fn payload_decoder_rejects_payload_from_wrong_inner_codec_after_frame_decode() {
        let topic = UUri::try_from("//vehicle/4210/1/8001").unwrap();
        let frame = UOwnedFrame::new(
            UFrameMetadata::publish(topic).with_encoding(RawBytes::encoding()),
            b"not protobuf".as_slice(),
        );
        let encoded = ProtobufUMessageFrame::serialize_frame(&frame).unwrap();
        let decoded = ProtobufUMessageFrame::deserialize_frame(&encoded).unwrap();

        let result = decoded.deserialize::<ProtobufPayload, StringValue>();

        assert!(matches!(
            result,
            Err(UWireError::UnsupportedEncoding { .. })
        ));
    }
}
