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
up-rust is the [Eclipse uProtocol&trade; Language Library](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/languages.adoc) for the
Rust programming language.

This crate can be used to

* implement uEntities that communicate with each other using the uProtocol [Communication Layer API](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l2/api.adoc)
  over one of the supported transport protocols.
* implement support for an additional transport protocol by means of implementing the [Transport & Session Layer API](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l1/README.adoc).

## Public API Tiers

The crate root is a compatibility import surface. Existing root imports such
as `UMessage`, `UTransport`, `UOwnedTransport`, `UZeroCopyTransport`, wire
helpers, payload helpers, mocks, and fixtures remain available. The tiers below
describe the intended starting points for new code; they are not deprecations or
capability removals.

* Ordinary application and service code should start from the Communication
  Layer roles in `communication`, such as publishers, subscribers, notifiers,
  RPC clients, and RPC servers.
* Compatibility transport code should use `UTransport`, `UListener`,
  `UMessage`, `UAttributes`, `UUri`, and `UStatus` when implementing or adapting
  the ordinary message transport contract.
* Native-frame, selected-wire, payload-codec, and zero-copy names are advanced
  Transport Layer and wire-representation surfaces for transport authors,
  codecs, routing adapters, and loan-backed paths. New selected-wire users
  should enable `selected-wire-user-api`; external wire authors should enable
  `wire-implementer-api`; physical transport authors should enable
  `transport-implementer-api`.
* Unsafe stable-payload transmit/init APIs and unchecked constructors are expert
  surfaces with caller-side safety obligations. They are not the default
  application path.
* Mocks, in-memory proof transports, vector-backed leases, payload fixtures, and
  benchmark fixtures are test/proof support surfaces rather than ordinary
  production application APIs.

Direct Transport Layer usage remains valid, and selected-wire or whole-frame
wire semantics remain Transport Layer representation/profile semantics consumed
by Communication Layer roles.

For direct up-L1 work, use the concrete public doors below rather than guessing
from the ordinary `communication` entry point:

| Use case | Public door | Notes |
| --- | --- | --- |
| Compatibility message transport | Root exports such as `UTransport`, `UListener`, `UMessage`, `UAttributes`, `UUri`, and `UStatus` | Direct up-L1 remains a supported compatibility path. |
| Owned native-frame transport | Root exports such as `UOwnedTransport`, `UOwnedFrame`, `ValidatedOwnedFrame`, `PreparedOwnedFrame`, and the role facade in `communication::owned` | Available with the owned-frame transport features; it does not replace `UTransport`. |
| Zero-copy transport and loans | Root exports such as `UZeroCopyTransport`, `UZeroCopyUninitTransport`, `UTxBuffer`, `UUninitTxBuffer`, `UZeroCopyRxLease`, and the publish facade in `communication::zero_copy` | Loan-backed mechanics remain up-L1 capability. The L2 facade is intentionally narrower than raw transport capability. |
| Selected-wire profiles | The public `wire` module plus root exports such as `UWire`, `UProtocolNativeWire`, `ProtobufWire`, `WireIdentity`, and selected-wire adapter exports such as `UWireTransport` | Selected-wire identity, metadata, and payload-family checks are up-L1 representation/profile semantics. |
| Whole-frame envelopes | The public `frame_wire` module and root exports such as `UFrameWireFormat` and `ProtobufUMessageFrame` when the required features are enabled | Whole-frame serialization is separate from selected-wire profile metadata. |
| Payload codecs and stable payload support | The public `payload` module plus root exports such as `PayloadCodec`, `EncodePayload`, `DecodePayload`, `StablePayload`, and `StablePayloadInit` | Safe derive/codegen paths are the supported route; the former manual unsafe slot APIs were removed. |
| Test, fake, and proof support | Feature-gated root exports such as `MockTransport`, `InMemoryZeroCopyTransport`, vector leases, and payload fixtures | These remain available for tests, benchmarks, and conformance proof, not as production transport evidence by themselves. |

## Features

None of the following features are enabled by default, so you can pick and choose which parts of the library you want to use by enabling the corresponding features. Note that some features depend on each other, so enabling one feature might automatically enable other features as well. For example, enabling `up-core-types` will also enable `protobuf-support` since the generated types require that.

* `cloudevents` Enables support for mapping [crate::UMessage]s to/from [crate::CloudEvent]s using Protobuf Format according to the
  [uProtocol specification](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l1/cloudevents.adoc).
