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

//! Native CloudEvents mapping for serializer-neutral uProtocol frames.

use std::{collections::BTreeMap, error::Error, fmt::Display, str::FromStr};

use bytes::Bytes;

use crate::{
    UAttributes, UCode, UEncoding, UFrameMetadata, UMessageType, UOwnedFrame, UPriority, UUri, UUID,
};

pub const CLOUDEVENTS_SPEC_VERSION: &str = "1.0";
pub const CONTENT_TYPE_CLOUDEVENTS_JSON: &str = "application/cloudevents+json";

const EXTENSION_NAME_COMMSTATUS: &str = "commstatus";
const EXTENSION_NAME_FORMAT_ID: &str = "uformatid";
const EXTENSION_NAME_PERMISSION_LEVEL: &str = "plevel";
const EXTENSION_NAME_PRIORITY: &str = "priority";
const EXTENSION_NAME_REQUEST_ID: &str = "reqid";
const EXTENSION_NAME_SINK: &str = "sink";
const EXTENSION_NAME_TOKEN: &str = "token";
const EXTENSION_NAME_TRACEPARENT: &str = "traceparent";
const EXTENSION_NAME_TTL: &str = "ttl";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudEvent {
    pub spec_version: String,
    pub id: String,
    pub type_: String,
    pub source: String,
    pub data_content_type: Option<String>,
    pub data_schema: Option<String>,
    pub extensions: BTreeMap<String, CloudEventAttributeValue>,
    pub data: Option<Bytes>,
}

