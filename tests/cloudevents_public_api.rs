// SPDX-License-Identifier: Apache-2.0
//! External consumers can name every type reachable from `CloudEvent` fields.
#![cfg(feature = "cloudevents")]

use up_rust::cloud_event::{cloud_event_attribute_value::Attr, Data};
use up_rust::prelude::*;
use up_rust::{
    CloudEvent, CloudEventAttributeValue, PayloadEncoding, UMessage, UMessageBuilder, UPriority,
    UUri,
};

fn attribute(value: &CloudEventAttributeValue) -> Option<&Attr> {
    value.attr.as_ref()
}

fn payload(event: &CloudEvent) -> Option<&Data> {
    event.data.as_ref()
}

fn string_value(text: &str) -> CloudEventAttributeValue {
    let mut value = CloudEventAttributeValue::new();
    value.set_ce_string(text.to_owned());
    value
}

#[test]
fn consumer_signatures_name_attribute_and_data_types() {
    let topic = UUri::try_from_parts("vehicle", 0x1_0001, 1, 0x8001).unwrap();
    let message = UMessageBuilder::publish(topic)
        .with_priority(UPriority::CS3)
        .build_with_payload("payload", PayloadEncoding::TEXT)
        .unwrap();
    let mut event = CloudEvent::try_from(message).unwrap();
    event
        .attributes
        .insert("xroute".to_owned(), string_value("bridge-1"));

    assert!(matches!(
        event.attributes.get("priority").and_then(attribute),
        Some(Attr::CeString(code)) if code == "CS3"
    ));
    assert!(matches!(
        event.attributes.get("pformat").and_then(attribute),
        Some(Attr::CeInteger(_))
    ));
    assert!(matches!(
        event.attributes.get("xroute").and_then(attribute),
        Some(Attr::CeString(route)) if route == "bridge-1"
    ));
    assert!(matches!(
        payload(&event),
        Some(Data::BinaryData(bytes)) if bytes.as_slice() == b"payload"
    ));

    // The uProtocol mapping ignores the custom extension attribute.
    let message = UMessage::try_from(event).unwrap();
    assert_eq!(message.priority(), Some(UPriority::CS3));
    assert_eq!(message.payload_encoding(), Some(PayloadEncoding::TEXT));
}
