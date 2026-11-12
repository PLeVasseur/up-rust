# Migrating from 0.9 to 0.10

Version 0.9 provides the classic message API (`UMessage`, `UMessageBuilder`,
`UTransport`), the Communication Layer traits with their default
implementations, and the generated core-service protobuf API. All of them remain
supported. The first sections below list the changes that affect existing 0.9
code. The final section summarizes opt-in capabilities that have no 0.9
predecessor; their guides hold the complete contracts.

## Feature and accessor names

Update direct feature selections in Cargo.toml and dependent-crate feature
forwarding. The old feature names are removed without aliases:

| Previous feature | Replacement | Capability |
| --- | --- | --- |
| `up-core-types` | `up-core-api` | Generated core-service protobuf APIs and conversions; implies `protobuf-support`. |
| `up-l2-api` | `communication-api` | Communication Layer traits, independently of the default role implementations. |

`util`, which provides `LocalTransport`, now also enables
`transport-implementer-api` (listener admission and bounded registrations). That
feature enables native descriptor support and therefore the `sha2` dependency;
message-only builds without `util` do not need it. The `simple_notify` example
additionally declares `protobuf-support`.

The hand-written public types use accessor names without `get_`. Generated
protobuf APIs retain their own generator-defined naming conventions.

| Previous accessor | Replacement |
| --- | --- |
| `UStatus::get_code` | `UStatus::code` |
| `UStatus::get_message` | `UStatus::message` |
| `UStatus::get_message_or_default` | `UStatus::message_or_default` |
| `UStatus::get_details` | `UStatus::details` |
| `UAny::get_type_url` | `UAny::type_url` |
| `UAny::get_value` | `UAny::value` |
| `UUID::get_time` | `UUID::time` |
| `UAttributesValidators::get_validator(message_type)` | `UAttributesValidators::validator_for(message_type)` |
| `UAttributesValidators::get_validator_for_attributes` | `UAttributesValidators::validator_for_attributes` |

The returned values and serialization are unchanged by these source-level renames.
For example:

```rust
use up_rust::{UAny, UCode, UStatus, UUID};

let detail = UAny::new("example.com/error".to_owned(), b"opaque".as_ref());
let status = UStatus::new(UCode::DataLoss, Some("invalid response"), Some(vec![detail]));
assert_eq!(status.code(), UCode::DataLoss);
assert_eq!(status.message(), Some("invalid response"));
assert_eq!(status.message_or_default("fallback"), "invalid response");
let detail = status.details().first().expect("one detail");
assert_eq!(detail.type_url(), "example.com/error");
assert_eq!(detail.value(), b"opaque");
let _creation_time_millis = UUID::build().time();
```

## URI text and entity identifiers

URI strings use four hexadecimal path segments, `/type/instance/version/resource`,
for example `up://vehicle/1234/ABCD/1/8001`. The 0.9 three-segment form with a
packed entity ID is rejected rather than reinterpreted, so migrate stored URI
strings and configuration values. Entity instances must be in `1..=0xFFFF`:
constructors reject a zero instance with `UUriPartsError::ZeroEntityInstance`, so
change packed default-instance IDs from `0x0000_TYPE` to `0x0001_TYPE`.
`try_from_parts` keeps its packed `u32` argument (type in the low 16 bits,
instance in the high 16 bits) and serializes to the four-segment form. The pinned
protobuf `UUri` carries separate `ue_type_id` and `ue_instance_id` fields, so its
wire schema differs from the 0.9 `ue_id` field.

Every build rejects IPvFuture authorities and authority arguments followed by a
path, query or fragment. `UUri::try_from_wire_parts` and its Copy `UUriPartsError`
reject received parts without allocating; `try_from_parts` retains its owned
validation errors. `UCode::from_i32` and `UUID::is_valid_u64_pair` are
allocation-free checks beside the error-returning `UCode::try_from_i32` and
`UUID::from_u64_pair`. With `zero-copy-transport`, `UUri` stores its authority
inline; other builds keep `String` storage. The [URI guide](src/guide/uris.md)
contains examples.

