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
    payload::{PayloadFormat, UDeserializer, UReadDeserializer, UWireError},
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
///
/// A transmit buffer is owned by the transport until it is committed with
/// [`UZeroCopyTransport::send_zero_copy`](crate::zero_copy::UZeroCopyTransport::send_zero_copy).
/// Frame metadata is fixed when the loan is reserved. Transports may use that
/// metadata to choose routes, encode native headers, compute payload offsets, or
/// allocate backing storage before handing the loan to the caller.
/// Serializers should write directly into [`Self::payload_mut`] so the payload
/// does not first have to be materialized as an owned [`Vec<u8>`] or
/// [`bytes::Bytes`].
pub trait UTxBuffer {
    /// Returns the immutable frame metadata associated with this transmit loan.
    fn metadata(&self) -> &UFrameMetadata;

    /// Returns the current payload bytes in the transmit loan.
    ///
    /// This view is borrowed from the loan and is valid only while `self` is
    /// borrowed.
    fn payload(&self) -> &[u8];

    /// Returns mutable payload storage for direct serialization into the loan.
    ///
    /// Implementations must expose exactly the payload range requested from the
    /// transport, excluding any transport metadata prefix, padding, or trailer.
    fn payload_mut(&mut self) -> &mut [u8];
}

/// Receive-side zero-copy frame lease.
///
/// The lease owns the lifetime of any borrowed payload views returned by this
/// trait. Dropping the lease releases the underlying transport storage. The base
/// receive capability is intentionally segmented-friendly: callers should prefer
/// [`Self::payload_reader`] or [`Self::payload_slices`] unless they explicitly
/// need a contiguous borrowed view.
pub trait UZeroCopyRxFrame {
    type PayloadReader<'a>: Read + 'a
    where
        Self: 'a;
    type PayloadSlices<'a>: Iterator<Item = &'a [u8]> + 'a
    where
        Self: 'a;

    /// Returns the native frame metadata reconstructed by the transport.
    fn metadata(&self) -> &UFrameMetadata;

    /// Returns the number of application payload bytes visible through this
    /// receive lease.
    ///
    /// Transport metadata prefixes, alignment padding, and protocol trailers
    /// must not be included in this length.
    fn payload_len(&self) -> usize;

    /// Returns an ordered reader over the application payload bytes.
    ///
    /// This is the preferred generic decode path because it works for both
    /// contiguous and segmented transport storage without forcing a coalescing
    /// copy.
    fn payload_reader(&self) -> Self::PayloadReader<'_>;

    /// Returns ordered borrowed payload slices.
    ///
    /// The iterator must yield the same byte sequence that was sent. It may
    /// yield more than one slice when the underlying transport stores payloads in
    /// segmented buffers.
    fn payload_slices(&self) -> Self::PayloadSlices<'_>;

    /// Returns a contiguous borrowed payload view when the transport can provide
    /// one without copying.
    ///
    /// A return value of `None` means callers should use [`Self::payload_reader`]
    /// or [`Self::payload_slices`]. Implementations must not allocate or coalesce
    /// segmented storage to satisfy this method.
    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        None
    }

    /// Deserializes this receive lease from its ordered payload reader.
    ///
    /// The method verifies the frame's [`UEncoding`](crate::UEncoding) against
    /// the selected [`PayloadFormat`] before invoking the reader-based deserializer.
    /// Use [`UContiguousZeroCopyRxFrame::deserialize_borrowed`] when the decoded
    /// value needs to borrow directly from a guaranteed contiguous payload.
    fn deserialize_from_reader<F, T>(&self) -> Result<T, UWireError>
    where
        F: PayloadFormat,
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
///
/// Implement this trait only for receive leases whose application payload is
/// always available as one borrowed byte slice without copying. Segmented
/// transports should implement only [`UZeroCopyRxFrame`] and rely on reader or
/// slice iteration based decoding.
pub trait UContiguousZeroCopyRxFrame: UZeroCopyRxFrame {
    /// Returns the application payload as one contiguous borrowed byte slice.
    ///
    /// The returned slice must exclude transport metadata, padding, and trailers
    /// and must remain valid only for the lifetime of the receive lease.
    fn contiguous_payload(&self) -> &[u8];

    /// Deserializes a value that may borrow directly from the contiguous payload.
    ///
    /// The method verifies the frame's [`UEncoding`](crate::UEncoding) against
    /// the selected [`PayloadFormat`] before invoking the borrowed deserializer.
    fn deserialize_borrowed<'a, F, T>(&'a self) -> Result<T, UWireError>
    where
        F: PayloadFormat,
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
///
/// These helpers intentionally copy. They are useful at adapter boundaries such
/// as routers that operate on [`UOwnedFrame`], but should not be used on a path
/// that claims to preserve end-to-end zero-copy delivery.
pub trait UZeroCopyPayloadCopyExt: UZeroCopyRxFrame {
    /// Copies the ordered payload bytes into `dst`.
    ///
    /// Returns the number of bytes copied, or an error if `dst` is too small or
    /// if the slice iterator does not produce exactly [`UZeroCopyRxFrame::payload_len`]
    /// bytes.
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

    /// Copies the ordered payload bytes into a newly allocated [`Vec<u8>`].
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
///
/// `UVecTxBuffer` implements [`UTxBuffer`] over an owned `Vec<u8>`. It is
/// intended for test fakes and examples; production shared-memory transports
/// should expose their own transport-specific loan type. Use [`UOwnedFrame`] as a
/// simple receive-side fake because it implements the zero-copy receive traits
/// over its owned payload bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UVecTxBuffer {
    metadata: UFrameMetadata,
    payload: Vec<u8>,
}

impl UVecTxBuffer {
    /// Creates an owned transmit buffer with `payload_len` zero-initialized bytes.
    pub fn new(metadata: UFrameMetadata, payload_len: usize) -> Self {
        Self {
            metadata,
            payload: vec![0_u8; payload_len],
        }
    }

    /// Converts the buffer into an owned frame, consuming the emulated loan.
    pub fn into_frame(self) -> UOwnedFrame {
        if self.metadata.encoding().is_some() {
            UOwnedFrame::new(self.metadata, self.payload)
        } else {
            UOwnedFrame::without_payload(self.metadata)
        }
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

    fn payload(&self) -> &[u8] {
        self.payload.as_ref()
    }

    fn payload_mut(&mut self) -> &mut [u8] {
        self.payload.as_mut_slice()
    }
}
