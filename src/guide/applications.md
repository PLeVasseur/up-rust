# Build an application

Applications use `communication::application`: enable the `typed-communication`
feature. Declare each operation once, bind it to a `Deployment`, and store small
role handles whose types name only your domain payloads.

The programs below run on the in-memory carriage, which also needs `test-util`.
A real program passes its binding's transport to `Deployment::with_defaults`.
These roles have been exercised mainly on in-process fixtures; native paths have only
run on test fixtures, SOME/IP is not yet admitted as a supported binding, and other
bindings have not been rebuilt against this API.

## Start with a small program

`examples/hello_publisher.rs` is a complete application:

```rust
# #[cfg(all(feature = "typed-communication", feature = "test-util"))]
# #[tokio::main(flavor = "current_thread")]
# async fn main() -> Result<(), Box<dyn std::error::Error>> {
use protobuf::well_known_types::wrappers::StringValue;
use std::sync::Arc;
use up_rust::communication::application::{Deployment, PublishOptions};
use up_rust::{InMemoryOwnedTransport, StaticUriProvider, UEntityId};

up_rust::declare_operations! {
    event DOOR_STATUS: StringValue = 0x8001;
}

let address = StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(0x1234), 1)?;
let transport = Arc::new(InMemoryOwnedTransport::default());
let local = Deployment::with_defaults(transport, Arc::new(address))?.protobuf();

let status = DOOR_STATUS.publisher(&local, PublishOptions::default())?;
let message = StringValue {
    value: "door closed".to_owned(),
    ..Default::default()
};
status.publish(&message).await?;
println!("published: {}", message.value);
# Ok(())
# }
# #[cfg(not(all(feature = "typed-communication", feature = "test-util")))]
# fn main() {}
```

Run it with `cargo run --example hello_publisher --features typed-communication,test-util`.
It prints `published: door closed`. Nobody needs to be listening: no role requires
an opposite peer.

`examples/hello_notify.rs` sends a one-way notification to a handler, and
`examples/hello_rpc.rs` runs a client and server in one process. The RPC program in
brief:

```rust
# #[cfg(all(feature = "typed-communication", feature = "test-util"))]
# #[tokio::main(flavor = "current_thread")]
# async fn main() -> Result<(), Box<dyn std::error::Error>> {
# use protobuf::well_known_types::wrappers::StringValue;
# use std::sync::Arc;
# use up_rust::communication::application::{Delivery, Deployment, OperationOptions, RequestHandler};
# use up_rust::{InMemoryOwnedTransport, StaticUriProvider, UCode, UEntityId};
# fn text(value: String) -> StringValue {
#     StringValue { value, ..Default::default() }
# }
# up_rust::declare_operations! {
#     rpc SAY_HELLO: StringValue => StringValue = 1;
# }
struct Greeter;

impl RequestHandler<StringValue> for Greeter {
    type Reply = String;

    async fn handle<D: Delivery<StringValue>>(&mut self, request: D) -> Result<String, UCode> {
        let name = request.payload().map_err(|_| UCode::InvalidArgument)?;
        Ok(format!("hello, {}", name.value))
    }
}
# let bus = Arc::new(InMemoryOwnedTransport::default());
# let service = StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(0x0100), 1)?;
# let service = Deployment::with_defaults(bus.clone(), Arc::new(service))?.protobuf();
# let caller = StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(0x0200), 1)?;
# let caller = Deployment::with_defaults(bus, Arc::new(caller))?.protobuf();
# let options = OperationOptions::default();

// `service` and `caller` are separate deployments; the peer is only an address.
let mut server = SAY_HELLO.server(&service, Greeter, text, options.server).await?;
let client = SAY_HELLO.client(&caller, service.address(), options.rpc).await?;

let pending = client.invoke(&text("uProtocol".to_owned())).await?;
let reply = pending.response().await?;
assert_eq!(reply.payload()?.value, "hello, uProtocol");
server.stop().await?;
# Ok(())
# }
# #[cfg(not(all(feature = "typed-communication", feature = "test-util")))]
# fn main() {}
```

The handler decides the reply; the writer passed at setup (`text`) builds it.
Handlers never receive a raw frame: a delivery owns its checked payload and
`context()` exposes its metadata.

## Address an entity

Name an entity by its 16-bit type and nonzero instance instead of a packed `u32`.
`UEntityId::first_instance(0x1234)` is the common case; instance numbers from
configuration go through `UEntityId::new(type_id, instance)`, which refuses 0:

```rust
use up_rust::{LocalUriProvider, StaticUriProvider, UEntityId, UUri};

let door = UEntityId::first_instance(0x1234);
let topic = UUri::for_entity("vehicle", door, 1, 0x8001)?;
let local = StaticUriProvider::for_entity("vehicle", door, 1)?;
assert_eq!(topic, local.get_resource_uri(0x8001));
# Ok::<(), Box<dyn std::error::Error>>(())
```

A packed ID keeps the instance in its high 16 bits, so `0x1234` means instance 0
and is rejected at runtime with an error that names this constructor. Existing
`UUri::try_from_parts` and `StaticUriProvider::new` callers keep working.