* `communication` Enables default implementations for all [Communication Layer API](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l2/api.adoc) traits on top of the [Transport & Session Layer API](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l1/README.adoc).
* `protobuf-support` Enables convenience functions on the Transport & Session as well as the Communcation Layer APIs for implicitly mapping objects to protobuf payloads on the fly. This is particularly useful for using protoc-generated types as payloads using the [message builder](crate::UMessageBuilder::build_with_protobuf_payload) or [payload constructor](crate::communication::UPayload::try_from_protobuf). The only requirement for the object types is that they implement the [crate::ProtobufMappable] trait. A blanket implementation for types generated by the `protobuf` crate is also provided. Note that it is still possible to use protobufs as payloads even without enabling this feature. The payload of a [crate::UMessage] can be set to an arbitrary byte array, so protobuf messages can be serialized to bytes and set as payload without any support from the library. Enabling this feature just adds some convenient helper functions for automatically handling the mapping to protobuf payloads. The examples also illustrate the usage of these helper functions.
* `symphony` Enables support for implementing an [Eclipse Symphony](https://github.com/eclipse-symphony) Target Provider as a uService exposed via the Communication Layer API's [RPC Server](crate::communication::RpcServer).
* `test-util` provides some useful mock implementations for testing. In particular, provides mock implementations of [UTransport] and Communication Layer API traits which make implementing unit tests a lot easier.
* `up-core-types` Enables support for mapping the crate's public API types to protobufs as defined in the uProtocol specification. This includes, for example, the `UStatus` type which is used in the Communication Layer API for conveying errors. Enabling this feature also enables `protobuf-support` since the generated types require that.
* `up-l2-api` Enables support for the [Communication Layer API](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l2/api.adoc). This includes the trait definitions for the various roles defined by the API, such as [Notifier](crate::communication::Notifier), [Publisher](crate::communication::Publisher), [Subscriber](crate::communication::Subscriber), [RpcClient](crate::communication::RpcClient) and [RpcServer](crate::communication::RpcServer).
* `up-l2-notifier` Enables a default implementation of the [Notifier](crate::communication::Notifier) trait on top of the Transport Layer API.
* `up-l2-publisher` Enables a default implementation of the [Publisher](crate::communication::Publisher) trait on top of the Transport Layer API.
* `up-l2-subscriber` Enables a default implementation of the [Subscriber](crate::communication::Subscriber) trait on top of the Transport Layer API.
* `up-l2-rpc-client` Enables a default implementation of the [RpcClient](crate::communication::RpcClient) trait on top of the Transport Layer API.
* `up-l2-rpc-server` Enables a default implementation of the [RpcServer](crate::communication::RpcServer) trait on top of the Transport Layer API.
* `udiscovery` Enables support for types required to interact with [uDiscovery service](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l3/udiscovery/v3/README.adoc)
  implementations.
* `usubscription` Enables support for types required to interact with [uSubscription service](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l3/usubscription/v3/README.adoc)
  implementations.
* `owned-frame-transport` enables an experimental native owned-frame transport API. This is additive to `UTransport` and does not replace the ordinary `UMessage` compatibility path.
* `payload-contract-fixtures` exposes representative benchmark payload fixtures, stable structs, validators, manifests, and protobuf schemas for transport benchmarks.
* `util` provides some useful helper structs. In particular, provides a [local, in-memory UTransport](crate::local_transport::LocalTransport) for exchanging messages within a single process. This transport is also used by the examples illustrating usage of the Communication Layer API.

## References

* [uProtocol Specification](https://github.com/eclipse-uprotocol/up-spec/tree/v1.6.0-alpha.7)

*/

extern crate self as up_rust;

#[cfg(feature = "cloudevents")]
mod cloudevents;
#[cfg(feature = "cloudevents")]
pub use cloudevents::{CloudEvent, CONTENT_TYPE_CLOUDEVENTS_PROTOBUF};

// [impl->dsn~communication-layer-api-namespace~1]
#[cfg(feature = "up-l2-api")]
pub mod communication;

#[cfg(any(feature = "udiscovery", feature = "usubscription"))]
pub mod core;

#[cfg(feature = "util")]
pub mod local_transport;

#[cfg(feature = "symphony")]
pub mod symphony;

pub use up_rust_macros::{ByteBackedStablePayload, StablePayload, StablePayloadInit};

#[cfg(feature = "payload-contract-fixtures")]
pub mod bench_fixtures;

mod frame_metadata;
pub use frame_metadata::{
    try_project_attributes_to_frame_metadata, try_project_frame_to_umessage,
    try_project_umessage_to_frame_metadata, FrameMessageKind, FramePriority, PayloadEncoding,
    UFrameMetadata, UFrameMetadataError,
};

pub mod frame_abi;
pub mod frame_codec;

#[cfg(feature = "wire-implementer-api")]
pub mod wire;
#[cfg(not(feature = "wire-implementer-api"))]
mod wire;
#[cfg(any(feature = "selected-wire-user-api", feature = "wire-implementer-api"))]
pub use wire::NativePrefixFrameMetadataCodec;
#[cfg(all(
    feature = "wire-implementer-api",
    feature = "selected-wire-protobuf-metadata"
))]
pub use wire::NativePrefixProtobufMetadataCodec;
#[cfg(any(feature = "selected-wire-user-api", feature = "wire-implementer-api"))]
pub use wire::{
    ProtobufWire, StableContainerWireFormat, UProtocolNativeWire, WireCompatibility, WireIdentity,
    WireIdentityRef, NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID, NATIVE_PREFIX_METADATA_LAYOUT_ID,
    PROTOBUF_PAYLOAD_FAMILY_ID, PROTOBUF_WIRE_ID, STABLE_CONTAINER_PAYLOAD_FAMILY_ID,
    STABLE_CONTAINER_WIRE_ID, UFRAME_FIELDS_METADATA_LAYOUT_ID, UPROTOCOL_NATIVE_WIRE_ID,
    XCDR_V2_PAYLOAD_FAMILY_ID, XCDR_V2_WIRE_ID,
};
#[cfg(feature = "wire-implementer-api")]
pub use wire::{UWire, UWireDecode, UWireEncode, UWirePayload, UWireReadDecode};
#[cfg(feature = "wire-implementer-api")]
pub use wire::{
    UWireMetadataCodec, UWireMetadataCodecFor, UWireMetadataContext, UWireMetadataError,
};

