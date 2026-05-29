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

use std::{error::Error, fmt::Display};

use crate::{UAttributes, UMessageType, UPriority, UUri};

/// Error returned when native uProtocol attributes fail validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UAttributesError {
    Multiple(Vec<UAttributesError>),
    ValidationError(String),
    ParsingError(String),
}

impl UAttributesError {
    pub fn validation_error(message: impl Into<String>) -> Self {
        Self::ValidationError(message.into())
    }

    pub fn parsing_error(message: impl Into<String>) -> Self {
        Self::ParsingError(message.into())
    }
}

impl Display for UAttributesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Multiple(errors) => f.write_str(
                &errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; "),
            ),
            Self::ValidationError(error) => {
                f.write_fmt(format_args!("Validation failure: {error}"))
            }
            Self::ParsingError(error) => f.write_fmt(format_args!("Parsing error: {error}")),
        }
    }
}

impl Error for UAttributesError {}

/// Validates native [`UAttributes`] according to their message type.
pub trait UAttributesValidator: Send {
    fn validate(&self, attributes: &UAttributes) -> Result<(), UAttributesError>;

    fn validate_type(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        let expected_type = self.message_type();
        let actual_type = attributes.message_type();
        if actual_type == expected_type {
            Ok(())
        } else {
            Err(UAttributesError::validation_error(format!(
                "Wrong Message Type [{actual_type:?}]"
            )))
        }
    }

    fn validate_id(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        if attributes.id().is_uprotocol_uuid() {
            Ok(())
        } else {
            Err(UAttributesError::validation_error(
                "Attributes must contain valid uProtocol UUID in id property",
            ))
        }
    }

    fn message_type(&self) -> UMessageType;

    fn validate_source(&self, attributes: &UAttributes) -> Result<(), UAttributesError>;

    fn validate_sink(&self, attributes: &UAttributes) -> Result<(), UAttributesError>;
}

pub fn validate_rpc_priority(attributes: &UAttributes) -> Result<(), UAttributesError> {
    if attributes.priority().value() < UPriority::CS4.value() {
        Err(UAttributesError::validation_error(
            "RPC message must have a priority of at least CS4",
        ))
    } else {
        Ok(())
    }
}

/// Native attribute validator selector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UAttributesValidators {
    Publish,
    Notification,
    Request,
    Response,
}

impl UAttributesValidators {
    pub fn validator(&self) -> Box<dyn UAttributesValidator> {
        match self {
            Self::Publish => Box::new(PublishValidator),
            Self::Notification => Box::new(NotificationValidator),
            Self::Request => Box::new(RequestValidator),
            Self::Response => Box::new(ResponseValidator),
        }
    }

    pub fn get_validator_for_attributes(attributes: &UAttributes) -> Box<dyn UAttributesValidator> {
        Self::get_validator(attributes.message_type())
    }

    pub fn get_validator(message_type: UMessageType) -> Box<dyn UAttributesValidator> {
        match message_type {
            UMessageType::Publish => Box::new(PublishValidator),
            UMessageType::Notification => Box::new(NotificationValidator),
            UMessageType::Request => Box::new(RequestValidator),
            UMessageType::Response => Box::new(ResponseValidator),
        }
    }
}

/// Validates attributes describing a Publish frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PublishValidator;

impl UAttributesValidator for PublishValidator {
    fn validate(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        aggregate_errors([
            self.validate_type(attributes),
            self.validate_id(attributes),
            self.validate_source(attributes),
            self.validate_sink(attributes),
            validate_no_rpc_only_fields(attributes, "publish frames"),
            validate_no_request_id(attributes, "publish frames"),
        ])
    }

    fn message_type(&self) -> UMessageType {
        UMessageType::Publish
    }

    fn validate_source(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        attributes.source().verify_event().map_err(|error| {
            UAttributesError::validation_error(format!("Invalid source URI: {error}"))
        })
    }

    fn validate_sink(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        if attributes.sink().is_some() {
            Err(UAttributesError::validation_error(
                "Attributes for a publish message must not contain a sink URI",
            ))
        } else {
            Ok(())
        }
    }
}

/// Validates attributes describing a Notification frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NotificationValidator;

impl UAttributesValidator for NotificationValidator {
    fn validate(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        aggregate_errors([
            self.validate_type(attributes),
            self.validate_id(attributes),
            self.validate_source(attributes),
            self.validate_sink(attributes),
            validate_no_rpc_only_fields(attributes, "notification frames"),
            validate_no_request_id(attributes, "notification frames"),
        ])
    }