impl CloudEvent {
    pub fn new(id: String, type_: String, source: String) -> Self {
        Self {
            spec_version: CLOUDEVENTS_SPEC_VERSION.to_string(),
            id,
            type_,
            source,
            data_content_type: None,
            data_schema: None,
            extensions: BTreeMap::new(),
            data: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudEventAttributeValue {
    Integer(i64),
    String(String),
    UriRef(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudEventError {
    MissingAttribute(&'static str),
    InvalidAttribute(String),
    UnsupportedSpecVersion(String),
}

impl Display for CloudEventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingAttribute(attribute) => {
                write!(f, "missing CloudEvents attribute: {attribute}")
            }
            Self::InvalidAttribute(message) => {
                write!(f, "invalid CloudEvents attribute: {message}")
            }
            Self::UnsupportedSpecVersion(version) => {
                write!(f, "unsupported CloudEvents spec version: {version}")
            }
        }
    }
}

impl Error for CloudEventError {}

impl TryFrom<UOwnedFrame> for CloudEvent {
    type Error = CloudEventError;

    fn try_from(frame: UOwnedFrame) -> Result<Self, Self::Error> {
        let header = frame.metadata().clone();
        let attributes = header.attributes();
        let mut event = Self::new(
            attributes.id().to_hyphenated_string(),
            message_type_to_cloud_event_type(attributes.message_type()).to_string(),
            attributes.source().to_uri(false),
        );
        if frame.has_payload() {
            let encoding = header
                .encoding()
                .ok_or(CloudEventError::MissingAttribute("payload encoding"))?;
            event.data_content_type = Some(encoding.content_type().to_string());
            event.data_schema = encoding.schema_ref().map(str::to_string);
            event.extensions.insert(
                EXTENSION_NAME_FORMAT_ID.to_string(),
                CloudEventAttributeValue::String(encoding.format_id().to_string()),
            );
        }
        if let Some(sink) = attributes.sink() {
            event.extensions.insert(
                EXTENSION_NAME_SINK.to_string(),
                CloudEventAttributeValue::UriRef(sink.to_uri(false)),
            );
        }
        if attributes.priority() != UPriority::default() {
            event.extensions.insert(
                EXTENSION_NAME_PRIORITY.to_string(),
                CloudEventAttributeValue::String(
                    priority_to_code(attributes.priority()).to_string(),
                ),
            );
        }
        if let Some(ttl) = attributes.ttl() {
            event.extensions.insert(
                EXTENSION_NAME_TTL.to_string(),
                CloudEventAttributeValue::Integer(i64::from(ttl)),
            );
        }
        if let Some(request_id) = attributes.request_id() {
            event.extensions.insert(
                EXTENSION_NAME_REQUEST_ID.to_string(),
                CloudEventAttributeValue::String(request_id.to_hyphenated_string()),
            );
        }
        if let Some(traceparent) = attributes.traceparent() {
            event.extensions.insert(
                EXTENSION_NAME_TRACEPARENT.to_string(),
                CloudEventAttributeValue::String(traceparent.to_string()),
            );
        }
        if let Some(token) = attributes.token() {
            event.extensions.insert(
                EXTENSION_NAME_TOKEN.to_string(),
                CloudEventAttributeValue::String(token.to_string()),
            );
        }
        if let Some(permission_level) = attributes.permission_level() {
            event.extensions.insert(
                EXTENSION_NAME_PERMISSION_LEVEL.to_string(),
                CloudEventAttributeValue::Integer(i64::from(permission_level)),
            );
        }
        if let Some(commstatus) = attributes.commstatus() {
            event.extensions.insert(
                EXTENSION_NAME_COMMSTATUS.to_string(),
                CloudEventAttributeValue::Integer(i64::from(commstatus.as_u8())),
            );
        }
        event.data = frame.into_payload();
        Ok(event)
    }
}

impl TryFrom<CloudEvent> for UOwnedFrame {
    type Error = CloudEventError;

    fn try_from(event: CloudEvent) -> Result<Self, Self::Error> {
        if event.spec_version != CLOUDEVENTS_SPEC_VERSION {
            return Err(CloudEventError::UnsupportedSpecVersion(event.spec_version));
        }
        let id = UUID::from_str(&event.id)
            .map_err(|error| CloudEventError::InvalidAttribute(error.to_string()))?;
        let source = UUri::try_from(event.source.as_str())
            .map_err(|error| CloudEventError::InvalidAttribute(error.to_string()))?;
        let sink = optional_uri_extension(&event, EXTENSION_NAME_SINK)?;
        let message_type = cloud_event_type_to_message_type(&event.type_)?;
        let mut attributes = UAttributes::new(id, source, sink, message_type);

        if let Some(priority) = optional_string_extension(&event, EXTENSION_NAME_PRIORITY)? {
            attributes = attributes.with_priority(code_to_priority(&priority)?);
        }
        if let Some(ttl) = optional_u32_extension(&event, EXTENSION_NAME_TTL)? {
            attributes = attributes.with_ttl(ttl);
        }
        if let Some(request_id) = optional_string_extension(&event, EXTENSION_NAME_REQUEST_ID)? {
            let request_id = UUID::from_str(&request_id)
                .map_err(|error| CloudEventError::InvalidAttribute(error.to_string()))?;
            attributes = attributes.with_request_id(request_id);
        }
        if let Some(traceparent) = optional_string_extension(&event, EXTENSION_NAME_TRACEPARENT)? {
            attributes = attributes.with_traceparent(traceparent);
        }
        if let Some(token) = optional_string_extension(&event, EXTENSION_NAME_TOKEN)? {
            attributes = attributes.with_token(token);
        }
        if let Some(permission_level) =
            optional_u32_extension(&event, EXTENSION_NAME_PERMISSION_LEVEL)?
        {
            attributes = attributes.with_permission_level(permission_level);
        }
        if let Some(commstatus) = optional_u8_extension(&event, EXTENSION_NAME_COMMSTATUS)? {
            let commstatus = UCode::from_u8(commstatus).ok_or_else(|| {
                CloudEventError::InvalidAttribute("unsupported commstatus".to_string())
            })?;
            attributes = attributes.with_comm_status(commstatus);
        }

        if let Some(data) = event.data.clone() {
            let format_id = required_string_extension(&event, EXTENSION_NAME_FORMAT_ID)?;
            let content_type = event
                .data_content_type
                .clone()
                .ok_or(CloudEventError::MissingAttribute("datacontenttype"))?;
            let schema_ref = event.data_schema.clone();
            let encoding = UEncoding::try_new(format_id, content_type, schema_ref)
                .map_err(|error| CloudEventError::InvalidAttribute(error.to_string()))?;
            let frame = UOwnedFrame::new(UFrameMetadata::new(attributes, encoding), data);
            validate_cloud_event_frame(&frame)?;
            Ok(frame)
        } else {
            let frame =
                UOwnedFrame::without_payload(UFrameMetadata::without_payload_encoding(attributes));
            validate_cloud_event_frame(&frame)?;
            Ok(frame)
        }
    }
}

fn validate_cloud_event_frame(frame: &UOwnedFrame) -> Result<(), CloudEventError> {
    frame
        .metadata()
        .validate()
        .map_err(|error| CloudEventError::InvalidAttribute(error.to_string()))?;
    match (frame.has_payload(), frame.metadata().encoding().is_some()) {
        (true, true) | (false, false) => Ok(()),
        (true, false) => Err(CloudEventError::InvalidAttribute(
            "payload is present but payload encoding is absent".to_string(),
        )),
        (false, true) => Err(CloudEventError::InvalidAttribute(
            "payload encoding is present but payload is absent".to_string(),
        )),
    }
}

fn optional_string_extension(
    event: &CloudEvent,
    name: &'static str,
) -> Result<Option<String>, CloudEventError> {
    event
        .extensions
        .get(name)
        .map(|value| match value {
            CloudEventAttributeValue::String(value) | CloudEventAttributeValue::UriRef(value) => {
                Ok(value.clone())
            }
            CloudEventAttributeValue::Integer(_) => Err(CloudEventError::InvalidAttribute(
                format!("extension {name} must be a string"),
            )),
        })
        .transpose()
}

fn required_string_extension(
    event: &CloudEvent,
    name: &'static str,
) -> Result<String, CloudEventError> {
    optional_string_extension(event, name)?.ok_or(CloudEventError::MissingAttribute(name))
}

fn optional_uri_extension(
    event: &CloudEvent,
    name: &'static str,
) -> Result<Option<UUri>, CloudEventError> {
    optional_string_extension(event, name)?
        .map(|value| {
            UUri::try_from(value.as_str())
                .map_err(|error| CloudEventError::InvalidAttribute(error.to_string()))
        })
        .transpose()
}

fn optional_u32_extension(
    event: &CloudEvent,
    name: &'static str,
) -> Result<Option<u32>, CloudEventError> {
    event
        .extensions
        .get(name)
        .map(|value| match value {
            CloudEventAttributeValue::Integer(value) => u32::try_from(*value).map_err(|error| {
                CloudEventError::InvalidAttribute(format!("extension {name}: {error}"))
            }),
            CloudEventAttributeValue::String(_) | CloudEventAttributeValue::UriRef(_) => Err(
                CloudEventError::InvalidAttribute(format!("extension {name} must be an integer")),
            ),
        })
        .transpose()
}

fn optional_u8_extension(
    event: &CloudEvent,
    name: &'static str,
) -> Result<Option<u8>, CloudEventError> {
    optional_u32_extension(event, name)?
        .map(|value| {
            u8::try_from(value).map_err(|error| {
                CloudEventError::InvalidAttribute(format!("extension {name}: {error}"))
            })
        })
        .transpose()
}

fn message_type_to_cloud_event_type(message_type: UMessageType) -> &'static str {
    match message_type {
        UMessageType::Publish => "up-pub.v1",
        UMessageType::Notification => "up-not.v1",
        UMessageType::Request => "up-req.v1",
        UMessageType::Response => "up-res.v1",
    }
}

fn cloud_event_type_to_message_type(type_: &str) -> Result<UMessageType, CloudEventError> {
    match type_ {
        "up-pub.v1" => Ok(UMessageType::Publish),
        "up-not.v1" => Ok(UMessageType::Notification),
        "up-req.v1" => Ok(UMessageType::Request),
        "up-res.v1" => Ok(UMessageType::Response),
        _ => Err(CloudEventError::InvalidAttribute(format!(
            "unsupported CloudEvents type {type_}"
        ))),
    }
}

fn priority_to_code(priority: UPriority) -> &'static str {
    match priority {
        UPriority::CS0 => "CS0",
        UPriority::CS1 => "CS1",
        UPriority::CS2 => "CS2",
        UPriority::CS3 => "CS3",
        UPriority::CS4 => "CS4",
        UPriority::CS5 => "CS5",
        UPriority::CS6 => "CS6",
    }
}

fn code_to_priority(priority: &str) -> Result<UPriority, CloudEventError> {
    match priority {
        "CS0" => Ok(UPriority::CS0),
        "CS1" => Ok(UPriority::CS1),
        "CS2" => Ok(UPriority::CS2),
        "CS3" => Ok(UPriority::CS3),
        "CS4" => Ok(UPriority::CS4),
        "CS5" => Ok(UPriority::CS5),
        "CS6" => Ok(UPriority::CS6),
        _ => Err(CloudEventError::InvalidAttribute(format!(
            "unsupported priority {priority}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MESSAGE_ID: &str = "00000000-0001-7000-8010-101010101a1a";
    const SOURCE: &str = "//vehicle/4210/1/8001";
    const RPC_METHOD: &str = "//vehicle/4210/1/1";
    const SINK: &str = "//vehicle/4210/1/0";
    const TRACEPARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00";

    #[test]
    fn owned_frame_round_trips_through_native_cloudevent() {
        let id = MESSAGE_ID.parse::<UUID>().unwrap();
        let request_id = UUID::build();
        let source = UUri::try_from(RPC_METHOD).unwrap();
        let sink = UUri::try_from(SINK).unwrap();
        let attributes = UAttributes::new(
            id.clone(),
            source.clone(),
            Some(sink.clone()),
            UMessageType::Response,
        )
        .with_priority(UPriority::CS4)
        .with_ttl(5_000)
        .with_request_id(request_id.clone())
        .with_traceparent(TRACEPARENT)
        .with_comm_status(UCode::UNAVAILABLE);
        let encoding = UEncoding::new(
            "protobuf",
            "application/x-protobuf",
            Some("type.googleapis.com/google.protobuf.StringValue"),
        );
        let frame = UOwnedFrame::new(
            UFrameMetadata::new(attributes, encoding.clone()),
            Bytes::from_static(b"payload"),
        );

        let event = CloudEvent::try_from(frame).unwrap();
        assert_eq!(event.spec_version, CLOUDEVENTS_SPEC_VERSION);
        assert_eq!(event.id, MESSAGE_ID);
        assert_eq!(event.type_, "up-res.v1");
        assert_eq!(event.source, RPC_METHOD);
        assert_eq!(
            event.data_content_type.as_deref(),
            Some("application/x-protobuf")
        );
        assert_eq!(
            event.data_schema.as_deref(),
            Some("type.googleapis.com/google.protobuf.StringValue")
        );

        let frame = UOwnedFrame::try_from(event).unwrap();
        let received = frame.metadata().attributes();

        assert_eq!(received.id(), &id);
        assert_eq!(received.source(), &source);
        assert_eq!(received.sink(), Some(&sink));
        assert_eq!(received.message_type(), UMessageType::Response);
        assert_eq!(received.priority(), UPriority::CS4);
        assert_eq!(received.ttl(), Some(5_000));
        assert_eq!(received.request_id(), Some(&request_id));
        assert_eq!(received.traceparent(), Some(TRACEPARENT));
        assert_eq!(received.commstatus(), Some(UCode::UNAVAILABLE));
        assert_eq!(frame.metadata().encoding(), Some(&encoding));
        assert_eq!(frame.payload_bytes(), b"payload");
    }

    #[test]
    fn invalid_cloudevent_spec_version_is_rejected() {
        let mut event = CloudEvent::new(
            MESSAGE_ID.to_string(),
            "up-pub.v1".to_string(),
            SOURCE.to_string(),
        );
        event.spec_version = "0.3".to_string();

        assert!(matches!(
            UOwnedFrame::try_from(event),
            Err(CloudEventError::UnsupportedSpecVersion(_))
        ));
    }

    #[test]
    fn invalid_cloudevent_payload_encoding_is_rejected_without_panic() {
        let mut event = CloudEvent::new(
            MESSAGE_ID.to_string(),
            "up-pub.v1".to_string(),
            SOURCE.to_string(),
        );
        event.data = Some(Bytes::from_static(b"payload"));
        event.data_content_type = Some("not a media type".to_string());
        event.extensions.insert(
            EXTENSION_NAME_FORMAT_ID.to_string(),
            CloudEventAttributeValue::String("protobuf".to_string()),
        );

        assert!(matches!(
            UOwnedFrame::try_from(event),
            Err(CloudEventError::InvalidAttribute(_))
        ));
    }

    #[test]
    fn invalid_cloudevent_message_type_metadata_is_rejected() {
        let mut event = CloudEvent::new(
            MESSAGE_ID.to_string(),
            "up-pub.v1".to_string(),
            SOURCE.to_string(),
        );
        event.extensions.insert(
            EXTENSION_NAME_REQUEST_ID.to_string(),
            CloudEventAttributeValue::String(UUID::build().to_hyphenated_string()),
        );

        assert!(matches!(
            UOwnedFrame::try_from(event),
            Err(CloudEventError::InvalidAttribute(_))
        ));
    }
}