#[cfg(feature = "wire-implementer-api")]
pub mod wire_implementer_api {
    //! External selected-wire/profile authoring surface.
    //!
    //! Ordinary metadata is the canonical UFrame field block
    //! ([`crate::NativePrefixFrameMetadataCodec`],
    //! [`crate::UFRAME_FIELDS_METADATA_LAYOUT_ID`]); the legacy
    //! protobuf-`UAttributes` profile is exported here only when the
    //! `selected-wire-protobuf-metadata` feature is enabled.
    #[cfg(feature = "selected-wire-protobuf-metadata")]
    pub use crate::NativePrefixProtobufMetadataCodec;
    pub use crate::{
        NativePrefixFrameMetadataCodec, ProtobufWire, StableContainerWireFormat,
        UProtocolNativeWire, UWire, UWireDecode, UWireEncode, UWireMetadataCodec,
        UWireMetadataCodecFor, UWireMetadataContext, UWireMetadataError, UWirePayload,
        UWireReadDecode, WireCompatibility, WireIdentity, WireIdentityRef,
        NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID, NATIVE_PREFIX_METADATA_LAYOUT_ID,
        PROTOBUF_PAYLOAD_FAMILY_ID, PROTOBUF_WIRE_ID, STABLE_CONTAINER_PAYLOAD_FAMILY_ID,
        STABLE_CONTAINER_WIRE_ID, UFRAME_FIELDS_METADATA_LAYOUT_ID, UPROTOCOL_NATIVE_WIRE_ID,
        XCDR_V2_PAYLOAD_FAMILY_ID, XCDR_V2_WIRE_ID,
    };
}

#[cfg(feature = "owned-frame-transport")]
pub mod frame_wire;
#[cfg(feature = "owned-frame-transport")]
pub use frame_wire::{UFrameWireError, UFrameWireFormat};

#[cfg(feature = "owned-frame-transport")]
mod owned_frame;
#[cfg(feature = "owned-frame-transport")]
pub use owned_frame::UOwnedFrame;

pub mod payload;
pub use payload::{
    assert_stable_payload_byte_backed_uninit, stable_payload_supports_byte_backed_uninit,
    ByteBackedStablePayload, DecodePayload, EncodePayload, InitializedStablePayload, LoanPayload,
    LoanUninitPayload, LoanedUninitPayload, PayloadCodec, PayloadFormat, PayloadLayout,
    ProtobufPayload, ReadDecodePayload, StableContainerPayload, StableContainerPayloadInfo,
    StablePayload, StablePayloadInit, UWireError,
};
#[doc(hidden)]
pub mod __derive_support {
    pub use crate::payload::{
        ByteBackedStablePayloadField, StablePayloadInitSet, StablePayloadInitSlot,
        StablePayloadInitUnset,
    };
}

mod zero_copy;
#[cfg(feature = "test-util")]
pub use zero_copy::InMemoryZeroCopyTransport;
pub use zero_copy::{
    validate_frame_view_for_transport, validate_tx_buffer_for_transport,
    verify_tx_buffer_payload_layout, verify_uninit_tx_buffer_payload_layout, LoanedPayload,
    LoanedPayloadUninitMut, PayloadAlignment, PayloadLoanProvenance, UFrameView,
    ULoanedContiguousZeroCopyRxFrame, UTxBuffer, UTxLoanSpec, UTxPayloadSpec, UUninitTxBuffer,
    UVecRxLease, UVecTxBuffer, UVecUninitTxBuffer, UZeroCopyListener, UZeroCopyRxLease,
    UZeroCopyTransport, UZeroCopyTransportExt, UZeroCopyTransportImpl, UZeroCopyUninitTransport,
    UZeroCopyUninitTransportExt, UZeroCopyUninitTransportImpl, ValidatedTxLoanSpec,
};

