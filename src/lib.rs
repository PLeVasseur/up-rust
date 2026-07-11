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
* `zero-copy-uninit` enables user-facing typed initialization directly in
  uninitialized transport loans. Implementer-side uninitialized buffer and core
  contracts remain available independently.
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
| Zero-copy transport and loans | Root exports such as `UZeroCopyTransport`, `UTxBuffer`, `UUninitTxBuffer`, and `UZeroCopyRxLease`; `zero-copy-uninit` adds the user uninitialized-loan and publish APIs | Loan-backed mechanics remain up-L1 capability. The L2 facade is intentionally narrower than raw transport capability. |
| Selected-wire profiles | The public `wire` module plus root exports such as `UWire`, `UProtocolNativeWire`, `ProtobufWire`, `WireIdentity`, and selected-wire adapter exports such as `UWireTransport` | Selected-wire identity, metadata, and payload-family checks are up-L1 representation/profile semantics. |
| Whole-frame envelopes | The public [`frame::envelope`] module and root exports such as `UFrameWireFormat` when `owned-frame-transport` is enabled | Whole-frame `UPFE` serialization is separate from selected-wire `UPWM` metadata prefixes. |
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
* `zero-copy-uninit` enables user-facing typed uninitialized-loan payload APIs, transport conveniences, and selected-wire publish helpers. Physical transport implementer contracts remain available without it.
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

pub mod frame;
pub use frame::metadata::{
    try_project_attributes_to_frame_metadata, try_project_frame_to_umessage,
    try_project_umessage_to_frame_metadata, FrameMessageKind, FramePriority, PayloadEncoding,
    UFrameMetadata, UFrameMetadataError,
};

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

