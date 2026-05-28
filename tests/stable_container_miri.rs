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

use std::{io::Cursor, mem};

#[cfg(feature = "expert-unsafe-payloads")]
use std::sync::Mutex;

#[cfg(feature = "expert-unsafe-payloads")]
use async_trait::async_trait;
use up_rust::{
    payload::{
        EncodePayload, LoanPayload, LoanUninitPayload, PlacementDefault, StableContainerPayload,
    },
    zero_copy::{
        LoanedPayload, PayloadLoanProvenance, ULoanedContiguousZeroCopyRxFrame, UTxBuffer,
        UUninitTxBuffer, UVecTxBuffer, UVecUninitTxBuffer, UZeroCopyRxFrame,
    },
    PayloadEncoding, UAttributes, UFrameMetadata, UMessageType, UUri, UUID,
};

#[cfg(feature = "expert-unsafe-payloads")]
use up_rust::{UOwnedFrame, UStatus, UTxLoanSpec, UZeroCopyUninitTransport};

#[cfg(feature = "expert-unsafe-payloads")]
use up_rust::zero_copy::{UZeroCopyTransport, UZeroCopyUninitTransportExt};

fn miri_publish_metadata(topic: UUri) -> UFrameMetadata {
    let id = UUID::from_u64_pair(0x0000_0000_0001_7000, 0x8010_1010_1010_1a1a)
        .expect("fixed UUID should be valid");
    UFrameMetadata::new(
        UAttributes::new(id, topic, None, UMessageType::Publish),
        None::<PayloadEncoding>,
    )
}

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    PlacementDefault,
    up_rust::StablePayload,
    up_rust::ByteBackedStablePayload,
)]
#[stable_payload(type_name = "example.miri.VehiclePose")]
struct VehiclePose {
    x: u32,
    y: u32,
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, up_rust::StablePayload, up_rust::ByteBackedStablePayload)]
#[stable_payload(type_name = "example.miri.NoCopyMarker")]
struct NoCopyMarker {
    value: u32,
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, up_rust::StablePayload, up_rust::ByteBackedStablePayload)]
#[stable_payload(type_name = "example.miri.NonCopyPose")]
struct NonCopyPose {
    x: u32,
    y: u32,
    marker: NoCopyMarker,
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, up_rust::StablePayload)]
#[stable_payload(type_name = "example.miri.PaddedPose")]
struct PaddedPose {
    small: u8,
    large: u32,
}

struct LoanedSliceFrame<'a> {
    metadata: UFrameMetadata,
    payload: &'a [u8],
}

impl UZeroCopyRxFrame for LoanedSliceFrame<'_> {
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

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        Some(self.payload)
    }
}

impl ULoanedContiguousZeroCopyRxFrame for LoanedSliceFrame<'_> {
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, up_rust::UWireError> {
        // SAFETY: The test frame keeps `payload` alive for `&self` and exposes
        // it as the exact visible loan-backed payload slice.
        Ok(unsafe {
            LoanedPayload::new_unchecked(self.payload, PayloadLoanProvenance::OpaqueTransportLoan)
        })
    }
}

fn with_stable_payload<T, R>(
    encoding: PayloadEncoding,
    payload: &[u8],
    f: impl FnOnce(&T) -> R,
) -> Result<R, up_rust::UWireError>
where
    T: up_rust::StablePayload,
{
    let frame = LoanedSliceFrame {
        metadata: miri_publish_metadata(UUri::try_from("//miri/4210/1/9000").unwrap())
            .with_encoding(encoding),
        payload,
    };
    let borrowed = frame.borrow_stable_payload::<T>()?;
    Ok(f(borrowed))
}

#[cfg(feature = "expert-unsafe-payloads")]
#[derive(Default)]
struct MiriUninitTransport {
    sent: Mutex<Option<UVecTxBuffer>>,
}

#[cfg(feature = "expert-unsafe-payloads")]
#[async_trait]
impl UZeroCopyTransport for MiriUninitTransport {
    type Tx = UVecTxBuffer;
    type Rx = UOwnedFrame;