mod uattributes;
pub use uattributes::{
    UAttributes, UAttributesError, UAttributesValidator, UAttributesValidators, UMessageType,
    UPayloadFormat, UPriority,
};

mod umessage;
pub use umessage::{UMessage, UMessageBuilder, UMessageError};

mod uri;
pub use uri::{ExactUUri, UUri, UUriError};

mod ustatus;
pub use ustatus::{UAny, UCode, UStatus};

mod utransport;
pub use utransport::{
    verify_filter_criteria, ComparableListener, LocalUriProvider, StaticUriProvider, UListener,
    UTransport,
};
#[cfg(feature = "test-util")]
pub use utransport::{MockTransport, MockUListener};
#[cfg(feature = "owned-frame-transport")]
pub use utransport::{UOwnedListener, UOwnedTransport, UOwnedTransportImpl, ValidatedOwnedFrame};

#[cfg(all(
    feature = "selected-wire-transport-core",
    feature = "transport-implementer-api"
))]
pub mod wire_transport;
#[cfg(all(
    feature = "selected-wire-transport-core",
    not(feature = "transport-implementer-api")
))]
mod wire_transport;
#[cfg(all(
    feature = "selected-wire-transport-core",
    any(
        feature = "transport-implementer-api",
        feature = "wire-implementer-api"
    )
))]
pub use wire_transport::UWireTransport;
#[cfg(all(
    feature = "owned-frame-transport",
    feature = "transport-implementer-api"
))]
pub use wire_transport::{
    EncodedOwnedFrame, PreparedOwnedFrame, UEncodedOwnedListener, UOwnedTransportCore,
};
#[cfg(feature = "transport-implementer-api")]
pub use wire_transport::{
    PreparedTxLoanSpec, UEncodedLoanedRxFrame, UEncodedRxFrame, UEncodedZeroCopyListener,
    UZeroCopyTransportCore, UZeroCopyUninitTransportCore,
};
#[cfg(feature = "selected-wire-user-api")]
pub use wire_transport::{
    ProtobufWireTransport, StableContainerWireTransport, UNativePrefixWireTransport,
    UWithNativePrefixWire,
};
#[cfg(feature = "selected-wire-user-api")]
pub use wire_transport::{UHasWire, USelectedWireZeroCopyTransport, UWireRx};

#[cfg(feature = "selected-wire-user-api")]
pub mod selected_wire_user_api {
    //! Selected-wire construction and route-use surface for users of a chosen wire profile.
    pub use crate::{
        ProtobufWire, ProtobufWireTransport, StableContainerWireFormat,
        StableContainerWireTransport, UHasWire, UNativePrefixWireTransport, UProtocolNativeWire,
        USelectedWireZeroCopyTransport, UWireRx, UWithNativePrefixWire, WireCompatibility,
        WireIdentity, WireIdentityRef,
    };
}

#[cfg(feature = "transport-implementer-api")]
pub mod transport_implementer_api {
    //! Physical transport-core implementation surface.
    #[cfg(feature = "owned-frame-transport")]
    pub use crate::{
        EncodedOwnedFrame, PreparedOwnedFrame, UEncodedOwnedListener, UOwnedTransportCore,
    };
    pub use crate::{
        PreparedTxLoanSpec, UEncodedLoanedRxFrame, UEncodedRxFrame, UEncodedZeroCopyListener,
        UWireTransport, UZeroCopyTransportCore, UZeroCopyUninitTransportCore,
    };
}

#[cfg(any(feature = "test-util", feature = "payload-contract-fixtures"))]
pub mod test_support {
    //! Test, fake, proof, and fixture support. These are not production transport evidence.
    #[cfg(feature = "payload-contract-fixtures")]
    pub use crate::bench_fixtures;
    #[cfg(feature = "test-util")]
    pub use crate::utransport::MockLocalUriProvider;
    #[cfg(feature = "test-util")]
    pub use crate::{InMemoryZeroCopyTransport, MockTransport, MockUListener};
}

mod uuid;
pub use uuid::UUID;

#[cfg(feature = "up-core-types")]
// protoc-generated types, see build.rs
pub(crate) mod up_core_api {
    include!(concat!(env!("OUT_DIR"), "/uprotocol/mod.rs"));
}

#[cfg(feature = "protobuf-support")]
mod protobuf_mappable;
#[cfg(feature = "protobuf-support")]
pub use protobuf_mappable::ProtobufMappable;

mod serialization_error;
pub use serialization_error::SerializationError;
