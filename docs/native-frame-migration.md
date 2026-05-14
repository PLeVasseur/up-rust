# Native Frame Migration Guide

This guide explains how to migrate code that used generated `UMessage` envelopes and generated-message builder helpers to the native frame APIs.

The native transport path uses `UOwnedFrame` for owned-buffer transports and zero-copy frame leases for shared-memory transports. Protocol Buffers are still supported as an optional payload codec, but they are no longer the transport envelope.

## Key Changes

Old generated-envelope path:

```rust
// Conceptual old shape.
// UMessage contained generated attributes plus serialized payload bytes.
let message = UMessageBuilder::publish(topic)
    .build_with_protobuf_payload(&payload)?;
transport.send(message).await?;
```

Native owned-frame path:

```rust
use up_rust::{UMessageBuilder, UOwnedTransport, UUri};

# async fn send<T>(transport: &T) -> Result<(), Box<dyn std::error::Error>>
# where
#     T: UOwnedTransport,
# {
let topic = UUri::try_from("//vehicle/4210/1/B24D")?;
let frame = UMessageBuilder::publish(topic).build_with_raw_payload(vec![0x01, 0x02])?;

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
| `UFrameMetadata` | In-memory metadata: `UAttributes` plus `UEncoding` |
| Payload bytes or loaned payload storage | Serializer-produced application payload |

Protocol Buffers are still supported for payloads when the `protobuf-wire` feature is enabled. They are not used to wrap `UAttributes` into a generated transport envelope.

## `UFrameMetadata` And `UAttributes`

`UFrameMetadata` is not a new protocol header. It is the native frame metadata object passed between applications, transports, and streamers. It contains:

| Field | Meaning |
| --- | --- |
| `attributes()` | The uProtocol `UAttributes` source of truth |
| `encoding()` | The payload representation as `UEncoding` |

Use `frame.metadata().attributes()` to inspect message type, source, sink, TTL, priority, request ID, token, permission level, traceparent, and communication status. Use `frame.metadata().encoding()` to preserve or validate the payload format.

Transport implementations should project these fields into their native metadata channel when one exists. Avoid serializing a generated attributes blob unless the transport truly has no metadata channel and the transport binding specifies an explicit native-frame prefix.

## Type Mapping

| Old concept | Native replacement |
| --- | --- |
| Generated `UMessage` transport envelope | `UOwnedFrame` or zero-copy frame lease |
| Generated `UAttributes` payload metadata | Native `UAttributes` |
| Generated payload-format enum | `UEncoding` |
| Generated `UMessageBuilder` | Native `UMessageBuilder` that builds `UOwnedFrame` |
| `UTransport::send` | `UOwnedTransport::send_owned` or `UZeroCopyTransport::send_zero_copy` |
| `UTransport::register_listener` | `register_owned_listener` or `register_zero_copy_listener` |
| Pull-style receive helpers | `receive_owned` default adapter over `register_owned_listener` where appropriate |

## Builder Entry Points

The native `UMessageBuilder` keeps the common builder ergonomics but returns `UOwnedFrame`:

| Builder | Required fields |
| --- | --- |
| `publish(topic)` | Source topic, no sink |
| `notification(origin, destination)` | Source and sink |
| `request(method_to_invoke, reply_to_address, ttl)` | Method sink, reply-to source, non-zero TTL |
| `response(reply_to_address, request_id, invoked_method)` | Reply-to sink, request ID, invoked method source |
| `response_for_request(request_attributes)` | Request attributes from the incoming request |

Payload helpers set `UEncoding` and payload bytes in one step:

| Helper | Use case |
| --- | --- |
| `build()` | Empty raw payload |
| `build_with_raw_payload(...)` | Already-encoded opaque bytes |
| `build_with_payload(encoding, payload)` | Explicit `UEncoding` and bytes |
| `build_with_serializable::<Wire, _>(...)` | Serializer-neutral typed payload |
| `build_with_protobuf_payload(...)` | Protobuf payload, only with `protobuf-wire` |

## Building Common Message Types

Publish:

```rust
use up_rust::{UMessageBuilder, UUri};

# fn build() -> Result<(), Box<dyn std::error::Error>> {
let topic = UUri::try_from("//vehicle/4210/1/B24D")?;
let frame = UMessageBuilder::publish(topic).build_with_raw_payload(b"reading".to_vec())?;
# let _ = frame;
# Ok(())
# }
```

Notification:

```rust
use up_rust::{UMessageBuilder, UUri};

# fn build() -> Result<(), Box<dyn std::error::Error>> {
let origin = UUri::try_from("//vehicle/4210/1/8001")?;
let destination = UUri::try_from("//backend/8000/1/0001")?;
let frame = UMessageBuilder::notification(origin, destination).build()?;
# let _ = frame;
# Ok(())
# }
```

Request:

```rust
use up_rust::{UMessageBuilder, UUri};

