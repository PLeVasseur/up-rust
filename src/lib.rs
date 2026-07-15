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

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(missing_docs, missing_debug_implementations)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

/*!
up-rust is the Rust language library for [Eclipse uProtocol&trade;](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/README.adoc) —
a protocol that lets software components (in vehicles and beyond) publish
data, subscribe to it, and call each other's services over whatever
messaging technology a deployment happens to use: MQTT, Zenoh, DDS,
shared memory, and more.

The idea in one sentence: **you write against one small messaging API, and
the technology underneath stays swappable.**

## Your first message

```rust,no_run
use up_rust::{UMessageBuilder, UPayloadFormat, UTransport, UUri};

async fn publish_engine_temp(transport: &dyn UTransport) -> Result<(), Box<dyn std::error::Error>> {
    // Every message has a source address: authority / entity / version / resource.
    let topic = UUri::try_from_parts("my-vehicle", 0x1_0001, 1, 0x8001)?;

    let message = UMessageBuilder::publish(topic)
        .build_with_payload("92.5", UPayloadFormat::Text)?;

    transport.send(message).await?;
    Ok(())
}
```

That is the whole application-side model: build a [`UMessage`], hand it to
a [`UTransport`] someone has configured, done. Receiving is the mirror
image — register a [`UListener`] for the addresses you care about.

## Which reader are you?

| You want to… | Start at | Cargo features |
| --- | --- | --- |
| **Write an application** — publish, subscribe, or call/serve RPC | The role traits in [`communication`]: `Publisher`, `Subscriber`, `Notifier`, `RpcClient`, `RpcServer` — ready-made on top of any transport | `communication` |
| **Connect a messaging technology** — make uProtocol run over your broker/bus | [`UTransport`] (start here), then the [`guide`]'s transport chapter for the richer frame-based options | `transport-implementer-api` |
| **Add a payload encoding** — carry a new serialization format efficiently | The [`guide`]'s wire chapter and [`wire_implementer_api`] | `wire-implementer-api` |
| **Move large payloads without copies** — camera frames, LiDAR scans, big tables | The typed path in the [transport chapter](crate::guide::applications::transport) (apps) or the [zero-copy tutorial](crate::guide::transports::zero_copy) (transports) | `zero-copy-transport` (+ your side's feature above) |

The [`guide`] walks each path end to end with code.

## Five words you'll meet everywhere

* **uEntity** — any software component that talks uProtocol (an app, a
  service, a sensor feed).
* **Transport Layer ("up-L1")** — the layer that physically moves message
  bytes; a *transport* is one implementation of it (Zenoh, MQTT, DDS, …).
* **Communication Layer ("up-L2")** — the friendly role API
  (publish/subscribe/RPC) built on top of any transport.
* **Wire format** — how a payload is encoded into bytes (protobuf, CDR,
  Arrow, …). Wires and transports are independent: any wire can ride any
  capable transport.
* **Zero-copy loan** — for large payloads, a transport can *lend* you its
  own transmit buffer so you write data exactly once, directly where it
  will be sent from. The receive side hands payloads out the same way, as
  read-only *leases*.

## Module map

* [`communication`] — the up-L2 roles most applications use.
* [`umessage`](UMessage), [`uri`](UUri), [`uattributes`](UAttributes) — the
  core message model.
* [`utransport`](UTransport) — the up-L1 contract transports implement.
* [`wire`] and [`payload`] — wire identities and payload codecs.
* [`frame`] — the validated frame model advanced transports exchange.
* [`guide`] — tutorials for every audience above.

## Features

No feature is enabled by default; enable what your role needs. Each
feature below silently enables everything it requires. Constrained
publish-only builds need **no feature at all**: use the Transport Layer
directly — it's the first example above, and it carries no protobuf and
no tokio.

*/
#![cfg_attr(feature = "document-features", doc = document_features::document_features!())]
#![doc = "## References"]
#![doc = ""]
#![doc = "* [uProtocol Specification](https://github.com/eclipse-uprotocol/up-spec/tree/v1.6.0-alpha.7)"]

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
pub use frame::{validate_frame_view_for_transport, UFrameView};
#[doc = include_str!("guide/README.md")]
pub mod guide {
    #[doc = include_str!("guide/applications.md")]
    pub mod applications {
        #[doc = include_str!("guide/communication.md")]
        pub mod communication {}
        #[doc = include_str!("guide/transport.md")]
        pub mod transport {}
    }
    #[doc = include_str!("guide/transports.md")]
    pub mod transports {
        #[doc = include_str!("guide/utransport.md")]
        pub mod utransport {}
        #[doc = include_str!("guide/owned.md")]
        pub mod owned {}
        #[doc = include_str!("guide/zero_copy.md")]
        pub mod zero_copy {}
    }
    #[doc = include_str!("guide/wires.md")]
    pub mod wires {}
    #[doc = include_str!("guide/trait_map.md")]
    pub mod trait_map {}
}
pub use frame::metadata::{
    FrameMessageKind, FramePriority, PayloadEncoding, UFrameMetadata, UFrameMetadataError,
};
// The projection free functions live in `frame::metadata`; the crate-root
// surface is the method forms: `UAttributes::to_frame_metadata`,
// `UMessage::to_frame_metadata`, `UOwnedFrame::to_umessage`.

