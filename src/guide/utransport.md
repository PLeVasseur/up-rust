# Implement UTransport

[Implementing a transport](crate::guide::transports) says which trait to implement;
this page covers the classic message-based trait.

[`UTransport`](crate::UTransport) adapts [`UMessage`](crate::UMessage) values
and [`UUri`](crate::UUri) filters to a messaging technology. Implement
[`UTransport::send`](crate::UTransport::send) and at least one receive mode:
push through listener registration, pull through
[`UTransport::receive`](crate::UTransport::receive), or both. The default
receive and listener methods return
[`UCode::Unimplemented`](crate::UCode::Unimplemented).

## Start with three contracts

| Source | What it defines |
| --- | --- |
| [`UTransport`](crate::UTransport) | Rust method signatures and default behavior |
| [L1 Transport Layer specification](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l1/README.adoc) | Observable operation, validation, authorization, delivery, and error requirements |
| [Zenoh](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l1/zenoh.adoc), [MQTT 5](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l1/mqtt_5.adoc), or [SOME/IP](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l1/someip.adoc) binding | Mapping between uProtocol messages and the selected protocol |

Use all three when implementing and testing a transport. This guide explains
how they fit together; it does not replace the complete L1 specification or the
selected binding.

## See the adapter boundary

A transport owns two directional paths:

```text
outbound: validated UMessage -> encode for binding -> native send path

inbound:  native PDU -> decode and validate for binding -> UMessage
                                                           |-> matching push listeners
                                                           `-> pull receive queue
```

The binding defines the native representation. L1 defines what callers can
observe at the `UTransport` methods.

## Implement outbound send

[`UTransport::send`](crate::UTransport::send) takes one complete `UMessage`.
The implementation should:

1. preserve all attributes and the payload while encoding the selected
   binding;
2. perform the protocol-specific send operation; and
3. convert failures into an appropriate [`UStatus`](crate::UStatus).

Keep attribute validation at the boundary that constructs a `UMessage`.
[`UMessageBuilder`](crate::UMessageBuilder) validates attributes before it
returns a message. Decoders and custom type mappings should likewise reject an
invalid representation instead of producing an invalid `UMessage`. A transport
therefore does not normally need to repeat the same validation in `send`.

The pinned L1 specification still requires
[`UCode::InvalidArgument`](crate::UCode::InvalidArgument) if an invalid message
does reach `send`. Treat that as a fallback for integrations that do not yet
enforce validation when constructing a `UMessage`, not as a reason to revalidate
values from validated constructors.

Successful `send` completion is not a transport-independent delivery receipt.
For protocol-backed transports, L1 says the message has been handed to the
underlying protocol's send path, but the observable checkpoint depends on the
implementation and protocol. For example, one transport might wait for a broker
acknowledgement while an in-process transport might complete after local
dispatch. Document that checkpoint without claiming that a recipient received
or processed the message.

## Build one inbound path

Decode each native protocol data unit according to the selected binding, rebuild
the `UMessage`, and validate its attributes before exposing it through push or
pull. Preserve the message metadata and payload during decoding.

For push, L1 requires invalid inbound data to be discarded. For pull, an
inbound protocol data unit that cannot produce a valid matching `UMessage` is
reported as [`UCode::NotFound`](crate::UCode::NotFound). Keep decoding and
validation shared so both receive modes interpret the binding consistently.

## Exact URI arguments

Use [`ExactUUri`](crate::ExactUUri) when an API accepts one resource rather than
a wildcard filter. Construction checks every authority, entity, version and
resource component. Borrow it with `as_uuri`, or recover the original value with
`into_inner`. This check says nothing about a binding's physical filtering or
payload-copy behavior; ordinary [`UUri`](crate::UUri) filters remain available.

```rust
use up_rust::{ExactUUri, UUri, UUriError};

let uri = UUri::try_from_parts("vehicle", 0x0001_2345, 1, 0x8001)?;
let exact = ExactUUri::try_from(&uri)?;
assert_eq!(exact.as_uuri(), &uri);
assert_eq!(exact.into_inner(), uri);
assert!(matches!(ExactUUri::try_from(UUri::any()), Err(UUriError::ValidationError(_))));
# Ok::<(), UUriError>(())
```

## Implement push delivery

Push delivery uses
[`UTransport::register_listener`](crate::UTransport::register_listener) and
[`UTransport::unregister_listener`](crate::UTransport::unregister_listener).
Both methods identify a registration with the source filter, optional sink
filter, and listener identity:

```rust
use std::sync::Arc;
use up_rust::{UListener, UUri};

