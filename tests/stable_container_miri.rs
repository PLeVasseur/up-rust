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

use std::mem::{self, MaybeUninit};

use up_rust::{
    LoanPayload, LoanUninitPayload, LoanedPayloadUninitMut, PayloadLoanProvenance,
    StableContainerPayload, StablePayloadInit, UFrameMetadata, ULoanedContiguousZeroCopyRxFrame,
    UMessageBuilder, UUri, UVecRxLease, UUID,
};

#[cfg(any(
    feature = "expert-unsafe-payloads",
    all(
        feature = "unsafe-stable-payload-tx",
        feature = "unsafe-stable-payload-init"
    )
))]
use up_rust::UZeroCopyUninitTransportExt;

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    up_rust::StablePayload,
    up_rust::ByteBackedStablePayload,
)]
#[stable_payload(type_name = "example.miri.StableBytes")]
struct StableBytes {
    bytes: [u8; 4],
}

#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    up_rust::StablePayload,
    up_rust::ByteBackedStablePayload,
    up_rust::StablePayloadInit,
)]
#[stable_payload(type_name = "example.miri.InitLeaf")]
struct InitLeaf {
    x: u16,
    y: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, up_rust::StablePayload, up_rust::StablePayloadInit)]
#[stable_payload(type_name = "example.miri.InitMessage")]
struct InitMessage {
    tag: u8,
    count: u32,
    leaf: InitLeaf,
    bytes: [u8; 4],
    words: [u16; 2],
    leaves: [InitLeaf; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, up_rust::StablePayload)]
#[stable_payload(type_name = "example.miri.ManualFields")]
struct ManualFields {
    x: u32,
    y: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, up_rust::StablePayload)]
#[stable_payload(type_name = "example.miri.PaddedPayload")]
struct PaddedPayload {
    small: u8,
    large: u32,
}

#[cfg(any(
    feature = "expert-unsafe-payloads",
    all(
        feature = "unsafe-stable-payload-tx",
        feature = "unsafe-stable-payload-init"
    )
))]
#[derive(Default)]
struct MiriUninitTransport {
    sent: std::sync::Mutex<Option<UVecRxLease>>,
}

#[cfg(any(
    feature = "expert-unsafe-payloads",
    all(
        feature = "unsafe-stable-payload-tx",
        feature = "unsafe-stable-payload-init"
    )
))]
#[async_trait::async_trait]
impl up_rust::UZeroCopyTransportImpl for MiriUninitTransport {
    type Tx = up_rust::UVecTxBuffer;
    type Rx = UVecRxLease;

    async fn loan_validated_tx(
        &self,
        spec: up_rust::ValidatedTxLoanSpec,
    ) -> Result<Self::Tx, up_rust::UStatus> {
        up_rust::UVecTxBuffer::with_alignment(
            spec.metadata().clone(),
            spec.payload_len(),
            spec.payload_alignment(),
        )
    }

    async fn send_validated_zero_copy(&self, buffer: Self::Tx) -> Result<(), up_rust::UStatus> {
        *self.sent.lock().expect("sent lock poisoned") = Some(buffer.into_rx_lease());
        Ok(())
    }
}

#[cfg(any(
    feature = "expert-unsafe-payloads",
    all(
        feature = "unsafe-stable-payload-tx",
        feature = "unsafe-stable-payload-init"
    )
))]
#[async_trait::async_trait]
impl up_rust::UZeroCopyUninitTransportImpl for MiriUninitTransport {
    type UninitTx = up_rust::UVecUninitTxBuffer;

    async fn loan_validated_uninit_tx(
        &self,
        spec: up_rust::ValidatedTxLoanSpec,
    ) -> Result<Self::UninitTx, up_rust::UStatus> {
        up_rust::UVecUninitTxBuffer::with_alignment(
            spec.metadata().clone(),
            spec.payload_len(),
            spec.payload_alignment(),
        )
    }
}

fn topic() -> UUri {
    UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).expect("valid topic")
}

fn metadata<T: up_rust::StablePayload>() -> UFrameMetadata {
    let fixed_id = UUID::from_u64_pair(0x0000_0000_0001_7000, 0x8010_1010_1010_1a1a)
        .expect("fixed UUID should be valid");
    let message = UMessageBuilder::publish(topic())
        .with_message_id(fixed_id)
        .build()
        .expect("message");
    up_rust::try_project_attributes_to_frame_metadata(
        message.attributes(),
        Some(StableContainerPayload::<T>::encoding()),
    )
    .expect("stable metadata")
}

fn stable_bytes(value: &StableBytes) -> Vec<u8> {
    // SAFETY: `StableBytes` is `repr(C)` over `[u8; 4]`, has alignment 1, and
    // every byte pattern is valid for the test payload.
    unsafe {
        std::slice::from_raw_parts(
            std::ptr::from_ref(value).cast::<u8>(),
            std::mem::size_of::<StableBytes>(),
        )
        .to_vec()
    }
}

