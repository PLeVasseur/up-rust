use std::io::Cursor;

use up_rust::{
    payload::StableContainerPayload,
    zero_copy::{
        LoanedPayload, PayloadLoanProvenance, UFrameView, ULoanedContiguousZeroCopyRxFrame,
        UZeroCopyRxLease,
    },
    StablePayload, UFrameMetadata, UUri,
};

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "example.BroadPadded")]
struct BroadPadded {
    small: u8,
    large: u32,
}

#[repr(C, align(4))]
struct AlignedBytes([u8; core::mem::size_of::<BroadPadded>()]);

struct LoanedFrame<'a> {
    metadata: UFrameMetadata,
    payload: &'a [u8],
}

impl UFrameView for LoanedFrame<'_> {
    type PayloadReader<'a>
        = Cursor<&'a [u8]>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = std::iter::Once<&'a [u8]>
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload_len(&self) -> usize {
        self.payload.len()
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        Cursor::new(self.payload)
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        std::iter::once(self.payload)
    }
}

impl UZeroCopyRxLease for LoanedFrame<'_> {}

impl ULoanedContiguousZeroCopyRxFrame for LoanedFrame<'_> {
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, up_rust::UWireError> {
        Ok(unsafe {
            LoanedPayload::new_unchecked(self.payload, PayloadLoanProvenance::OpaqueTransportLoan)
        })
    }
}

fn main() {
    let mut storage = AlignedBytes([0; core::mem::size_of::<BroadPadded>()]);
    storage.0[0] = 1;
    storage.0[4..8].copy_from_slice(&2_u32.to_ne_bytes());

    let frame = LoanedFrame {
        metadata: UFrameMetadata::publish_unchecked(UUri::try_from("//vehicle/4210/1/9000").unwrap())
            .with_encoding(StableContainerPayload::<BroadPadded>::encoding()),
        payload: storage.0.as_slice(),
    };
    let _ = frame.borrow_stable_payload::<BroadPadded>().unwrap();
}
