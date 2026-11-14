# Implementing a transport

A transport carries messages using a broker, bus, shared memory, or another
technology. Choose which trait to implement first.

## Which trait do I implement?

| If you are writing | Implement | The application roles reach it through | Notes |
| --- | --- | --- | --- |
| A binding for the classic message API, or you already have one | [`UTransport`](crate::UTransport) | `ClassicCarriage::new(transport)` | Sending roles need nothing more; receive roles also need `attach_listener`. See the [`UTransport` guide](crate::guide::utransport). |
| A new serialized binding (recommended) | `serialized_dispatch::StaticOwnedTransport` | `InlineOwnedTransport::new(binding)` | Plain `async fn` methods, no `#[async_trait]`; each send future is stored inline instead of boxed. |
| An existing `#[async_trait]` owned binding | `UOwnedTransportImpl` | The implementation itself, as `Arc<dyn UOwnedTransport>` | Compatibility path: send stays boxed unless you override `dispatch_owned`. |
| A binding that carries encoded frames | `serialized_dispatch::StaticOwnedCore` (recommended) or `UOwnedTransportCore` (compatibility) | `InlineOwnedCore`, then `OwnedWireTransport` or `UWireTransport` | The adapter owns metadata encoding, envelope identity and frame validation; you move opaque bytes. |
| Native shared-memory middleware | `PreparedTransportCore`, plus `NativeLoanCore` for application roles | `native::Deployment` or `UWireTransport::with_native_agreement` | Real TX loans and retained RX; see [native loan mechanics](crate::guide::zero_copy). |

Enable the single `transport-implementer` feature for a binding: it turns on the
implementer contracts (`transport-implementer-api`), the owned-frame and native-loan
families and the selected-wire adapters. Compose the parts only if you need less.
A binding whose middleware owns RPC correlation also implements `owned_rpc::OwnedRpc`
and returns it from `owned_rpc`; the default `None` keeps the SDK's correlation.
Envelope authors implement `UFrameEnvelope`, independently of any domain mapping.

The SDK owns semantic frame validation, typed decoding, subscription gating, bounded
RPC correlation and cancellation. Do not reimplement publisher, subscriber or RPC
engines inside a transport; implement send, scoped attach and payload admission.

The send future is stored inline and its size bound is checked when the generic is
instantiated during code generation, so `cargo check` can pass while `cargo build`
fails with `future exceeds inline dispatch capacity`. Build your binding in CI rather
than only checking it, and keep large send state out of the future. The limits are
`inline_dispatch::SEND_FUTURE_BYTES` (`4096`) and `CORE_FUTURE_BYTES` (`2048`).

## Admit payload contracts at setup

Every role asks the transport to admit its payload identity and family before the
role does anything else. `admit_payload_contract` is the general hook; returning
`Ok` for every fixed encoding is fine for a payload-agnostic binding, and the default
refuses so an unconfigured transport fails closed. One-way carriage (publish, notify
and their receivers) calls `admit_carriage_payload_contract`, which defaults to
`admit_payload_contract`. A binding whose one-way carriage assumes a single payload
encoding overrides it to refuse a mismatch at setup rather than on the first send; RPC
roles keep the general hook because they carry payload identity per request.

## Serve receive roles

Publishers and notifiers only need `send_validated_owned`. Every receive role,
including RPC clients waiting for responses, attaches a listener through
`attach_validated_owned_listener` and keeps the returned `OwnedAttachment`. This
small bus serves all of them, with `InlineOwnedTransport` as the hand-off:

```rust
# #[cfg(all(feature = "owned-frame-transport", feature = "transport-implementer-api"))]
# mod example {
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use up_rust::{
    serialized_dispatch::StaticOwnedTransport, OwnedAttachment, PayloadIdentity, UCode,
    UOwnedFrame, UOwnedListener, UStatus, UUri, UWireError, WireIdentity,
};

type Registration = (u64, UUri, Option<UUri>, Arc<dyn UOwnedListener>);

/// A minimal in-process bus; a real binding maps this onto its middleware.
#[derive(Default)]
pub struct Bus {
    next_id: AtomicU64,
    listeners: Arc<Mutex<Vec<Registration>>>,
}

fn poisoned() -> UStatus {
    UStatus::fail_with_code(UCode::Internal, "listener table poisoned")
}

impl StaticOwnedTransport for Bus {
    fn admit_payload_contract(
        &self,
        identity: PayloadIdentity,
        _family: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        if matches!(identity, PayloadIdentity::Fixed(_)) {
            Ok(())
        } else {
            Err(UWireError::invalid_payload("this bus has no native storage"))
        }
    }

    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        let matching: Vec<_> = {
            let metadata = frame.metadata();
            self.listeners
                .lock()
                .map_err(|_| poisoned())?
                .iter()
                .filter(|(_, source, sink, _)| {
                    source.matches(metadata.source())
                        && match (sink, metadata.sink()) {
                            (Some(filter), Some(actual)) => filter.matches(actual),
                            (None, None) => true,
                            _ => false,
                        }
                })
                .map(|(.., listener)| listener.clone())
                .collect()
        };
        for listener in matching {
            listener.on_receive_owned(frame.clone()).await;
        }
        Ok(())
    }

    async fn attach_validated_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<OwnedAttachment, UStatus> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let listeners = self.listeners.clone();
        listeners
            .lock()
            .map_err(|_| poisoned())?
            .push((id, source.clone(), sink.cloned(), listener));
        // The release hook removes exactly this registration, synchronously.
        Ok(OwnedAttachment::new(move || {
            listeners
                .lock()
                .map_err(|_| poisoned())?
                .retain(|registration| registration.0 != id);
            Ok(())
        }))
    }
}
# }
# fn main() {}
```

