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

use std::io::{Cursor, Read};

use super::{
    frame::{UFrameMetadata, UOwnedFrame},
    wire::{UDeserializer, UReadDeserializer, UWireError, WireFormat},
};

impl UZeroCopyRxFrame for UOwnedFrame {
    type PayloadReader<'a>
        = Cursor<&'a [u8]>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = std::iter::Once<&'a [u8]>
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata {
        self.metadata()
    }

    fn payload_len(&self) -> usize {
        self.payload_bytes().len()
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        Cursor::new(self.payload_bytes())
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        std::iter::once(self.payload_bytes())
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        Some(self.payload_bytes())
    }
}

impl UContiguousZeroCopyRxFrame for UOwnedFrame {
    fn contiguous_payload(&self) -> &[u8] {
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
    type PayloadReader<'a>: Read + 'a
    where
        Self: 'a;
    type PayloadSlices<'a>: Iterator<Item = &'a [u8]> + 'a
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata;
    fn payload_len(&self) -> usize;
    fn payload_reader(&self) -> Self::PayloadReader<'_>;
    fn payload_slices(&self) -> Self::PayloadSlices<'_>;

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        None
    }

    fn deserialize_from_reader<F, T>(&self) -> Result<T, UWireError>
    where
        F: WireFormat,
        T: UReadDeserializer<F>,
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
        T::deserialize_from_reader(self.payload_reader(), self.payload_len())
    }
}

/// Receive-side frame lease with a guaranteed contiguous payload view.
pub trait UContiguousZeroCopyRxFrame: UZeroCopyRxFrame {
    fn contiguous_payload(&self) -> &[u8];

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
        T::deserialize_from(self.contiguous_payload())
    }
}

/// Explicit helpers for crossing from zero-copy receive leases into owned bytes.
pub trait UZeroCopyPayloadCopyExt: UZeroCopyRxFrame {
    fn copy_payload_to(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        let expected = self.payload_len();
        if dst.len() < expected {
            return Err(UWireError::buffer_too_small(expected, dst.len()));
        }

        let mut written = 0_usize;
        let mut copy_result = Ok(());
        for slice in self.payload_slices() {
            if copy_result.is_err() {
                break;
            }
            let Some(end) = written.checked_add(slice.len()) else {
                copy_result = Err(UWireError::invalid_payload("payload length overflow"));
                break;
            };
            let Some(target) = dst.get_mut(written..end) else {
                copy_result = Err(UWireError::buffer_too_small(expected, dst.len()));
                break;
            };
            target.copy_from_slice(slice);
            written = end;
        }
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
        for slice in self.payload_slices() {
            payload.extend_from_slice(slice);
        }
        payload
    }
}

impl<T> UZeroCopyPayloadCopyExt for T where T: UZeroCopyRxFrame + ?Sized {}

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
