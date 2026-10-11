# Owned-frame communication

The owned-frame family (`owned-frame-transport`) exchanges validated metadata
and payload storage as owned values. Selected-wire adapters can apply external
or deployment-specific formats above an encoded transport core.
This is useful when a transport supplies complete owned buffers or queue
entries. The [wire chapter](crate::guide::wires) explains the codecs
and metadata layouts used by those adapters.

## Choose the API

For application roles, enable `owned-frame-transport` and `usubscription` (or
`communication`, which includes subscription support). `communication::owned::Endpoint`
constructs Publisher, Subscriber, Notifier, RpcClient and RpcServer facades.
The subscriber also requires a uSubscription service. Notification receive uses
the notifier's listening API and ordinary `UListener`. Add `test-util` only for
the public in-memory test transport.

The README provisional-interface policy applies to frame-family APIs.

## Implement owned carriage

You implement `UOwnedTransportImpl`; users call `UOwnedTransport` and
receive through `UOwnedListener`. These interfaces use frames where `UTransport`
uses messages. `UOwnedFrame` defaults to `UOwnedFrame<Validated>`, the state
accepted by the public transport methods.

```rust,no_run
use up_rust::{UOwnedFrame, UOwnedTransportImpl, UStatus};

struct MyTransport { /* connection handle */ }
# struct Backend; impl Backend { fn publish(&self, _m: &up_rust::UFrameMetadata, _p: Option<&bytes::Bytes>) -> Result<(), UStatus> { Ok(()) } }
# impl MyTransport { fn backend(&self) -> Backend { Backend } }

#[async_trait::async_trait]
impl UOwnedTransportImpl for MyTransport {
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        // `frame` is `UOwnedFrame<Validated>` by type: metadata checked,
        // payload presence is already consistent.
        self.backend().publish(frame.metadata(), frame.payload())
    }
    // register/unregister owned listeners: same registry pattern as the
    // UTransport tutorial.
}
```

## Construct validated frames

Frame constructors validate metadata and payload presence before returning:

```rust
use up_rust::{PayloadEncoding, UMessageBuilder, UOwnedFrame, UUri};

let topic = UUri::try_from_parts("demo", 0x1_0001, 1, 0x8001)?;
let message = UMessageBuilder::publish(topic).build()?;

// Frame metadata is a projection of the same attributes a UMessage carries:
let metadata = message.attributes().to_frame_metadata(PayloadEncoding::RAW)?;
let frame = UOwnedFrame::with_payload(metadata, b"reading".to_vec())?;

assert_eq!(frame.payload_bytes(), b"reading");
# Ok::<(), Box<dyn std::error::Error>>(())
```

Raw receive code can construct `UOwnedFrame<`[`Unvalidated`](crate::Unvalidated)`>`.
[`UOwnedFrame::validate`](crate::UOwnedFrame::validate) checks it and either returns
an error or produces `UOwnedFrame<Validated>`. The public listener signature
prevents passing an unvalidated value directly to the callback.

Choose owned frames when your technology hands you complete buffers and you
want wire pluggability plus validation at the public boundary.

For transport code that handles encoded metadata and payload bytes, implement
`UOwnedTransportCore`. `UWireTransport` (from `transport-implementer-api`) applies
the selected codec above it, allowing the same core to support multiple wires.

See the transport implementation guide for shared requirements and
[the trait map](crate::guide::trait_map) for the interfaces.

## Send and receive a notification

This runnable example uses the explicit test utility and a Tokio runtime. A
production endpoint supplies its transport and URI provider in the same way.

```rust
# #[cfg(all(feature = "owned-frame-transport", feature = "usubscription", feature = "test-util"))]
# {
use std::sync::{Arc, Mutex};
use up_rust::{InMemoryOwnedTransport, LocalUriProvider, PayloadEncoding,
    StaticUriProvider, UListener, UMessage};
use up_rust::communication::{CallOptions, UPayload};
use up_rust::communication::owned::Endpoint;

#[derive(Default)]
struct Capture(Mutex<Vec<UMessage>>);
#[async_trait::async_trait]
impl UListener for Capture {
    async fn on_receive(&self, message: UMessage) {
        self.0.lock().unwrap().push(message);
    }
}

tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
    let transport = Arc::new(InMemoryOwnedTransport::default());
    let provider = Arc::new(StaticUriProvider::new("vehicle", 0x1234, 1)?);
    let endpoint = Endpoint::new(transport, provider.clone());
    let notifier = endpoint.notifier();
    let source = provider.get_resource_uri(0x8001);
    let listener = Arc::new(Capture::default());
    notifier.start_listening(&source, listener.clone()).await?;
    notifier.notify(0x8001, &provider.get_source_uri(),
        CallOptions::for_notification(None, None, None),
        Some(UPayload::new(b"ready".as_ref(), PayloadEncoding::TEXT))).await?;
    assert_eq!(listener.0.lock().unwrap().first().unwrap().payload().as_deref(), Some(b"ready".as_ref()));
    notifier.stop_listening(&source, listener).await?;
    Ok::<(), Box<dyn std::error::Error>>(())
})?;
# }
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Publish, subscribe and RPC

`Endpoint::publisher()` publishes topic messages. `Endpoint::subscriber(service)`
performs the subscription service interaction and registers the supplied listener.
`Endpoint::rpc_client()` invokes methods; `Endpoint::rpc_server()` registers a
`RequestHandler`. These facades implement the corresponding classic communication
traits while their underlying transport exchanges validated owned frames.

Owned RPC request listeners keep a weak reference to their reply transport, so a
transport retaining listeners cannot form a strong ownership cycle. Each entered
request temporarily retains a live transport across the handler await and response
send. A callback arriving after transport destruction returns without invoking the
application handler. Explicitly unregister endpoints to remove them while the
transport is still live; dropping the server is not asynchronous unregistration.

## Errors and teardown

Invalid topic/filter and message data return the corresponding typed role errors;
transport failures retain their `UStatus`. Classic-handler projection can fail when
metadata cannot be represented faithfully. Unregister closes future delivery under
the transport's admission contract and never invalidates an already received frame.