`ExactUUri` is available at the crate root for APIs requiring a URI with no
wildcard components. Convert an owned or borrowed `UUri` with `TryFrom` and handle
`UUriError::ValidationError` for wildcard input. Existing `UUri` filter APIs keep
their matching semantics; no implicit conversion narrows a wildcard filter.

## Extensible errors and UUID representation

`UUriError`, `UAttributesError`, `ServiceInvocationError` and `PubSubError` are
non-exhaustive. Downstream matches must retain a fallback arm for future variants
rather than assuming the current list is closed. Named variants keep their meaning.
Converting `ServiceInvocationError::RpcError` into `UStatus` returns the carried
status rather than a generic `UCode::Unknown` status.

`UUID::as_u64_pair` returns the most- and least-significant 64-bit words, the inverse
of `UUID::from_u64_pair`, without changing the UUID representation:

```rust
use up_rust::{UUriError, UUID};

fn classify(error: &UUriError) -> &'static str {
    match error {
        UUriError::SerializationError(_) => "syntax",
        UUriError::ValidationError(_) => "validation",
        _ => "other URI error",
    }
}
assert_eq!(classify(&UUriError::ValidationError("wildcard".into())), "validation");
let uuid = UUID::build();
let (most, least) = uuid.as_u64_pair();
assert_eq!(UUID::from_u64_pair(most, least).unwrap(), uuid);
```

## Payload identity and opaque carriage

`UPayloadFormat` is removed. Use the numeric `PayloadEncoding` identity without
equating structural validity with registry assignment or decoder support:

| Previous API | Replacement |
| --- | --- |
| `UPayloadFormat::{ProtobufWrappedInAny, Protobuf, Json, Someip, SomeipTlv, Raw, Text}` | `PayloadEncoding::{PROTOBUF_WRAPPED_IN_ANY, PROTOBUF, JSON, SOMEIP, SOMEIP_TLV, RAW, TEXT}` |
| `UPayloadFormat::Shm` | No constant: ID 8 is unassigned in the pinned registry. `PayloadEncoding::from_id(8)` still carries it opaquely. |
| `UPayloadFormat::Unspecified` for a present contract-defined payload | `PayloadEncoding::CONTRACT_DEFINED`, explicit ID 0; no payload uses `None` instead. |
| `UPayloadFormat::as_i32`, `UPayloadFormat::try_from_i32` | `PayloadEncoding::id`, `PayloadEncoding::from_id` |
| `UPayloadFormat::to_media_type`, `UPayloadFormat::from_media_type`, `UPayloadError` | `PayloadEncoding::media_type` and `PayloadEncoding::from_media_type`, which returns `UAttributesError` and rejects the ambiguous protobuf media type and unregistered media types rather than guessing |
| `UAttributes::payload_format`, `UMessage::payload_format` | `payload_encoding`, returning `Option<PayloadEncoding>` |
| `payload_format_unchecked` on a message | Prefer checked `payload_encoding`; `payload_encoding_unchecked` still requires presence. |
| `UAttributes::payload_format_unchecked` | Use `payload_encoding` and handle the optional identity explicitly. |
| `UPayload::new(bytes, format)`, `UPayload::payload_format` | `UPayload::new(bytes, encoding)`, `UPayload::payload_encoding` |
| `UMessageBuilder::build_with_payload(bytes, format)` | `build_with_payload(bytes, encoding)` |
| Generated protobuf field `payload_format` | Optional `payload_encoding_id`, field 12, with uint32 wire storage and a 16-bit accepted range |

ID 0 displays as `0 (contract-defined)` and requires an applicable contract; it
is not absence or an instruction to guess an encoding. `from_id` preserves every
structurally valid 16-bit value, including unknown, unassigned, reserved and
private identifiers. `from_registry_entry` checks the assignment range;
`private_use` checks the private range `0xF000..=0xFFFE`. Neither constructor
establishes an allocation or decoding agreement.

Builders reject origination of reserved IDs (`0xE000..=0xEFFF` and `0xFFFF`) with
`UMessageError::AttributesValidationError`. Received/forwarded reserved identities
remain valid opaque carriage. `with_declared_payload_encoding` is the explicit
carrier/decorator operation for existing payload bytes; it does not turn an
absent payload into a present one. Payload and identifier must both be present
or both absent. A zero-length present payload is still present; mismatched
presence produces `UMessageError::PayloadError`.

