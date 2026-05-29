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
    payload::UWireError,
    zero_copy::{PayloadLoanProvenance, UTxBuffer, UZeroCopyRxLease},
};

/// Experimental frame view that keeps receive storage alive while routing.
///
/// `LoanedFrame` is a copy-minimized routing building block. It lets generic
/// routing code inspect metadata and copy ordered payload slices directly into an
/// egress transmit loan without first materializing an intermediate
/// [`UOwnedFrame`] payload buffer. It does not imply zero-copy-preserving
/// forwarding: the generic helper in this module still copies payload bytes into
/// the egress loan.
///
/// This API is experimental and available only with the
/// `experimental-loaned-frame` Cargo feature.
pub trait LoanedFrame: Send {
    /// Returns the native frame metadata reconstructed by the ingress transport.
    fn metadata(&self) -> &UFrameMetadata;

    /// Returns whether this frame carries a payload, including a present empty
    /// payload.
    fn has_payload(&self) -> bool;

    /// Returns the number of visible application payload bytes.
    fn payload_len(&self) -> usize;

    /// Returns the backing transport loan class when it is known.
    ///
    /// `None` means the frame is owned bytes or the erased receive lease cannot
    /// expose a useful transport-domain classification. Callers must not infer
    /// zero-copy-preserving forwarding capability from this value alone.
    fn payload_loan_provenance(&self) -> Option<PayloadLoanProvenance>;

    /// Returns a contiguous borrowed payload view when one is available without
    /// coalescing or allocation.
    fn try_contiguous_payload(&self) -> Option<&[u8]>;

    /// Visits ordered borrowed payload slices.
    ///
    /// Implementations must yield the same byte sequence described by
    /// [`Self::payload_len`]. They may yield no slices for no-payload frames and
    /// may yield an empty slice for present-empty payloads.
    fn visit_payload_slices(&self, visit: &mut dyn FnMut(&[u8]));
}

impl LoanedFrame for UOwnedFrame {
    fn metadata(&self) -> &UFrameMetadata {
        self.metadata()
    }

    fn has_payload(&self) -> bool {
        UOwnedFrame::has_payload(self)
    }

    fn payload_len(&self) -> usize {
        self.payload_bytes().len()
    }

    fn payload_loan_provenance(&self) -> Option<PayloadLoanProvenance> {
        None
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        self.payload().map(|payload| payload.as_ref())
    }

    fn visit_payload_slices(&self, visit: &mut dyn FnMut(&[u8])) {
        if self.has_payload() {
            visit(self.payload_bytes());
        }
    }
}

/// Experimental adapter that owns a zero-copy receive lease as a [`LoanedFrame`].
///
/// The wrapped receive lease remains alive for as long as this value is alive, so
/// borrowed payload slices returned through [`LoanedFrame`] cannot outlive the
/// transport lease they come from.
pub struct ZeroCopyLoanedFrame<Rx> {
    frame: Rx,
    payload_loan_provenance: Option<PayloadLoanProvenance>,
}

impl<Rx> ZeroCopyLoanedFrame<Rx> {
    /// Creates a loaned-frame adapter for a zero-copy receive lease.
    pub fn new(frame: Rx) -> Self {
        Self {
            frame,
            payload_loan_provenance: None,
        }
    }

    /// Creates a loaned-frame adapter with an explicit backing loan class.
    pub fn with_payload_loan_provenance(
        frame: Rx,
        payload_loan_provenance: PayloadLoanProvenance,
    ) -> Self {
        Self {
            frame,
            payload_loan_provenance: Some(payload_loan_provenance),
        }
    }

    /// Returns the wrapped receive lease.
    pub fn into_inner(self) -> Rx {
        self.frame
    }
}