    async fn loan_tx(&self, spec: UTxLoanSpec) -> Result<Self::Tx, UStatus> {
        UVecTxBuffer::with_alignment(
            spec.metadata().clone(),
            spec.payload_len(),
            spec.payload_alignment(),
        )
        .map_err(UStatus::from)
    }

    async fn send_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
        *self.sent.lock().expect("sent lock poisoned") = Some(buffer);
        Ok(())
    }
}

#[cfg(feature = "expert-unsafe-payloads")]
#[async_trait]
impl UZeroCopyUninitTransport for MiriUninitTransport {
    type UninitTx = UVecUninitTxBuffer;

    async fn loan_uninit_tx(&self, spec: UTxLoanSpec) -> Result<Self::UninitTx, UStatus> {
        UVecUninitTxBuffer::with_alignment(
            spec.metadata().clone(),
            spec.payload_len(),
            spec.payload_alignment(),
        )
        .map_err(UStatus::from)
    }
}

#[test]
fn stable_container_loan_then_borrow_is_miri_friendly() {
    let topic = UUri::try_from("//miri/4210/1/9000").unwrap();
    let mut buffer = UVecTxBuffer::with_alignment(
        miri_publish_metadata(topic)
            .with_encoding(StableContainerPayload::<VehiclePose>::encoding()),
        mem::size_of::<VehiclePose>(),
        mem::align_of::<VehiclePose>(),
    )
    .unwrap();

    {
        let pose = <StableContainerPayload<VehiclePose> as LoanPayload<VehiclePose>>::loan_payload(
            buffer.payload_mut(),
        )
        .unwrap();
        pose.x = 7;
        pose.y = 9;
    }

    with_stable_payload::<VehiclePose, _>(
        StableContainerPayload::<VehiclePose>::encoding(),
        buffer.payload(),
        |borrowed| assert_eq!(borrowed, &VehiclePose { x: 7, y: 9 }),
    )
    .unwrap();
}

#[test]
fn stable_container_encode_then_borrow_is_miri_friendly() {
    #[repr(C, align(4))]
    struct AlignedVehiclePoseBytes([u8; mem::size_of::<VehiclePose>()]);

    let value = VehiclePose { x: 3, y: 5 };
    let mut storage = AlignedVehiclePoseBytes([0; mem::size_of::<VehiclePose>()]);

    <StableContainerPayload<VehiclePose> as EncodePayload<VehiclePose>>::encode_payload(
        &value,
        &mut storage.0,
    )
    .unwrap();

    with_stable_payload::<VehiclePose, _>(
        StableContainerPayload::<VehiclePose>::encoding(),
        storage.0.as_slice(),
        |borrowed| assert_eq!(borrowed, &value),
    )
    .unwrap();
}

#[test]
fn stable_container_borrows_broad_padded_initialized_bytes_under_miri() {
    #[repr(C, align(4))]
    struct AlignedPaddedPoseBytes([u8; mem::size_of::<PaddedPose>()]);

    let mut storage = AlignedPaddedPoseBytes([0; mem::size_of::<PaddedPose>()]);
    storage.0[0] = 1;
    storage.0[4..8].copy_from_slice(&2_u32.to_ne_bytes());

    with_stable_payload::<PaddedPose, _>(
        StableContainerPayload::<PaddedPose>::encoding(),
        storage.0.as_slice(),
        |borrowed| {
            assert_eq!(borrowed.small, 1);
            assert_eq!(borrowed.large, 2);
        },
    )
    .unwrap();
}

#[test]
fn stable_container_uninit_write_initializes_non_copy_payload_under_miri() {
    let topic = UUri::try_from("//miri/4210/1/9001").unwrap();
    let mut buffer = UVecUninitTxBuffer::with_alignment(
        miri_publish_metadata(topic)
            .with_encoding(StableContainerPayload::<NonCopyPose>::encoding()),
        mem::size_of::<NonCopyPose>(),
        mem::align_of::<NonCopyPose>(),
    )
    .unwrap();

    {
        let loaned_payload = buffer.payload_uninit_mut();
        let slot =
            <StableContainerPayload<NonCopyPose> as LoanUninitPayload<NonCopyPose>>::loan_uninit_payload(
                loaned_payload,
            )
            .unwrap();
        let _initialized = slot.write(NonCopyPose {
            x: 11,
            y: 13,
            marker: NoCopyMarker { value: 17 },
        });
    }

    // SAFETY: `slot.write` initialized exactly one `NonCopyPose` in the visible
    // payload range before the uninit buffer is committed.
    let buffer = unsafe { buffer.assume_payload_init() };
    with_stable_payload::<NonCopyPose, _>(
        StableContainerPayload::<NonCopyPose>::encoding(),
        buffer.payload(),
        |borrowed| {
            assert_eq!(borrowed.x, 11);
            assert_eq!(borrowed.y, 13);
            assert_eq!(borrowed.marker.value, 17);
        },
    )
    .unwrap();
}

