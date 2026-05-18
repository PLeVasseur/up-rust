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
mod payload;
mod zero_copy;

pub use builder::{UFrameBuilder, UFrameBuilderError};
pub use frame::{
    UAttributes, UEncoding, UEncodingError, UFrameMetadata, UMessageType, UOwnedFrame, UPriority,
};
pub use frame_wire::{UFrameWireError, UFrameWireFormat};
pub use payload::{
    PayloadFormat, RawBytes, UDeserializer, UErasedSerializer, UReadDeserializer, USerializer,
    UWireError,
};
pub use zero_copy::{
    UContiguousZeroCopyRxFrame, UTxBuffer, UVecTxBuffer, UZeroCopyPayloadCopyExt, UZeroCopyRxFrame,
};

#[cfg(test)]
mod tests;
