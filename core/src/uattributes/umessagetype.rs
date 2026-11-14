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

//! The four uProtocol message kinds.

const CE_TYPE_PUBLISH: &str = "up-pub.v1";
const CE_TYPE_NOTIFICATION: &str = "up-not.v1";
const CE_TYPE_REQUEST: &str = "up-req.v1";
const CE_TYPE_RESPONSE: &str = "up-res.v1";

/// The four uProtocol message kinds.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
#[repr(C)]
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

    /// The CloudEvent type name of this message type.
    ///
    /// The `String`-returning `to_cloudevent_type` lives on
    /// `up_rust::prelude::UMessageTypeExt`.
    #[must_use]
    pub const fn cloudevent_type(self) -> &'static str {
        match self {
            Self::Publish => CE_TYPE_PUBLISH,
            Self::Request => CE_TYPE_REQUEST,
            Self::Response => CE_TYPE_RESPONSE,
            Self::Notification => CE_TYPE_NOTIFICATION,
        }
    }

    /// Resolves a CloudEvent type name, or `None` for an unknown name.
    #[must_use]
    pub fn from_cloudevent_type(name: &str) -> Option<Self> {
        match name {
            CE_TYPE_PUBLISH => Some(Self::Publish),
            CE_TYPE_NOTIFICATION => Some(Self::Notification),
            CE_TYPE_REQUEST => Some(Self::Request),
            CE_TYPE_RESPONSE => Some(Self::Response),
            _ => None,
        }
    }
}
