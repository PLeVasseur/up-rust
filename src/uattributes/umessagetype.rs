/********************************************************************************
 * Copyright (c) 2023 Contributors to the Eclipse Foundation
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

use crate::uattributes::UAttributesError;

const CE_TYPE_PUBLISH: &str = "up-pub.v1";
const CE_TYPE_NOTIFICATION: &str = "up-not.v1";
const CE_TYPE_REQUEST: &str = "up-req.v1";
const CE_TYPE_RESPONSE: &str = "up-res.v1";

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
#[repr(C)]
/// The four uProtocol message kinds.
pub enum UMessageType {
    /// A publication to a topic; delivered to all subscribers.
    Publish,
    /// An RPC request targeting a method.
    Request,
    /// An RPC response correlated to a request.
    Response,
    /// A notification targeting one uEntity.
    Notification,
}

impl UMessageType {
    /// Returns the explicit selected-wire/ABI code for this message kind.
    ///
    /// These codes are independent of Rust enum discriminants: publish = 1,
    /// request = 2, response = 3 and notification = 4.
    #[must_use]
    pub const fn wire_code(self) -> u8 {
        match self {
            Self::Publish => 1,
            Self::Request => 2,
            Self::Response => 3,
            Self::Notification => 4,
        }
    }

    /// Decodes an explicit selected-wire/ABI code; reserved codes return `None`.
    #[must_use]
    pub const fn from_wire_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Publish),
            2 => Some(Self::Request),
            3 => Some(Self::Response),
            4 => Some(Self::Notification),
            _ => None,
        }
    }

    /// Gets this message type's CloudEvent type name.
    ///
    /// # Returns
    ///
    /// The value to use for the *type* property when mapping to a CloudEvent.
    #[must_use]
    pub fn to_cloudevent_type(&self) -> String {
        match self {
            UMessageType::Publish => CE_TYPE_PUBLISH,
            UMessageType::Request => CE_TYPE_REQUEST,
            UMessageType::Response => CE_TYPE_RESPONSE,
            UMessageType::Notification => CE_TYPE_NOTIFICATION,
        }
        .to_string()
    }

    /// Gets the message type for a CloudEvent type name.
    ///
    /// # Errors
    ///
    /// Returns a [`UAttributesError::ParsingError`] if the given name does not match
    /// any of the supported message types.
    pub fn try_from_cloudevent_type<S: Into<String>>(value: S) -> Result<Self, UAttributesError> {
        let type_string = value.into();
        match type_string.as_str() {
            CE_TYPE_PUBLISH => Ok(UMessageType::Publish),
            CE_TYPE_NOTIFICATION => Ok(UMessageType::Notification),
            CE_TYPE_REQUEST => Ok(UMessageType::Request),
            CE_TYPE_RESPONSE => Ok(UMessageType::Response),
            _ => Err(UAttributesError::parsing_error(format!(
                "unknown message type: {type_string}"
            ))),
        }
    }
}

#[cfg(feature = "up-core-api")]
mod core_types_support {
    use super::*;
    use crate::up_core_api::uattributes::UMessageType as UMessageTypeProto;

    impl TryFrom<UMessageTypeProto> for UMessageType {
        type Error = UAttributesError;

        fn try_from(proto_message_type: UMessageTypeProto) -> Result<Self, Self::Error> {
            match proto_message_type {
                UMessageTypeProto::UMESSAGE_TYPE_PUBLISH => Ok(UMessageType::Publish),
                UMessageTypeProto::UMESSAGE_TYPE_REQUEST => Ok(UMessageType::Request),
                UMessageTypeProto::UMESSAGE_TYPE_RESPONSE => Ok(UMessageType::Response),
                UMessageTypeProto::UMESSAGE_TYPE_NOTIFICATION => Ok(UMessageType::Notification),
                _ => Err(UAttributesError::parsing_error(format!(
                    "invalid UMessageType value: {}",
                    proto_message_type as i32
                ))),
            }
        }
    }

    impl From<&UMessageType> for UMessageTypeProto {
        fn from(value: &UMessageType) -> Self {
            match value {
                UMessageType::Publish => UMessageTypeProto::UMESSAGE_TYPE_PUBLISH,
                UMessageType::Request => UMessageTypeProto::UMESSAGE_TYPE_REQUEST,
                UMessageType::Response => UMessageTypeProto::UMESSAGE_TYPE_RESPONSE,
                UMessageType::Notification => UMessageTypeProto::UMESSAGE_TYPE_NOTIFICATION,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use test_case::test_case;

    use crate::{UAttributesError, UMessageType};

    const TYPE_PUBLISH: &str = "up-pub.v1";
    const TYPE_NOTIFICATION: &str = "up-not.v1";
    const TYPE_REQUEST: &str = "up-req.v1";
    const TYPE_RESPONSE: &str = "up-res.v1";

    #[test_case(1, Some(UMessageType::Publish); "publish code")]
    #[test_case(2, Some(UMessageType::Request); "request code")]
    #[test_case(3, Some(UMessageType::Response); "response code")]
    #[test_case(4, Some(UMessageType::Notification); "notification code")]
    #[test_case(0, None; "absent kind is invalid")]
    #[test_case(5, None; "first reserved kind")]
    #[test_case(255, None; "last reserved kind")]
    fn wire_kind_codes(code: u8, expected: Option<UMessageType>) {
        assert_eq!(UMessageType::from_wire_code(code), expected);
        if let Some(kind) = expected {
            assert_eq!(kind.wire_code(), code);
        }
    }

    #[test_case(UMessageType::Publish, TYPE_PUBLISH; "for PUBLISH")]
    #[test_case(UMessageType::Notification, TYPE_NOTIFICATION; "for NOTIFICATION")]
    #[test_case(UMessageType::Request, TYPE_REQUEST; "for REQUEST")]
    #[test_case(UMessageType::Response, TYPE_RESPONSE; "for RESPONSE")]
    fn test_to_cloudevent_type(message_type: UMessageType, expected_ce_name: &str) {
        assert_eq!(message_type.to_cloudevent_type(), expected_ce_name);
    }

    #[test_case(TYPE_PUBLISH, Some(UMessageType::Publish); "succeeds for PUBLISH")]
    #[test_case(TYPE_NOTIFICATION, Some(UMessageType::Notification); "succeeds for NOTIFICATION")]
    #[test_case(TYPE_REQUEST, Some(UMessageType::Request); "succeeds for REQUEST")]
    #[test_case(TYPE_RESPONSE, Some(UMessageType::Response); "succeeds for RESPONSE")]
    #[test_case("foo.bar", None; "fails for unknown type")]
    fn test_try_from_cloudevent_type(
        cloudevent_type: &str,
        expected_message_type: Option<UMessageType>,
    ) {
        let result = UMessageType::try_from_cloudevent_type(cloudevent_type);
        assert!(result.is_ok() == expected_message_type.is_some());
        if let Some(message_type) = expected_message_type {
            assert_eq!(result.unwrap(), message_type)
        } else {
            assert!(matches!(
                result.unwrap_err(),
                UAttributesError::ParsingError(_msg)
            ))
        }
    }
}
