# Native Frame Migration Guide

This guide explains how to migrate code that used generated `UMessage` envelopes and generated-message builder helpers to the native frame APIs.

The native transport path uses `UOwnedFrame` for owned-buffer transports and zero-copy frame leases for shared-memory transports. Protocol Buffers are still supported as an optional payload codec, but they are no longer the transport envelope.

## Key Changes

Old generated-envelope path:

```rust
// Conceptual old shape.
// UMessage contained generated attributes plus serialized payload bytes.
let message = GeneratedUMessageBuilder::publish(topic)
    .build_with_protobuf_payload(&payload)?;
transport.send(message).await?;
```

Native owned-frame path:

```rust
use up_rust::{UFrameBuilder, UOwnedTransport, UUri};

# async fn send<T>(transport: &T) -> Result<(), Box<dyn std::error::Error>>
# where
#     T: UOwnedTransport,
# {
let topic = UUri::try_from("//vehicle/4210/1/B24D")?;
let frame = UFrameBuilder::publish(topic).build_with_raw_payload(vec![0x01, 0x02])?;

transport.send_owned(frame).await?;
# Ok(())
# }
```

The important difference is that transports receive native frame metadata plus payload bytes. They do not receive a generated `UMessage` wrapper.

## Generated `UMessage` Removal

The native transport API intentionally removes generated `UMessage` as the transport envelope. `UMessage` serialized all transport metadata through Protocol Buffers, which made every transport pay the same encode/decode cost and made shared-memory zero-copy transports impossible to model honestly.

Applications should now treat the frame as two native parts:

| Frame part | Purpose |
| --- | --- |
| `UFrameMetadata` | In-memory metadata: `UAttributes` plus optional `PayloadEncoding` |
| Payload bytes or loaned payload storage | Serializer-produced application payload |

Protocol Buffers are still supported for payloads when the `protobuf-wire` feature is enabled. They are not used to wrap `UAttributes` into a generated transport envelope.

## `UFrameMetadata` And `UAttributes`

`UFrameMetadata` is not a new protocol header. It is the native frame metadata object passed between applications, transports, and streamers. It contains:

| Field | Meaning |
| --- | --- |
| `attributes()` | The uProtocol `UAttributes` source of truth |
| `encoding()` | `Option<&PayloadEncoding>` for the payload representation |

Use `frame.metadata().attributes()` to inspect message type, source, sink, TTL, priority, request ID, token, permission level, traceparent, and communication status. Use `frame.metadata().encoding()` to preserve or validate the payload format when `frame.has_payload()` is true. Frames without payload intentionally carry no payload encoding.

Transport implementations should project these fields into their native metadata channel when one exists. Avoid serializing a generated attributes blob unless the transport truly has no metadata channel and the transport binding specifies an explicit native-frame prefix.

## Migration Matrix

