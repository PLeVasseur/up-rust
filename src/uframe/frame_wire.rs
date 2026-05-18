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

use bytes::Bytes;

use super::frame::UOwnedFrame;

/// Error type used by whole-frame wire formats.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UFrameWireError {
    InvalidFrame(String),
    UnsupportedPayloadEncoding(String),
    SerializationError(String),
}

impl UFrameWireError {
    pub fn invalid_frame(message: impl Into<String>) -> Self {
        Self::InvalidFrame(message.into())
    }

    pub fn unsupported_payload_encoding(message: impl Into<String>) -> Self {
        Self::UnsupportedPayloadEncoding(message.into())
    }

    pub fn serialization_error(message: impl Into<String>) -> Self {
        Self::SerializationError(message.into())
    }
}

impl Display for UFrameWireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFrame(message) => write!(f, "invalid frame: {message}"),
            Self::UnsupportedPayloadEncoding(message) => {
                write!(f, "unsupported payload encoding: {message}")
            }
            Self::SerializationError(message) => write!(f, "frame serialization error: {message}"),
        }
    }
}

impl Error for UFrameWireError {}

/// Whole-frame wire format for transporting a complete uProtocol frame.
///
/// A frame wire format serializes and deserializes the outer carrier: frame
/// attributes, payload encoding metadata, and payload bytes. This is distinct
/// from payload codecs, which only transform an application value into the frame
/// payload bytes.
pub trait UFrameWireFormat {
    fn name() -> &'static str;

    fn content_type() -> &'static str;

    fn serialize_frame(frame: &UOwnedFrame) -> Result<Bytes, UFrameWireError>;

    fn deserialize_frame(src: &[u8]) -> Result<UOwnedFrame, UFrameWireError>;
}
