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

# Common Path

Applications usually start with [`UFrameBuilder`] and [`UOwnedFrame`]: build a
Publish, Notification, Request, or Response frame, then send it with an
[`UOwnedTransport`].

For discoverability, the same public types are grouped by role under [`frame`],
[`wire`], [`transport`], [`zero_copy`], and [`prelude`]. The crate root keeps the
most common re-exports for short examples.

# Advanced Paths

Custom payload codecs implement [`WireFormat`], [`USerializer`], and
[`UDeserializer`]. Shared-memory transports implement [`UZeroCopyTransport`]
only when they can honestly loan transmit storage and return receive leases.
Routing code that needs a single owned-frame facade over either capability can
use [`UOwnedFrameEndpoint`]. Its name is intentional: adapting a zero-copy
transport to this facade copies receive leases into owned frames and copies
owned sends into transmit loans.

# Features

* `util` enables in-crate utility implementations such as the local transport
  and communication-layer helpers. It is enabled by default.
* `protobuf-wire` enables optional Protocol Buffers payload codec support.
  Protocol Buffers remain payload bytes only; they are not used as the frame
  envelope. uSubscription service DTO payloads use this feature for their
  protobuf-defined service wire format.
* `symphony` enables Symphony deployment-target helpers.
* `test-util` enables `mockall` mocks for supported public traits and in-memory
  transport fakes under [`test_util`].

Service-specific data transfer objects are grouped under modules such as
[`usubscription`] instead of being wildcard-exported at the crate root.
*/

#[cfg(feature = "util")]
pub mod local_transport;

pub mod cloudevents;

pub mod communication;

pub mod core;

#[cfg(any(test, feature = "test-util"))]
pub mod test_util;

#[cfg(feature = "symphony")]
pub mod symphony;

#[cfg(feature = "protobuf-wire")]
pub mod protobuf_wire;

mod uframe;
pub use uframe::{
    RawBytes, UAttributes, UDeserializer, UEncoding, UErasedSerializer, UFrameBuilder,
    UFrameBuilderError, UFrameMetadata, UMessageType, UOwnedFrame, UPriority, USerializer,
    UTxBuffer, UVecTxBuffer, UWireError, UZeroCopyRxFrame, WireFormat,
};

mod uattributes;
pub use uattributes::{
    validate_rpc_priority, NotificationValidator, PublishValidator, RequestValidator,
    ResponseValidator, UAttributesError, UAttributesValidator, UAttributesValidators,
};

#[cfg(feature = "protobuf-wire")]
pub use protobuf_wire::ProtobufWire;

mod uri;
pub use uri::{UUri, UUriError};

mod transport_endpoint;
pub use transport_endpoint::{
    UOwnedFrameEndpoint, UOwnedFrameEndpointMode, UOwnedFrameEndpointRegistration,
};

pub mod usubscription;

mod ustatus;
pub use ustatus::{UCode, UStatus};

mod utransport;
pub use utransport::{
    verify_filter_criteria, ComparableOwnedListener, LocalUriProvider, StaticUriProvider,
    UOwnedListener, UOwnedTransport, UOwnedTransportExt, UZeroCopyListener, UZeroCopyTransport,
    UZeroCopyTransportExt,
};

#[cfg(any(test, feature = "test-util"))]
pub use utransport::{
    MockLocalUriProvider, MockUOwnedListener, MockUOwnedTransport, MockUZeroCopyTransport,
};

mod uuid;
pub use uuid::UUID;

#[cfg(feature = "protobuf-wire")]
pub mod up_core_api {
    include!(concat!(env!("OUT_DIR"), "/uprotocol/mod.rs"));
}

/// Frame metadata, builders, and owned-frame types for the common path.
pub mod frame {
    pub use crate::{
        UAttributes, UFrameBuilder, UFrameBuilderError, UFrameMetadata, UMessageType, UOwnedFrame,
        UPriority,
    };
}

/// Serializer-neutral payload wire-format contracts and helpers.
pub mod wire {
    pub use crate::{
        RawBytes, UDeserializer, UEncoding, UErasedSerializer, USerializer, UWireError, WireFormat,
    };
}

/// Owned-buffer transport APIs and endpoint adapters.
pub mod transport {
    pub use crate::{
        verify_filter_criteria, ComparableOwnedListener, LocalUriProvider, StaticUriProvider,
        UOwnedFrameEndpoint, UOwnedFrameEndpointMode, UOwnedFrameEndpointRegistration,
        UOwnedListener, UOwnedTransport, UOwnedTransportExt,
    };
}

/// Zero-copy transport capability APIs.
pub mod zero_copy {
    pub use crate::{
        UTxBuffer, UVecTxBuffer, UZeroCopyListener, UZeroCopyRxFrame, UZeroCopyTransport,
        UZeroCopyTransportExt,
    };
}

/// Common imports for applications using native owned frames.
pub mod prelude {
    pub use crate::{
        frame::{UFrameBuilder, UFrameMetadata, UMessageType, UOwnedFrame, UPriority},
        transport::{UOwnedListener, UOwnedTransport, UOwnedTransportExt},
        wire::{RawBytes, UDeserializer, UEncoding, USerializer, UWireError, WireFormat},
        UCode, UStatus, UUri, UUID,
    };
}
