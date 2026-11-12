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

//! Family-neutral transmit loans and receive leases.
//!
//! These contracts separate storage lifetime, frame validation and typed access.
//! Bindings own physical mappings, allocation, readiness and copy behavior.

mod frame;
mod loan;
mod opaque;
#[cfg(any(
    test,
    all(feature = "test-util", feature = "transport-implementer-api")
))]
mod pool;
#[cfg(all(
    any(test, feature = "test-util"),
    feature = "transport-implementer-api"
))]
mod pool_core;
#[cfg(feature = "transport-implementer-api")]
mod prepared;
#[cfg(feature = "transport-implementer-api")]
mod registered;
#[cfg(feature = "transport-implementer-api")]
pub use registered::{WireRegistration, WireRegistrationStopError};
#[cfg(feature = "transport-implementer-api")]
mod route;
#[cfg(feature = "transport-implementer-api")]
mod rpc;
#[cfg(all(
    any(test, feature = "test-util"),
    feature = "transport-implementer-api"
))]
pub use pool_core::{PoolCore, PoolCoreConfig, PoolFrame};
#[cfg(feature = "transport-implementer-api")]
pub use rpc::{
    NativeCallError, NativeRequestHandler, PendingNativeCall, PreparedRpcClient, PreparedRpcServer,
    RpcClientConfig, RpcClientStats, RpcServerConfig, RpcServerStats, RpcServerStopError,
};
mod rx;
mod token;
mod tx;

pub use frame::{AdmissionError, Frame, RxAdmission, RxLease, RxStats, ZeroCopyHandler};
pub use opaque::{HeaderAdmission, HeaderAdmissionStats, HeaderHandler, HeaderLease};
#[cfg(feature = "transport-implementer-api")]
pub use opaque::{OpaqueRoute, OpaqueTemplate};
#[cfg(feature = "transport-implementer-api")]
pub use prepared::{
    FrameSink, InitializedTx, LoanError, PreparedTransportCore, PreparedTxStorage, RouteLimits,
    SendError, SetupError, TxFrameLayout,
};
#[cfg(feature = "transport-implementer-api")]
pub use route::{MessageFields, PayloadInit, RouteConfig, RouteDefinition, TxRoute, TxTemplate};
pub use token::{TokenBuffer, TokenBufferSetupError, TokenError};

pub use loan::{LoanedPayload, PayloadAlignment, PayloadLoanProvenance};
pub use rx::{ULoanedContiguousZeroCopyRxFrame, UVecRxLease, UZeroCopyRxLease};
pub use tx::{
    UTxBuffer, UTxLoanSpec, UTxPayloadSpec, UUninitTxBuffer, UVecTxBuffer, UVecUninitTxBuffer,
};

#[cfg(feature = "transport-implementer-api")]
pub use prepared::NativeLoanCore;

#[cfg(test)]
mod tests {
    use core::mem::{align_of, size_of, MaybeUninit};

    use crate::payload::stable::{StablePayloadField as _, StablePayloadInit as _};
    use crate::test_support::self_agreement;
    use crate::{
        NativeProfile, NativeProfileAgreement, PayloadCodec, PayloadEncoding,
        StableContainerPayload, StablePayload, UFrameMetadata, UMessageBuilder, UTxBuffer,
        UUninitTxBuffer, UUri,
    };

    use super::*;

    #[repr(C)]
    #[derive(Debug, Eq, PartialEq, crate::StablePayload, crate::StablePayloadInit)]
    #[stable_payload(type_name = "uprotocol.test.ZeroCopy")]
    struct ZeroCopyValue {
        bytes: [u8; 4],
    }

    fn metadata() -> UFrameMetadata {
        let topic = UUri::try_from_parts("vehicle", 0x0001_4210, 0x01, 0x9000).unwrap();
        UFrameMetadata::publish(topic)
            .with_native_payload_identity(
                StableContainerPayload::<ZeroCopyValue>::identity(&profile()).unwrap(),
            )
            .build()
            .unwrap()
    }

    fn profile() -> NativeProfileAgreement {
        let profile = NativeProfile::new(
            "test",
            1,
            PayloadEncoding::from_id(0xF101).unwrap(),
            [ZeroCopyValue::native_representation()],
        )
        .unwrap();
        self_agreement(profile)
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
        // SAFETY: The typestate initializer above wrote every field of the
        // payload in this buffer's storage before `finish()`.
        let initialized_buffer = unsafe { uninit.assume_payload_initialized() };
        assert_eq!(initialized_buffer.payload(), b"test");
    }