impl<Rx> LoanedFrame for ZeroCopyLoanedFrame<Rx>
where
    Rx: UZeroCopyRxLease + Send,
{
    fn metadata(&self) -> &UFrameMetadata {
        self.frame.metadata()
    }

    fn has_payload(&self) -> bool {
        self.frame.has_payload()
    }

    fn payload_len(&self) -> usize {
        self.frame.payload_len()
    }

    fn payload_loan_provenance(&self) -> Option<PayloadLoanProvenance> {
        self.payload_loan_provenance
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        if self.frame.has_payload() {
            self.frame.try_contiguous_payload()
        } else {
            None
        }
    }

    fn visit_payload_slices(&self, visit: &mut dyn FnMut(&[u8])) {
        if self.frame.has_payload() {
            for slice in self.frame.payload_slices() {
                visit(slice);
            }
        }
    }
}

/// Copies a loaned frame's ordered payload slices into an egress transmit loan.
///
/// This helper avoids an intermediate owned payload allocation but still copies
/// payload bytes into `buffer`. The caller is responsible for reserving `buffer`
/// with compatible metadata and exactly [`LoanedFrame::payload_len`] visible
/// payload bytes.
pub fn copy_loaned_frame_payload_to_tx(
    frame: &(impl LoanedFrame + ?Sized),
    buffer: &mut impl UTxBuffer,
) -> Result<usize, UWireError> {
    let expected = frame.payload_len();
    let dst = buffer.payload_mut();
    if dst.len() != expected {
        return Err(UWireError::invalid_payload_length(expected, dst.len()));
    }

    let mut written = 0_usize;
    let mut copy_result = Ok(());
    frame.visit_payload_slices(&mut |slice| {
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
            "loaned frame payload slices yielded {written} bytes but payload_len returned {expected} bytes"
        )));
    }

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use crate::uframe::{PayloadCodec, RawBytes, UFrameView, UVecTxBuffer};
    use crate::UUri;

    fn vec_as_slice(bytes: &Vec<u8>) -> &[u8] {
        bytes.as_slice()
    }

    struct SegmentedRxFrame {
        metadata: UFrameMetadata,
        segments: Vec<Vec<u8>>,
        has_payload: bool,
    }

    impl SegmentedRxFrame {
        fn new(metadata: UFrameMetadata, segments: Vec<Vec<u8>>) -> Self {
            Self {
                metadata,
                segments,
                has_payload: true,
            }
        }

        fn no_payload(metadata: UFrameMetadata) -> Self {
            Self {
                metadata,
                segments: Vec::new(),
                has_payload: false,
            }
        }
    }

    impl UFrameView for SegmentedRxFrame {
        type PayloadReader<'a>
            = Cursor<Vec<u8>>
        where
            Self: 'a;
        type PayloadSlices<'a>
            = std::iter::Map<std::slice::Iter<'a, Vec<u8>>, fn(&Vec<u8>) -> &[u8]>
        where
            Self: 'a;

        fn metadata(&self) -> &UFrameMetadata {
            &self.metadata
        }

        fn payload_len(&self) -> usize {
            self.segments.iter().map(Vec::len).sum()
        }

        fn has_payload(&self) -> bool {
            self.has_payload
        }

        fn payload_reader(&self) -> Self::PayloadReader<'_> {
            Cursor::new(self.segments.concat())
        }

        fn payload_slices(&self) -> Self::PayloadSlices<'_> {
            self.segments
                .iter()
                .map(vec_as_slice as fn(&Vec<u8>) -> &[u8])
        }

        fn try_contiguous_payload(&self) -> Option<&[u8]> {
            match self.segments.as_slice() {
                [segment] => Some(segment.as_slice()),
                _ => None,
            }
        }
    }

    impl UZeroCopyRxLease for SegmentedRxFrame {}

    fn raw_metadata() -> UFrameMetadata {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 1, 0x9000).expect("valid topic");
        UFrameMetadata::publish_unchecked(topic).with_encoding(RawBytes::payload_encoding())
    }

    #[test]
    fn loaned_frame_copies_segmented_payload_into_tx_loan() {
        let metadata = raw_metadata();
        let rx = SegmentedRxFrame::new(
            metadata.clone(),
            vec![b"one".to_vec(), b"-two".to_vec(), b"-three".to_vec()],
        );
        let loaned = ZeroCopyLoanedFrame::with_payload_loan_provenance(
            rx,
            PayloadLoanProvenance::SharedMemory,
        );
        let mut tx = UVecTxBuffer::new(metadata.clone(), loaned.payload_len());

        let copied = copy_loaned_frame_payload_to_tx(&loaned, &mut tx).expect("copy succeeds");

        assert_eq!(copied, b"one-two-three".len());
        assert_eq!(tx.payload(), b"one-two-three");
        assert_eq!(loaned.metadata(), &metadata);
        assert_eq!(
            loaned.payload_loan_provenance(),
            Some(PayloadLoanProvenance::SharedMemory)
        );
        assert!(loaned.try_contiguous_payload().is_none());
    }

    #[test]
    fn loaned_frame_preserves_no_payload_and_present_empty_payload() {
        let no_payload = UOwnedFrame::without_payload_unchecked(raw_metadata());
        let present_empty = UOwnedFrame::with_payload_unchecked(raw_metadata(), Vec::<u8>::new());

        assert!(!LoanedFrame::has_payload(&no_payload));
        assert_eq!(LoanedFrame::try_contiguous_payload(&no_payload), None);
        assert!(LoanedFrame::has_payload(&present_empty));
        assert_eq!(
            LoanedFrame::try_contiguous_payload(&present_empty),
            Some([].as_slice())
        );
    }

    #[test]
    fn loaned_frame_fanout_copies_to_each_egress_loan() {
        let metadata = raw_metadata();
        let rx = SegmentedRxFrame::new(metadata.clone(), vec![b"fan".to_vec(), b"out".to_vec()]);
        let loaned: Box<dyn LoanedFrame> = Box::new(ZeroCopyLoanedFrame::new(rx));
        let mut first = UVecTxBuffer::new(metadata.clone(), loaned.payload_len());
        let mut second = UVecTxBuffer::new(metadata, loaned.payload_len());

        copy_loaned_frame_payload_to_tx(loaned.as_ref(), &mut first).expect("first copy succeeds");
        copy_loaned_frame_payload_to_tx(loaned.as_ref(), &mut second)
            .expect("second copy succeeds");

        assert_eq!(first.payload(), b"fanout");
        assert_eq!(second.payload(), b"fanout");
    }

    #[test]
    fn loaned_frame_rejects_mismatched_egress_loan_length() {
        let metadata = raw_metadata();
        let rx = SegmentedRxFrame::new(metadata.clone(), vec![b"payload".to_vec()]);
        let loaned = ZeroCopyLoanedFrame::new(rx);
        let mut tx = UVecTxBuffer::new(metadata, loaned.payload_len() + 1);

        let err = copy_loaned_frame_payload_to_tx(&loaned, &mut tx)
            .expect_err("wrong-length egress loan must fail");

        assert!(err.to_string().contains("invalid payload length"));
    }

    #[test]
    fn no_payload_loaned_frame_copies_zero_bytes() {
        let metadata = UFrameMetadata::publish_unchecked(
            UUri::try_from_parts("vehicle", 0x4210, 1, 0x9000).expect("valid topic"),
        );
        let rx = SegmentedRxFrame::no_payload(metadata.clone());
        let loaned = ZeroCopyLoanedFrame::new(rx);
        let mut tx = UVecTxBuffer::new(metadata, 0);

        let copied = copy_loaned_frame_payload_to_tx(&loaned, &mut tx).expect("copy succeeds");

        assert_eq!(copied, 0);
        assert!(!loaned.has_payload());
        assert_eq!(tx.payload(), b"");
    }
}
