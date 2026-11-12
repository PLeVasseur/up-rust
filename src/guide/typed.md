# Typed static engines

`communication::advanced::typed` holds the statically typed engines beneath the domain-only
roles of `communication::application`; the former `communication::typed` path remains
available but is not documented. Use it directly for advanced or reusable
code that selects `Serialized<T, K>` or `Native<C>` explicitly; native deployment
bindings compose it. Enable `typed-communication`, plus `up-l2-zero-copy` for
native loan construction. The example `examples/typed_door.rs` and its
`support/door.rs` contain a complete five-role application on this layer;
`tests/typed_application.rs` exercises its ownership semantics with in-process
carriage. Loopback and `PoolCore` tests do not establish physical middleware
behavior. Classic message recipes remain in the
[compatibility guide](crate::guide::compatibility_communication).

## What setup names

Setup names domain types, their actual codec mapping (or stable native
representation), a configured carriage and URI provider, route addresses, bounded
capacities and deadlines. Subscribers supply a real uSubscription service.
Deployment/binding owners supply actual peer agreements, selected-envelope
declarations and middleware capabilities. Applications need not implement
transport traits or name wire/family/token IDs at each message operation.

`Serialized<T, K>` uses existing `EncodePayload`/`DecodePayload` mappings. Classic
carriage is payload-agnostic. Selected owned carriage additionally checks K's
`payload_family()` against the configured envelope context during setup. A custom
codec without a provisioned family can use classic carriage but is refused by a
selected-family adapter. Payload encoding identifies an encoding, not a Rust type
or schema. Each route still selects its agreed application mapping. An already
erased `Arc<dyn UOwnedTransport>` uses `Serialized::<dyn UOwnedTransport, K>::from_dyn`.

`Native<C>` requires `NativeLoanCore`, an explicit contract for a configured
native-only middleware binding. `PreparedTransportCore` conformance alone is
insufficient. The implementation must initialize directly in middleware TX storage,
synchronously publish that same loan and retain genuine native RX storage. Missing
capability is a compile error; missing agreement/type/route/capacity is a setup
error. No failure selects a copy fallback. A Zenoh binding must require SHM TX and
reject non-SHM RX; iceoryx2 and LoLa bindings must bind their actual middleware
storage.

## One complete door-control flow

The publisher reports `DoorState`, the subscriber observes that event, a notifier
sends the state to an exact panel address, and an RPC client asks a server to
`SetDoor`. `DoorObserver` and `DoorController` are the same implementations for
both representations. The controller checks the request's `door-control` token
through `Delivery::context()` and returns a boolean business decision.

| Role | Setup | Serialized message operation | Native message operation |
| --- | --- | --- | --- |
| Publisher | `publisher::<DoorState>(resource, options)` | `publish(&state).await` | `publish_in_place(initializer)` |
| Subscriber | `subscriber(service).subscription::<DoorState, _>(topic, handler).await`, then `ticket.activate().await` | Owning `Delivery`, checked `payload()` and borrowed `context()` | Same handler and accessors on a retained lease |
| Notifier | `notifier::<DoorState>(resource, exact_destination, options)` | `notify(&state).await` | `notify_in_place(initializer)` |
| Notification receiver | `listen::<DoorState, _>(topic, handler).await` | Same handler | Same handler |
| RPC client | `rpc_client::<SetDoor, DoorState>(method, options).await` | `invoke(&request).await` | `invoke_in_place(initializer)` |
| Response wait | Returned call owns correlation, never a TX loan | `pending.response().await` | Same operation after synchronous commit |
| RPC server | `rpc_server::<SetDoor, DoorState, _, _>(resource, business_handler, response_constructor, options).await` | Constructor returns `DoorState` for encoding | Constructor writes directly through `DoorState::Initializer` |

Native construction is `|loan| loan.into_initializer().open(true).finish()`.
The server's constructor is `|open, init| init.into_initializer().open(open).finish()`, replacing
serialized `|open| DoorState { open }`. This is the essential ownership difference:
a preconstructed owned value is not initialization inside middleware loan storage.
There is no TX loan in a future. A delivery owns storage across awaits and retains
its original semantic metadata; native token/trace strings borrow the checked
header, with no UMessage conversion or payload allocation.

## Migration deltas

The [migration guide](crate::guide::migration) has the exact application-level
diffs: change the profile selected with `Deployment::bind` (a `BoundDeployment`)
or the carriage passed to `Deployment::with_defaults`, and leave the stored
handles and handlers alone. At this static layer the same changes are:

1. **Protobuf → JSON:** bind the JSON profile instead of the Protobuf one at both
   peers (for Door, `deployment.bind(Encoding::Json)`; at this layer, `DoorJson` as
   the K of a static `Serialized<T, K>`). The real mappings use generated
   `google.protobuf.BoolValue` or strict `{"open": boolean}`. The five-role body
   and handlers do not change. A selected envelope also needs a full-wire
   declaration naming the new family; an existing Protobuf wire cannot be relabelled.
2. **Envelope change:** keep the mapping and supply compatible configured carriage. The
   independently named `door_adoption_codec_envelope_orthogonality_all_five_roles`
   fixture holds native-prefix/field-block framing while changing both real codecs,
   then holds each codec while changing to `ClassicCarriage<LocalTransport>`.
   `EnvelopeLoop` separately uses both actual whole-frame serializers. These are
   ordinary serialized paths with honest allocation/copy costs.
3. **Compatible transport change:** pass another `Arc<dyn UOwnedTransport>` to
   `Deployment::new` or `Deployment::with_defaults`, or replace C in `native_flow<C>`,
   using the actual admitted binding. Mapping and handlers remain unchanged. A
   binding lacking scoped receive support refuses receive-role setup.
4. **Serialized ↔ native:** provision the complete target/profile/generation for
   `DoorState` and `SetDoor`, change setup to `Native::new`, replace the three value
   sends and server response constructor as above. Subscription, receive handlers,
   authorization, addresses and RPC response waiting stay the same. `String`/`Vec`
   retain first-class serialized support; using native requires an explicitly
   different stable representation and conversion where needed.
5. **Classic carriage:** wrap a real `UTransport` in `ClassicCarriage`. All five
   roles use that single adapter; receive roles also need the binding's
   `UTransport::attach_listener`. `UMessageBuilder`, `UTransport` and generated
   `.proto` APIs remain available. Absent and present-empty payloads are distinct.
   Native-token-bearing projection explicitly refuses; encoding or size cannot
   reconstruct the missing token.

## Selected-envelope provisioning

`OwnedWireDeclaration` names full wire, actual payload family, metadata layout and
format version. `OwnedWireContext::agree<C>` compares independent declarations;
`OwnedWireTransport::new(core, codec, context)` checks the actual metadata codec.
`Serialized::new` then admits K with that context. Both TX and RX use it. Existing
`UWireTransport<W,C>` remains an exact preset with its original identities/bytes.
Stable-container geometry and native agreement remain unchanged.

`examples/declarations/door-{protobuf,json}.json` explicitly name deployment-private
fixture identities, never registry assignments. `up-native-profile
verify-owned-context --local LOCAL.json --peer PEER.json` validates independent
files, exact identity namespaces, owners/versions and the entire combination.
`OwnedContextDeclaration::require_context` binds an application's static SDK
context to the provisioned file without leaking strings into static storage.
The native agreement artifact also exposes `require_context` for namespace
membership; that alone does not declare the meaning of a new serialized
combination. Tooling performs no negotiation or identity allocation.
Provision/freeze real peer files before constructing the production SDK context.
Fixture equality is not deployment verification.

## Lifecycle and correlation

A subscription ticket prepares local capability before remote side effects and
keeps business delivery gated off. `activate(&mut self)` records `RemoteUnknown`
before awaiting the service. If cancelled or failed, retain the ticket and
independently observe the real service state before calling `reconcile(status)`.
No timeout is evidence of remote rollback. `unsubscribe(&mut self)` closes local
admission and attempts exact detach synchronously, even before its returned future
is polled. The future performs remote cleanup and drains accepted work. Detach
failure retains the exact handle for retry; successful remote cleanup is not
repeated. Drop performs local cleanup only, without a hidden cleanup task or a
claim about remote completion.

Stop failures say whether retrying can finish: `completion()` on
`SerializedStopError`, `ReceiverStopError` and `RpcServerStopError` returns
`Completion::Pending` while remote state, a failed detach or a nonterminal
same-worker refusal remains, and `Completion::Failed` once a worker failed
terminally (`RegistrationStopError::is_terminal`). A handler that stops its own
role gets the nonterminal `RegistrationStopError::CalledFromWorker` inside that
stop error; retain the owner and complete the stop from another task.

Both RPC paths reserve/register bounded correlation before send. Full response
kind/source/sink/request ID and representation admission precede delivery. Status
errors use payload-free responses. Timeout, cancellation, duplicate/late replies
and close recover pending slots and retained frame ownership. Server client and
native reply-route capacity remain bounded. `prepare_client` can reserve known
reply destinations before traffic. Notification addressing is exact even if the
binding uses broader physical filters. A binding whose middleware owns
correlation can supply it through `UOwnedTransportImpl::owned_rpc`; the serialized
RPC roles then use that authority exclusively, and their handles do not change.
