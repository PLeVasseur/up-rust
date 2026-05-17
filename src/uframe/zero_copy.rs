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

    /// Returns the payload as one contiguous byte slice.
    ///
    /// This is the ergonomic fast path for transports whose receive loans are
    /// naturally contiguous. Transports that can receive segmented payloads
    /// should override [`Self::payload_contiguous`], [`Self::payload_len`], and
    /// [`Self::for_each_payload_slice`] so generic adapters do not accidentally
    /// force a coalescing copy.
    fn payload(&self) -> &[u8];

    fn payload_len(&self) -> usize {
        self.payload().len()
    }

    fn payload_contiguous(&self) -> Option<&[u8]> {
        Some(self.payload())
    }

    fn for_each_payload_slice(&self, visitor: &mut dyn FnMut(&[u8])) {
        visitor(self.payload());
    }

    fn copy_payload_to(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        let expected = self.payload_len();
        if dst.len() < expected {
            return Err(UWireError::buffer_too_small(expected, dst.len()));
        }

        let mut written = 0_usize;
        let mut copy_result = Ok(());
        self.for_each_payload_slice(&mut |slice| {
            if copy_result.is_err() {
                return;
            }
            let Some(end) = written.checked_add(slice.len()) else {
                copy_result = Err(UWireError::invalid_payload("payload length overflow"));
                return;
            };
            let Some(target) = dst.get_mut(written..end) else {
                copy_result = Err(UWireError::buffer_too_small(expected, dst.len()));
                return;
            };
            target.copy_from_slice(slice);
            written = end;
        });
        copy_result?;
        if written != expected {
            return Err(UWireError::invalid_payload(format!(
                "payload slices yielded {written} bytes but payload_len returned {expected} bytes"
            )));
        }
        Ok(written)
    }

    fn payload_to_vec(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(self.payload_len());
        self.for_each_payload_slice(&mut |slice| payload.extend_from_slice(slice));
        payload
    }

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
        let payload = self.payload_contiguous().ok_or_else(|| {
            UWireError::invalid_payload(
                "borrowed deserialization requires a contiguous receive payload",
            )
        })?;
        T::deserialize_from(payload)
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