The address names authority, entity, major version and resource: resource 0
receives notifications and responses, `0x0001..=0x7FFF` selects RPC methods, and
`0x8000..=0xFFFE` selects event and notification topics. Notification destinations
are exact. Subscriptions use the real uSubscription service.

## Choose a schema path

Choose the smallest form that fits:

* **Generated protobuf type, one codec:** no enum and no mapping code:
  `Deployment::with_defaults(transport, address)?.protobuf()` selects
  `ProtobufProfile`. The small programs above use this.
* **JSON for serde types:** `up_rust::json_codec!` declares the codec; the SDK has
  no serde dependency, and the macro calls `serde_json` in your crate. Use
  `#[serde(deny_unknown_fields)]` for exact-field schemas.
* **One hand-mapped codec per type:** the struct form of `schema_mapping!`, then
  `deployment.bind(Schema)`.
* **Deployment chooses between codecs:** a profile enum with one `schema_mapping!`
  invocation per domain type. Request and response codecs need not match.
* **Domain type mapped through a wire message:** `up_rust::mapped_codec!` builds a
  codec on an existing one, such as `ProtobufPayload`.

The last three look like this; `ReadingCodec`, `DoorState` and the other names are yours:

```rust,ignore
up_rust::schema_mapping!(pub struct Schema { Reading => ReadingCodec, Query => QueryCodec });

up_rust::schema_mapping!(Format, Reading {
    Format::Protobuf => (ReadingProtobuf, ReadingProtobuf),
    Format::Json => (ReadingJson, ReadingJson),
});

up_rust::mapped_codec! {
    pub struct DoorProtobuf = "vehicle.door.protobuf.v1" via ProtobufPayload, wire BoolValue;
    DoorState => |state| wire_of(state), |wire| domain_of(wire);
}
```

A domain type with a JSON codec, end to end:

```rust
# #[cfg(all(feature = "typed-communication", feature = "test-util"))]
# #[tokio::main(flavor = "current_thread")]
# async fn main() -> Result<(), Box<dyn std::error::Error>> {
# use std::sync::Arc;
# use up_rust::communication::application::{Deployment, PublishOptions};
# use up_rust::{InMemoryOwnedTransport, StaticUriProvider, UEntityId};
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Level {
    value: u32,
}

// Every peer must declare exactly this family literal and compact ID.
up_rust::json_codec!(struct LevelJson = ("example.level.json-v1", 0xF1A2) for Level);
up_rust::schema_mapping!(struct Schema { Level => LevelJson });
up_rust::declare_operations! {
    event LEVEL: Level = 0x8001;
}
# let address = StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(0x1234), 1)?;
# let transport = Arc::new(InMemoryOwnedTransport::default());

let local = Deployment::with_defaults(transport, Arc::new(address))?.bind(Schema);
let level = LEVEL.publisher(&local, PublishOptions::default())?;
level.publish(&Level { value: 7 }).await?;
# Ok(())
# }
# #[cfg(not(all(feature = "typed-communication", feature = "test-util")))]
# fn main() {}
```

`declare_operations!` is the only declaration shape, and role factories take the
deployment they act on. Mappings are stateless: `Encoder` and `Decoder` are function
tables resolved at setup, so per-session or schema-registry codecs are not supported.
Each role admits its own mapping against the carriage before it attaches anything,
and `Event::admit`, `Notification::admit` and `Rpc::admit` can check a peer pair up
front. A schema declaration does not prove the peer agrees and nothing negotiates:
deployments keep the mapping and family declarations in step on both sides.

`Deployment::configured` and `OperationOptions` hold the named limits;
`Deployment::with_defaults` picks bounded defaults (see `guide::independent_l2_user`).
The default overflow policy drops the newest item, which is not reliable delivery.
Advanced setup can build `Encoder::from_schema_mapping::<K>()` and
`Decoder::from_schema_mapping::<K>()` for `Deployment::publisher` and the other
explicit factories. Serialized roles pay ordinary encoding, metadata and setup costs;
nothing here is a zero-allocation promise.

## Own several roles: `role_set!`

A program with several roles should not hand-write `Option` slots and a `started`
flag. `role_set!` owns any mix of roles, including repeats, for serialized or
native applications. Declare the fields and write one `start` function listing
acquisition order and per-role configuration. The SDK keeps owners and original
errors on failed or cancelled setup, refuses a second attempt, closes roles
independently under a per-role deadline, and keeps the first error plus every
attempt in `cleanup.<role>`:

```rust
# #[cfg(feature = "typed-communication")]
# mod example {
use protobuf::well_known_types::wrappers::{StringValue, UInt32Value};
use std::time::Duration;
use up_rust::communication::application::*;
use up_rust::{Completion, LocalUriProvider};

up_rust::declare_operations! {
    pub event STATUS: StringValue = 0x8001;
    pub rpc SET_LEVEL: UInt32Value => UInt32Value = 1;
}

up_rust::role_set! {
    /// The roles this one program needs: any subset, repeats allowed.
    pub mod roles {
        status: Publisher<StringValue>,
        set_level: RpcClient<UInt32Value, UInt32Value>,
    }
}

async fn start(
    setup: &mut roles::Setup,
    local: &BoundDeployment<ProtobufProfile>,
    peer: &dyn LocalUriProvider,
) -> Result<(), SerializedError> {
    let slots = setup.begin()?; // one attempt; a repeat is refused
    slots
        .status
        .store(STATUS.publisher(local, PublishOptions::default())?);
    slots
        .set_level
        .acquire(SET_LEVEL.client(local, peer, OperationOptions::default().rpc))
        .await?;
    Ok(())
}

pub async fn run(
    local: &BoundDeployment<ProtobufProfile>,
    peer: &dyn LocalUriProvider,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut setup = roles::Setup::default();
    if let Err(original) = start(&mut setup, local, peer).await {
        // A failed setup keeps what it acquired; clean up, then report.
        while setup.close(Duration::from_secs(1)).await == Completion::Pending {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        return Err(original.into());
    }
    let mut app = setup.finish()?;
    app.status.publish(&StringValue::new()).await?;
    // Keep `app` until close stops reporting Pending; its errors stay in `app.cleanup`.
    while app.close(Duration::from_secs(1)).await == Completion::Pending {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}
# }
# fn main() {}
```

`begin` returns the slots once. Each owner is stored before the next await, so
failure or cancellation keeps exactly what was acquired; `setup.close(deadline)`
cleans up, and `setup.finish()?` promotes a complete set to typed `Roles`. Native
sets name their core once, as `pub mod roles<C: NativeLoanCore>` with fields such as
`state: native::Publisher<C, DoorState>`. Field names become public fields of the
generated `Roles` and `Slots`; `cleanup` is reserved. `acquire`, `CleanupLeg`,
`SerializedCleanup` and `native::NativeCleanup` remain public primitives for
applications that manage owners themselves, and a reusable library's owners can
join a role set by implementing `ManagedRole`.

Dropping a role releases its local attachment only; it does not undo remote
state, so retain an owner until its `close` or `stop` reports completion.

## Subscribe to events

A subscription needs the uSubscription service. `EVENT.subscription(&local,
publisher_address, service, handler).await?` returns an inactive `Subscription`
ticket; `activate().await?` then records the remote subscription. Keep the ticket
through `unsubscribe().await`. If activation or cleanup fails or is cancelled,
`Subscription::observe_remote(deadline)` asks the real subscription service for the
ticket's state and reconciles on a definite answer; timeouts and errors change
nothing but `remote_observations()`. It relies on the service returning a complete
subscriber list, so a paginated or eventually consistent service needs a stronger
check of its own.

## Errors you will handle

Setup and sends return `SerializedError` (`Config`, `Payload`, `Transport` or
`Prepare`). `invoke` and `response` return `CallError`:

| `CallError` | Meaning | Usual recovery |
| --- | --- | --- |
| `TooManyInFlight` | Every pending slot is occupied | Wait for capacity; do not replay an already submitted call |
| `TimedOut` | The deadline passed, so completion is unknown | Reconcile before retrying |
| `Closed` | The client has closed | Create a new client |
| `Status(code)` | The server answered with an error status | Depends on the code, for example repair credentials on `PermissionDenied` |
| `Submit(error)` | Mapping or physical send failure | Inspect it; delivery may be uncertain |
| `Response(error)` | The reply could not be decoded | Check the peer's declaration |

A handler's `Err(UCode)` reaches the caller as `CallError::Status`; message text is
not carried. Stop errors say whether a retry can finish through `completion()`.

## Names

Ordinary code uses `communication::application`: `Deployment`, `Publisher`,
`Notifier`, `Subscription`, `Receiver`, `RpcClient`, `RpcServer`, `PendingCall`,
`CallError`. Native handles live in `application::native` under the same role
names (`native::Publisher<C, P>`, and so on); `native_roles!` generates concrete
aliases so ordinary native programs never write the core parameter. Enable
`up-l2-zero-copy` for native loans; the [typed guide](crate::guide::typed) covers
ownership, and business handlers stay shared with the serialized path. The static
engines are reachable, for reusable libraries, at `communication::advanced`.

## Where next

* `examples/support/independent_apps.rs`: publisher-only, subscriber-only,
  client-only, server-only, notifier-only and mixed programs (compiled by the
  independent-role tests).
* [Communication roles](crate::guide::applications::communication): every role at a glance.
* [Migrating from 0.9 to 0.10](crate::guide::migration): classic-to-application walkthrough
  and exact diffs for codec, envelope, transport and representation changes.
* `examples/user_first_door.rs`: the advanced lifecycle example, with retained cleanup
  after a lost unsubscribe acknowledgement, on all six handles. It is not a starting
  point; finish the programs above first.
* [The Transport Layer directly](crate::guide::applications::transport),
  [classic communication recipes](crate::guide::compatibility_communication) and
  [implementing a transport](crate::guide::transports).