```rust
use up_rust::{PayloadEncoding, UAttributesError, UMessageBuilder, UMessageError, UUri};

let topic = UUri::try_from_parts("vehicle", 0x0001_1234, 1, 0x8001)?;
let present = UMessageBuilder::publish(topic.clone())
    .build_with_payload(Vec::new(), PayloadEncoding::CONTRACT_DEFINED)?;
assert_eq!(present.payload_encoding(), Some(PayloadEncoding::CONTRACT_DEFINED));
assert!(present.payload().unwrap().is_empty());
let reserved = PayloadEncoding::from_id(0xE000)?;
assert!(matches!(
    UMessageBuilder::publish(topic).build_with_payload("new payload", reserved),
    Err(UMessageError::AttributesValidationError(UAttributesError::ValidationError(_)))
));
let carried = present.with_declared_payload_encoding(reserved)?;
assert_eq!(carried.payload_encoding(), Some(reserved));
carried.validate()?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

CloudEvents output carries payload octets opaquely: binary data in Protobuf Event
Format, and `data_base64` rather than parsed `data` in JSON Event Format. `pformat`
preserves the identifier and the carriage media type is `application/octet-stream`.
JSON consumers must decode `data_base64` before parsing the application JSON, or
apply an explicit application-aware projection. Any present data property without
`pformat` is rejected regardless of its type. Consumers reject `text_data` and
`proto_data`, including empty values with otherwise valid metadata; producers use
opaque `binary_data` or structured JSON `data_base64` with an explicit `pformat`.
`CloudEventAttributeValue` and the nested `cloud_event` module (`Data` and
`cloud_event_attribute_value::Attr`) are exported beside `CloudEvent`, so
consumer signatures can name the event's attribute and data field types.

TTL zero means no expiration (`None`); positive `u32` millisecond values are
preserved without clamping, where 0.9 capped `with_ttl` at `i32::MAX`. The pinned
protobuf `ttl` field is a plain `uint32`, so zero and absence are the same
no-expiry value on the wire. Requests still require a positive TTL. Bindings with
a smaller carrier range must report unrepresentability.

## Message kinds and priorities

`UMessageType` and `UPriority` implement `Eq` and `Hash`; `UPriority` also
implements `Ord`, preserving its CS0-through-CS6 ordering. `wire_code()` and
`from_wire_code()` give the explicit encoded codes used by frame metadata and the
fixed ABI; never use Rust discriminants as wire codes. Absent priority is `None`,
encoded as zero, and reserved kind/priority codes are invalid. The existing
priority-code and CloudEvent type conversions remain available.

## Classic communication roles and transports

The Communication Layer traits and `SimplePublisher`, `SimpleNotifier`,
`InMemorySubscriber`, `InMemoryRpcClient` and `InMemoryRpcServer` remain supported.
Apart from the payload, accessor and error changes above, their signatures are
unchanged; the recipes are in the
[compatibility communication guide](src/guide/compatibility_communication.md).
Existing `UTransport` implementations keep working with them.

The application roles described below reach an existing binding through
`ClassicCarriage::new(transport)`. Sending roles need nothing more. Receive roles,
including RPC clients waiting for responses, also need `UTransport::attach_listener`
(with `owned-frame-transport`): return an `OwnedAttachment` whose synchronous
release removes exactly that registration, and leave no registration behind on
failure or cancellation. The default implementation refuses, so receive-role
setup fails explicitly instead of leaking a listener. Classic carriage never
carries native type tokens.

## Moving classic roles to the application roles

`SimplePublisher`, `InMemoryRpcClient` and the other classic roles above keep working.
To move a program to the domain-only handles of `communication::application`
(feature `typed-communication`), keep your binding and change the code that creates
and calls roles. `ClassicCarriage::new(transport)` adapts an existing `UTransport`,
so the roles can reach it without a new binding. The programs below are
`examples/simple_publish.rs` and `examples/simple_rpc.rs` next to
`examples/hello_publisher.rs` and `examples/hello_rpc.rs`.

### `SimplePublisher` to a stored publisher

```diff
+up_rust::declare_operations! {
+    event DOOR_STATUS: StringValue = 0xb4c1;
+}
+
 let uri_provider = Arc::new(StaticUriProvider::for_entity(
     "my-vehicle",
     UEntityId::first_instance(0xA34B),
     1,
 )?);
 let transport = Arc::new(LocalTransport::default());