The application then builds `Arc::new(InlineOwnedTransport::new(Bus::default()))` and
passes it to `Deployment::with_defaults`. The contract for your real binding:

* A failed or cancelled attach leaves no registration. There is no `.await` between
  registering and returning here, so cancellation cannot leak one; a binding that
  awaits its middleware must roll back if cancelled.
* The release hook runs synchronously and must not block on an async runtime. It may
  be retried, so it must be idempotent; return an error to keep it retryable.
* Filtering by the exact source and sink is yours, as above; the SDK filters decoded
  frames again before business handlers run.

A binding that cannot release a listener synchronously refuses
`attach_validated_owned_listener` and leaves the default in place, rather than returning
a no-op handle.

Whatever the family, the shared obligations below apply.

## Declare supported transport families

A transport declares `UTransport` and owned-frame support independently, and
downstream routers and tests dispatch on those declarations. Declare only what
the implementation supports. Unsupported families fail explicitly rather
than hanging or degrading to a different family.

## Payload and routing integrity

For `UTransport`-family and owned carriage, preserve three payload
states: absent, present with zero bytes, and present with nonempty
bytes. Presence is not inferred from length.

Encoded metadata and payload regions are opaque to the encoded-core
`UOwnedTransportCore` (available with `transport-implementer-api` and
`owned-frame-transport`): a core moves those bytes and never interprets
them. If the underlying protocol or broker mirrors routing fields in
its own native headers, classify those fields explicitly. Required mirrors must
agree with decoded metadata. Documented untrusted routing hints may select
physical queues or candidate listeners, but cannot override decoded source/sink
metadata or satisfy the public listener filter on their own. The shared wire
adapter performs metadata validation and final filtering before public delivery.
Never derive routing or wire compatibility from application payload bytes.

## Listener lifecycle and readiness

Received leases and transmit loans own their backing lifetime. Native mappings,
proxies, subscriptions or allocators required by an outstanding object must remain
alive until that object is released, even if a listener is unregistered or its
transport handle is dropped. Unregister stops future delivery; it does not revoke
frames already handed to the application. Verify this with real backend tests that
retain metadata/payload access and original addresses across teardown, including
uninitialized TX loans. A heap-backed fixture alone cannot prove native ownership.

Registration and removal use the complete filter and listener identity.
The loopback example in the UTransport tutorial includes a listener registry.

Registration completes only when the binding can state what readiness
means for that technology. Discovery-backed transports should expose a
bounded peer or subscription readiness result, not use duplicate
application sends as a readiness protocol. An absent peer returns a
bounded, observable status.

Local registration and remote discovery are different milestones. A binding which
accepts registration before a peer exists must document that state and how the
first sample is retained or how bounded data-ready discovery is observed. A success
marker in an application must not claim a stronger state than the binding provides.

Listener dispatch preserves the binding's documented ordering and is
bounded by an explicit queue, worker, or backpressure policy.
Successful unregister closes admission: a callback already entered may complete
if the binding says so, but no later callback may begin. Bounded quiescence is
an additional completion contract. Cancellation wakes blocking receive/poll operations, workers
expose health, shutdown joins them, and native entities are deleted
deterministically so a transport can be recreated in the same domain.

The `transport-implementer-api` feature, also implied by `util`, provides
`ListenerAdmission` for bindings whose policy
allows an entered callback to finish. It closes admission at the actual first
poll, supports self-unregister, and leaves queueing and worker teardown with the
binding. It is not a substitute for retaining native loan owners.
The admission boundary is not a callback-draining guarantee: `stop` can return
while a previously entered callback is pending. A binding that promises completed
callbacks or joined workers must provide that additional teardown policy.

Polling is an acceptable native fallback when its wait is wakeable or
bounded, each iteration bounds take and dispatch work, failures become
health/status signals, and drop cannot leave an unjoined worker.

## TTL when crossing frame and classic boundaries

Frame metadata stores an optional `Duration` in nanoseconds. Rust `UAttributes`
uses optional whole milliseconds, while protobuf's TTL field is a scalar `u32`.
The SDK performs these explicit projections:

| Frame TTL | Rust attribute TTL | Protobuf TTL | Reverse frame projection |
| --- | --- | --- | --- |
| Absent | `None` | `0` | Absent |
| Present `Duration::ZERO` | `None` | `0` | Absent (canonical no-expiry form) |
| Positive exact milliseconds within `u32` | `Some(milliseconds)` | That positive value | The same duration |
| Positive fractional milliseconds or overflow | Projection error | No successful projection | Not applicable |

Absent and zero both mean no expiration for messages that permit it. This is
many-to-one canonicalization: explicit native zero-presence does not survive a
classic/protobuf round trip. Requests require a positive TTL; zero/absence does
not make a request non-expiring. Native field-block decode retains explicit zero
before any classic projection, as pinned by `wire_metadata_golden`.

Bindings must preserve original producer lifetime where required by their mapping;
remaining middleware lifetime is a separate quantity. Binding-specific expiry
representations and rounding rules belong in the binding's documentation.

## Validation

Run conformance checks for each supported transport family. This crate tests
owned-frame lifecycle, validation, filtering and selected-wire identity behavior.
Backend tests must verify native ownership and readiness. Integration tests check
cross-transport routing against the declared capabilities.

The [trait map](crate::guide::trait_map) lists the contract each owner holds.