    fn message_type(&self) -> UMessageType {
        UMessageType::Notification
    }

    fn validate_source(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        let source = attributes.source();
        if source.is_rpc_response() {
            Err(UAttributesError::validation_error(
                "Origin must not be an RPC response URI",
            ))
        } else {
            source.verify_no_wildcards().map_err(|error| {
                UAttributesError::validation_error(format!("Invalid source URI: {error}"))
            })
        }
    }

    fn validate_sink(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        let Some(sink) = attributes.sink() else {
            return Err(UAttributesError::validation_error(
                "Attributes for a notification message must contain a sink URI",
            ));
        };
        if !sink.is_notification_destination() {
            Err(UAttributesError::validation_error(
                "Destination's resource ID must be 0",
            ))
        } else {
            sink.verify_no_wildcards().map_err(|error| {
                UAttributesError::validation_error(format!("Invalid sink URI: {error}"))
            })
        }
    }
}

/// Validates attributes describing an RPC Request frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RequestValidator;

impl RequestValidator {
    pub fn validate_ttl(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        match attributes.ttl() {
            Some(ttl) if ttl > 0 => Ok(()),
            Some(invalid_ttl) => Err(UAttributesError::validation_error(format!(
                "RPC request message's TTL must be a positive integer [{invalid_ttl}]"
            ))),
            None => Err(UAttributesError::validation_error(
                "RPC request message must contain a TTL",
            )),
        }
    }
}

impl UAttributesValidator for RequestValidator {
    fn validate(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        aggregate_errors([
            self.validate_type(attributes),
            self.validate_id(attributes),
            self.validate_ttl(attributes),
            self.validate_source(attributes),
            self.validate_sink(attributes),
            validate_rpc_priority(attributes),
            validate_no_commstatus(attributes, "request frames"),
            validate_no_request_id(attributes, "request frames"),
        ])
    }

    fn message_type(&self) -> UMessageType {
        UMessageType::Request
    }

    fn validate_source(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        UUri::verify_rpc_response(attributes.source()).map_err(|error| {
            UAttributesError::validation_error(format!("Invalid source URI: {error}"))
        })
    }

    fn validate_sink(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        let Some(sink) = attributes.sink() else {
            return Err(UAttributesError::validation_error(
                "Attributes for a request message must contain a method-to-invoke in the sink property",
            ));
        };
        UUri::verify_rpc_method(sink).map_err(|error| {
            UAttributesError::validation_error(format!("Invalid sink URI: {error}"))
        })
    }
}

/// Validates attributes describing an RPC Response frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResponseValidator;

impl ResponseValidator {
    pub fn validate_reqid(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        if attributes
            .request_id()
            .is_some_and(crate::UUID::is_uprotocol_uuid)
        {
            Ok(())
        } else {
            Err(UAttributesError::validation_error(
                "Request ID is not a valid uProtocol UUID",
            ))
        }
    }

    pub fn validate_commstatus(&self, _attributes: &UAttributes) -> Result<(), UAttributesError> {
        Ok(())
    }
}

impl UAttributesValidator for ResponseValidator {
    fn validate(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        aggregate_errors([
            self.validate_type(attributes),
            self.validate_id(attributes),
            self.validate_source(attributes),
            self.validate_sink(attributes),
            self.validate_reqid(attributes),
            self.validate_commstatus(attributes),
            validate_rpc_priority(attributes),
            validate_no_request_authorization(attributes, "response frames"),
        ])
    }

    fn message_type(&self) -> UMessageType {
        UMessageType::Response
    }

    fn validate_source(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        UUri::verify_rpc_method(attributes.source()).map_err(|error| {
            UAttributesError::validation_error(format!("Invalid source URI: {error}"))
        })
    }

    fn validate_sink(&self, attributes: &UAttributes) -> Result<(), UAttributesError> {
        let Some(sink) = attributes.sink() else {
            return Err(UAttributesError::validation_error("Missing Sink"));
        };
        UUri::verify_rpc_response(sink).map_err(|error| {
            UAttributesError::validation_error(format!("Invalid sink URI: {error}"))
        })
    }
}

fn aggregate_errors<const N: usize>(
    results: [Result<(), UAttributesError>; N],
) -> Result<(), UAttributesError> {
    let errors = results
        .into_iter()
        .filter_map(Result::err)
        .collect::<Vec<_>>();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(UAttributesError::Multiple(errors))
    }
}

