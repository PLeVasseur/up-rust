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

## Type Mapping

| Old concept | Native replacement |
| --- | --- |
| Generated `UMessage` transport envelope | `UOwnedFrame` or zero-copy frame lease |
| Generated `UAttributes` payload metadata | Native `UAttributes` |
| Generated payload-format enum | `UEncoding` |
| Generated `UMessageBuilder` | Native `UMessageBuilder` that builds `UOwnedFrame` |
| `UTransport::send` | `UOwnedTransport::send_owned` or `UZeroCopyTransport::send_zero_copy` |
| `UTransport::register_listener` | `register_owned_listener` or `register_zero_copy_listener` |

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
        let attributes = frame.header().attributes();
        let encoding = frame.header().encoding();
        let payload = frame.payload_bytes();

        // Project attributes and encoding into the transport-native metadata channel.
        // Send payload exactly as serializer-produced bytes.
        let _ = (attributes, encoding, payload);
        Ok(())
    }
}
```

Zero-copy transports should implement `UZeroCopyTransport` only when the transport can honestly loan transmit storage and return receive leases. Network and broker transports should not fake zero-copy by copying into hidden buffers.

## Validation Checklist

Use this checklist when migrating an application or transport:

1. Replace generated `UMessage` transport envelopes with `UOwnedFrame` or zero-copy frame leases.
2. Keep `UAttributes` native and project fields into transport metadata instead of serializing a generated attributes blob by default.
3. Preserve `UEncoding` across the transport boundary.
4. Use `WireFormat` serializers for typed payloads.
5. Enable `protobuf-wire` only when Protocol Buffers payload support is needed.
6. Use `send_owned` for owned transports and `send_zero_copy` only for true loaned-storage transports.
7. Add tests for raw payloads, custom wire formats, optional protobuf payloads, and wrong-wire-format rejection.