fn uninit_bytes<T>(storage: &mut MaybeUninit<T>) -> &mut [MaybeUninit<u8>] {
    // SAFETY: The returned byte slice covers exactly the uninitialized storage
    // for `T` and inherits its alignment.
    unsafe {
        std::slice::from_raw_parts_mut(
            std::ptr::from_mut(storage).cast::<MaybeUninit<u8>>(),
            mem::size_of::<T>(),
        )
    }
}

#[test]
fn stable_payload_derive_loan_borrow_is_miri_friendly() {
    up_rust::assert_stable_payload_byte_backed_uninit::<StableBytes>();

    let value = StableBytes { bytes: *b"miri" };
    let frame = UVecRxLease::new(metadata::<StableBytes>(), Some(stable_bytes(&value)))
        .expect("loan-backed stable frame");

    let borrowed = frame
        .borrow_stable_payload::<StableBytes>()
        .expect("borrow stable payload");

    assert_eq!(borrowed, &value);
}

#[test]
fn stable_payload_init_builder_initializes_fields_and_padding() -> Result<(), up_rust::UWireError> {
    let mut storage = MaybeUninit::<InitMessage>::uninit();
    let init = InitMessage::init_from_uninit_bytes(uninit_bytes(&mut storage))?;
    let init = init
        .tag(7)
        .count(42)
        .leaf_value(InitLeaf { x: 1, y: 2 })
        .bytes_from_array(b"miri")
        .words_from_slice(&[10, 11])?
        .leaves(|index, leaf| leaf.x(index as u16).y(9).finish())?;
    let _finished = init.finish()?;

    // SAFETY: `finish()` proves every semantic field and generated padding gap
    // has been initialized.
    let raw = unsafe {
        std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), mem::size_of::<InitMessage>())
    };
    assert_eq!(raw.get(1..4).expect("padding bytes"), &[0, 0, 0]);

    // SAFETY: The builder completion proof above initialized one `InitMessage`.
    let value = unsafe { storage.assume_init() };
    assert_eq!(value.tag, 7);
    assert_eq!(value.count, 42);
    assert_eq!(value.leaf, InitLeaf { x: 1, y: 2 });
    assert_eq!(value.bytes, *b"miri");
    assert_eq!(value.words, [10, 11]);
    assert_eq!(
        value.leaves,
        [InitLeaf { x: 0, y: 9 }, InitLeaf { x: 1, y: 9 }]
    );

    Ok(())
}

#[test]
fn stable_payload_init_rejects_wrong_length() {
    let mut bytes = vec![MaybeUninit::<u8>::uninit(); mem::size_of::<InitMessage>() - 1];
    let err = match InitMessage::init_from_uninit_bytes(&mut bytes) {
        Ok(_) => panic!("wrong-length init unexpectedly succeeded"),
        Err(err) => err,
    };

    assert!(
        err.to_string().contains("payload length must be"),
        "unexpected error: {err}"
    );
}

#[test]
fn stable_initialized_tx_loan_payload_is_miri_friendly() -> Result<(), up_rust::UWireError> {
    let mut bytes = vec![0_u8; mem::size_of::<StableBytes>()];

    let payload = StableContainerPayload::<StableBytes>::loan_payload(&mut bytes)?;
    payload.bytes.copy_from_slice(b"send");

    let frame = UVecRxLease::new(metadata::<StableBytes>(), Some(bytes)).expect("stable frame");
    let borrowed = frame
        .borrow_stable_payload::<StableBytes>()
        .expect("borrow sent stable payload");

    assert_eq!(borrowed.bytes, *b"send");
    Ok(())
}

#[test]
fn stable_uninit_tx_loan_payload_is_miri_friendly() -> Result<(), up_rust::UWireError> {
    let mut storage = MaybeUninit::<StableBytes>::uninit();
    let payload = uninit_bytes(&mut storage);
    // SAFETY: `payload` covers exactly the uninitialized storage for one
    // `StableBytes` value and is uniquely borrowed for this test.
    let payload = unsafe {
        LoanedPayloadUninitMut::new_unchecked(payload, PayloadLoanProvenance::OpaqueTransportLoan)
    };
    let slot = StableContainerPayload::<StableBytes>::loan_uninit_payload(payload)?;
    let _initialized = slot.write(StableBytes { bytes: *b"zero" });

    // SAFETY: the loaned uninit slot wrote one initialized `StableBytes` value.
    let value = unsafe { storage.assume_init() };
    assert_eq!(value.bytes, *b"zero");
    Ok(())
}