-let publisher = SimplePublisher::new(transport, uri_provider);
-let payload = UPayload::try_from_protobuf(message)?;
-publisher
-    .publish(0xb4c1, CallOptions::for_publish(None, None, None), Some(payload))
-    .await?;
+let carriage = Arc::new(ClassicCarriage::new(transport));
+let local = Deployment::with_defaults(carriage, uri_provider)?.protobuf();
+let status = DOOR_STATUS.publisher(&local, PublishOptions::default())?;
+status.publish(&message).await?;
```

`Publisher<StringValue>` is the only type you store; the codec, route and carriage
are bound at setup. Sending needs nothing more from an existing binding.

### `InMemorySubscriber` to a subscription

```diff
-let subscriber = InMemorySubscriber::new(transport, uri_provider).await?;
-subscriber.subscribe(&topic, Arc::new(listener), None).await?;
+let service = Arc::new(RpcClientUSubscription::new(usubscription_rpc_client));
+let mut subscription = DOOR_STATUS
+    .subscription(&local, publisher_address, service, handler)
+    .await?;
+subscription.activate().await?;
```

The classic subscriber builds its own uSubscription client; the role takes
the service explicitly (`core::usubscription::RpcClientUSubscription` wraps an
`RpcClient`). `publisher_address` is the publisher's `LocalUriProvider`, not its
deployment. `activate` makes the remote change; retain the ticket through
`unsubscribe`, and read the [application guide](src/guide/applications.md) for
reconciling a failed activation.

### `InMemoryRpcClient` and `InMemoryRpcServer` to RPC roles

Client side:

```diff
+up_rust::declare_operations! {
+    rpc SAY_HELLO: StringValue => StringValue = 0x00a0;
+}
+
-let client = InMemoryRpcClient::new(transport, uri_provider.clone()).await?;
-let response = client
-    .invoke_method(
-        uri_provider.get_resource_uri(0x00a0),
-        CallOptions::for_rpc_request(1_000, None, None, None),
-        Some(UPayload::try_from_protobuf(request)?),
-    )
-    .await?;
-let reply = response.expect("a reply payload").extract_protobuf::<StringValue>()?;
+let carriage = Arc::new(ClassicCarriage::new(transport));
+let local = Deployment::with_defaults(carriage, uri_provider.clone())?.protobuf();
+let options = RpcOptions {
+    ttl: Duration::from_secs(1),
+    ..OperationOptions::default().rpc
+};
+let client = SAY_HELLO.client(&local, server_address, options).await?;
+let pending = client.invoke(&request).await?;
+let reply = pending.response().await?;
+let reply: &StringValue = reply.payload()?;
```

Server side, with a method-specific handler:

```diff
-#[async_trait::async_trait]
-impl RequestHandler for EchoOperation {
-    async fn handle_request(
-        &self,
-        _resource_id: u16,
-        _message_attributes: &UAttributes,
-        request_payload: Option<UPayload>,
-    ) -> Result<Option<UPayload>, ServiceInvocationError> {
-        let name = request_payload
-            .ok_or_else(|| ServiceInvocationError::InvalidArgument("no payload".to_string()))?
-            .extract_protobuf::<StringValue>()
-            .map_err(|_| ServiceInvocationError::InvalidArgument("not a StringValue".to_string()))?;
-        Ok(Some(UPayload::try_from_protobuf(text(format!("hello, {}", name.value)))?))
-    }
-}
-
-let rpc_server = InMemoryRpcServer::new(transport, uri_provider);
-rpc_server.register_endpoint(None, 0x00a0, echo_op.clone()).await?;
-// ... serve ...
-rpc_server.unregister_endpoint(None, 0x00a0, echo_op).await?;
+impl RequestHandler<StringValue> for Greeter {
+    type Reply = String;
+
+    async fn handle<D: Delivery<StringValue>>(&mut self, request: D) -> Result<String, UCode> {
+        let name = request.payload().map_err(|_| UCode::InvalidArgument)?;
+        Ok(format!("hello, {}", name.value))
+    }
+}
+
+let options = OperationOptions::default();
+let mut server = SAY_HELLO.server(&local, Greeter, text, options.server).await?;
+// ... serve ...
+server.stop().await?;
```

Here `text` builds a `StringValue` from the reply; the handler decides and the
writer builds, which is the same split the native roles use.

What actually changes besides the calls:

* **Options move to setup.** Per-call `CallOptions` become role options: `PublishOptions`
  on publishers and notifiers, `RpcOptions` (including the response deadline, `ttl`) on an
  RPC client. Call `set_token` on the client to change its credential.
* **Errors are typed per role.** `PubSubError` and `ServiceInvocationError` become
  `SerializedError` and `CallError`; `CallError` separates `TimedOut`, `Closed`,
  `Status(code)` and `Response(error)`, and the application guide says what to do with each.
* **A handler's error is a `UCode`.** Its message text does not reach the caller.
* **Lifecycle is explicit.** `stop` borrows the role and reports whether retrying can
  finish; retain the owner until it does, or use `role_set!` to own several roles.
* **Receive roles need scoped attachment.** Sending works on any `UTransport` through
  `ClassicCarriage`; receive roles, including RPC clients waiting for responses, also need
  `UTransport::attach_listener` (the local transport has it; other bindings may refuse).

## Changing codec, envelope, transport or representation

Each diff below shows the exact source change. Unless noted, stored handles, operations
and handlers stay the same, and what changes is setup that a schema or deployment
owner performs once. Keep both peers in step: a schema mapping does not prove the
peer agrees.

### Protobuf to JSON

For an application with its own domain types and both mappings declared, the
deployment owner selects another profile on every peer:

```diff
 // Schema owner, once per domain type (examples/support/user_first_door.rs).
 up_rust::schema_mapping!(Encoding, DoorState {
     Encoding::Protobuf => (schema::DoorProtobuf, schema::DoorProtobuf),
+    Encoding::Json => (schema::DoorJson, schema::DoorJson),
 });

 // Deployment owner, on every peer.