| Old API or concept | Native replacement | Feature | Notes |
| --- | --- | --- | --- |
| Generated `UMessage` transport envelope | `UOwnedFrame` or `zero_copy::UZeroCopyRxLease` receive lease | Always | Transports receive native metadata plus payload bytes, not a generated wrapper. |
| Generated `UAttributes` payload metadata | Native `UAttributes` inside `UFrameMetadata` | Always | Use accessors and builders instead of generated public fields. |
| Generated payload-format enum | `PayloadEncoding` | Always | `PayloadEncoding::Standard(UPayloadFormat)` preserves upstream-compatible formats; `PayloadEncoding::Custom` identifies native-only byte-compatible layouts. |
| Generated `UMessageBuilder` | Native `UFrameBuilder` that builds `UOwnedFrame` | Always | The new name makes the native frame output explicit. |
| `UTransport::send` | `UOwnedTransport::send_owned` or `UZeroCopyTransport::loan_tx(UTxLoanSpec)` plus `send_zero_copy` | Always | Zero-copy send is intentionally two-phase so serializers can write directly into transport-loaned storage. |
| `UTransport::register_listener` | `register_owned_listener` or `register_zero_copy_listener` | Always | Listener type follows transport capability: `UOwnedListener` or `UZeroCopyListener`. |
| Pull-style receive helpers | `receive_owned` or `receive_zero_copy` implemented by transports that truly support pull receive | Always | The default returns `UNIMPLEMENTED`, matching mainline `UTransport`; listener-backed push receive is not hidden behind the pull API. |
| Stable-layout payload contracts | `StablePayload` plus `StableContainerPayload<T>` | Always | Stable-container metadata now uses type name, `variant=fixed`, exact size, and sufficient alignment instead of layout hashes or field descriptors. |
| Protobuf payload helpers | `build_with_protobuf_payload`, `ProtobufPayload` | `protobuf-wire` | Protobuf is a payload codec, not the transport envelope. |
| uSubscription service payloads | Generated `up-core-api` protobuf DTOs | `protobuf-wire` | uSubscription service methods use their full-fidelity protobuf service DTOs inside native frames. |
| `up_rust::*` service DTO imports | Module imports such as `up_rust::usubscription::Subscription` | Always | Service DTOs are grouped under service modules to keep the crate root focused on common frame and transport APIs. |
| Flat root-only imports | Role-focused modules such as `frame`, `payload`, `frame_wire`, `transport`, `zero_copy`, and `prelude` | Always | Root re-exports remain focused on common owned-frame application code; advanced endpoint and zero-copy types live under their role modules. |
| Generated/mock transport test helpers | `test-util` mocks and `test_util::{InMemoryOwnedTransport, InMemoryZeroCopyTransport, RecordingOwnedListener}` | `test-util` | Use mocks for communication/service traits and in-memory transports for borrowed-filter transport traits. |

Removed APIs that do not have a direct replacement are intentionally retired rather than shimmed:

| Removed API or behavior | Replacement or retirement rationale |
| --- | --- |
| Generated `UMessage` as the transport argument and listener callback value | Retired from the transport boundary. Use `UOwnedFrame` or a `zero_copy::UZeroCopyRxLease` receive lease so transports do not have to encode all metadata through Protocol Buffers. |
| Generated `UPayloadFormat` as the only payload identity | Native APIs now use `PayloadEncoding`, which can carry either a standard upstream `UPayloadFormat` or a native-only custom encoding. |
| `UTransport`, `UListener`, `ComparableListener`, and `MockTransport` | Replaced by `UOwnedTransport`, `UOwnedListener`, `transport::ComparableOwnedListener`, `MockUOwnedTransport`, and `MockUOwnedListener` for owned-buffer transports. Use `zero_copy::UZeroCopyTransport` and `zero_copy::UZeroCopyListener` only for true loaned-storage transports. |
| Panic-based builder setters from generated `UMessageBuilder` | Replaced by `UFrameBuilder` terminal methods returning `Result<UOwnedFrame, UFrameBuilderError>`. Invalid metadata is reported as a recoverable construction error. |
| `build_with_wrapped_protobuf_payload` as a transport-envelope path | Replaced by `build_with_protobuf_any_payload` / `ProtobufAnyPayload` for a payload that is itself `google.protobuf.Any`. Plain protobuf payloads use `ProtobufPayload`. |
| Direct generated `UAttributes` public-field mutation | Replaced by native `UAttributes` accessors, `with_*` modifiers, validators, and checked `UFrameBuilder`/`UFrameMetadata::try_*` constructors. |
| Generated service DTOs wildcard-exported at crate root | Replaced by module imports such as `up_rust::usubscription::SubscriptionRequest` with `protobuf-wire` enabled. |
| Listener-backed default pull receive helper | Retired. `receive_owned` and `receive_zero_copy` default to `UNIMPLEMENTED`; push-oriented transports should expose listener registration instead of hiding temporary registrations behind pull receive. |
| Stable-layout public types such as `PayloadContract`, `StableLayout`, `StableFromBytes`, `StableIntoBytes`, `StableFieldDescriptor`, `StableLayoutDescriptor`, and `PayloadEndian` | Retired. Use `#[derive(StablePayload)]` and `StableContainerPayload<T>` for fixed-size typed zero-copy payloads. |
| Stable metadata fields `version`, `endian`, `layout`, and `layout_hash` | Retired. New stable-container metadata is `type`, `variant=fixed`, `size`, and `align`; legacy metadata is rejected. |

