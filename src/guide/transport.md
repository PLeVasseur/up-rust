# The Transport Layer (up-L1)

Sometimes you want the floor itself: no role features, no protobuf, no runtime
beyond your own — just build a [`UMessage`](crate::UMessage) and hand it to the
[`UTransport`](crate::UTransport) your deployment configured.

## Publish

```rust
use up_rust::{PayloadEncoding, UMessageBuilder, UTransport, UUri};

async fn publish(transport: &dyn UTransport) -> Result<(), Box<dyn std::error::Error>> {
    let topic = UUri::try_from_parts("my-vehicle", 0x1_0001, 1, 0x8001)?;
    let message = UMessageBuilder::publish(topic)
        .build_with_payload("92.5", PayloadEncoding::TEXT)?;
    transport.send(message).await?;
    Ok(())
}
```

## Receive

```rust
use std::sync::Arc;
use up_rust::{UListener, UMessage, UTransport, UUri};

struct TempListener;

#[async_trait::async_trait]
impl UListener for TempListener {
    async fn on_receive(&self, message: UMessage) {
        // Prints: engine temp update: Some(b"92.5") for the publish above.
        println!("engine temp update: {:?}", message.payload());
    }
}

async fn subscribe(transport: &dyn UTransport) -> Result<(), Box<dyn std::error::Error>> {
    let topic = UUri::try_from_parts("my-vehicle", 0x1_0001, 1, 0x8001)?;
    transport.register_listener(&topic, None, Arc::new(TempListener)).await?;
    Ok(())
}
```

## Filters and wildcards

`register_listener` takes a **source filter** and an optional sink filter. A
filter component may be a wildcard: authority `"*"`, resource id `0xFFFF` (any
resource), or the entity-instance wildcard. For example:

```rust
# use up_rust::UUri;
// Everything entity 0x1_0001 publishes, on any topic.
let all_topics = UUri::try_from_parts("my-vehicle", 0x1_0001, 1, 0xFFFF)?;

// One exact topic.
let one_topic = UUri::try_from_parts("my-vehicle", 0x1_0001, 1, 0x8001)?;
# let _ = (all_topics, one_topic);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Exact registrations receive one topic; wildcard registrations fan in. The
matching rules are the same ones a transport implements. The
[`UTransport` tutorial](crate::guide::utransport) shows them from the provider
side.

For which trait is which, see the [trait map](crate::guide::trait_map).
