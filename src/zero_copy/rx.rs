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

use super::*;

/// Neutral view of frame metadata plus ordered payload bytes.
pub trait UFrameView {
    type PayloadReader<'a>: Read + 'a
    where
        Self: 'a;
    type PayloadSlices<'a>: Iterator<Item = &'a [u8]> + 'a
    where
        Self: 'a;

    /// Returns the native frame metadata.
    fn metadata(&self) -> &UFrameMetadata;

    /// Returns the number of application payload bytes visible through this view.
    fn payload_len(&self) -> usize;

    /// Returns whether this view carries a payload, including a present empty payload.
    fn has_payload(&self) -> bool {
        self.payload_len() > 0 || self.metadata().payload_encoding().is_some()
    }

    /// Returns an ordered reader over the application payload bytes.
    fn payload_reader(&self) -> Self::PayloadReader<'_>;

    /// Returns ordered borrowed payload slices.
    fn payload_slices(&self) -> Self::PayloadSlices<'_>;

    /// Returns a contiguous borrowed payload view when this view can provide one without copying.
    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        None
    }

    /// Decodes this frame view from its ordered payload reader with codec `C`.
    ///
    /// This path works for both contiguous and segmented receive storage without
    /// forcing a coalescing copy before the selected codec sees the bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the frame has no payload, has missing or incompatible
    /// encoding metadata, or if the codec cannot decode the payload bytes.
    fn decode_payload_from_reader_as<C, T>(&self) -> Result<T, UWireError>
    where
        C: PayloadCodec + ReadDecodePayload<T>,
    {
        C::verify_encoding(self.metadata().payload_encoding())?;
        if !self.has_payload() {
            return Err(UWireError::MissingPayload);
        }
        C::decode_payload_from_reader(self.payload_reader(), self.payload_len())
    }
}

#[cfg(feature = "owned-frame-transport")]
impl UFrameView for UOwnedFrame {
    type PayloadReader<'a>
        = Cursor<&'a [u8]>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = std::option::IntoIter<&'a [u8]>
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata {
        self.metadata()
    }

    fn payload_len(&self) -> usize {
        self.payload_bytes().len()
    }

    fn has_payload(&self) -> bool {
        UOwnedFrame::has_payload(self)
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        Cursor::new(self.payload_bytes())
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        self.payload().map(bytes::Bytes::as_ref).into_iter()
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        self.payload().map(bytes::Bytes::as_ref)
    }
}

/// Receive-side zero-copy frame lease returned by a transport.
pub trait UZeroCopyRxLease: UFrameView {}

/// Receive lease that can expose a contiguous payload from loan-backed storage.
pub trait ULoanedContiguousZeroCopyRxFrame: UZeroCopyRxLease {
    /// Returns one contiguous loan-backed application payload view.
    ///
    /// Implementations must not allocate, copy, or coalesce payload bytes to
    /// satisfy this method.
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError>;

    /// Returns diagnostic provenance for successful loaned payload borrows.
    fn payload_loan_provenance(&self) -> Result<PayloadLoanProvenance, UWireError> {
        Ok(self.loaned_contiguous_payload()?.provenance())
    }

    /// Returns only loan-backed contiguous payload bytes.
    fn try_loaned_contiguous_payload(&self) -> Result<&[u8], UWireError> {
        Ok(self.loaned_contiguous_payload()?.as_bytes())
    }

    /// Borrows one stable-container value from loan-backed contiguous storage.
    fn borrow_stable_payload<T>(&self) -> Result<&T, UWireError>
    where
        T: StablePayload,
    {
        self.borrow_payload_as::<StableContainerPayload<T>, T>()
    }

    /// Borrows one typed value from loan-backed contiguous storage using codec `C`.
    ///
    /// This is the low-level codec-selected receive form. Selected-wire receive
    /// wrappers expose a wire-selected `borrow_payload` helper so ordinary callers
    /// do not need to name `C`.
    fn borrow_payload_as<C, T>(&self) -> Result<&T, UWireError>
    where
        C: BorrowPayload<T>,
    {
        C::verify_encoding(self.metadata().payload_encoding())?;
        if !self.has_payload() {
            return Err(UWireError::MissingPayload);
        }
        let payload = self.loaned_contiguous_payload()?;
        C::borrow_payload(payload.as_bytes())
    }
}

/// A handler for processing zero-copy receive leases.
#[async_trait]
pub trait UZeroCopyListener<Rx>: Send + Sync
where
    Rx: UZeroCopyRxLease + Send + 'static,
{
    /// Handles one received zero-copy frame lease.
    async fn on_receive_zero_copy(&self, frame: Rx);
}

/// Validates a frame view before returning it from a public zero-copy boundary.
///
/// # Errors
///
/// Returns an error if metadata is invalid, payload presence disagrees with
/// payload encoding, or ordered slices do not match `payload_len`.
pub fn validate_frame_view_for_transport(
    frame: &(impl UFrameView + ?Sized),
) -> Result<(), UStatus> {
    validate_metadata(frame.metadata())?;
    validate_payload_presence(
        frame.has_payload(),
        frame.metadata().payload_encoding().is_some(),
        "frame view",
    )?;
    let mut observed = 0_usize;
    for slice in frame.payload_slices() {
        observed = observed
            .checked_add(slice.len())
            .ok_or_else(|| internal_zero_copy_error("frame view payload slices overflow usize"))?;
    }
    if observed != frame.payload_len() {
        return Err(internal_zero_copy_error(format!(
            "frame view payload slices yielded {observed} bytes but payload_len returned {}",
            frame.payload_len()
        )));
    }
    Ok(())
}