    #[test_case::test_case(false; "private assignment")]
    #[test_case::test_case(true; "explicit zero assignment")]
    fn receive_lease_borrows_valid_stable_payload(contract_defined: bool) {
        let agreement = if contract_defined {
            let representation = ZeroCopyValue::native_representation();
            let profile = NativeProfile::new(
                "test",
                1,
                PayloadEncoding::CONTRACT_DEFINED,
                [representation],
            )
            .unwrap();
            self_agreement(profile)
        } else {
            profile()
        };
        let identity = StableContainerPayload::<ZeroCopyValue>::identity(&agreement).unwrap();
        let metadata = metadata()
            .without_payload_encoding()
            .with_native_payload_identity(identity)
            .unwrap();
        let lease = UVecRxLease::new(metadata, Some(b"wire".to_vec())).unwrap();
        assert_eq!(
            lease
                .borrow_stable_payload::<ZeroCopyValue>(&agreement)
                .unwrap(),
            &ZeroCopyValue { bytes: *b"wire" }
        );
        assert_eq!(
            lease.payload_loan_provenance().unwrap(),
            PayloadLoanProvenance::OwnedReceiveLease
        );
    }

    #[test]
    fn loan_spec_rejects_metadata_payload_mismatch() {
        let topic = UUri::try_from_parts("vehicle", 0x0001_4210, 0x01, 0x9000).unwrap();
        let message = UMessageBuilder::publish(topic).build().unwrap();
        let no_payload =
            crate::frame::metadata::try_project_umessage_to_frame_metadata(&message).unwrap();
        let error = UTxLoanSpec::new(
            no_payload,
            UTxPayloadSpec::Present {
                len: 1,
                alignment: PayloadAlignment::new(1).unwrap(),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::UCode::InvalidArgument);
    }

    #[test]
    fn native_codec_rejects_missing_context() {
        assert!(matches!(
            StableContainerPayload::<ZeroCopyValue>::payload_identity(None),
            Err(crate::UWireError::NativeProfile(
                crate::NativeProfileError::InvalidProfile(_)
            ))
        ));
    }

    #[test]
    fn fixed_decoder_does_not_ignore_native_identity() {
        use crate::UFrameView;
        let metadata = metadata()
            .without_payload_encoding()
            .with_payload_encoding(PayloadEncoding::RAW)
            .unwrap()
            .with_native_type_token(crate::NativeTypeToken::from_u32(0))
            .unwrap();
        let lease = UVecRxLease::new(metadata, Some(b"wire".to_vec())).unwrap();
        assert!(matches!(
            lease.decode_payload_from_reader_as::<crate::payload::codec::RawBytes, Vec<u8>>(
                crate::PayloadDecodeLimit::new(4)
            ),
            Err(crate::UWireError::UnsupportedNativeTypeToken {
                expected: None,
                actual: Some(_)
            })
        ));
    }

    #[test_case::test_case(4; "scalar alignment")]
    #[test_case::test_case(64; "cache line alignment")]
    #[test_case::test_case(4096; "page alignment")]
    fn cloning_transmit_buffer_preserves_alignment_and_bytes(alignment: usize) {
        let mut original = UVecTxBuffer::with_alignment(metadata(), 4, alignment).unwrap();
        original.payload_mut().copy_from_slice(b"wire");
        let cloned = original.clone();
        assert_eq!((cloned.payload().as_ptr() as usize) % alignment, 0);
        assert_eq!(cloned.payload(), b"wire");
        original.payload_mut().fill(0);
        assert_eq!(cloned.payload(), b"wire");
    }

    #[test_case::test_case(None; "missing carried token")]
    #[test_case::test_case(Some(0); "wrong carried token")]
    fn receive_lease_rejects_identity_before_borrow(token: Option<u32>) {
        let encoding = PayloadEncoding::from_id(0xF101).unwrap();
        let mut metadata = metadata()
            .without_payload_encoding()
            .with_payload_encoding(encoding)
            .unwrap();
        if let Some(token) = token {
            metadata = metadata
                .with_native_type_token(crate::NativeTypeToken::from_u32(token))
                .unwrap();
        }
        let lease = UVecRxLease::new(metadata, Some(b"wire".to_vec())).unwrap();
        assert!(matches!(
            lease.borrow_stable_payload::<ZeroCopyValue>(&profile()),
            Err(crate::UWireError::UnsupportedNativeTypeToken { .. })
        ));
    }

    #[test]
    fn maybe_uninit_byte_layout_matches_bytes() {
        assert_eq!(size_of::<MaybeUninit<u8>>(), 1);
    }
}