/// # Start here: implementing a WIRE format
///
/// You define how payloads become bytes; you never touch a transport:
///
/// ```text
/// payload codec:   impl PayloadCodec + EncodePayload + DecodePayload
/// wire identity:   register ids per up-spec/basics/uframe.adoc §Registry
/// metadata codec:  reuse the canonical field block (crate::frame::codec);
///                  the selected-wire adapter writes the UPWM identity prefix
/// conformance:     wire-metadata conformance + golden suites pin you;
///                  up-wire-xcdrv2 is the reference external wire
/// ```
#[cfg(feature = "wire-implementer-api")]
pub mod wire_implementer_api {
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
pub use frame::envelope::{UFrameWireError, UFrameWireFormat};

#[cfg(feature = "owned-frame-transport")]
mod owned_frame;
#[cfg(feature = "owned-frame-transport")]
pub use owned_frame::UOwnedFrame;

pub mod payload;
pub use payload::codec::{
    DecodePayload, EncodePayload, PayloadCodec, PayloadFormat, PayloadLayout, ProtobufPayload,
    ReadDecodePayload,
};
pub use payload::loan::LoanPayload;
#[cfg(feature = "zero-copy-uninit")]
pub use payload::loan::{LoanUninitPayload, LoanedInitPayload, LoanedUninitPayload};
pub use payload::stable::{
    assert_stable_payload_byte_backed_uninit, stable_payload_supports_byte_backed_uninit,
    ByteBackedStablePayload, InitializedStablePayload, StableContainerPayload,
    StableContainerPayloadInfo, StablePayload, StablePayloadInit, StablePayloadInitContext,
};
pub use payload::UWireError;
/// # Start here: the end-user surface
///
/// You are an application developer: build, send, and receive uProtocol
/// messages. You need this module and rarely anything below it.
///
/// ```text
/// build:    UMessageBuilder::{publish, notification, request, response}
/// send:     any UTransport (pick a transport crate; you never implement one)
/// receive:  register a UListener with source/sink filters
/// payloads: build_with_payload(bytes, UPayloadFormat) — or, for encodings
///           outside the legacy enum, with_assumed_payload_encoding(&enc)
/// L2:       communication::{RpcClient, RpcServer} for request/response
///           without touching transports directly
/// ```
///
/// With `zero-copy-uninit`,
/// `UZeroCopyUninitTransportExt::send_uninit_stable_payload_as` plus the
/// derive macros provide checked, borrow-backed initialization for fixed-size
/// payloads.
pub mod prelude {
    #[cfg(feature = "up-l2-notifier")]
    pub use crate::communication::Notifier;
    #[cfg(feature = "up-l2-publisher")]
    pub use crate::communication::Publisher;
    #[cfg(feature = "up-l2-rpc-client")]
    pub use crate::communication::RpcClient;
    #[cfg(feature = "up-l2-rpc-server")]
    pub use crate::communication::RpcServer;
    #[cfg(feature = "up-l2-subscriber")]
    pub use crate::communication::Subscriber;
    pub use crate::{
        UAttributes, UAttributesValidators, UCode, UListener, UMessage, UMessageBuilder,
        UMessageType, UPayloadFormat, UPriority, UStatus, UTransport, UUri, UUID,
    };
}

#[doc(hidden)]
pub mod __derive_support {
    pub use crate::payload::stable::{
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
    UZeroCopyTransport, UZeroCopyTransportExt, UZeroCopyTransportImpl,
    UZeroCopyUninitTransportImpl, ValidatedTxLoanSpec,
};
#[cfg(feature = "zero-copy-uninit")]
pub use zero_copy::{UZeroCopyUninitTransport, UZeroCopyUninitTransportExt};

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

/// # Selected-wire as a USER
///
/// Sending typed payloads over a configured wire without implementing
/// anything: wrap any zero-copy core with the wire adapters here; encoding
/// identity and envelopes are handled for you. Wire choice is per-link
/// configuration — see `req~selected-wire-configuration~1`.
#[cfg(feature = "selected-wire-user-api")]
pub mod selected_wire_user_api {
    pub use crate::{
        ProtobufWire, ProtobufWireTransport, StableContainerWireFormat,
        StableContainerWireTransport, UHasWire, UNativePrefixWireTransport, UProtocolNativeWire,
        USelectedWireZeroCopyTransport, UWireRx, UWithNativePrefixWire, WireCompatibility,
        WireIdentity, WireIdentityRef,
    };
    #[cfg(feature = "zero-copy-uninit")]
    pub use crate::{UZeroCopyUninitTransport, UZeroCopyUninitTransportExt};
}

/// # Start here: implementing a TRANSPORT
///
/// Implement the smallest contract that fits your technology; the generic
/// layers above give you every wire format and family behavior:
///
/// ```text
/// classic only:        impl UTransport             (up-l1/README.adoc)
/// owned frames:        impl UOwnedTransportImpl    (validation handled above you)
/// zero-copy:           impl UZeroCopyTransportImpl (loans + leases)
/// zero-copy + uninit:  add the uninit core methods; the safe witness
///                      machinery above you does the rest
/// ```
///
/// Your core's obligations are `req~transport-core-*` in
/// `up-spec/up-l1/transport_families.adoc`: move bytes; never interpret,
/// relabel, or re-encode. Test against `InMemoryZeroCopyTransport`
/// (`test-util`) — the executable specification.
#[cfg(feature = "transport-implementer-api")]
pub mod transport_implementer_api {
    #[cfg(feature = "owned-frame-transport")]
    pub use crate::{
        EncodedOwnedFrame, PreparedOwnedFrame, UEncodedOwnedListener, UOwnedTransportCore,
    };
    pub use crate::{
        PreparedTxLoanSpec, UEncodedLoanedRxFrame, UEncodedRxFrame, UEncodedZeroCopyListener,
        UWireTransport, UZeroCopyTransportCore, UZeroCopyUninitTransportCore,
    };
}

#[cfg(any(test, feature = "test-util", feature = "payload-contract-fixtures"))]
pub mod test_support;

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
