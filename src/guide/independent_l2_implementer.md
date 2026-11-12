# Reusable L2 implementation author

`PublicationProtocol<P>` in `examples/support/independent_layers.rs` is an actual
small event-publishing protocol over L1, not a business `Handler` implementation.
It admits an explicit schema and valid event source at setup, stores a domain-only
handle, creates publication metadata/identity for each send, encodes, and invokes
`UOwnedTransport::send_owned`. It constructs no subscriber or notification/RPC role.

```rust,ignore
let protocol = PublicationProtocol::<Reading>::for_schema::<ReadingProtobuf>(
    carriage, local_address.as_ref(), &schema::TEMPERATURE,
)?;
protocol.publish(&Reading { value: 21 }).await?;
```

This generic schema selection belongs to the reusable protocol/schema author;
ordinary SDK applications instead use `schema::bind` and `Event::publisher`.
The educational protocol supports default event publication only, not full SDK
options, subscriptions or RPC. It is not a replacement engine to integrate.

For reusable subscriptions/RPC, compose SDK registrations, exact attachments,
route validation, deadlines and retained cleanup owners. Do not reimplement those
in a business handler. A business `RequestHandler` computes a decision; it does not
implement L2 request/response correlation.

For reusable RPC, `up_rust::correlation` publishes the SDK's bounded table:
`PendingCalls::new(capacity)`, `reserve(&id)` before sending, `deliver(&id, frame)`
after checking exact source, sink, kind and identity, and the consuming
`PendingGuard::wait().await`:

```rust
use up_rust::correlation::PendingCalls;
use up_rust::UUID;

# #[tokio::main(flavor = "current_thread")]
# async fn main() -> Result<(), Box<dyn std::error::Error>> {
let pending = PendingCalls::new(8)?;
let id = UUID::build();
let guard = pending.reserve(&id)?; // before sending the request
// ... send it, and when the response arrives check its source, sink, kind and identity ...
assert!(pending.deliver(&id, "response frame"));
assert_eq!(guard.wait().await, Ok("response frame"));
# Ok(())
# }
```

Dropping a guard cancels its reservation, so a late reply is rejected, and `close`
wakes every waiter with `PendingClosed`. Correlation is not a whole RPC protocol:
deadlines (wrap `wait` in `tokio::time::timeout`), route validation, status mapping
and retained cleanup remain yours. `role_set!`'s `ManagedRole` trait lets your owners
join the same cleanup model.

`ReusablePublisher<Core>` in `independent_native.rs` is the separate static-native
library case: its core determines route storage, TX loan lifetime, RX owner and
attachment type. One core parameter is deliberate monomorphization, not ordinary
application plumbing. `native_roles!` generates the selected concrete names and
retained response association for applications. No virtual native TX, boxed
per-message callback, heap-staged payload or unsafe lifetime erasure is introduced.

Costs: the educational owned protocol pays owned encoding allocations, and whether
its `send_owned` call is boxed depends on the carriage: `#[async_trait]`
implementations box each send, while the `serialized_dispatch` inline adapters
store it inline. Native static instantiations cost code size. Neither path removes
the backend's own capacity or allocation costs.