## Stable-Container Payload Migration

Fixed-size typed zero-copy payloads now use one public safety contract. The `StablePayload` derive macro emits the matching `ZeroCopySend` implementation, so application payload types no longer derive both traits for uProtocol stable containers.

Old shape:

```rust,ignore
#[repr(C)]
#[derive(PlacementDefault, ZeroCopySend, StablePayload)]
#[stable_payload(type_id = "example.vehicle.VehiclePose", version = 1)]
struct VehiclePose {
    x: u32,
    y: u32,
}
```

New shape:

```rust,ignore
#[repr(C)]
#[derive(PlacementDefault, StablePayload)]
#[stable_payload(type_name = "example.vehicle.VehiclePose")]
struct VehiclePose {
    x: u32,
    y: u32,
}
```

Keep the old stable `type_id` string as the new `type_name` unless the payload is intentionally a different wire type. Do not append a version suffix only because the attribute changed. For `VehiclePose` above, the stable-container custom content type becomes `application/vnd.uprotocol.stable-container;type="example.vehicle.VehiclePose";variant=fixed;size=8;align=4`.

Runtime-length dynamic slice payloads are not part of this migration. They need a separate slice API because transmit loaning and receive borrowing must carry an element count and expose `&mut [T]` or `&[T]`, not one `T`. Until that API exists, `StableContainerPayload<T>` emits and accepts only `variant=fixed`.

## Owned And Zero-Copy Transport Parity

The native Rust API keeps the old `UTransport` operations but splits them by buffer ownership. Compare operations first, then map to the trait that matches the transport capability.

Use `UOwnedTransport` for the normal network, brokered, and in-process path. Use `zero_copy::UZeroCopyTransport` only when the transport can honestly loan transmit storage and return receive leases. A transport that copies into hidden buffers should stay owned.

| Operation | Owned-buffer API | Zero-copy API | Parity note |
| --- | --- | --- | --- |
| Send one frame | `send_owned(UOwnedFrame)` | `loan_tx(zero_copy::UTxLoanSpec)` then `send_zero_copy(Tx)` | This is the one intentional shape difference. Public APIs validate metadata and payload presence before delegating to implementation traits with validated input wrappers. The serializer writes only into `Tx::payload_mut()` before the loan is sent. |
| Pull receive | `receive_owned(&UUri, Option<&UUri>) -> UOwnedFrame` | `receive_zero_copy(&UUri, Option<&UUri>) -> Rx` | Both default to `UNIMPLEMENTED`; implement only for transports that truly support pull receive. |
| Push callback | `UOwnedListener::on_receive_owned(UOwnedFrame)` | `UZeroCopyListener<Rx>::on_receive_zero_copy(Rx)` | The zero-copy `Rx` value is the receive lease and should release transport resources when dropped. |
| Register listener | `register_owned_listener(...)` | `register_zero_copy_listener(...)` | Same filter semantics; listener type follows frame ownership. |
| Unregister listener | `unregister_owned_listener(...)` | `unregister_zero_copy_listener(...)` | Same registration identity semantics. |
| Typed send helper | `UOwnedTransportExt::send_serialized::<Codec, _>(...)` | `UZeroCopyTransportExt::send_serialized_zero_copy::<Codec, _>(...)` or `UZeroCopyUninitTransportExt::send_uninit_loaned_payload_as::<StableContainerPayload<T>, T>(...)` | Serialized zero-copy reserves a loan and writes bytes into it. Stable typed uninit constructs `T` directly in transport storage without a source payload copy or default pre-initialization. |
| Typed receive decode | `UOwnedFrame::deserialize::<Codec, T>()` or `zero_copy::UFrameView::deserialize_from_reader::<Codec, T>()` | `zero_copy::UZeroCopyRxLease::deserialize_from_reader::<Codec, T>()` or `zero_copy::UContiguousZeroCopyRxFrame::deserialize_borrowed::<Codec, T>()` | All check `PayloadEncoding` before decoding. Reader decode works for owned frame views and segmented receive leases; borrowed decode requires an explicit contiguous receive-lease capability. |
| Test fakes | `test_util::InMemoryOwnedTransport` | `test_util::InMemoryZeroCopyTransport` | The zero-copy fake uses `zero_copy::UVecTxBuffer` and `zero_copy::UVecRxLease` to exercise the lease shape without shared-memory middleware. |
| Trait mocks | `MockUOwnedTransport`, `MockUOwnedListener`, communication mocks such as `communication::MockRpcServer` | `zero_copy::MockUZeroCopyTransport` | Enabled by `test-util`; mock the trait shape you are depending on. |

