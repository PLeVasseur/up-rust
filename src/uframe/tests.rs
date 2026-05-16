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

use super::*;
use crate::{UCode, UUri, UUID};

#[test]
fn raw_bytes_serialize_and_deserialize_without_copying_on_read() {
    let input: &[u8] = &[1, 2, 3, 4];

    let payload = input.serialize_owned().unwrap();
    let decoded = <&[u8] as UDeserializer<RawBytes>>::deserialize_from(&payload).unwrap();

    assert_eq!(decoded, input);
}

struct ShortWriteSerializer;

impl USerializer<RawBytes> for ShortWriteSerializer {
    fn encoded_len(&self) -> usize {
        2
    }

    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        let actual = dst.len();
        let out = dst
            .get_mut(..1)
            .ok_or_else(|| UWireError::buffer_too_small(1, actual))?;
        *out.first_mut()
            .ok_or_else(|| UWireError::buffer_too_small(1, actual))? = 0x01;
        Ok(1)
    }
}

#[test]
fn owned_serializer_rejects_mismatched_written_length() {
    let error = ShortWriteSerializer.serialize_owned().unwrap_err();

    assert!(
        matches!(error, UWireError::InvalidPayload(message) if message.contains("encoded_len returned 2"))
    );
}

#[test]
fn owned_frame_distinguishes_absent_payload_from_empty_payload() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let absent = UFrameBuilder::publish(topic.clone()).build().unwrap();
    let empty = UFrameBuilder::publish(topic)
        .build_with_raw_payload(Vec::<u8>::new())
        .unwrap();

    assert!(!absent.has_payload());
    assert_eq!(absent.metadata().encoding(), None);
    assert_eq!(absent.payload(), None);
    assert_eq!(absent.payload_bytes(), b"");
    assert!(empty.has_payload());
    assert_eq!(empty.metadata().encoding(), Some(&RawBytes::encoding()));
    assert_eq!(empty.payload_bytes(), b"");
}

#[test]
fn owned_frame_deserialize_rejects_absent_payload() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UFrameBuilder::publish(topic).build().unwrap();

    assert!(matches!(
        frame.deserialize::<RawBytes, &[u8]>(),
        Err(UWireError::MissingPayload)
    ));
}

#[test]
fn encoding_treats_empty_schema_ref_as_absent() {
    let encoding = UEncoding::new("json", "application/json", Some(""));

    assert_eq!(encoding.schema_ref(), None);
}

#[test]
fn encoding_rejects_invalid_content_type() {
    let error = UEncoding::try_new("json", "not a media type", None::<String>).unwrap_err();

    assert!(matches!(error, UEncodingError::InvalidContentType(_)));
}

#[test]
fn owned_frame_uses_selected_wire_format() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UOwnedFrame::from_serializable::<RawBytes, _>(
        UFrameMetadata::publish(topic),
        &&[0x0a_u8, 0x0b_u8][..],
    )
    .unwrap();

    assert_eq!(frame.metadata().encoding(), Some(&RawBytes::encoding()));
    assert_eq!(frame.payload_bytes(), &[0x0a_u8, 0x0b_u8]);
}

struct OtherWire;

impl WireFormat for OtherWire {
    fn name() -> &'static str {
        "other"
    }

    fn encoding() -> UEncoding {
        UEncoding::new(Self::name(), "application/x-other", None::<String>)
    }
}

impl<'a> UDeserializer<'a, OtherWire> for &'a [u8] {
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
        Ok(src)
    }
}

struct OtherSchemaWire;

impl WireFormat for OtherSchemaWire {
    fn name() -> &'static str {
        "raw-other-schema"
    }

    fn encoding() -> UEncoding {
        UEncoding::with_schema_ref(
            "raw-bytes",
            "application/octet-stream",
            "urn:example:Other:v1",
        )
    }
}

impl<'a> UDeserializer<'a, OtherSchemaWire> for &'a [u8] {
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
        Ok(src)
    }
}

#[test]
fn owned_frame_deserialize_rejects_wrong_wire_format() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UOwnedFrame::from_serializable::<RawBytes, _>(
        UFrameMetadata::publish(topic),
        &&[0x0a_u8, 0x0b_u8][..],
    )
    .unwrap();

    assert!(matches!(
        frame.deserialize::<OtherWire, &[u8]>(),
        Err(UWireError::UnsupportedEncoding { .. })
    ));
}