-let door = door::deployment(transport, door::Encoding::Protobuf)?;
+let door = door::deployment(transport, door::Encoding::Json)?;
```

A generated protobuf type used directly has no JSON form, so it first needs a domain
type and codec declaration:

```diff
-use protobuf::well_known_types::wrappers::StringValue;
+#[derive(serde::Serialize, serde::Deserialize)]
+#[serde(deny_unknown_fields)]
+struct DoorStatus {
+    text: String,
+}
+up_rust::json_codec!(struct StatusJson = ("example.status.json-v1", 0xF101) for DoorStatus);
+up_rust::schema_mapping!(struct Schema { DoorStatus => StatusJson });

 up_rust::declare_operations! {
-    event DOOR_STATUS: StringValue = 0x8001;
+    event DOOR_STATUS: DoorStatus = 0x8001;
 }

-let local = Deployment::with_defaults(transport, Arc::new(address))?.protobuf();
+let local = Deployment::with_defaults(transport, Arc::new(address))?.bind(Schema);
```

The family literal and compact ID are the deployment's private declaration: every peer
must declare exactly the same pair, and a selected envelope (next section) also needs a
full-wire declaration naming the new family. The SDK has no serde dependency; the macro
calls `serde_json` in your crate.

### Envelope

The envelope belongs to the carriage. Application setup does not change; the deployment
owner replaces the wire declarations it agrees with its peer:

```diff
 // Deployment owner, on each peer; peer declarations are provisioned separately.
-let context = OwnedWireContext::agree::<UFrameFieldsMetadataCodec>(local_wire, &peer_wire)?;
+let context = OwnedWireContext::agree::<UFrameFieldsMetadataCodec>(new_local_wire, &new_peer_wire)?;
 let carriage = Arc::new(OwnedWireTransport::new(core, UFrameFieldsMetadataCodec, context)?);
 let local = Deployment::with_defaults(carriage, address)?.bind(schema);