The send mapping is easiest to remember as a buffer-ownership swap:

```rust
// Owned path: build the payload bytes first, then hand the whole frame to the transport.
let frame = UFrameBuilder::publish(topic.clone())
    .build_with_serializable::<TemperaturePayload, _>(&reading)?;
transport.send_owned(frame).await?;

// Zero-copy path: hand metadata to the transport first, then serialize into its loan.
let metadata = UFrameMetadata::try_publish(topic)?;
transport
    .send_serialized_zero_copy::<TemperaturePayload, _>(metadata, &reading)
    .await?;
```

The lower-level zero-copy equivalent is:

```rust
let payload_len = reading.encoded_len();
let layout = PayloadLayout::new(
    payload_len,
    <TemperatureReading as USerializer<TemperaturePayload>>::ALIGNMENT,
)?;
let spec = UTxLoanSpec::payload(metadata.with_encoding(TemperaturePayload::encoding()), layout)?;
let mut loan = transport
    .loan_tx(spec)
    .await?;
reading.serialize_into(loan.payload_mut())?;
transport.send_zero_copy(loan).await?;
```

Do not build a `UOwnedFrame` first just to call a zero-copy transport unless you are intentionally crossing an owned/zero-copy adapter boundary. That adds the copy the zero-copy API is designed to avoid.

`UOwnedFrameEndpoint` is that adapter boundary. It gives routing code one owned-frame facade over either transport capability. `UOwnedFrameEndpoint::from_zero_copy_copying_adapter` copies an owned send into a transmit loan and copies a zero-copy receive lease into an owned listener callback. Use it when a generic router needs one owned-frame type; do not use it to claim end-to-end zero-copy behavior.

The examples mirror the same serializer-neutral contract from different angles. [`owned_payload_codec.rs`](../examples/owned_payload_codec.rs) shows owned sends and owned listener callbacks. [`zero_copy_payload_codec.rs`](../examples/zero_copy_payload_codec.rs) shows zero-copy send helpers, pull receive, and borrowed deserialization. The payload examples differ so the zero-copy example can demonstrate borrowed receive views, but the `PayloadFormat`, `USerializer`, and `UDeserializer` contracts are the same.

## Builder Entry Points

The native `UFrameBuilder` builds `UOwnedFrame` values directly:

| Builder | Required fields |
| --- | --- |
| `publish(topic)` | Source topic, no sink |
| `notification(origin, destination)` | Source and sink |
| `request(method_to_invoke, reply_to_address, ttl)` | Method sink, reply-to source, non-zero TTL |
| `response(reply_to_address, request_id, invoked_method)` | Reply-to sink, request ID, invoked method source |
| `response_for_request(request_attributes)` | Request attributes from the incoming request |

Payload helpers set `PayloadEncoding` and payload bytes in one step when a payload is present:

| Helper | Use case |
| --- | --- |
| `build()` | No payload and no payload encoding |
| `build_with_raw_payload(...)` | Already-encoded opaque bytes |
| `build_with_payload(payload, encoding)` | Explicit `PayloadEncoding` and bytes |
| `build_with_serializable::<Codec, _>(...)` | Serializer-neutral typed payload |
| `build_with_protobuf_payload(...)` | Protobuf payload, only with `protobuf-wire` |