fn validate_no_rpc_only_fields(
    attributes: &UAttributes,
    frame_kind: &str,
) -> Result<(), UAttributesError> {
    if attributes.token().is_some()
        || attributes.permission_level().is_some()
        || attributes.commstatus().is_some()
    {
        Err(UAttributesError::validation_error(format!(
            "{frame_kind} must not carry RPC-only attributes"
        )))
    } else {
        Ok(())
    }
}

fn validate_no_request_authorization(
    attributes: &UAttributes,
    frame_kind: &str,
) -> Result<(), UAttributesError> {
    if attributes.token().is_some() || attributes.permission_level().is_some() {
        Err(UAttributesError::validation_error(format!(
            "{frame_kind} must not carry request authorization attributes"
        )))
    } else {
        Ok(())
    }
}

fn validate_no_commstatus(
    attributes: &UAttributes,
    frame_kind: &str,
) -> Result<(), UAttributesError> {
    if attributes.commstatus().is_some() {
        Err(UAttributesError::validation_error(format!(
            "{frame_kind} must not carry communication status"
        )))
    } else {
        Ok(())
    }
}

fn validate_no_request_id(
    attributes: &UAttributes,
    frame_kind: &str,
) -> Result<(), UAttributesError> {
    if attributes.request_id().is_some() {
        Err(UAttributesError::validation_error(format!(
            "{frame_kind} must not carry a request ID"
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UCode, UUID};

    fn publish_topic() -> UUri {
        UUri::try_from_parts("vehicle", 0x5410, 0x01, 0xa010).unwrap()
    }

    fn origin() -> UUri {
        UUri::try_from_parts("vehicle", 0x3c00, 0x02, 0x9a00).unwrap()
    }

    fn destination() -> UUri {
        UUri::try_from_parts("vehicle", 0x3d07, 0x01, 0x0000).unwrap()
    }

    fn reply_to_address() -> UUri {
        UUri::try_from_parts("client", 0x010b, 0x01, 0x0000).unwrap()
    }

    fn method_to_invoke() -> UUri {
        UUri::try_from_parts("vehicle", 0x03ae, 0x01, 0x00e2).unwrap()
    }

    #[test]
    fn publish_validator_enforces_topic_source_and_no_sink() {
        let valid =
            UAttributes::new_unchecked(UUID::build(), publish_topic(), None, UMessageType::Publish);
        assert!(PublishValidator.validate(&valid).is_ok());

        let invalid = UAttributes::new_unchecked(
            UUID::build(),
            reply_to_address(),
            Some(destination()),
            UMessageType::Publish,
        );
        assert!(PublishValidator.validate(&invalid).is_err());
    }

    #[test]
    fn notification_validator_enforces_destination_sink() {
        let valid = UAttributes::new_unchecked(
            UUID::build(),
            origin(),
            Some(destination()),
            UMessageType::Notification,
        );
        assert!(NotificationValidator.validate(&valid).is_ok());

        let invalid = UAttributes::new_unchecked(
            UUID::build(),
            origin(),
            Some(method_to_invoke()),
            UMessageType::Notification,
        );
        assert!(NotificationValidator.validate(&invalid).is_err());
    }

    #[test]
    fn request_validator_enforces_rpc_fields() {
        let valid = UAttributes::new_unchecked(
            UUID::build(),
            reply_to_address(),
            Some(method_to_invoke()),
            UMessageType::Request,
        )
        .with_priority(UPriority::CS4)
        .with_ttl(1_000)
        .with_token("token");
        assert!(RequestValidator.validate(&valid).is_ok());

        let invalid = valid.clone().with_priority(UPriority::CS3);
        assert!(RequestValidator.validate(&invalid).is_err());

        let invalid = valid.with_comm_status(UCode::CANCELLED);
        assert!(RequestValidator.validate(&invalid).is_err());
    }

    #[test]
    fn response_validator_enforces_rpc_fields() {
        let valid = UAttributes::new_unchecked(
            UUID::build(),
            method_to_invoke(),
            Some(reply_to_address()),
            UMessageType::Response,
        )
        .with_priority(UPriority::CS4)
        .with_request_id(UUID::build());
        assert!(ResponseValidator.validate(&valid).is_ok());

        let invalid = valid.with_token("token");
        assert!(ResponseValidator.validate(&invalid).is_err());
    }
}
