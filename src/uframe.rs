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

//! Serialization-neutral frame, frame-wire, and payload-codec primitives.

mod builder;
mod frame;
mod frame_wire;
#[cfg(feature = "experimental-loaned-frame")]
mod loaned_frame;
mod payload;
mod zero_copy;

pub use builder::{UFrameBuilder, UFrameBuilderError};
pub use frame::{
    CustomPayloadEncoding, PayloadEncoding, PayloadEncodingError, UAttributes, UFrameMetadata,
    UMessageType, UOwnedFrame, UPayloadFormat, UPriority,
};
pub use frame_wire::{UFrameWireError, UFrameWireFormat};
#[cfg(feature = "experimental-loaned-frame")]
pub use loaned_frame::{copy_loaned_frame_payload_to_tx, LoanedFrame, ZeroCopyLoanedFrame};
pub use payload::{
    assert_stable_payload_byte_backed_uninit, stable_payload_supports_byte_backed_uninit,
    BorrowPayload, ByteBackedStablePayload, ByteBackedStablePayloadField, BytePayloadCodec,
    DecodePayload, DynPayloadCodec, EncodePayload, EncodedPayload, LoanPayload, LoanUninitPayload,
    LoanedInitPayload, LoanedUninitPayload, McapPayload, PayloadCodec, PayloadCodecCapabilities,
    PayloadCodecRegistry, PayloadFormat, PayloadLayout, PlacementDefault, RawBytes,
    ReadDecodePayload, StableContainerPayload, StableContainerPayloadInfo, StablePayload,
    StablePayloadVariant, StableTypeDetail, TypedPayloadCodec, UDeserializer, UErasedSerializer,
    UReadDeserializer, USerializer, UWireError, ZeroCopySend,
};
#[cfg(any(
    feature = "unsafe-stable-payload-tx",
    feature = "expert-unsafe-payloads"
))]
pub use payload::{UnsafeStablePayloadTxSlot, ZeroedStablePayloadTxSlot};
pub use zero_copy::{
    verify_contiguous_rx_payload_layout, verify_loaned_rx_payload_layout,
    verify_tx_buffer_payload_layout, verify_uninit_tx_buffer_payload_layout, LoanedPayload,
    LoanedPayloadMut, LoanedPayloadUninitMut, LoanedUninitByteWriter, PayloadLoanProvenance,
    UContiguousZeroCopyRxFrame, UFrameView, ULoanedContiguousZeroCopyRxFrame, UTxBuffer,
    UUninitTxBuffer, UVecRxLease, UVecTxBuffer, UVecUninitTxBuffer, UZeroCopyPayloadCopyExt,
    UZeroCopyRxLease,
};

#[cfg(test)]
mod tests;
