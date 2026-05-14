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

use std::{error::Error, fmt::Display};

/// Native uProtocol status code.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[allow(non_camel_case_types)]
pub enum UCode {
    #[default]
    OK,
    CANCELLED,
    UNKNOWN,
    INVALID_ARGUMENT,
    DEADLINE_EXCEEDED,
    NOT_FOUND,
    ALREADY_EXISTS,
    PERMISSION_DENIED,
    RESOURCE_EXHAUSTED,
    FAILED_PRECONDITION,
    ABORTED,
    OUT_OF_RANGE,
    UNIMPLEMENTED,
    INTERNAL,
    UNAVAILABLE,
    DATA_LOSS,
    UNAUTHENTICATED,
}

impl UCode {
    pub fn as_u8(self) -> u8 {
        match self {
            Self::OK => 0,
            Self::CANCELLED => 1,
            Self::UNKNOWN => 2,
            Self::INVALID_ARGUMENT => 3,
            Self::DEADLINE_EXCEEDED => 4,
            Self::NOT_FOUND => 5,
            Self::ALREADY_EXISTS => 6,
            Self::PERMISSION_DENIED => 7,
            Self::RESOURCE_EXHAUSTED => 8,
            Self::FAILED_PRECONDITION => 9,
            Self::ABORTED => 10,
            Self::OUT_OF_RANGE => 11,
            Self::UNIMPLEMENTED => 12,
            Self::INTERNAL => 13,
            Self::UNAVAILABLE => 14,
            Self::DATA_LOSS => 15,
            Self::UNAUTHENTICATED => 16,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::OK),
            1 => Some(Self::CANCELLED),
            2 => Some(Self::UNKNOWN),
            3 => Some(Self::INVALID_ARGUMENT),
            4 => Some(Self::DEADLINE_EXCEEDED),
            5 => Some(Self::NOT_FOUND),
            6 => Some(Self::ALREADY_EXISTS),
            7 => Some(Self::PERMISSION_DENIED),
            8 => Some(Self::RESOURCE_EXHAUSTED),
            9 => Some(Self::FAILED_PRECONDITION),
            10 => Some(Self::ABORTED),
            11 => Some(Self::OUT_OF_RANGE),
            12 => Some(Self::UNIMPLEMENTED),
            13 => Some(Self::INTERNAL),
            14 => Some(Self::UNAVAILABLE),
            15 => Some(Self::DATA_LOSS),
            16 => Some(Self::UNAUTHENTICATED),
            _ => None,
        }
    }
}

/// Native uProtocol status value.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UStatus {
    code: UCode,
    message: String,
}

impl UStatus {
    /// Creates a status representing success.
    pub fn ok() -> Self {
        Self {
            code: UCode::OK,
            message: String::new(),
        }
    }

    /// Creates a failure status with [`UCode::UNKNOWN`].
    pub fn fail<M: Into<String>>(msg: M) -> Self {
        Self::fail_with_code(UCode::UNKNOWN, msg)
    }

    /// Creates a failure status with an explicit code.
    pub fn fail_with_code<M: Into<String>>(code: UCode, msg: M) -> Self {
        Self {
            code,
            message: msg.into(),
        }
    }

    /// Checks whether this status represents failure.
    pub fn is_failed(&self) -> bool {
        self.code != UCode::OK
    }

    /// Checks whether this status represents success.
    pub fn is_success(&self) -> bool {
        self.code == UCode::OK
    }

    /// Gets the status message.
    pub fn get_message(&self) -> String {
        self.message.clone()
    }

    /// Gets the status code.
    pub fn get_code(&self) -> UCode {
        self.code
    }
}

impl Display for UStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.message.is_empty() {
            write!(f, "{:?}", self.code)
        } else {
            write!(f, "{:?}: {}", self.code, self.message)
        }
    }
}

impl Error for UStatus {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_and_failure_statuses_work_without_generated_types() {
        assert!(UStatus::ok().is_success());
        assert!(UStatus::fail_with_code(UCode::DATA_LOSS, "lost").is_failed());
        assert_eq!(
            UStatus::fail_with_code(UCode::DATA_LOSS, "lost").get_code(),
            UCode::DATA_LOSS
        );
    }

    #[test]
    fn status_codes_have_stable_byte_representation() {
        for code in [
            UCode::OK,
            UCode::CANCELLED,
            UCode::UNKNOWN,
            UCode::INVALID_ARGUMENT,
            UCode::DEADLINE_EXCEEDED,
            UCode::NOT_FOUND,
            UCode::ALREADY_EXISTS,
            UCode::PERMISSION_DENIED,
            UCode::RESOURCE_EXHAUSTED,
            UCode::FAILED_PRECONDITION,
            UCode::ABORTED,
            UCode::OUT_OF_RANGE,
            UCode::UNIMPLEMENTED,
            UCode::INTERNAL,
            UCode::UNAVAILABLE,
            UCode::DATA_LOSS,
            UCode::UNAUTHENTICATED,
        ] {
            assert_eq!(UCode::from_u8(code.as_u8()), Some(code));
        }

        assert_eq!(UCode::from_u8(17), None);
    }
}
