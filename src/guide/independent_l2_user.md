# L2 end user

The existing `MultiplePublishers` temperature/pressure service now uses the
common bounded `Publisher<Reading>`. Call `MultiplePublishers::prepare` with
`PublisherDeployment`, enter runtime, then activate the result. Its actual
`publish_readings` body is shared by the host fixture and the no_std library
example `independent_publishers`. Setup returns core `StartupError`; runtime
errors are fixed-size `PublishError`, with no automatic retry after partial
delivery. The bounded Reading profile currently refuses JSON explicitly.

The single-role and mixed examples below retain their older owned-carriage
and native contracts. In particular `PublisherOnly`, `MixedSubset`, Door and
Meter still use `LegacyPublisher`; their naming correction is compatibility
maintenance, not common-core migration.

`independent_schema.rs` declares resources once. `independent_apps.rs` shows
publisher-only, subscriber-only, client-only, server-only, notifier-only,
receiver-only, two publishers and a publisher/client subset. None constructs the
opposite role. Each local deployment can use a different process and transport.

```rust,ignore
let local = schema::bind(Deployment::with_defaults(carriage, local_address)?);
let app = PublisherOnly::new(&local)?;
app.reading.publish(&Reading { value: 21 }).await?;
```

Stored legacy serialized fields are `LegacyPublisher<Reading>`, `RpcClient<Query, Reading>`,
`Subscription`, `Receiver` or `RpcServer`. Schema enums, encoders, transport
generics and counterpart endpoints do not appear in them. Receivers/clients take
peer ADDRESS data, not a peer deployment. Servers need no client address at setup;
native servers explicitly admit response destinations with `prepare_client`.

`SubscriberOnly::prepare` returns an owned inactive ticket: retain it, call
`activate`, and retain it through unsubscribe/reconciliation. `stop`/`unsubscribe`
remain borrowed operations; preserve errors and the owner on failure or cancelled
cleanup. A stop error's `completion()` reports `Pending` while a retry can still
finish and `Failed` after a terminal worker error. The recovery owner's supervisor
supplies aggregate recovery; these examples deliberately do not reimplement it.
Compose already prepared owners with
`mixed_subset` rather than dropping them on a later setup error, or let
`role_set!` own any mix of roles; see [building an application](crate::guide::applications).

For native, `independent_native.rs` stores `selected::Temperature`,
`selected::ReadClient` and `Option<selected::ReadResponse>`. SDK generation supplies
all engine and associated RX types. `publish`/request initialization is synchronous
in-place; only the response wait awaits. `ClientOnly::latest` borrows its retained
owner, never an erased `'static` payload. This source selection uses the explicit
test-only `FixtureCore` witness, which delegates to heap-backed `PoolCore`; an
installed middleware binding needs its own selection and physical tests.

Legacy defaults: 1 MiB encoded input, 64 queued items with DropNewest, 256 token bytes;
RPC 16 in-flight, five-second deadline, CS4; server 16 destinations. DropNewest is
not reliable delivery. Advanced users choose `Deployment::configured`,
`schema::bind_format` (JSON versus Protobuf) and explicit operation options.
Native setup still requires a genuine agreement/config; primitive native factories
accept custom limits. One-sided admission does not prove peer schema compatibility.