For response communication status, prefer `with_comm_status(...)`. The older `with_commstatus(...)` spelling remains available for code that mirrors the wire-field name.

## Building Common Message Types

Publish:

```rust
use up_rust::{UFrameBuilder, UUri};

# fn build() -> Result<(), Box<dyn std::error::Error>> {
let topic = UUri::try_from("//vehicle/4210/1/B24D")?;
let frame = UFrameBuilder::publish(topic).build_with_raw_payload(b"reading".to_vec())?;
# let _ = frame;
# Ok(())
# }
```

Notification:

```rust
use up_rust::{UFrameBuilder, UUri};

# fn build() -> Result<(), Box<dyn std::error::Error>> {
let origin = UUri::try_from("//vehicle/4210/1/8001")?;
let destination = UUri::try_from("//backend/8000/1/0001")?;
let frame = UFrameBuilder::notification(origin, destination).build()?;
# let _ = frame;
# Ok(())
# }
```

Request:

```rust
use up_rust::{UFrameBuilder, UUri};

# fn build() -> Result<(), Box<dyn std::error::Error>> {
let method = UUri::try_from("//vehicle/4210/1/0001")?;
let reply_to = UUri::try_from("//backend/8000/1/0001")?;
let frame = UFrameBuilder::request(method, reply_to, 1_000).build()?;
# let _ = frame;
# Ok(())
# }
```

Response from request attributes:

```rust
use up_rust::{UAttributes, UFrameBuilder};

# fn build(request_attributes: &UAttributes) -> Result<(), Box<dyn std::error::Error>> {
let frame = UFrameBuilder::response_for_request(request_attributes).build()?;
# let _ = frame;
# Ok(())
# }
```

## Custom Payload Codecs

Use `PayloadFormat`, `USerializer`, and `UDeserializer` for non-Protobuf payloads. The same serializer contract works for owned buffers and zero-copy transmit loans.

```rust
use up_rust::{
    payload::{PayloadFormat, UDeserializer, USerializer, UWireError},
    PayloadEncoding, UFrameBuilder, UUri,
};

struct TemperaturePayload;

struct Temperature {
    celsius: i16,
}

impl PayloadFormat for TemperaturePayload {
    fn name() -> &'static str {
        "temperature-v1"
    }

    fn encoding() -> PayloadEncoding {
        PayloadEncoding::custom("temperature-v1", "application/x.temperature")
    }
}

impl USerializer<TemperaturePayload> for Temperature {
    fn encoded_len(&self) -> usize {
        2
    }

    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        if dst.len() < 2 {
            return Err(UWireError::buffer_too_small(2, dst.len()));
        }
        dst[..2].copy_from_slice(&self.celsius.to_le_bytes());
        Ok(2)
    }
}

impl<'a> UDeserializer<'a, TemperaturePayload> for Temperature {
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
        let bytes = src
            .get(..2)
            .ok_or_else(|| UWireError::invalid_payload("temperature payload is too short"))?;
        let value = bytes
            .try_into()
            .map_err(|_| UWireError::invalid_payload("temperature payload is malformed"))?;
        Ok(Self { celsius: i16::from_le_bytes(value) })
    }
}

# fn build() -> Result<(), Box<dyn std::error::Error>> {
let topic = UUri::try_from("//vehicle/4210/1/B24D")?;
let value = Temperature { celsius: 23 };
let frame = UFrameBuilder::publish(topic)
    .build_with_serializable::<TemperaturePayload, _>(&value)?;
# let _ = frame;
# Ok(())
# }
```

Typed frame deserialization checks `PayloadEncoding` before decoding bytes. If a frame says it contains one payload codec and the caller asks for another, deserialization fails instead of handing bytes to the wrong decoder.

`PayloadEncoding::Standard` is the upstream-compatible path and can be represented by a generated `UMessage` protobuf envelope through `UAttributes.payload_format`. `PayloadEncoding::Custom` is native-frame-only metadata for byte-compatible layouts such as zero-copy structs or transport-local binary formats. Custom encodings are preserved by native transports and rejected by `ProtobufUMessageFrame` rather than being silently downgraded.

