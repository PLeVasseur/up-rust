# The UTransport family

A transport moves messages over one technology: a broker, a bus, or shared
memory. Every transport implements the same contract so application code does
not depend on that technology.

A `UTransport`-family transport implements one trait,
[`UTransport`](crate::UTransport): deliver a [`UMessage`](crate::UMessage),
and keep a registry of listeners with their source/sink filters. Here is
a **complete, working transport** — an in-process loopback — as a
running example:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use up_rust::{
    verify_filter_criteria, ComparableListener, UCode, UListener, UMessage, UStatus, UTransport,
    UUri,
};

struct Registered {
    source: UUri,
    sink: Option<UUri>,
    listener: ComparableListener,
}

#[derive(Default)]
struct MiniTransport {
    listeners: RwLock<Vec<Registered>>,
}

#[async_trait::async_trait]
impl UTransport for MiniTransport {
    async fn send(&self, message: UMessage) -> Result<(), UStatus> {
        // A real transport hands the bytes to its technology here.
        // The loopback "technology" is: match filters, dispatch locally.
        let matching_listeners = {
            let source = message.attributes().source();
            let sink = message.attributes().sink();
            self.listeners
                .read()
                .await
                .iter()
                .filter(|registered| {
                    let source_ok = registered.source.matches(source);
                    let sink_ok = match (&registered.sink, sink) {
                        (Some(pattern), Some(candidate)) => pattern.matches(candidate),
                        (None, None) => true,
                        _ => false,
                    };
                    source_ok && sink_ok
                })
                .map(|registered| registered.listener.clone())
                .collect::<Vec<_>>()
        };

        // Never hold the registry lock while invoking application code.
        for listener in matching_listeners {
            listener.on_receive(message.clone()).await;
        }
        Ok(())
    }

    async fn register_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        // Reject filter combinations the spec forbids before storing anything.
        verify_filter_criteria(source_filter, sink_filter).map_err(|e| *e)?;
        let registered = Registered {
            source: source_filter.to_owned(),
            sink: sink_filter.map(ToOwned::to_owned),
            listener: ComparableListener::new(listener),
        };
        let mut listeners = self.listeners.write().await;
        if listeners.iter().any(|existing| {
            existing.source == registered.source
                && existing.sink == registered.sink
                && existing.listener == registered.listener
        }) {
            return Err(UStatus::fail_with_code(
                UCode::AlreadyExists,
                "listener already registered for filters",
            ));
        }
        listeners.push(registered);
        Ok(())
    }

    async fn unregister_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        let target = ComparableListener::new(listener);
        let mut listeners = self.listeners.write().await;
        let before = listeners.len();
        listeners.retain(|r| {
            !(r.source == *source_filter
                && r.sink.as_ref() == sink_filter
                && r.listener == target)
        });
        if listeners.len() < before {
            Ok(())
        } else {
            Err(UStatus::fail_with_code(UCode::NotFound, "no such listener"))
        }
    }
}

// Prove it works: register, send, receive.
struct Capture(tokio::sync::mpsc::UnboundedSender<UMessage>);
#[async_trait::async_trait]
impl UListener for Capture {
    async fn on_receive(&self, message: UMessage) {
        let _ = self.0.send(message);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use up_rust::{UMessageBuilder, UPayloadFormat};
    let transport = MiniTransport::default();
    let topic = UUri::try_from_parts("demo", 0x1_0001, 1, 0x8001)?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let listener: Arc<dyn UListener> = Arc::new(Capture(tx));
    transport.register_listener(&topic, None, listener.clone()).await?;
    let duplicate = transport.register_listener(&topic, None, listener).await.unwrap_err();
    assert_eq!(duplicate.get_code(), UCode::AlreadyExists);
    transport
        .send(UMessageBuilder::publish(topic).build_with_payload("42", UPayloadFormat::Text)?)
        .await?;

    let received = rx.recv().await.expect("message delivered");
    assert_eq!(received.payload().unwrap().as_ref(), b"42");
    Ok(())
}
```

The crate's own [`LocalTransport`](crate::local_transport::LocalTransport)
is this same shape grown up: a `HashSet` keyed on
[`ComparableListener`](crate::ComparableListener) so duplicate
registrations are rejected with `AlreadyExists`, and filter matching via
the message's attributes. Read its source when you outgrow the sketch.

## Payload and routing integrity

Preserve all three payload states: absent, present with zero bytes, and present
with nonempty bytes. Payload presence is not inferred from length.

If the underlying protocol or broker mirrors routing fields in native headers,
validate those fields against the message attributes before invoking a callback
or forwarding. Route from the validated source and sink, never from payload
bytes.

## Listener lifecycle and readiness

Getting registration right matters more than it looks: a listener registered
twice double-delivers, and one dropped early loses messages silently.
Registration completes only when the binding can state what readiness means for
its technology. Discovery-backed transports should expose a bounded peer or
subscription readiness result, not use duplicate application sends as a
readiness protocol. An absent peer returns a bounded, observable status.

Listener dispatch preserves the binding's documented ordering and is bounded by
an explicit queue, worker, or backpressure policy. Successful unregister is a
quiescence boundary: a callback already running may complete if the binding says
so, but no later callback may begin. Cancellation wakes blocking receive or poll
operations, shutdown joins workers, and native entities are deleted
deterministically so a transport can be recreated in the same domain.

Polling is an acceptable native fallback when its wait is wakeable or bounded,
each iteration bounds take and dispatch work, failures become health or status
signals, and dropping the transport cannot leave an unjoined worker.

What a real technology changes: `send` maps the message attributes and payload
to that binding's wire representation and hands it to the broker or bus. The
receive side runs the inverse mapping and rebuilds a `UMessage` before
dispatching to listeners.

That is the whole level. Error mapping: use
[`UCode`](crate::UCode) faithfully — `AlreadyExists` for duplicate
registration, `NotFound` for unknown listeners, `InvalidArgument` for
filters your technology cannot express, `Unavailable` when the link is
down. Applications and the roles built above you switch on these codes.

## Definition of done

"It compiles and my demo works" is not enough for a transport. Its tests should
cover payload presence, exact and wildcard routing, duplicate registration,
unregister quiescence, bounded failure, reconnect, and deterministic shutdown.

[the guide hub](crate::guide).
