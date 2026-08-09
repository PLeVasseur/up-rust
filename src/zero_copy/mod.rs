/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

mod loan;
mod rx;
mod transport;
mod tx;

pub use loan::{LoanedPayload, PayloadAlignment, PayloadLoanProvenance};
pub use rx::{ULoanedContiguousZeroCopyRxFrame, UVecRxLease, UZeroCopyRxLease};
pub use transport::{
    UZeroCopyListener, UZeroCopyTransport, UZeroCopyTransportExt, UZeroCopyTransportImpl,
    UZeroCopyUninitTransportImpl,
};
pub use tx::{
    UTxBuffer, UTxLoanSpec, UTxPayloadSpec, UUninitTxBuffer, UVecTxBuffer, UVecUninitTxBuffer,
};

#[cfg(test)]
mod tests {
    use core::mem::{align_of, size_of, MaybeUninit};

    use crate::payload::stable::{StablePayloadField as _, StablePayloadInit as _};
    use crate::{UFrameMetadata, UMessageBuilder, UTxBuffer, UUninitTxBuffer, UUri};

    use super::*;

    #[repr(C)]
    #[derive(Debug, Eq, PartialEq, crate::StablePayload, crate::StablePayloadInit)]
    #[stable_payload(type_name = "uprotocol.test.ZeroCopy")]
    struct ZeroCopyValue {
        bytes: [u8; 4],
    }

    fn metadata() -> UFrameMetadata {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let message = UMessageBuilder::publish(topic).build().unwrap();
        crate::frame::metadata::try_project_attributes_to_frame_metadata(
            message.attributes(),
            Some(crate::StableContainerPayload::<ZeroCopyValue>::encoding()),
        )
        .unwrap()
    }

    #[test]
    fn aligned_initialized_and_uninitialized_buffers_preserve_layout() {
        let mut buffer = UVecTxBuffer::with_alignment(
            metadata(),
            size_of::<ZeroCopyValue>(),
            align_of::<ZeroCopyValue>(),
        )
        .unwrap();
        assert_eq!(
            (buffer.payload().as_ptr() as usize) % align_of::<ZeroCopyValue>(),
            0
        );
        buffer.payload_mut().copy_from_slice(b"wire");

        let mut uninit = UVecUninitTxBuffer::with_alignment(
            metadata(),
            size_of::<ZeroCopyValue>(),
            align_of::<ZeroCopyValue>(),
        )
        .unwrap();
        let initialized = ZeroCopyValue::init(uninit.payload_uninit_mut())
            .unwrap()
            .bytes_from_slice(b"test")
            .unwrap()
            .finish();
        assert!(ZeroCopyValue::validate_field_bytes(initialized.as_bytes()));
        let _ = initialized;
        let initialized_buffer = unsafe { uninit.assume_payload_initialized() };
        assert_eq!(initialized_buffer.payload(), b"test");
    }

    #[test]
    fn receive_lease_borrows_valid_stable_payload() {
        let lease = UVecRxLease::new(metadata(), Some(b"wire".to_vec())).unwrap();
        assert_eq!(
            lease.borrow_stable_payload::<ZeroCopyValue>().unwrap(),
            &ZeroCopyValue { bytes: *b"wire" }
        );
        assert_eq!(
            lease.payload_loan_provenance().unwrap(),
            PayloadLoanProvenance::OwnedReceiveLease
        );
    }

    #[test]
    fn loan_spec_rejects_metadata_payload_mismatch() {
        let topic = UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap();
        let message = UMessageBuilder::publish(topic).build().unwrap();
        let no_payload = crate::frame::metadata::try_project_attributes_to_frame_metadata(
            message.attributes(),
            None,
        )
        .unwrap();
        assert!(UTxLoanSpec::new(
            no_payload,
            UTxPayloadSpec::Present {
                len: 1,
                alignment: PayloadAlignment::new(1).unwrap(),
            },
        )
        .is_err());
    }

    #[test]
    fn maybe_uninit_byte_layout_matches_bytes() {
        assert_eq!(size_of::<MaybeUninit<u8>>(), 1);
    }
}