## Metadata Validation

Use `UFrameBuilder` or `UFrameMetadata::try_publish`, `try_notification`, `try_request`, and `try_response` when constructing application frames. These checked paths validate message IDs, URI roles, request TTL, RPC priority, response request IDs, and message-type-specific fields.

The explicit `UFrameMetadata::publish_unchecked`, `notification_unchecked`, `request_unchecked`, and `response_unchecked` constructors skip validation. Use them only in tests, adapters, and low-level receive/projection code that validates separately with `UAttributes::validate`, `UFrameMetadata::validate`, `UOwnedFrame::try_from_parts`, or the public transport boundary.

Native domain types with invariants keep their fields private. Use `UUri::try_from_parts`, `UUri::try_from`, `UUID::build`, `UUID::from_u64_pair`, and accessors such as `UUri::authority_name`, `UUri::ue_id`, `UUri::resource_id_raw`, `UUID::msb`, and `UUID::lsb` instead of struct literals or field mutation. Use `from_parts_unchecked` helpers only when an adapter has already validated data or needs to preserve wire-level values before explicit validation.

## Protocol Buffers Payloads

Protocol Buffers support is available behind the `protobuf-wire` feature. Enabling the feature runs build-script code generation for the checked-in `up-spec/up-core-api` protobuf files using a vendored `protoc` binary; builds without `protobuf-wire` do not require protobuf code generation.

```toml
[dependencies]
up-rust = { version = "0.10", features = ["protobuf-wire"] }
```

With the feature enabled, use `ProtobufPayload` or the builder convenience method for ordinary protobuf message bytes:

```rust
use up_rust::{UFrameBuilder, UUri};

# fn build<T>(payload: &T) -> Result<(), Box<dyn std::error::Error>>
# where
#     T: up_rust::USerializer<up_rust::ProtobufPayload>,
# {
let topic = UUri::try_from("//vehicle/4210/1/B24D")?;
let frame = UFrameBuilder::publish(topic).build_with_protobuf_payload(payload)?;
# let _ = frame;
# Ok(())
# }
```

Protocol Buffers are payload bytes only in this path. They do not wrap the transport frame. If the application payload is itself `google.protobuf.Any`, use `ProtobufAnyPayload` or `build_with_protobuf_any_payload`; that maps to upstream `UPAYLOAD_FORMAT_PROTOBUF_WRAPPED_IN_ANY`.

uSubscription service APIs are available with `protobuf-wire` and use the generated `up-core-api` DTOs directly, for example `up_rust::core::usubscription::SubscriptionRequest`. Helper functions convert between native `UUri` values and generated protobuf URI DTOs when constructing service requests.

## Constructor And Import Migration

Unchecked reconstruction APIs are now visibly named `*_unchecked`. Use them only at transport receive/projection boundaries that validate before send or delivery.

| Old low-level constructor | New unchecked name | Preferred checked/common path |
| --- | --- | --- |
| `UAttributes::new(...)` | `UAttributes::new_unchecked(...)` | `UFrameBuilder` or `UAttributes::try_new(...)` |
| `UFrameMetadata::new(...)` | `UFrameMetadata::new_unchecked(...)` | `UFrameMetadata::try_new(...)` or `UFrameBuilder::build_metadata()` |
| `UFrameMetadata::publish(...)` | `UFrameMetadata::publish_unchecked(...)` | `UFrameMetadata::try_publish(...)` or `UFrameBuilder::publish(...)` |
| `UFrameMetadata::notification(...)` | `UFrameMetadata::notification_unchecked(...)` | `UFrameMetadata::try_notification(...)` or `UFrameBuilder::notification(...)` |
| `UFrameMetadata::request(...)` | `UFrameMetadata::request_unchecked(...)` | `UFrameMetadata::try_request(...)` or `UFrameBuilder::request(...)` |
| `UFrameMetadata::response(...)` | `UFrameMetadata::response_unchecked(...)` | `UFrameMetadata::try_response(...)` or `UFrameBuilder::response(...)` |
| `UOwnedFrame::new(...)` / `with_payload(...)` | `UOwnedFrame::with_payload_unchecked(...)` | `UOwnedFrame::try_with_payload(...)`, `try_from_parts(...)`, or `UFrameBuilder` |
| `UOwnedFrame::without_payload(...)` | `UOwnedFrame::without_payload_unchecked(...)` | `UOwnedFrame::try_without_payload(...)` or `UFrameBuilder::build()` |