#[test]
fn uninit_byte_writer_commits_exact_payload_under_miri() {
    let topic = UUri::try_from("//miri/4210/1/9002").unwrap();
    let mut buffer =
        UVecUninitTxBuffer::with_alignment(miri_publish_metadata(topic), 3, 1).unwrap();

    {
        let mut writer = buffer.payload_uninit_mut().into_writer();
        writer.write_all(b"abc").unwrap();
        let _initialized = writer.finish().unwrap();
    }

    // SAFETY: `LoanedUninitByteWriter::finish` proved all visible payload bytes
    // were initialized before commit.
    let buffer = unsafe { buffer.assume_payload_init() };
    assert_eq!(buffer.payload(), b"abc");
}

#[test]
fn uninit_buffer_initializes_hidden_padding_under_miri() {
    let topic = UUri::try_from("//miri/4210/1/9003").unwrap();
    let mut buffer =
        UVecUninitTxBuffer::with_alignment(miri_publish_metadata(topic), 3, 4096).unwrap();

    {
        let mut writer = buffer.payload_uninit_mut().into_writer();
        writer.write_all(b"abc").unwrap();
        let _initialized = writer.finish().unwrap();
    }

    // SAFETY: The byte writer initialized the visible payload range; the buffer
    // initializes hidden alignment padding before conversion.
    let buffer = unsafe { buffer.assume_payload_init() };
    assert_eq!(buffer.payload(), b"abc");
}

#[cfg(any(
    feature = "unsafe-stable-payload-init",
    feature = "expert-unsafe-payloads"
))]
#[test]
fn stable_container_raw_field_initialization_is_miri_friendly() {
    let topic = UUri::try_from("//miri/4210/1/9004").unwrap();
    let mut buffer = UVecUninitTxBuffer::with_alignment(
        miri_publish_metadata(topic)
            .with_encoding(StableContainerPayload::<NonCopyPose>::encoding()),
        mem::size_of::<NonCopyPose>(),
        mem::align_of::<NonCopyPose>(),
    )
    .unwrap();

    {
        let loaned_payload = buffer.payload_uninit_mut();
        let mut slot =
            <StableContainerPayload<NonCopyPose> as LoanUninitPayload<NonCopyPose>>::loan_uninit_payload(
                loaned_payload,
            )
            .unwrap();
        // SAFETY: This feature-gated test writes every field of `NonCopyPose`
        // before calling `assume_init`; the type has no implicit padding.
        let ptr = unsafe { slot.as_mut_ptr() };
        // SAFETY: `ptr` came from the loaned slot and points to enough storage
        // for `NonCopyPose`; this only forms a raw field pointer.
        let x = unsafe { std::ptr::addr_of_mut!((*ptr).x) };
        // SAFETY: Same slot/provenance proof as for `x` above.
        let y = unsafe { std::ptr::addr_of_mut!((*ptr).y) };
        // SAFETY: Same slot/provenance proof as for `x` above.
        let marker = unsafe { std::ptr::addr_of_mut!((*ptr).marker) };
        // SAFETY: `x` points to the uninitialized `x` field and is written once.
        unsafe { x.write(19) };
        // SAFETY: `y` points to the uninitialized `y` field and is written once.
        unsafe { y.write(23) };
        // SAFETY: `marker` points to the uninitialized marker field and is
        // written once.
        unsafe { marker.write(NoCopyMarker { value: 29 }) };
        // SAFETY: All fields have been initialized and `NonCopyPose` has no
        // implicit padding.
        let _initialized = unsafe { slot.assume_init() };
    }

    // SAFETY: The raw field initialization above produced an initialized marker
    // before the uninit buffer is committed.
    let buffer = unsafe { buffer.assume_payload_init() };
    with_stable_payload::<NonCopyPose, _>(
        StableContainerPayload::<NonCopyPose>::encoding(),
        buffer.payload(),
        |borrowed| {
            assert_eq!(borrowed.x, 19);
            assert_eq!(borrowed.y, 23);
            assert_eq!(borrowed.marker.value, 29);
        },
    )
    .unwrap();
}