struct Registration {
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: Arc<dyn UListener>,
}
```

Validate a filter pair before changing registration state. The SDK helper
implements the shared resource-ID checks from L1:

```rust
use up_rust::{verify_filter_criteria, UStatus, UUri};

fn validate_filters(
    source_filter: &UUri,
    sink_filter: Option<&UUri>,
) -> Result<(), UStatus> {
    verify_filter_criteria(source_filter, sink_filter)
        .map_err(|status| *status)
}
```

A push implementation must make these lifecycle points observable:

* Repeating registration with the same filters and listener has the same effect
  as registering once.
* Registration does not succeed until messages received afterward can reach the
  listener.
* Every valid matching message received after successful registration invokes
  each matching listener at least once.
* Successful unregistration is a barrier: the removed registration receives no
  later messages.
* Configured total and per-filter listener limits fail registration with
  [`UCode::ResourceExhausted`](crate::UCode::ResourceExhausted).

Use listener identity, not merely its concrete type, when comparing
registrations. [`ComparableListener`](crate::ComparableListener) is available
to implementations that need hashable and comparable `Arc<dyn UListener>`
values.

### Scoped attachment for application roles

With `owned-frame-transport`, the application roles reach a `UTransport` through
`ClassicCarriage`. Their receive roles, including RPC clients waiting for
responses, call `UTransport::attach_listener`, which returns an `OwnedAttachment`.
Its synchronous release must remove exactly that registration and reclaim its
local delivery capacity, and a failed or cancelled attachment must leave no
registration behind. The default implementation refuses, so receive-role setup
fails explicitly; sending roles need only `send`.

### Admission and native quiescence

With `transport-implementer-api`, `ListenerAdmission` supplies a gate per
registration; `util` also enables that surface.
`stop().await` is the classic first-poll admission barrier, not a drain: an entered
callback may still be pending when it returns. Transport queue limits and retained
payload storage are separate from this gate.

Native bindings that promise bounded quiescence close admission with `cancel()`,
derive their deadline from `closed_at()`, and wait with `quiesce(deadline)` unless
`is_current_callback()` identifies the current/enclosing callback. Their README
and configuration must name the chosen timeout interval. Timeout returns
`ListenerQuiesceError::QuiesceTimedOut`; admission stays closed and live callbacks
and loans retain their owners. A timeout does not permit storage reclamation.

```rust,no_run
# #[cfg(feature = "transport-implementer-api")]
# async fn example(gate: &up_rust::ListenerAdmission) -> Result<(), up_rust::ListenerQuiesceError> {
use std::time::Duration;
gate.cancel();
let deadline = gate.closed_at().expect("closed") + Duration::from_secs(5);
if !gate.is_current_callback() {
    gate.quiesce(deadline).await?;
}
# Ok(())
# }
```

The entered counter is per registration and its guard releases on completion,
cancellation or panic. Async dispatch scopes task-local context through the future's
polls, including thread migration. Synchronous native callbacks use
`enter_native_callback()` and keep its thread-bound guard through the callback.
Native admission never blocks or waits, so it may run on any callback thread,
including one that drives an async runtime. An attempt racing with `cancel()` is
either counted for `quiesce` or rejected. Up to eight live guards per thread need
no allocation.

```rust
# #[cfg(feature = "transport-implementer-api")]
# {
use up_rust::ListenerAdmission;

// Invoked on whichever thread the middleware chooses.
fn on_native_sample(gate: &ListenerAdmission) -> bool {
    let Some(_entered) = gate.enter_native_callback() else {
        return false; // closed: release the sample without delivery
    };
    true // deliver while the guard is live
}