#[test]
fn owned_frame_deserialize_allows_generic_decoder_for_schema_ref() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(topic).with_encoding(UEncoding::with_schema_ref(
            "raw-bytes",
            "application/octet-stream",
            "urn:example:Bytes:v1",
        )),
        vec![0x0a_u8, 0x0b_u8],
    );

    assert_eq!(
        frame.deserialize::<RawBytes, &[u8]>().unwrap(),
        &[0x0a_u8, 0x0b_u8]
    );
}

#[test]
fn owned_frame_deserialize_rejects_wrong_schema_ref() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(topic).with_encoding(UEncoding::with_schema_ref(
            "raw-bytes",
            "application/octet-stream",
            "urn:example:Bytes:v1",
        )),
        vec![0x0a_u8, 0x0b_u8],
    );

    assert!(matches!(
        frame.deserialize::<OtherSchemaWire, &[u8]>(),
        Err(UWireError::UnsupportedEncoding { .. })
    ));
}

#[test]
fn frame_builder_builds_publish_frame_with_raw_payload() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let message_id = UUID::build();
    let traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let frame = UFrameBuilder::publish(topic.clone())
        .with_message_id(message_id.clone())
        .with_priority(UPriority::CS2)
        .with_ttl(5_000)
        .with_traceparent(traceparent)
        .build_with_raw_payload(vec![0x01, 0x02])
        .unwrap();

    let attributes = frame.metadata().attributes();
    assert_eq!(attributes.id(), &message_id);
    assert_eq!(attributes.message_type(), UMessageType::Publish);
    assert_eq!(attributes.priority(), UPriority::CS2);
    assert_eq!(attributes.source(), &topic);
    assert_eq!(attributes.sink(), None);
    assert_eq!(attributes.ttl(), Some(5_000));
    assert_eq!(attributes.traceparent(), Some(traceparent));
    assert_eq!(frame.metadata().encoding(), Some(&RawBytes::encoding()));
    assert_eq!(frame.payload_bytes(), &[0x01, 0x02]);
}

#[test]
fn frame_builder_builds_response_from_request_attributes() {
    let method = UUri::try_from("//vehicle/4210/1/0001").unwrap();
    let reply_to = UUri::try_from("//client/ABCD/1/0000").unwrap();
    let request = UFrameBuilder::request(method.clone(), reply_to.clone(), 5_000)
        .with_priority(UPriority::CS5)
        .build()
        .unwrap();
    let response_id = UUID::build();
    let response = UFrameBuilder::response_for_request(request.metadata().attributes())
        .with_message_id(response_id.clone())
        .with_comm_status(UCode::DEADLINE_EXCEEDED)
        .build()
        .unwrap();

    let attributes = response.metadata().attributes();
    assert_eq!(attributes.id(), &response_id);
    assert_eq!(attributes.message_type(), UMessageType::Response);
    assert_eq!(attributes.priority(), UPriority::CS5);
    assert_eq!(attributes.source(), &method);
    assert_eq!(attributes.sink(), Some(&reply_to));
    assert_eq!(
        attributes.request_id(),
        Some(request.metadata().attributes().id())
    );
    assert_eq!(attributes.commstatus(), Some(UCode::DEADLINE_EXCEEDED));
    assert_eq!(attributes.ttl(), Some(5_000));
}

#[test]
fn frame_builder_rejects_low_rpc_priority() {
    let method = UUri::try_from("//vehicle/4210/1/0001").unwrap();
    let reply_to = UUri::try_from("//client/ABCD/1/0000").unwrap();
    let result = UFrameBuilder::request(method, reply_to, 5_000)
        .with_priority(UPriority::CS3)
        .build();

    assert!(matches!(
        result,
        Err(UFrameBuilderError::AttributesValidationError(_))
    ));
}

#[test]
fn frame_builder_uses_selected_wire_format_for_typed_payload() {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D").unwrap();
    let frame = UFrameBuilder::publish(topic)
        .build_with_serializable::<RawBytes, _>(&&[0x0a_u8, 0x0b_u8][..])
        .unwrap();

    assert_eq!(frame.metadata().encoding(), Some(&RawBytes::encoding()));
    assert_eq!(frame.payload_bytes(), &[0x0a_u8, 0x0b_u8]);
}