#[cfg(feature = "wire-implementer-api")]
pub mod wire;
#[cfg(not(feature = "wire-implementer-api"))]
mod wire;
#[cfg(any(feature = "selected-wire-user-api", feature = "wire-implementer-api"))]
pub use wire::NativePrefixFrameMetadataCodec;
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

/// # Implementing a wire format
///
/// A *wire format* is one payload encoding — how a Rust value becomes
/// bytes on the wire and back. Implement it once, in one small crate, and
/// it runs over **every** capable transport with no transport-specific
/// code: the library composes your codec above any transport core.
///
/// Read the [`guide`]'s wire chapter first for the annotated
/// code skeleton; this page is the contract-level checklist.
///
/// ## The walk
///
/// 1. Define the representation profile, canonical bytes or allowed variants,
///    byte order, complete-consumption rule, and negative vectors.
/// 2. Implement [`PayloadCodec`], [`EncodePayload`], [`DecodePayload`], and,
///    where ordered-reader decode is useful, [`ReadDecodePayload`].
/// 3. Define a literal-labeled experimental [`WireIdentity`] and marker
///    implementing [`UWire`]. Compact codes are unique within the selected-wire,
///    metadata-layout, or payload-family namespace; experimental codes are not
///    registered or stable.
/// 4. Associate payload types through [`UWirePayload`]. The associated codec can
///    be a separate type. Reuse [`NativePrefixFrameMetadataCodec`] unless the
///    wire needs another documented metadata profile.
/// 5. Exercise wrong-wire, wrong-payload-family, unknown-layout, full-consumption,
///    golden/independent-vector, malformed-reader, and round-trip cases. The
///    external XCDRv2, Arrow, and OMGIDL crates show different profile choices;
///    none replaces the governing selected-wire contract.
///
/// The selected-wire adapter writes and checks the `UPWM` identity prefix
/// before profile decoding. Unknown identities therefore fail before bytes can
/// reach the wrong metadata or payload decoder
/// (`req~selected-wire-explicit-rejection~1`). Identity allocation, payload
/// representation, metadata layout, and transport carriage are separate
/// concerns; a wire implementation must not silently merge their registries or
/// put transport-specific mechanics in its codec.
#[cfg(feature = "wire-implementer-api")]
pub mod wire_implementer_api {
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

mod validation_state;
pub use validation_state::{Unvalidated, Validated};

/// Typed payload machinery: codecs, loans, and stable payloads.
pub mod payload;
pub use payload::codec::{
    DecodePayload, EncodePayload, PayloadCodec, PayloadFormat, PayloadLayout, ProtobufPayload,
    ReadDecodePayload,
};
pub use payload::loan::LoanPayload;
#[cfg(feature = "zero-copy-transport")]
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
/// With `zero-copy-transport`,
/// `UZeroCopyUninitTransportExt::send_uninit_stable_payload_as` plus the
/// derive macros provide checked, borrow-backed initialization for fixed-size
/// payloads.
///
/// The compatibility message path is available with no optional feature:
///
/// ```
/// use up_rust::prelude::*;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let topic = UUri::try_from("//my-vehicle/4210/1/B24D")?;
/// let message = UMessageBuilder::publish(topic)
///     .build_with_payload("closed", UPayloadFormat::Text)?;
/// assert!(message.is_publish());
/// # Ok(())
/// # }
/// ```
pub mod prelude {
    #[cfg(feature = "up-l2-api")]
    pub use crate::communication::Notifier;
    #[cfg(feature = "up-l2-api")]
    pub use crate::communication::Publisher;
    #[cfg(feature = "up-l2-api")]
    pub use crate::communication::RpcClient;
    #[cfg(feature = "up-l2-api")]
    pub use crate::communication::RpcServer;
    #[cfg(feature = "up-l2-api")]
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

#[cfg(feature = "zero-copy-transport")]
mod zero_copy;
#[cfg(all(feature = "test-util", feature = "owned-frame-transport"))]
pub use owned_frame::InMemoryOwnedTransport;
#[cfg(all(feature = "test-util", feature = "zero-copy-transport"))]
pub use zero_copy::InMemoryZeroCopyTransport;
#[cfg(feature = "perf-diagnostics")]
#[doc(hidden)]
pub use zero_copy::UninitStableSendPhases;
#[cfg(feature = "zero-copy-transport")]
pub use zero_copy::{
    validate_tx_buffer_for_transport, verify_tx_buffer_payload_layout,
    verify_uninit_tx_buffer_payload_layout, LoanedPayload, LoanedPayloadUninitMut,
    PayloadAlignment, PayloadLoanProvenance, ULoanedContiguousZeroCopyRxFrame, UTxBuffer,
    UTxLoanSpec, UTxPayloadSpec, UUninitTxBuffer, UVecRxLease, UVecTxBuffer, UVecUninitTxBuffer,
    UZeroCopyListener, UZeroCopyRxLease, UZeroCopyTransport, UZeroCopyTransportExt,
    UZeroCopyTransportImpl, UZeroCopyUninitTransportImpl,
};
#[cfg(feature = "zero-copy-transport")]
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
pub use utransport::{UOwnedListener, UOwnedTransport, UOwnedTransportImpl};

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
#[cfg(feature = "transport-implementer-api")]
pub use wire_transport::UEncodedRxFrame;
#[cfg(all(feature = "selected-wire-user-api", feature = "zero-copy-transport"))]
pub use wire_transport::USelectedWireZeroCopyTransport;
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
#[cfg(all(feature = "transport-implementer-api", feature = "zero-copy-transport"))]
pub use wire_transport::{
    PreparedTxLoanSpec, UEncodedLoanedRxFrame, UEncodedZeroCopyListener, UZeroCopyTransportCore,
    UZeroCopyUninitTransportCore,
};
#[cfg(feature = "selected-wire-user-api")]
pub use wire_transport::{
    ProtobufWireTransport, StableContainerWireTransport, UNativePrefixWireTransport,
    UWithNativePrefixWire,
};
#[cfg(feature = "selected-wire-user-api")]
pub use wire_transport::{UHasWire, UWireRx};

/// # Selected-wire as a user
///
/// Wrap an encoded core (`UZeroCopyTransportCore`/`UOwnedTransportCore`) once, then use typed payload helpers without
/// implementing a wire or transport. Wire choice is construction-time link
/// configuration, never per-message negotiation
/// (`req~selected-wire-configuration~1`).
///
/// ```text
/// use up_rust::{StableContainerWireFormat, UWithNativePrefixWire};
///
/// let transport = core.into_native_prefix_wire_transport(StableContainerWireFormat);
/// transport.send_uninit_stable_payload::<MyStablePayload>(metadata, |init| {
///     init.field(value)?.finish()
/// }).await?;
/// ```
///
/// The snippet is schematic because `core`, metadata, and the payload type come
/// from the chosen transport/application. The constructor and helper names are
/// the real API. Receive leases expose [`UFrameView`] directly; contiguous
/// stable payloads are borrowed with
/// [`ULoanedContiguousZeroCopyRxFrame::borrow_stable_payload`].
#[cfg(feature = "selected-wire-user-api")]
pub mod selected_wire_user_api {
    #[cfg(feature = "zero-copy-transport")]
    pub use crate::USelectedWireZeroCopyTransport;
    pub use crate::{
        ProtobufWire, ProtobufWireTransport, StableContainerWireFormat,
        StableContainerWireTransport, UHasWire, UNativePrefixWireTransport, UProtocolNativeWire,
        UWireRx, UWithNativePrefixWire, WireCompatibility, WireIdentity, WireIdentityRef,
    };
    #[cfg(feature = "zero-copy-transport")]
    pub use crate::{UZeroCopyUninitTransport, UZeroCopyUninitTransportExt};
}

/// # Implementing a transport
///
/// A *transport* moves message bytes over one technology (broker, bus,
/// shared memory). Capability comes in three families — `UTransport` messages,
/// owned frames, zero-copy loans — and you implement only the levels your
/// technology honestly supports. For zero-copy there are two entry
/// points, named by trait: [`UZeroCopyTransportImpl`] if your API speaks
/// the library's frame types directly, or [`UZeroCopyTransportCore`] if
/// your technology should stay a dumb byte pipe while the library layers
/// wires and codecs on top (the recommended default). They are related
/// but not interchangeable; the [`guide`]'s transport
/// chapter walks both with code.
///
/// 1. Implement [`UTransport`] for standard `UMessage` carriage.
/// 2. Implement `UOwnedTransportImpl` or [`UZeroCopyTransportImpl`] when the
///    technology exposes semantic native frames. These traits receive validated
///    semantic metadata.
/// 3. Implement the encoded core traits exported by this module when selected
///    wires should compose above the technology. [`UZeroCopyTransportCore`]
///    receives encoded metadata bytes and storage layouts, not attributes.
///
/// For zero-copy, the required semantic operations are TX loan and commit;
/// pull receive and listener registration hooks have default unsupported
/// implementations. [`UZeroCopyUninitTransportImpl`] adds one required
/// uninitialized-loan operation. The encoded core has the corresponding split.
///
/// Implementing the encoded core buys every compatible selected wire, metadata
/// validation and identity rejection, typed stable initialization, and family
/// adapters without teaching the physical transport those semantics. Its
/// obligations are the `req~transport-core-*~1` and `req~zero-copy-*~1`
/// requirements in `up-spec/up-l1/transport_families.adoc`.
///
/// Use `InMemoryZeroCopyTransport` (feature `test-util`) for semantic behavior and the
/// `wire_transport_adapter` plus payload-contract suites for encoded-core
/// conformance.
#[cfg(feature = "transport-implementer-api")]
pub mod transport_implementer_api {
    #[cfg(feature = "owned-frame-transport")]
    pub use crate::{
        EncodedOwnedFrame, PreparedOwnedFrame, UEncodedOwnedListener, UOwnedTransportCore,
    };
    #[cfg(feature = "zero-copy-transport")]
    pub use crate::{
        PreparedTxLoanSpec, UEncodedLoanedRxFrame, UEncodedZeroCopyListener,
        UZeroCopyTransportCore, UZeroCopyUninitTransportCore,
    };
    pub use crate::{UEncodedRxFrame, UWireTransport};
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