#[cfg(any(
    feature = "unsafe-stable-payload-init",
    feature = "expert-unsafe-payloads"
))]
#[test]
fn raw_field_initialization_is_miri_friendly() {
    let mut storage = MaybeUninit::<ManualFields>::uninit();
    let ptr = std::ptr::NonNull::from(&mut storage);
    // SAFETY: `ptr` is a unique, aligned slot for one `ManualFields` value.
    let mut slot = unsafe { up_rust::LoanedUninitPayload::new_unchecked(ptr) };

    // SAFETY: This test writes every field of `ManualFields` before calling
    // `assume_init`; the type has no implicit padding.
    let ptr = unsafe { slot.as_mut_ptr() };
    // SAFETY: `ptr` came from the loaned slot and points to enough storage for
    // `ManualFields`; this only forms raw field pointers.
    let x = unsafe { std::ptr::addr_of_mut!((*ptr).x) };
    // SAFETY: Same slot/provenance proof as for `x` above.
    let y = unsafe { std::ptr::addr_of_mut!((*ptr).y) };
    // SAFETY: `x` points to the uninitialized field and is written once.
    unsafe { x.write(19) };
    // SAFETY: `y` points to the uninitialized field and is written once.
    unsafe { y.write(23) };
    // SAFETY: Both fields have been initialized and `ManualFields` has no
    // implicit padding.
    let _initialized = unsafe { slot.assume_init() };

    // SAFETY: The raw field initialization above produced an initialized value.
    let value = unsafe { storage.assume_init() };
    assert_eq!(value, ManualFields { x: 19, y: 23 });
}

#[cfg(any(
    feature = "expert-unsafe-payloads",
    all(
        feature = "unsafe-uninit-payload-bytes",
        feature = "unsafe-stable-payload-init"
    )
))]
#[test]
fn raw_uninit_payload_bytes_are_miri_checked_when_fully_initialized() {
    let mut storage = MaybeUninit::<StableBytes>::uninit();
    let ptr = std::ptr::NonNull::from(&mut storage);
    // SAFETY: `ptr` is a unique, aligned slot for one `StableBytes` value.
    let mut slot = unsafe { up_rust::LoanedUninitPayload::new_unchecked(ptr) };

    // SAFETY: This feature-gated test writes every byte returned by the raw
    // uninit view before marking the typed slot initialized.
    let bytes = unsafe { slot.as_uninit_bytes_mut() };
    for (slot, byte) in bytes.iter_mut().zip(*b"miri") {
        slot.write(byte);
    }
    // SAFETY: Every byte in the raw view was initialized by the loop above.
    let _initialized = unsafe { slot.assume_init() };

    // SAFETY: The raw byte initialization above produced an initialized value.
    let value = unsafe { storage.assume_init() };
    assert_eq!(value.bytes, *b"miri");
}

#[cfg(any(
    feature = "expert-unsafe-payloads",
    all(
        feature = "unsafe-stable-payload-tx",
        feature = "unsafe-stable-payload-init"
    )
))]
#[tokio::test(flavor = "current_thread")]
async fn expert_padded_stable_payload_tx_is_miri_friendly_when_zeroed() {
    fn init_padded<'payload>(
        slot: up_rust::UnsafeStablePayloadTxSlot<'payload, PaddedPayload>,
    ) -> Result<up_rust::LoanedInitPayload<'payload, PaddedPayload>, up_rust::UWireError> {
        let mut slot = slot.zeroed();
        // SAFETY: `zeroed()` initialized all transported bytes; the raw pointer
        // is used only for field writes before commit.
        let ptr = unsafe { slot.as_mut_ptr() };
        // SAFETY: `ptr` came from the loaned slot and points to enough storage
        // for `PaddedPayload`; this only forms raw field pointers.
        let small = unsafe { std::ptr::addr_of_mut!((*ptr).small) };
        // SAFETY: Same slot/provenance proof as for `small` above.
        let large = unsafe { std::ptr::addr_of_mut!((*ptr).large) };
        // SAFETY: `small` points to the uninitialized field and is written once.
        unsafe { small.write(31) };
        // SAFETY: `large` points to the uninitialized field and is written once.
        unsafe { large.write(37) };
        // SAFETY: `zeroed()` initialized padding and both fields have been
        // written with valid values.
        Ok(unsafe { slot.assume_init() })
    }

    let transport = MiriUninitTransport::default();
    // SAFETY: `init_padded` zeroes the full transported byte range, writes both
    // semantic fields, and only then returns the initialized marker.
    unsafe {
        transport
            .send_uninit_stable_payload_unchecked::<PaddedPayload>(
                metadata::<PaddedPayload>(),
                init_padded,
            )
            .await
    }
    .expect("send expert stable payload");

    let frame = transport
        .sent
        .lock()
        .expect("sent lock poisoned")
        .take()
        .expect("transport should have sent one frame");
    let borrowed = frame
        .borrow_stable_payload::<PaddedPayload>()
        .expect("borrow expert stable payload");
    assert_eq!(borrowed.small, 31);
    assert_eq!(borrowed.large, 37);
}