The crate root now exports common owned-frame APIs only. Import zero-copy, stable-container, uninitialized-loan, transport-implementation, and conformance helpers from `zero_copy`, `payload`, `transport`, or `test_util` modules.

## Transport Migration

Owned transport crates should implement `transport::UOwnedTransportImpl`. Application code still uses the public `UOwnedTransport` API, whose blanket implementation validates before calling the implementation trait:

```rust
use async_trait::async_trait;
use up_rust::{transport::{UOwnedTransportImpl, ValidatedOwnedFrame}, UStatus};

struct MyTransport;

#[async_trait]
impl UOwnedTransportImpl for MyTransport {
    async fn send_validated_owned(&self, frame: ValidatedOwnedFrame) -> Result<(), UStatus> {
        let attributes = frame.metadata().attributes();
        let encoding = frame.metadata().encoding();
        let payload = frame.payload_bytes();

        // Project attributes and encoding into the transport-native metadata channel.
        // Send payload exactly as serializer-produced bytes.
        let _ = (attributes, encoding, payload);
        Ok(())
    }
}
```

Zero-copy transport crates should implement `zero_copy::UZeroCopyTransportImpl` only when the transport can honestly loan transmit storage and return receive leases. Network and broker transports should not fake zero-copy by copying into hidden buffers. The public `loan_tx(spec)` operation maps to `ValidatedTxLoanSpec`, serializer writes into the returned `Tx` buffer, and `send_zero_copy(tx)` validates before calling `send_validated_zero_copy(tx)`. Metadata and payload presence are intentionally fixed in the loan spec so transports can compute native headers, hidden prefixes, payload offsets, and allocation sizes before exposing the payload loan.

### Pull Receive On Push-Oriented Transports

`UOwnedTransport::receive_owned` defaults to `UNIMPLEMENTED`, just like mainline `UTransport::receive`. Transports that truly support pull receive should implement it directly. Push-oriented transports should implement `register_owned_listener` and `unregister_owned_listener` without relying on a hidden listener-backed pull adapter.

This avoids a cancellation trap: a default listener-backed `receive_owned(&self, ...)` cannot guarantee async unregister cleanup if the waiting future is cancelled.

### Generic Endpoint Adapters

Use `UOwnedFrameEndpoint` when routing code needs one object that can send owned frames through either an owned transport or a true zero-copy transport:

```rust
use std::sync::Arc;
use up_rust::{transport::UOwnedFrameEndpoint, UOwnedTransport};

# fn wrap(transport: Arc<dyn UOwnedTransport>) {
let endpoint = UOwnedFrameEndpoint::from_owned(transport);
# let _ = endpoint;
# }
```

For zero-copy transports, `UOwnedFrameEndpoint::from_zero_copy_copying_adapter` copies an owned frame payload into a transmit loan for sends and copies zero-copy receive leases into owned listener callbacks for generic routing. It does not make a network or broker transport zero-copy; only transports that implement true loaned storage should use the zero-copy constructor. Treat the endpoint as an owned-frame facade for routing convenience, not as a zero-copy-preserving abstraction.

## Downstream Transport Notes

Downstream transport crates should migrate in lockstep with this branch:

