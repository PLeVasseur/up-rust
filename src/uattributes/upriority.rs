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

pub use up_rust_core::uattributes::UPriority;

/// The classic conversion on [`UPriority`], with its classic error type.
pub trait UPriorityExt: Sized {
    /// Gets the priority for a string.
    ///
    /// The encoding of priorities to strings is defined in the
    /// [uProtocol Core API](https://github.com/eclipse-uprotocol/up-core-api/blob/main/uprotocol/uattributes.proto).
    ///
    /// # Errors
    ///
    /// Returns an error if the given string does not match a supported priority.
    ///
    /// # Examples
    ///
    /// ```
    /// use up_rust::{prelude::*, UPriority};
    ///
    /// let priority = UPriority::try_from_priority_code("CS2").unwrap();
    /// assert_eq!(priority, UPriority::CS2);
    ///
    /// assert!(UPriority::try_from_priority_code("not-supported").is_err());
    /// ```
    fn try_from_priority_code<T>(code: T) -> Result<Self, UAttributesError>
    where
        T: Into<String>;
}

impl UPriorityExt for UPriority {
    fn try_from_priority_code<T>(code: T) -> Result<Self, UAttributesError>
    where
        T: Into<String>,
    {
        let prio: String = code.into();
        UPriority::from_priority_code(prio.as_str()).ok_or_else(|| {
            UAttributesError::parsing_error(format!("unknown priority code [{prio}]"))
        })
    }
}

#[cfg(feature = "up-core-api")]
mod core_types_support {
    use super::*;
    use crate::up_core_api::uattributes::UPriority as UPriorityProto;

    impl From<&UPriority> for UPriorityProto {
        fn from(value: &UPriority) -> Self {
            match value {
                UPriority::CS0 => UPriorityProto::UPRIORITY_CS0,
                UPriority::CS1 => UPriorityProto::UPRIORITY_CS1,
                UPriority::CS2 => UPriorityProto::UPRIORITY_CS2,
                UPriority::CS3 => UPriorityProto::UPRIORITY_CS3,
                UPriority::CS4 => UPriorityProto::UPRIORITY_CS4,
                UPriority::CS5 => UPriorityProto::UPRIORITY_CS5,
                UPriority::CS6 => UPriorityProto::UPRIORITY_CS6,
            }
        }
    }

    impl TryFrom<UPriorityProto> for UPriority {
        type Error = UAttributesError;

        fn try_from(value: UPriorityProto) -> Result<Self, Self::Error> {
            match value {
                UPriorityProto::UPRIORITY_CS0 => Ok(UPriority::CS0),
                UPriorityProto::UPRIORITY_CS1 => Ok(UPriority::CS1),
                UPriorityProto::UPRIORITY_CS2 => Ok(UPriority::CS2),
                UPriorityProto::UPRIORITY_CS3 => Ok(UPriority::CS3),
                UPriorityProto::UPRIORITY_CS4 => Ok(UPriority::CS4),
                UPriorityProto::UPRIORITY_CS5 => Ok(UPriority::CS5),
                UPriorityProto::UPRIORITY_CS6 => Ok(UPriority::CS6),
                _ => Err(UAttributesError::parsing_error(format!(
                    "invalid UPriority value: {}",
                    value as i32
                ))),
            }
        }
    }
}

#[cfg(test)]
mod wire_code_tests {
    use super::UPriority;
    use test_case::test_case;

    #[test_case(1, Some(UPriority::CS0); "CS0")]
    #[test_case(2, Some(UPriority::CS1); "CS1")]
    #[test_case(3, Some(UPriority::CS2); "CS2")]
    #[test_case(4, Some(UPriority::CS3); "CS3")]
    #[test_case(5, Some(UPriority::CS4); "CS4")]
    #[test_case(6, Some(UPriority::CS5); "CS5")]
    #[test_case(7, Some(UPriority::CS6); "CS6")]
    #[test_case(0, None; "absent priority")]
    #[test_case(8, None; "first reserved priority")]
    #[test_case(255, None; "last reserved priority")]
    fn wire_priority_codes(code: u8, expected: Option<UPriority>) {
        assert_eq!(UPriority::from_wire_code(code), expected);
        if let Some(priority) = expected {
            assert_eq!(priority.wire_code(), code);
        }
    }
}