#[cfg(any(
    feature = "unsafe-uninit-payload-bytes",
    feature = "expert-unsafe-payloads"
))]
#[test]
fn raw_uninit_payload_bytes_are_miri_checked_when_fully_initialized() {
    let topic = UUri::try_from("//miri/4210/1/9005").unwrap();
    let mut buffer =
        UVecUninitTxBuffer::with_alignment(miri_publish_metadata(topic), 4, 1).unwrap();

    {
        let mut payload = buffer.payload_uninit_mut();
        // SAFETY: This feature-gated test writes every byte returned by the raw
        // uninit view before converting the payload into initialized bytes.
        let bytes = unsafe { payload.as_uninit_bytes_mut() };
        for (slot, byte) in bytes.iter_mut().zip(*b"miri") {
            slot.write(byte);
        }
        // SAFETY: Every byte in the raw view was initialized by the loop above.
        let _initialized = unsafe { payload.assume_init() };
    }

    // SAFETY: The raw byte path produced an initialized payload marker before
    // the uninit buffer is committed.
    let buffer = unsafe { buffer.assume_payload_init() };
    assert_eq!(buffer.payload(), b"miri");
}

#[cfg(feature = "expert-unsafe-payloads")]
#[tokio::test(flavor = "current_thread")]
async fn expert_padded_stable_payload_tx_is_miri_friendly_when_zeroed() {
    fn init_padded_pose<'payload>(
        slot: up_rust::payload::UnsafeStablePayloadTxSlot<'payload, PaddedPose>,
    ) -> Result<
        up_rust::payload::LoanedInitPayload<'payload, PaddedPose>,
        up_rust::payload::UWireError,
    > {
        let mut slot = slot.zeroed();
        // SAFETY: `zeroed()` initialized all transported bytes; the raw pointer
        // is used only for field writes before commit.
        let ptr = unsafe { slot.as_mut_ptr() };
        // SAFETY: `ptr` came from the loaned slot and points to enough storage
        // for `PaddedPose`; this only forms a raw field pointer.
        let small = unsafe { std::ptr::addr_of_mut!((*ptr).small) };
        // SAFETY: Same slot/provenance proof as for `small` above.
        let large = unsafe { std::ptr::addr_of_mut!((*ptr).large) };
        // SAFETY: `small` points to the uninitialized field and is written
        // before commit.
        unsafe { small.write(31) };
        // SAFETY: `large` points to the uninitialized field and is written
        // before commit.
        unsafe { large.write(37) };
        // SAFETY: `zeroed()` initialized padding and both fields have been
        // written with valid values.
        Ok(unsafe { slot.assume_init() })
    }

    let topic = UUri::try_from("//miri/4210/1/9006").unwrap();
    let transport = MiriUninitTransport::default();

    // SAFETY: The closure zero-initializes the full transported byte range,
    // writes both semantic fields, and only then returns the initialized marker.
    let send = unsafe {
        transport.send_uninit_stable_payload_unchecked::<PaddedPose>(
            miri_publish_metadata(topic),
            init_padded_pose,
        )
    };
    send.await.unwrap();

    let frame = transport
        .sent
        .lock()
        .expect("sent lock poisoned")
        .take()
        .expect("transport should have sent one frame");
    assert_eq!(frame.payload().len(), mem::size_of::<PaddedPose>());
    with_stable_payload::<PaddedPose, _>(
        StableContainerPayload::<PaddedPose>::encoding(),
        frame.payload(),
        |borrowed| {
            assert_eq!(borrowed.small, 31);
            assert_eq!(borrowed.large, 37);
        },
    )
    .unwrap();
}