| Crate | Native-frame expectation |
| --- | --- |
| `up-transport-zenoh-rust` | Implement `UOwnedTransport`. Preserve `UAttributes` and `PayloadEncoding` in Zenoh attachments; payload bytes remain exactly the serializer output. Push-only receive should return `UNIMPLEMENTED` for `receive_owned`. |
| `up-transport-mqtt5-rust` | Implement `UOwnedTransport`. Preserve `UAttributes` and `PayloadEncoding` in MQTT 5 properties; standard encodings use the upstream payload-format value and custom encodings use a native custom encoding ID plus content type. Push-only receive should return `UNIMPLEMENTED` for `receive_owned`. |
| `up-transport-vsomeip-rust` | Implement `UOwnedTransport`. Preserve frame metadata in the documented native SOME/IP prefix because vsomeip exposes only payload bytes to the language binding. The prefix is transport metadata, not a generated protobuf envelope. |
| `up-transport-iceoryx2-rust` | Implement `UZeroCopyTransport` for true shared-memory loans and may also implement `UOwnedTransport` as a copying convenience. Preserve variable metadata in the `UFM1` prefix hidden from the application payload view exposed by `payload_mut()` and `contiguous_payload()`. |
| `up-streamer-rust` | Route through `UOwnedFrameEndpoint` when one owned-frame routing abstraction is needed. Treat zero-copy endpoints wrapped this way as copy boundaries, not end-to-end zero-copy forwarding. |

## Release And PR Notes

Suggested PR/release wording:

```text
This branch intentionally changes the Rust transport boundary from generated protobuf UMessage envelopes to native serializer-neutral frames. The break removes mandatory protobuf envelope encoding from transports, makes protobuf an optional payload codec, and adds explicit owned-buffer and zero-copy transport capabilities.

No compatibility shim is provided for the old UTransport/UMessage surface because a shim would either reintroduce the generated envelope as the common transport contract or hide copies at the exact boundary this change is making explicit. Existing applications should migrate to UFrameBuilder, UOwnedFrame, UOwnedTransport, and PayloadEncoding. Transports that cannot loan storage should implement only UOwnedTransport; shared-memory transports that can loan storage should implement UZeroCopyTransport.

Downstream crates must preserve UAttributes and, when payload is present, PayloadEncoding across their transport-specific metadata channel. Standard encodings carry upstream UPayloadFormat values; custom encodings carry a custom ID and content type. Application payload bytes must remain exactly the bytes produced by the selected PayloadFormat serializer. Frames without payload must not synthesize payload encoding metadata.
```

## Test Utilities

Enable the `test-util` feature to get `mockall` mocks for the supported object-safe public traits and in-memory transport fakes for borrowed-filter transport APIs:

```toml
[dev-dependencies]
up-rust = { version = "0.10", features = ["test-util"] }
```

`up_rust::test_util::InMemoryOwnedTransport` implements `UOwnedTransport`. It records sent frames and dispatches cloned owned frames to matching owned listeners. `up_rust::test_util::InMemoryZeroCopyTransport` implements `zero_copy::UZeroCopyTransport` using `zero_copy::UVecTxBuffer` and `UOwnedFrame`, which is useful for unit tests that need to exercise the zero-copy trait shape without launching shared-memory middleware. It records sent frames, queues receive leases for `receive_zero_copy`, and dispatches matching zero-copy listener callbacks. `up_rust::test_util::RecordingOwnedListener` records delivered owned frames for assertions; zero-copy listener assertions should capture `Rx` frames directly or use `transport::UOwnedFrameEndpoint` when the code under test intentionally consumes owned callbacks.

## Validation Checklist

Use this checklist when migrating an application or transport:

1. Replace generated `UMessage` transport envelopes with `UOwnedFrame` or zero-copy frame leases.
2. Keep `UAttributes` native and project fields into transport metadata instead of serializing a generated attributes blob by default.
3. Preserve `PayloadEncoding` across the transport boundary.
4. Use `PayloadFormat` serializers for typed payloads.
5. Enable `protobuf-wire` only when Protocol Buffers payload support is needed.
6. Use `send_owned` for owned transports and `send_zero_copy` only for true loaned-storage transports.
7. Do not rely on a listener-backed default `receive_owned`; the default returns `UNIMPLEMENTED`, so implement pull receive directly only when the transport truly supports it.
8. Use `UOwnedFrameEndpoint` for generic owned/zero-copy routing boundaries.
9. Add tests for raw payloads, custom payload codecs, optional protobuf payloads, and wrong-payload-codec rejection.
