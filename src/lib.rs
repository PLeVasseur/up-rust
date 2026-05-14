/********************************************************************************
 * Copyright (c) 2023 Contributors to the Eclipse Foundation
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

/*!
up-rust is the Eclipse uProtocol Rust language library.

The public API is native Rust and serializer-neutral. Transports exchange
frames and wire-format metadata without generated message envelopes.
*/

#[cfg(feature = "util")]
pub mod local_transport;

pub mod cloudevents;

pub mod communication;

pub mod core;

#[cfg(feature = "protobuf-wire")]
pub mod protobuf_wire;

mod uframe;
pub use uframe::{
    RawBytes, UAttributes, UDeserializer, UEncoding, UErasedSerializer, UFrameHeader,
    UMessageBuilder, UMessageBuilderError, UMessageType, UOwnedFrame, UPriority, USerializer,
    UTxBuffer, UVecTxBuffer, UWireError, UZeroCopyRxFrame, WireFormat,
};

#[cfg(feature = "protobuf-wire")]
pub use protobuf_wire::ProtobufWire;

mod uri;
pub use uri::{UUri, UUriError};

pub mod usubscription;
pub use usubscription::*;

mod ustatus;
pub use ustatus::{UCode, UStatus};

mod utransport;
pub use utransport::{
    verify_filter_criteria, ComparableOwnedListener, LocalUriProvider, StaticUriProvider,
    UOwnedListener, UOwnedTransport, UOwnedTransportExt, UZeroCopyListener, UZeroCopyTransport,
    UZeroCopyTransportExt,
};

mod uuid;
pub use uuid::UUID;