# fn build() -> Result<(), Box<dyn std::error::Error>> {
let method = UUri::try_from("//vehicle/4210/1/0001")?;
let reply_to = UUri::try_from("//backend/8000/1/0001")?;
let frame = UMessageBuilder::request(method, reply_to, 1_000).build()?;
# let _ = frame;
# Ok(())
# }
```

Response from request attributes:

```rust
use up_rust::{UAttributes, UMessageBuilder};

# fn build(request_attributes: &UAttributes) -> Result<(), Box<dyn std::error::Error>> {
let frame = UMessageBuilder::response_for_request(request_attributes).build()?;
# let _ = frame;
# Ok(())
# }
```

## Custom Wire Formats

Use `WireFormat`, `USerializer`, and `UDeserializer` for non-Protobuf payloads. The same serializer contract works for owned buffers and zero-copy transmit loans.

```rust
use up_rust::{UDeserializer, UEncoding, UMessageBuilder, USerializer, UUri, UWireError, WireFormat};

struct TemperatureWire;

struct Temperature {
    celsius: i16,
}

impl WireFormat for TemperatureWire {
    fn name() -> &'static str {
        "temperature-v1"
    }

    fn encoding() -> UEncoding {
        UEncoding::new("temperature-v1", "application/x.temperature", None::<String>)
    }
}

impl USerializer<TemperatureWire> for Temperature {
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

impl<'a> UDeserializer<'a, TemperatureWire> for Temperature {
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
let frame = UMessageBuilder::publish(topic)
    .build_with_serializable::<TemperatureWire, _>(&value)?;
# let _ = frame;
# Ok(())
# }
```

Typed frame deserialization checks `UEncoding` before decoding bytes. If a frame says it contains one wire format and the caller asks for another, deserialization fails instead of handing bytes to the wrong decoder.

## Protocol Buffers Payloads

Protocol Buffers support is available behind the `protobuf-wire` feature.

```toml
[dependencies]
up-rust = { version = "0.10", features = ["protobuf-wire"] }
```

With the feature enabled, use `ProtobufWire` or the builder convenience method:

```rust
use up_rust::{UMessageBuilder, UUri};

# fn build<T>(payload: &T) -> Result<(), Box<dyn std::error::Error>>
# where
#     T: up_rust::USerializer<up_rust::ProtobufWire>,
# {
let topic = UUri::try_from("//vehicle/4210/1/B24D")?;
let frame = UMessageBuilder::publish(topic).build_with_protobuf_payload(payload)?;
# let _ = frame;
# Ok(())
# }
```

Protocol Buffers are payload bytes only in this path. They do not wrap the transport frame.

## Transport Migration

Owned transports should implement `UOwnedTransport` and accept `UOwnedFrame`:

```rust
use async_trait::async_trait;
use up_rust::{UOwnedFrame, UOwnedTransport, UStatus, UUri};

struct MyTransport;

#[async_trait]
impl UOwnedTransport for MyTransport {
    async fn send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
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

Zero-copy transports should implement `UZeroCopyTransport` only when the transport can honestly loan transmit storage and return receive leases. Network and broker transports should not fake zero-copy by copying into hidden buffers.

### Pull Receive On Push-Oriented Transports

`UOwnedTransport::receive_owned` has a default implementation that registers a temporary owned listener and returns the first matching frame. Transports that already implement `register_owned_listener` and `unregister_owned_listener` usually do not need a separate `receive_owned` queue.

This is intentionally not a zero-copy adapter. It is an owned-frame convenience for transports whose natural receive model is callback or subscription based.

### Generic Endpoint Adapters

Use `UTransportEndpoint` when routing code needs one object that can send owned frames through either an owned transport or a true zero-copy transport:

```rust
use std::sync::Arc;
use up_rust::{UOwnedTransport, UTransportEndpoint};

# fn wrap(transport: Arc<dyn UOwnedTransport>) {
let endpoint = UTransportEndpoint::from_owned(transport);
# let _ = endpoint;
# }
```

For zero-copy transports, `UTransportEndpoint::from_zero_copy` copies an owned frame payload into a transmit loan for sends and adapts zero-copy receive leases to owned listener callbacks for generic routing. It does not make a network or broker transport zero-copy; only transports that implement true loaned storage should use the zero-copy constructor.

## Validation Checklist

Use this checklist when migrating an application or transport:

1. Replace generated `UMessage` transport envelopes with `UOwnedFrame` or zero-copy frame leases.
2. Keep `UAttributes` native and project fields into transport metadata instead of serializing a generated attributes blob by default.
3. Preserve `UEncoding` across the transport boundary.
4. Use `WireFormat` serializers for typed payloads.
5. Enable `protobuf-wire` only when Protocol Buffers payload support is needed.
6. Use `send_owned` for owned transports and `send_zero_copy` only for true loaned-storage transports.
7. Prefer the default `receive_owned` adapter instead of adding duplicate pull queues to push-oriented owned transports.
8. Use `UTransportEndpoint` for generic owned/zero-copy routing boundaries.
9. Add tests for raw payloads, custom wire formats, optional protobuf payloads, and wrong-wire-format rejection.
