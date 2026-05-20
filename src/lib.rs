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

The public API separates whole-frame wire formats from application payload
codecs. Transports exchange frames without requiring generated message
envelopes, but bindings can still choose a generated `UMessage` frame wire
format when interoperability calls for it.

# Common Path

Applications usually start with [`UFrameBuilder`] and [`UOwnedFrame`]: build a
Publish, Notification, Request, or Response frame, then send it with an
[`UOwnedTransport`].

For discoverability, the same public types are grouped by role under [`frame`],
[`frame_wire`], [`payload`], [`transport`], [`zero_copy`], and [`prelude`]. The
crate root keeps the most common re-exports for short examples.

# Advanced Paths

Whole-frame wire formats implement [`frame_wire::UFrameWireFormat`]. Custom
payload codecs implement [`payload::PayloadFormat`], [`payload::USerializer`],
and [`payload::UDeserializer`]. Shared-memory transports implement
[`zero_copy::UZeroCopyTransport`] only when they can honestly loan transmit
storage and return receive leases. Zero-copy metadata is fixed at reserve time;
payload bytes are the mutable loaned storage. Routing code that needs a single
owned-frame facade over either capability can use [`transport::UOwnedFrameEndpoint`].
Its name is intentional: adapting a zero-copy transport to this facade copies
receive leases into owned frames and copies owned sends into transmit loans.

# Features

* `util` enables in-crate utility implementations such as the local transport
  and communication-layer helpers. It is enabled by default.
* `protobuf-wire` enables optional Protocol Buffers support for both the
  generated `UMessage` frame wire format and Protocol Buffers payload codec.
* `cloudevents` enables native-frame mapping to and from CloudEvents. It is
  optional, matching mainline behavior, because most applications and transport
  bindings do not need CloudEvents support in the common path.
* `symphony` enables Symphony deployment-target helpers.
* `test-util` enables `mockall` mocks for supported public traits and in-memory
  transport fakes under [`test_util`].

Service-specific data transfer objects are grouped under modules such as
[`usubscription`] instead of being wildcard-exported at the crate root.
*/

#![warn(rustdoc::bare_urls, rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "util")]
#[cfg_attr(docsrs, doc(cfg(feature = "util")))]
pub mod local_transport;

#[cfg(feature = "cloudevents")]
#[cfg_attr(docsrs, doc(cfg(feature = "cloudevents")))]
pub mod cloudevents;

#[cfg(feature = "cloudevents")]
#[cfg_attr(docsrs, doc(cfg(feature = "cloudevents")))]
pub use cloudevents::{CloudEvent, CloudEventAttributeValue, CloudEventError};

pub mod communication;

pub mod core;

#[cfg(any(test, feature = "test-util"))]
#[cfg_attr(docsrs, doc(cfg(feature = "test-util")))]
pub mod test_util;

#[cfg(feature = "symphony")]
#[cfg_attr(docsrs, doc(cfg(feature = "symphony")))]
pub mod symphony;

#[cfg(feature = "protobuf-wire")]
#[cfg_attr(docsrs, doc(cfg(feature = "protobuf-wire")))]
pub mod protobuf;

mod uframe;
pub use uframe::{
    UAttributes, UEncoding, UEncodingError, UFrameBuilder, UFrameBuilderError, UFrameMetadata,
    UFrameWireError, UFrameWireFormat, UMessageType, UOwnedFrame, UPriority,
};

mod uattributes;
pub use uattributes::{
    validate_rpc_priority, NotificationValidator, PublishValidator, RequestValidator,
    ResponseValidator, UAttributesError, UAttributesValidator, UAttributesValidators,
};

#[cfg(feature = "protobuf-wire")]
#[cfg_attr(docsrs, doc(cfg(feature = "protobuf-wire")))]
pub use protobuf::{ProtobufPayload, ProtobufUMessageFrame};

mod uri;
pub use uri::{UUri, UUriError};

mod transport_endpoint;

pub mod usubscription;

mod ustatus;
pub use ustatus::{UCode, UStatus};

mod utransport;
pub use utransport::{
    validate_frame_metadata_for_payload, validate_frame_metadata_for_transport,
    validate_owned_frame_for_transport, LocalUriProvider, StaticUriProvider, UOwnedListener,
    UOwnedTransport, UOwnedTransportExt,
};

#[cfg(any(test, feature = "test-util"))]
pub use utransport::{MockLocalUriProvider, MockUOwnedListener, MockUOwnedTransport};

mod uuid;
pub use uuid::UUID;

#[cfg(feature = "protobuf-wire")]
#[cfg_attr(docsrs, doc(cfg(feature = "protobuf-wire")))]
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

/// Whole-frame wire-format contracts.
pub mod frame_wire {
    pub use crate::uframe::{UFrameWireError, UFrameWireFormat};

    #[cfg(feature = "protobuf-wire")]
    #[cfg_attr(docsrs, doc(cfg(feature = "protobuf-wire")))]
    pub use crate::protobuf::ProtobufUMessageFrame;
}

/// Application payload codec contracts and helpers.
pub mod payload {
    pub use crate::uframe::{
        PayloadFormat, RawBytes, UDeserializer, UEncoding, UEncodingError, UErasedSerializer,
        UReadDeserializer, USerializer, UWireError,
    };

    #[cfg(feature = "protobuf-wire")]
    #[cfg_attr(docsrs, doc(cfg(feature = "protobuf-wire")))]
    pub use crate::protobuf::ProtobufPayload;
}

/// Owned-buffer transport APIs and endpoint adapters.
pub mod transport {
    pub use crate::transport_endpoint::{
        UOwnedFrameEndpoint, UOwnedFrameEndpointMode, UOwnedFrameEndpointRegistration,
    };
    pub use crate::utransport::{
        validate_frame_metadata_for_payload, validate_frame_metadata_for_transport,
        validate_owned_frame_for_transport, verify_filter_criteria, ComparableOwnedListener,
        LocalUriProvider, StaticUriProvider, UOwnedListener, UOwnedTransport, UOwnedTransportExt,
    };
}

/// Zero-copy transport capability APIs.
pub mod zero_copy {
    pub use crate::uframe::{
        UContiguousZeroCopyRxFrame, UTxBuffer, UVecTxBuffer, UZeroCopyPayloadCopyExt,
        UZeroCopyRxFrame,
    };
    pub use crate::utransport::{UZeroCopyListener, UZeroCopyTransport, UZeroCopyTransportExt};

    #[cfg(any(test, feature = "test-util"))]
    #[cfg_attr(docsrs, doc(cfg(feature = "test-util")))]
    pub use crate::utransport::MockUZeroCopyTransport;
}

/// Common imports for applications using native owned frames.
pub mod prelude {
    pub use crate::{
        frame::{UFrameBuilder, UFrameMetadata, UMessageType, UOwnedFrame, UPriority},
        frame_wire::{UFrameWireError, UFrameWireFormat},
        payload::{
            PayloadFormat, RawBytes, UDeserializer, UEncoding, UReadDeserializer, USerializer,
            UWireError,
        },
        transport::{UOwnedListener, UOwnedTransport, UOwnedTransportExt},
        UCode, UStatus, UUri, UUID,
    };

    #[cfg(feature = "protobuf-wire")]
    #[cfg_attr(docsrs, doc(cfg(feature = "protobuf-wire")))]
    pub use crate::frame_wire::ProtobufUMessageFrame;
}
