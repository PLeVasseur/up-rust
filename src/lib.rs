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
crate root keeps common application-facing frame, URI/status, payload-codec, and
owned-transport re-exports for short examples. Transport implementation helpers
and validators remain root-visible because owned transport implementations use
them to share the same public boundary checks as application sends; they are also
grouped under [`transport`]. Advanced zero-copy and unsafe payload surfaces
require role-module imports instead of root imports.

# Advanced Paths

Whole-frame wire formats implement [`frame_wire::UFrameWireFormat`]. Custom
payload codecs implement [`payload::PayloadCodec`] and whichever encode/decode
capability traits they support. Shared-memory transports implement
[`zero_copy::UZeroCopyTransport`] only when they can honestly loan transmit
storage and return receive leases. Zero-copy metadata is fixed at reserve time;
payload bytes are the mutable loaned storage. Payload-level typed zero-copy
receive is represented by [`zero_copy::ULoanedContiguousZeroCopyRxFrame`]; a
merely contiguous payload is not automatically loan-backed. Routing code that
needs a single owned-frame facade over either capability can use
[`transport::UOwnedFrameEndpoint`]. Its name is intentional: adapting a zero-copy
transport to this facade copies receive leases into owned frames and copies owned
sends into transmit loans.

# Stable Payloads

[`payload::StableContainerPayload`] is the fixed-size typed zero-copy payload
codec. A [`payload::StablePayload`] type follows the iceoryx2 `ZeroCopySend`
safety model: the stable type name is the cross-process identity, and runtime
compatibility uses the type name, `variant=fixed`, exact size, and sufficient
advertised alignment. The core stable-container path does not use layout hashes,
field descriptors, or fingerprints. Transports must preserve custom encoding
metadata and payload bytes, but they must not parse stable-container type-detail
parameters.

The `StablePayload` derive macro owns the matching `ZeroCopySend` implementation.
Use `#[stable_payload(type_name = "...")]` on `#[repr(C)]` or
`#[repr(transparent)]` fixed-size payload structs. Add
`ByteBackedStablePayload` derive for payloads used with safe stable-container TX
or encode. Runtime-length slice payloads are reserved for a future API; this
release only emits and accepts `variant=fixed`.

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
  transport fakes under `test_util`.
* `experimental-loaned-frame` exposes the experimental [`LoanedFrame`] routing
  building block. It keeps a receive lease alive while ordered payload slices are
  copied directly into a transmit loan. It is for copy-minimized routing and does
  not imply zero-copy-preserving forwarding.
* `unsafe-stable-payload-tx`, `unsafe-stable-payload-init`, and
  `unsafe-uninit-payload-bytes` expose expert payload APIs with caller-side
  initialization proof obligations. Direct manual `ByteBackedStablePayload`
  impls are ordinary unsafe impls; prefer the checked derive unless an expert
  FFI/codegen boundary owns the byte-level proof. `expert-unsafe-payloads`
  enables all unsafe payload features.

Service-specific data transfer objects are grouped under modules such as
[`usubscription`] instead of being wildcard-exported at the crate root.
*/

#![warn(rustdoc::bare_urls, rustdoc::broken_intra_doc_links)]
#![deny(unsafe_op_in_unsafe_fn)]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate self as up_rust;

pub use up_rust_macros::{ByteBackedStablePayload, StablePayload};

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
    BorrowPayload, BytePayloadCodec, CustomPayloadEncoding, DecodePayload, EncodePayload,
    EncodedPayload, McapPayload, PayloadCodec, PayloadEncoding, PayloadEncodingError,
    PayloadFormat, RawBytes, UAttributes, UDeserializer, UFrameBuilder, UFrameBuilderError,
    UFrameMetadata, UMessageType, UOwnedFrame, UPayloadFormat, UPriority, USerializer, UWireError,
};

mod uattributes;
pub use uattributes::{
    validate_rpc_priority, NotificationValidator, PublishValidator, RequestValidator,
    ResponseValidator, UAttributesError, UAttributesValidator, UAttributesValidators,
};

#[cfg(feature = "protobuf-wire")]
#[cfg_attr(docsrs, doc(cfg(feature = "protobuf-wire")))]
pub use protobuf::{ProtobufAnyPayload, ProtobufPayload, ProtobufUMessageFrame};

mod uri;
pub use uri::{UUri, UUriError};

mod transport_endpoint;

pub mod usubscription;

mod ustatus;
pub use ustatus::{UCode, UStatus};

#[doc(hidden)]
pub mod __derive_support {
    pub use crate::uframe::ByteBackedStablePayloadField;
}