```

`OwnedWireDeclaration` names the full wire, the actual payload family, the metadata layout
and a format version; `OwnedWireContext::agree` checks the two declarations, and
`up-native-profile verify-owned-context` validates independently provisioned declaration
files. The declaration must name the mapping's payload family, so a codec change that
alters the family also needs a new declaration. Both whole-frame envelopes have been
exercised with both codecs on in-process fixtures only (`tests/user_first_application.rs`);
no external binding has run this path.

### Transport

Replace the carriage object; the rest of setup and the application stay the same:

```diff
-let transport: Arc<dyn UOwnedTransport> = Arc::new(InMemoryOwnedTransport::default());
+let transport: Arc<dyn UOwnedTransport> = connect_my_binding(config).await?;
 let local = Deployment::with_defaults(transport, Arc::new(address))?.bind(schema);
```

Keeping a binding built on the classic API is the same line with
`Arc::new(ClassicCarriage::new(existing_utransport))`. What changes around this line is
the binding's own configuration (discovery, peers, QoS) and what the binding refuses at
setup: a receive role needs the binding's scoped listener attachment, a payload encoding
it cannot carry is refused through `admit_payload_contract` or, for one-way roles,
`admit_carriage_payload_contract`, and a binding whose middleware owns RPC correlation
answers `UOwnedTransportImpl::owned_rpc`. Bindings are not interchangeable beyond what
their documentation states; SOME/IP is not yet admitted as a supported binding and other
bindings have not been rebuilt against this API.

### Native instead of serialized

This is the one change that is not a setup swap: the stored handle types, setup, sends,
server reply construction and error types all change, and only declarations, handlers and
the response wait stay the same. Native code also has to choose stable representations.
The native path has only been exercised on `FixtureCore` over the heap-backed `PoolCore`;
no real middleware has run it.

```diff
 up_rust::role_set! {
-    pub(crate) mod roles {
-        state: Publisher<DoorState>,
-        changed: Notifier<DoorState>,
-        subscription: Subscription,
-        set: RpcClient<SetDoor, DoorState>,
-        notifications: Receiver,
-        server: RpcServer,
+    pub(crate) mod roles<C: NativeLoanCore> {
+        state: native::Publisher<C, DoorState>,
+        changed: native::Notifier<C, DoorState>,
+        subscription: native::Subscription<C>,
+        set: native::RpcClient<C, SetDoor, DoorState>,
+        notifications: native::Receiver<C>,
+        server: native::RpcServer<C, DoorState>,
     }
 }

-let local = Deployment::with_defaults(transport, address)?.bind(encoding);
-slots.state.store(STATE.publisher(&local, options.publish)?);
-slots.set.acquire(SET.client(&local, device, options.rpc)).await?;
+let local = native::Deployment::with_defaults(core, agreement, address)?;
+slots.state.store(STATE.native_publisher(&local, options.publish)?);
+slots.set.acquire(SET.native_client(&local, device, options.rpc)).await?;

-state.publish(&DoorState { open: false }).await?;
-let pending = set.invoke(&SetDoor { open: true }).await?;
+state.publish_in_place(|loan| loan.into_initializer().open(false).finish())?;
+let pending = set.invoke_in_place(|loan| loan.into_initializer().open(true).finish())?;
 let response = pending.response().await?;

-SET.server(&local, DoorController, |open| DoorState { open }, options.server)
+SET.native_server(&local, DoorController, |open, loan| loan.into_initializer().open(open).finish(), options.server)
```

| Part | Serialized | Native |
| --- | --- | --- |
| Declarations (`declare_operations!`) | Same | Same |
| Handlers (`Handler`, `RequestHandler`) | Same | Same |
| Response wait | `pending.response().await` | Same |
| Stored handle types | `Publisher<P>` | `native::Publisher<C, P>`, or concrete aliases from `native_roles!` |
| Deployment | `Deployment::with_defaults(transport, address)` | `native::Deployment::with_defaults(core, agreement, address)`, with a real `NativeProfileAgreement` |
| Factories | `publisher`, `client`, `server`, and so on | `native_publisher`, `native_client`, `native_server`, and so on |
| Send | `publish(&value).await`, a borrowed value | `publish_in_place(initializer)`, synchronous, in middleware storage |
| Payload types | Any type with a codec, including `String` and `Vec` | Stable representations only: `repr(C)` plus the `StablePayload` and `StablePayloadInit` derives |
| Errors | `SerializedError`, `CallError` | `SetupError`, `SendError`, `NativeCallError`, `TokenError`, `AdmissionError` |
| Cleanup primitives | `SerializedCleanup` | `native::NativeCleanup`, or `role_set!` |

`native::Deployment::with_defaults` takes the binding's core and its `NativeProfileAgreement`;
a binding constructs that core. No native binding is built against this source, so
treat native setup as a design that has only run on fixtures.

## Renamed and removed during 0.10 development

These replace names introduced and removed during 0.10 development; 0.9 did not have them.

| Before | After |
| --- | --- |
| `up_rust::CallError` (native) | `up_rust::NativeCallError` |
| `up_rust::RequestHandler` (native engine) | `up_rust::NativeRequestHandler` |
| `application::Call` | `application::PendingCall` |
| `application::native::Call` | `application::native::Invoke` (`close` moved to `native::ClientClose`) |
| `application::native_binding::*` | `application::native::*` |
| `schema_bindings!`, `native_schema_bindings!` | `declare_operations!` plus role factories on `Event`/`Notification`/`Rpc` |
| Hand-written `Option` slots, `started` flag, `acquire()` per await, `SerializedCleanup` | `role_set!` |
| `communication::typed::...` | `communication::advanced::typed::...` (old path retained, undocumented) |
| Packed IDs such as `0x1234` | `UEntityId::first_instance(0x1234)` or `UEntityId::new(type, instance)` |
| `NativeCleanup::close::<Req, Resp, _, _, _, _>` | `NativeCleanup::close(...)` (no turbofish) |

`role_set!` field names are public field names of the generated `Roles` and `Slots`;
`cleanup` is reserved.

## New opt-in capabilities

These capabilities have no 0.9 predecessor. Frame APIs, metadata codecs, the
fixed ABI and native profile formats are provisional: they carry explicit
versions, require peer agreement and fail closed on a mismatch.

**Application roles (`typed-communication`).** Start with `examples/hello_publisher.rs`
and `examples/hello_rpc.rs`. Ordinary applications use
`communication::application`. A schema package declares each event, notification
and RPC method once (`declare_operations!`, `Event`, `Notification`, `Rpc`)
together with its payload mappings (`MappingProfile`, for example through
`schema_mapping!`). Setup binds them to a `Deployment` that owns the carriage,
local address and bounded limits (`Deployment::with_defaults` or
`Deployment::configured`). Applications store domain-only handles:
`Publisher<P>`, `Notifier<P>`, `Subscription`, `Receiver`, `RpcClient<Req, Resp>`
and `RpcServer`, usually through one `role_set!` that owns partial setup and cleanup.
Any subset of roles can be deployed without an opposite peer;
`examples/user_first_door.rs` is the advanced lifecycle example. RPC clients
reserve bounded correlation before sending. A subscription ticket prepares local
state before remote side effects; retain it through activation, reconciliation
and unsubscribe. Stop failures report whether a retry can complete through
`completion()` on `SerializedStopError`, `ReceiverStopError` and
`RpcServerStopError`, built on `RegistrationStopError::is_terminal`.
`communication::advanced::typed::Endpoint` with `Serialized<T, K>` or `Native<C>` exposes the
underlying static engines for advanced and reusable code; see the
[typed guide](src/guide/typed.md) and `examples/typed_door.rs`.

**Native loans (`up-l2-zero-copy`).** Native application roles require an explicit
`NativeLoanCore` implementation for a configured middleware binding; generic
prepared pools do not satisfy it. Sends initialize the payload synchronously in
the binding's loan (`publish_in_place`, `notify_in_place`, and `invoke_in_place`
followed by `.response().await`); no TX loan crosses an await. Receive handlers
share the serialized `Delivery` contract, with checked `payload()` and borrowed
`context()` over retained RX storage. A deployment binding selects one core once,
for example with `native_roles!`. The SDK's own native tests use the in-process
`PoolCore`; they exercise SDK ownership mechanics, not physical middleware.

**Owned frames and transport implementation (`owned-frame-transport`,
`transport-implementer-api`, or the umbrella `transport-implementer`).** `UOwnedFrame` carries validated metadata and owned
payload storage; unchecked frames carry `Unvalidated` and must pass `validate`
before they can be sent. Bindings implement `UOwnedTransportImpl` (applications
call `UOwnedTransport`), admit payload contracts at setup and provide scoped
receive through an `OwnedAttachment`. The
[implementing a transport guide](src/guide/transports.md) says which trait to implement. New implementations can instead implement
`serialized_dispatch::StaticOwnedTransport` (semantic frames) or
`StaticOwnedCore` (encoded frames) with plain `async fn` methods and no
`#[async_trait]`, wrapped in `InlineOwnedTransport` or `InlineOwnedCore`: the SDK
then stores each send future inline instead of boxing it. Setup and receive
futures remain boxed at the object-safe boundary, and existing `#[async_trait]`
implementations keep their boxed send. A binding whose middleware owns RPC
correlation, such as SOME/IP sessions, returns it from
`UOwnedTransportImpl::owned_rpc`; serialized RPC roles then use that authority
exclusively. `ListenerAdmission` separates
callback admission from bounded native quiescence, and `RegistrationInbox` with
`Registration` provides bounded receive queues with one generic worker. See the
[UTransport guide](src/guide/utransport.md) and the
[registration guide](src/guide/registrations.md).
`InMemoryOwnedTransport` is public only with `test-util`.

**Frame metadata, codecs and selected wires (`wire-implementer-api`,
`selected-wire-*`).** `UFrameMetadata` validates semantic metadata independently
of any envelope; carriers implement `UFrameView::has_payload` explicitly, because
an empty byte slice does not mean absence. `PayloadCodec`, `PayloadLayout` and
`ReadDecodePayload` separate encoding identity, byte layout and bounded decoding,
and `UWireError` distinguishes missing, unsupported, malformed, short-output and
schema-mismatch failures. `frame::metadata::try_project_umessage_to_frame_metadata`
and `try_project_frame_to_umessage`, plus `UAttributes::try_project_to_frame_metadata`
and `UFrameMetadata::try_project_to_attributes`, convert between frames and classic
messages; they reject fractional or overflowing TTLs and native-token-bearing
frames rather than truncate values or drop identity. Wire contracts use crate-root
imports. `UWireTransport` composes an encoded core with a selected `UWire` marker
and `UWireMetadataCodec`; `OwnedWireTransport` admits an independently provisioned
`OwnedWireContext`. See the [frame metadata guide](src/guide/frame_metadata.md)
and the [wire guide](src/guide/wires.md).

**Native identity and stable payloads (`native-identity`, `zero-copy-transport`,
`native-inventory`).** `NativeRepresentation`, `NativeTypeToken` and
`NativeProfileAgreement` bind target, layout and field semantics to a deployment.
`NativeProfile::new(domain, generation, encoding, representations)` agrees one
native encoding for a set of complete representations; independent 32-bit tokens
distinguish them and allocate no payload IDs. Frame metadata carries
`Option<NativeTypeToken>` in every build. `zero-copy-transport` provides the
`StablePayload` and `StablePayloadInit` derives (`repr(C)` or `repr(transparent)`,
explicit padding, no drop glue; `revision` for descriptor revisions and
`crate = "path"` when the SDK dependency is renamed), checked borrowing,
`TokenBuffer`, `RxAdmission`/`RxLease`, prepared routes, prepared RPC
(`PreparedRpcClient`, `PreparedRpcServer`) and opaque forwarding
(`HeaderAdmission`, `OpaqueRoute`). Manual unsafe stable-payload implementations
must exclude interior mutability. `StableContainerWire` adapters require
`UWireTransport::with_native_agreement` or `into_stable_container_transport`. The
`up-native-profile` tool generates and verifies deployment artifacts; the
[native identity guide](src/guide/native_identity.md) covers descriptors,
generations and agreement.
