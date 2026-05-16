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

use super::{
    frame::{UFrameMetadata, UOwnedFrame},
    wire::{UDeserializer, UWireError, WireFormat},
};

impl UZeroCopyRxFrame for UOwnedFrame {
    fn metadata(&self) -> &UFrameMetadata {
        self.metadata()
    }

    fn payload(&self) -> &[u8] {
        self.payload_bytes()
    }
}

/// Mutable transmit storage reserved from a zero-copy transport.
pub trait UTxBuffer {
    fn metadata(&self) -> &UFrameMetadata;
    fn metadata_mut(&mut self) -> &mut UFrameMetadata;
    fn payload(&self) -> &[u8];
    fn payload_mut(&mut self) -> &mut [u8];
}

/// Receive-side zero-copy frame lease.
pub trait UZeroCopyRxFrame {
    fn metadata(&self) -> &UFrameMetadata;
    fn payload(&self) -> &[u8];

    fn deserialize_borrowed<'a, F, T>(&'a self) -> Result<T, UWireError>
    where
        F: WireFormat,
        T: UDeserializer<'a, F>,
    {
        let expected = F::encoding();
        let actual = self
            .metadata()
            .encoding()
            .ok_or(UWireError::MissingEncoding)?;
        if !actual.is_compatible_with(&expected) {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(actual.clone()),
            });
        }
        T::deserialize_from(self.payload())
    }
}

/// Owned buffer useful for tests, examples, and adapters that emulate a transmit loan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UVecTxBuffer {
    metadata: UFrameMetadata,
    payload: Vec<u8>,
}

impl UVecTxBuffer {
    pub fn new(metadata: UFrameMetadata, payload_len: usize) -> Self {
        Self {
            metadata,
            payload: vec![0_u8; payload_len],
        }
    }

    pub fn into_frame(self) -> UOwnedFrame {
        UOwnedFrame::new(self.metadata, self.payload)
    }
}

impl AsRef<[u8]> for UVecTxBuffer {
    fn as_ref(&self) -> &[u8] {
        self.payload()
    }
}

impl UTxBuffer for UVecTxBuffer {
    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut UFrameMetadata {
        &mut self.metadata
    }

    fn payload(&self) -> &[u8] {
        self.payload.as_ref()
    }

    fn payload_mut(&mut self) -> &mut [u8] {
        self.payload.as_mut_slice()
    }
}