let gate = ListenerAdmission::new();
assert!(on_native_sample(&gate));
gate.cancel();
assert!(!on_native_sample(&gate));
# }
```

An unrelated registration or spawned task does not become re-entrant because it
shares a thread. Arbitrary child tasks do not inherit a parent callback's context;
applications must avoid cleanup dependency cycles across separately spawned work.
The deadline covers the racing admission barrier and entered work. As with any
async timeout, user callbacks must not block all executor threads.

## Implement pull delivery

[`UTransport::receive`](crate::UTransport::receive) selects by the same source
and optional sink filter shape used for push. If multiple unexpired messages
match, return the oldest one. Return
[`UCode::NotFound`](crate::UCode::NotFound) when no valid matching message is
available, and [`UCode::Unimplemented`](crate::UCode::Unimplemented) when the
transport does not support pull.

The implementation decides how to buffer native messages, but it must preserve
the L1 selection and expiry behavior at the method boundary.

## Map errors by operation

### Selected-wire cores and adapters

With the corresponding frame-family features, a transport can implement
`UOwnedTransportCore` and carry encoded metadata without choosing a wire. The
`UWireTransport` adapter combines that core with the selected `UWire` and
`UWireMetadataCodec`. The metadata codec owns its layout; the binding owns physical
storage and mirror-field validation. Pull failures map to a typed `UStatus`, while
invalid listener input is discarded before the application callback.

New owned implementations can use plain `async fn` methods instead of
`#[async_trait]`: implement `serialized_dispatch::StaticOwnedCore` for encoded
frames, or `StaticOwnedTransport` for validated semantic frames, and wrap it in
`InlineOwnedCore` or `InlineOwnedTransport`. The SDK then stores each send future
inline rather than boxing it. Setup and receive futures stay boxed at the
object-safe boundary, and existing `#[async_trait]` implementations keep their
boxed send. A semantic-frame binding whose middleware owns request/response
correlation returns it from `UOwnedTransportImpl::owned_rpc` (or
`StaticOwnedTransport::owned_rpc`); the default `None` keeps the SDK's bounded
correlation table.

```rust
# #[cfg(all(feature = "transport-implementer-api", feature = "wire-implementer-api", feature = "owned-frame-transport"))]
# {
use up_rust::{UFrameFieldsMetadataCodec, UOwnedTransportCore, UProtocolNativeWire, UWireTransport};

fn wrap<C: UOwnedTransportCore>(core: C)
    -> UWireTransport<C, UProtocolNativeWire, UFrameFieldsMetadataCodec>
{
    UWireTransport::new(core, UProtocolNativeWire, UFrameFieldsMetadataCodec)
}
# }
```

Ordinary constructors reject wire markers that require a native agreement. A
serialized codec does not acquire a native requirement merely by using this adapter.

### Operation results

These notable L1 outcomes are not an exhaustive list of native failures:

| Method | Required outcome |
| --- | --- |
| `send` | `InvalidArgument` if an invalid message reaches the method; `PermissionDenied` is recommended when unauthorized sending can be determined locally |
| `receive` | `Unimplemented` when pull is unsupported; `NotFound` when no valid matching message is available |
| `register_listener` | `Unimplemented` when push is unsupported; `InvalidArgument` for invalid filters; `ResourceExhausted` at a configured listener limit; `PermissionDenied` is recommended when unauthorized consumption can be determined locally |
| `unregister_listener` | `Unimplemented` when push is unsupported; `InvalidArgument` for invalid filters; `NotFound` when the registration does not exist |

Map other native failures to the closest `UCode`, retain useful diagnostic
detail in `UStatus`, and document any transport-specific outcomes.

## Document transport-specific behavior

L1 and the selected binding remain the conformance authority. A concrete
transport should additionally document behavior that its technology determines:

* **send completion:** which native event allows `send` to return success;
* **reconnect:** whether registrations and buffered messages survive a lost
  connection;
* **ordering:** which messages, if any, are observed in a stable order;
* **backpressure:** whether senders wait, fail, or drop when capacity is
  exhausted; and
* **shutdown:** how pending sends, receives, and listener callbacks finish.

Do not turn those transport-specific choices into general `UTransport`
guarantees.

## Existing implementations

For a minimal in-process push implementation, see
[`LocalTransport`](https://docs.rs/up-rust/latest/up_rust/local_transport/struct.LocalTransport.html).
For protocol-backed designs, the published
[`up-transport-zenoh`](https://crates.io/crates/up-transport-zenoh),
[`up-transport-mqtt5`](https://crates.io/crates/up-transport-mqtt5), and
[`up-transport-vsomeip`](https://crates.io/crates/up-transport-vsomeip) crates
show how concrete technologies implement the interface. Use them as design
references; use L1 and the selected binding as the conformance authority.

Return to the [guide](crate::guide).
