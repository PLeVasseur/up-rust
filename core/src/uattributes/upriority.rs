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

//! QoS priority classes.

/// QoS priority classes defined by the uProtocol specification.
#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(C)]
pub enum UPriority {
    /// Best effort (lowest priority).
    CS0,
    /// Priority class CS1.
    CS1,
    /// Priority class CS2.
    CS2,
    /// Priority class CS3.
    CS3,
    /// Priority class CS4 (streaming).
    CS4,
    /// Priority class CS5 (RPC default).
    CS5,
    /// Priority class CS6 (highest; network control).
    CS6,
}

impl UPriority {
    /// Returns the explicit selected-wire/ABI code, CS0 = 1 through CS6 = 7.
    ///
    /// Rust enum discriminants are not serialization codes. Zero denotes an
    /// absent priority in the encoded representation, rather than a QoS class.
    #[must_use]
    pub const fn wire_code(self) -> u8 {
        match self {
            Self::CS0 => 1,
            Self::CS1 => 2,
            Self::CS2 => 3,
            Self::CS3 => 4,
            Self::CS4 => 5,
            Self::CS5 => 6,
            Self::CS6 => 7,
        }
    }

    /// Decodes an explicit selected-wire/ABI code; absent/reserved codes return `None`.
    #[must_use]
    pub const fn from_wire_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::CS0),
            2 => Some(Self::CS1),
            3 => Some(Self::CS2),
            4 => Some(Self::CS3),
            5 => Some(Self::CS4),
            6 => Some(Self::CS5),
            7 => Some(Self::CS6),
            _ => None,
        }
    }

    /// Encodes this priority to a string.
    ///
    /// The encoding of priorities to strings is defined in the
    /// [uProtocol Core API](https://github.com/eclipse-uprotocol/up-core-api/blob/main/uprotocol/uattributes.proto).
    #[must_use]
    pub const fn to_priority_code(self) -> &'static str {
        match self {
            Self::CS0 => "CS0",
            Self::CS1 => "CS1",
            Self::CS2 => "CS2",
            Self::CS3 => "CS3",
            Self::CS4 => "CS4",
            Self::CS5 => "CS5",
            Self::CS6 => "CS6",
        }
    }

    /// Gets the priority for a string, or `None` for an unknown code.
    ///
    /// The `std`-only `UPriority::try_from_priority_code` keeps today's signature
    /// and error type; it lives on `up_rust::prelude::UPriorityExt`.
    #[must_use]
    pub fn from_priority_code(code: &str) -> Option<Self> {
        match code {
            "CS0" => Some(Self::CS0),
            "CS1" => Some(Self::CS1),
            "CS2" => Some(Self::CS2),
            "CS3" => Some(Self::CS3),
            "CS4" => Some(Self::CS4),
            "CS5" => Some(Self::CS5),
            "CS6" => Some(Self::CS6),
            _ => None,
        }
    }
}
