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

use bytes::Bytes;

use crate::{
    wire::{RawBytes, UDeserializer, USerializer, UWireError, WireFormat},
    UEncoding,
};

/// Native payload bytes plus their serializer-neutral encoding metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UPayload {
    encoding: UEncoding,
    payload: Bytes,
}

impl UPayload {
    /// Creates a payload from bytes and explicit encoding metadata.
    pub fn new<T: Into<Bytes>>(payload: T, encoding: UEncoding) -> Self {
        Self {
            encoding,
            payload: payload.into(),
        }
    }

    /// Creates a raw byte payload.
    pub fn from_raw<T: Into<Bytes>>(payload: T) -> Self {
        Self::new(payload, RawBytes::encoding())
    }

    /// Serializes a typed payload into its wire format.
    pub fn from_serializable<F, T>(value: &T) -> Result<Self, UWireError>
    where
        F: WireFormat,
        T: USerializer<F>,
    {
        Ok(Self::new(value.serialize_owned()?, F::encoding()))
    }

    /// Deserializes the payload using a selected wire format.
    pub fn deserialize<'a, F, T>(&'a self) -> Result<T, UWireError>
    where
        F: WireFormat,
        T: UDeserializer<'a, F>,
    {
        if !self.encoding.is_compatible_with(&F::encoding()) {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(F::encoding()),
                actual: Box::new(self.encoding.clone()),
            });
        }
        T::deserialize_from(self.payload_bytes())
    }

    /// Gets the payload encoding metadata.
    pub fn encoding(&self) -> &UEncoding {
        &self.encoding
    }

    /// Gets the payload bytes.
    pub fn payload_bytes(&self) -> &[u8] {
        self.payload.as_ref()
    }

    /// Consumes this payload and returns its bytes.
    pub fn payload(self) -> Bytes {
        self.payload
    }

    /// Consumes this payload and returns encoding metadata and bytes.
    pub fn into_parts(self) -> (UEncoding, Bytes) {
        (self.encoding, self.payload)
    }
}