mod utransport;
pub use utransport::{
    validate_frame_metadata_for_payload, validate_frame_metadata_for_transport,
    validate_owned_frame_for_transport, verify_filter_criteria, LocalUriProvider,
    StaticUriProvider, UOwnedListener, UOwnedTransport, UOwnedTransportExt,
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
        CustomPayloadEncoding, PayloadEncoding, PayloadEncodingError, UAttributes, UFrameBuilder,
        UFrameBuilderError, UFrameMetadata, UMessageType, UOwnedFrame, UPayloadFormat, UPriority,
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
        assert_stable_payload_byte_backed_uninit, stable_payload_supports_byte_backed_uninit,
        BorrowPayload, ByteBackedStablePayload, BytePayloadCodec, CustomPayloadEncoding,
        DecodePayload, DynPayloadCodec, EncodePayload, EncodedPayload, LoanPayload,
        LoanUninitPayload, LoanedInitPayload, LoanedUninitPayload, McapPayload, PayloadCodec,
        PayloadCodecCapabilities, PayloadCodecRegistry, PayloadEncoding, PayloadEncodingError,
        PayloadFormat, PayloadLayout, PlacementDefault, RawBytes, ReadDecodePayload,
        StableContainerPayload, StableContainerPayloadInfo, StablePayload, StablePayloadVariant,
        StableTypeDetail, TypedPayloadCodec, UDeserializer, UErasedSerializer, UPayloadFormat,
        UReadDeserializer, USerializer, UWireError, ZeroCopySend,
    };
    #[cfg(any(
        feature = "unsafe-stable-payload-tx",
        feature = "expert-unsafe-payloads"
    ))]
    pub use crate::uframe::{UnsafeStablePayloadTxSlot, ZeroedStablePayloadTxSlot};

    #[cfg(feature = "protobuf-wire")]
    #[cfg_attr(docsrs, doc(cfg(feature = "protobuf-wire")))]
    pub use crate::protobuf::{ProtobufAnyPayload, ProtobufPayload};
}

/// Owned-buffer transport APIs and endpoint adapters.
pub mod transport {
    pub use crate::transport_endpoint::{
        UOwnedFrameEndpoint, UOwnedFrameEndpointMode, UOwnedFrameEndpointRegistration,
    };
    pub use crate::utransport::{
        validate_frame_metadata_for_payload, validate_frame_metadata_for_transport,
        validate_frame_view_for_transport, validate_owned_frame_for_transport,
        verify_filter_criteria, ComparableOwnedListener, LocalUriProvider, StaticUriProvider,
        UOwnedListener, UOwnedTransport, UOwnedTransportExt, UOwnedTransportImpl, UTxLoanSpec,
        UTxPayloadSpec, ValidatedOwnedFrame, ValidatedTxLoanSpec,
    };
}

/// Zero-copy transport capability APIs.
pub mod zero_copy {
    #[cfg(feature = "experimental-loaned-frame")]
    #[cfg_attr(docsrs, doc(cfg(feature = "experimental-loaned-frame")))]
    pub use crate::uframe::{copy_loaned_frame_payload_to_tx, LoanedFrame, ZeroCopyLoanedFrame};
    pub use crate::uframe::{
        verify_contiguous_rx_payload_layout, verify_loaned_rx_payload_layout,
        verify_tx_buffer_payload_layout, verify_uninit_tx_buffer_payload_layout, LoanedPayload,
        LoanedPayloadMut, LoanedPayloadUninitMut, LoanedUninitByteWriter, PayloadLoanProvenance,
        UContiguousZeroCopyRxFrame, UFrameView, ULoanedContiguousZeroCopyRxFrame, UTxBuffer,
        UUninitTxBuffer, UVecRxLease, UVecTxBuffer, UVecUninitTxBuffer, UZeroCopyPayloadCopyExt,
        UZeroCopyRxLease,
    };
    pub use crate::utransport::{
        UTxLoanSpec, UTxPayloadSpec, UZeroCopyListener, UZeroCopyTransport, UZeroCopyTransportExt,
        UZeroCopyTransportImpl, UZeroCopyUninitTransport, UZeroCopyUninitTransportExt,
        UZeroCopyUninitTransportImpl, ValidatedTxLoanSpec,
    };

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
            BorrowPayload, BytePayloadCodec, DecodePayload, EncodePayload, EncodedPayload,
            McapPayload, PayloadCodec, PayloadEncoding, PayloadFormat, RawBytes, UDeserializer,
            UPayloadFormat, USerializer, UWireError,
        },
        transport::{UOwnedListener, UOwnedTransport, UOwnedTransportExt},
        UCode, UStatus, UUri, UUID,
    };

    #[cfg(feature = "protobuf-wire")]
    #[cfg_attr(docsrs, doc(cfg(feature = "protobuf-wire")))]
    pub use crate::frame_wire::ProtobufUMessageFrame;
}
