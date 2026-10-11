# Stable-native communication

Enable `up-l2-zero-copy` for `communication::zero_copy::{Endpoint, Publisher,
Notifier, RpcClient, RpcServer}`. Add `usubscription` for `Subscriber` and supply
its subscription-service client. Notification receive uses `Endpoint::listen`.
The roles use `StableContainerWire`; other selected wires use
the transport-layer API.

## Configure an endpoint

### Prepared transmit routes

`TxTemplate<P>` resolves representation, complete identity and the selected prefix
once. It creates `TxRoute<C, P>` values from a fixed `RouteDefinition` and
`RouteConfig` over a `PreparedTransportCore`. New route entries come from the
core's reserved tables, allowing bounded RPC reply caches to prepare first-contact
routes without allocating another descriptor or prefix. The default token capacity
is 2048 bytes, matching `HeaderLimits::default`; traceparent is opt-in.

`MessageFields` borrows per-message token/traceparent and correlation overrides.
`TxRoute::send` writes the complete header into the loan, initializes the payload
through `PayloadInit`, checks the returned witness's range and field bits, then
publishes synchronously. No TX loan spans an await. Invalid overrides, pool
exhaustion and witness/field rejection use fixed-size errors. Application filler
code remains responsible for its own allocation behavior.

The raw core is wire-agnostic: it reserves addressing and capacities, lends separate
metadata/payload views and publishes an `InitializedTx`. SDK-visible initialization
does not initialize unrelated native framing/padding; that remains the binding's
obligation. Core registration uses concrete futures and a synchronous inbox sink.
Advanced producers can use `InitializedTx::from_initialized` only after proving
all of its metadata, payload and ownership obligations; ordinary routes perform
the initialization-witness and bit-validity checks automatically.

`PoolCore` is an explicitly opt-in, heap-backed conformance implementation. Its
slots, route/registration tables, fan-out and per-slot recipient scratch are all
reserved at creation. Publishing releases the registry lock before calling an
inbox, so a blocked push does not retain the lock needed for detachment. This
does not substitute for a real binding's SHM, native allocation or lifetime gates.

`TxRoute::prepare_metadata` and `send_metadata` also accept a caller-built
`UFrameMetadata`. They verify the fixed route fields and full payload identity
before obtaining storage, and encode the supplied metadata directly into that
storage. Token and traceparent strings are borrowed, not cloned. This preserves
semantic metadata construction for advanced callers while keeping SDK writes
inside the prepared storage path; the caller owns its construction costs.

`PreparedTxStorage` is an unsafe implementation boundary: a binding must retain
exclusive, disjoint, immobile metadata/payload storage and preserve all initialized
bytes between accessor/recording calls. The SDK checks returned extents and
alignment, but relies on those ownership guarantees when establishing an
`InitializedTx` proof. Each binding implementation must document that justification.

### Repeated array initialization

Generated array initializers provide `<field>_fill_pattern(&[Element])` for
repeating a small borrowed stable pattern directly into a loan. It copies complete
plain-data representations in bounded bulk steps, without invoking Clone or
constructing a payload-sized temporary. Empty patterns return Copy
`StablePayloadPatternError::EmptyPattern` for nonempty fields; zero-length fields
accept an empty pattern. Stable field implementations must support representation
duplication and cannot contain ownership-sensitive handles or uniqueness state.

### Opaque selected-wire carriage

`HeaderAdmission::for_wire<W>` validates the exact wire/family/layout identities
and frame metadata in place, retaining opaque payload storage in `HeaderLease<F>`.
Unknown and reserved incoming payload IDs remain uninterpreted. This API does not
need a native agreement solely to carry bytes and offers no typed payload borrow.
Exact matching accepts valid registered compact/literal forms and requires literal
private identities; it does not apply custom identity-alias callbacks.
`HeaderAdmission::prefix` borrows the canonical setup prefix; accepted registered
literal spellings are not limited to that one canonical byte sequence.

`HeaderHandler<F>` runs on a generic registration worker. `register_filtered`
rechecks decoded URI filters after physical delivery; `sink=None` means sinkless.
Admission and filtering have separate counters. `OpaqueTemplate` reserves an
exact-address `OpaqueRoute` with explicit metadata/payload bounds. `send_bytes`
rejects reserved origination, whereas `forward` can preserve an admitted reserved
ID and token. Forwarding cannot relabel wire/family/layout identity. It copies the
opaque payload once to egress storage and is copy-minimized, not end-to-end zero-copy.

Selected-wire adapters expose `prepare_header_receiver` and
`prepare_opaque_template`; native typed routes continue to use their explicit
agreement and `RxAdmission<P>` as described below.
`register_header_handler` combines an opaque worker with the physical subscription;
`register_stable_handler` does the same for checked typed leases. Both return a
`WireRegistration`: explicit `detach` retains a failed physical handle for retry,
`stop` closes and drains, and Drop cancels. Partial source filters and sinkless
`None` are checked after decoding in both paths. Typed/opaque admission and decoded
filtering have distinct counters.

### Prepared receive admission

`Frame` is the binding's retained immutable metadata/payload storage. Its optional
payload loan distinguishes absence from a present empty payload and records the
binding's provenance. Dropping a transport or registration must not invalidate a
frame retained by an application.

`RxAdmission<P>` validates the Rust representation and resolves the complete
native identity at setup, retaining the explicit agreement generation. It obtains
the native prefix through the selected metadata codec. Admission checks that
prefix by its identities: valid literal spellings of registered identities remain
accepted, and a private identity still requires its literal. It checks header
structure, role invariants, token cap, payload presence, both
identity components, exact size, alignment and field bits before creating a
typed `RxLease<P, F>`. Failure uses Copy `AdmissionError` values. The default header
token limit is 2048 bytes; explicit zero admits absent or empty tokens only.

A lease's header borrows token/traceparent from the retained frame. Its payload
accessor rechecks the frame before returning a typed reference. The checked stable
payload contract excludes pointers, drop glue and interior mutability, including
atomic/cell fields that could write through an immutable byte slice.

With the implementer feature, `RxAdmission::register` composes this admission with
one generic `ZeroCopyHandler<P, F>` worker and a bounded registration inbox.
Rejected frames increment the receiver's rejection counter and never reach the
application handler. No per-frame handler future is boxed by this path.

### Prepared RPC state

`PreparedRpcClient<C, Req, Resp>` reserves its request route, pending table and
token storage at creation. A synchronous response sink validates addressing and
correlation and parks the frame directly in its reserved pending entry. A call
arms that entry before publishing, releases its token read lock before physical
publication, and holds no TX loan across the response await. Cancellation returns
the entry; late/unknown responses are counted. Timeout, capacity, status and
admission outcomes use Copy `CallError` values.

`PreparedRpcServer<C, Resp>` reserves a fixed response-route cache and one generic
request worker. First contact fills an existing cache/table slot using a prepared
`TxTemplate`, without constructing another descriptor or prefix. Its
`RequestHandler` consumes the request lease in `handle`, allowing Send-only binding
frames to move with the future, and writes a reply afterward through `write_reply`.
No response TX loan or cache lock spans the asynchronous handler. Request and
response types may differ. Cache exhaustion refuses a new client without evicting
an existing route; handler status errors produce correlated status-only replies.
The server rechecks the decoded request destination before using its reply cache
or invoking user code; physical delivery filters are not metadata authority.

### Fixed-capacity token storage

`TokenBuffer` reserves credential storage at creation. Updates and clear do not
allocate. An oversized update returns `TokenError::TooLong` without changing the
previous value. Overwrite, clear and Drop wipe the owned buffer; Debug exposes only
capacity, presence and length. The caller's input and copies held elsewhere remain
the responsibility of their owners.

```rust
use up_rust::{TokenBuffer, TokenError};

let mut token = TokenBuffer::new(16)?;
token.set("current")?;
assert_eq!(token.set("a-token-that-will-not-fit"), Err(TokenError::TooLong));
assert_eq!(token.as_str(), Some("current"));
token.clear();
assert_eq!(token.as_str(), None);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Empty and absent are distinct: setting `""` creates a present empty token, and
clear removes its presence. Capacity counts UTF-8 bytes. A route or registration
can impose a separate admission policy. Received tokens borrow their frame bytes
through `RxHeader`; they are not copied into an owned token buffer automatically.

### Adapter and URI provider

An `Endpoint<C, U>` pairs an explicitly agreed stable-container adapter over
`Arc<C>` with the application's URI provider. Construct the adapter with
`UWireTransport::with_native_agreement`, after checking independently provisioned
peer configuration and required local types.
The [native identity guide](crate::guide::native_identity) covers those contracts.

The binding implements `PreparedTransportCore`. Payload data is initialized in
its loan; received `StableRx<C, P>` values retain their agreement and storage.
Heap-backed leases and physical native loans have distinct provenance; these role
APIs alone do not prove end-to-end no-copy delivery. `SendConfig` fixes TTL,
priority and route capacities at creation. `MessageFields` borrows each send's
ID, token, traceparent and other permitted overrides.

## Publish and borrow stable values

```rust,no_run
use up_rust::{AdmissionError, LocalUriProvider, MessageFields, PreparedTransportCore,
    SendError, SetupError, StablePayload, StablePayloadInit};
use up_rust::communication::zero_copy::{Endpoint, Publisher, SendConfig, StableRx};

#[repr(C)]
#[derive(Debug, StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.Reading", revision = 1)]
struct Reading { value: u32 }

fn prepare<C, U>(endpoint: &Endpoint<C, U>) -> Result<Publisher<C, Reading>, SetupError>
where C: PreparedTransportCore, U: LocalUriProvider + ?Sized {
    endpoint.publisher(0x8001, SendConfig::default())
}

fn publish<C: PreparedTransportCore>(publisher: &Publisher<C, Reading>) -> Result<(), SendError> {
    publisher.publish_stable(MessageFields::default(), |init| {
        init.into_initializer().value(42).finish()
    })
}

fn read<C: PreparedTransportCore>(frame: &StableRx<C, Reading>) -> Result<&Reading, AdmissionError> {
    frame.payload()
}
```

The agreement must contain the complete `Reading` descriptor. The derive requires
stable layout and explicit padding. Large arrays use borrowed input, `*_fill` or
`*_fill_with` setters instead of constructing a second whole payload value.
Initialized/default buffers can zero or copy the whole payload; bindings document
their physical behavior.

## Receive events and notifications

`Subscriber::subscribe` contacts the subscription service before creating a generic
`ZeroCopyHandler` worker with `ReceiveConfig`. Local setup failure attempts to roll
back the service subscription and reports a failed rollback as well as its original
cause. The returned `Subscription::unsubscribe` attempts both service removal and
local drain, retaining both errors. Drop and cancellation close local delivery;
asynchronous service cleanup requires an explicit unsubscribe/reconciliation.

`Endpoint::listen` creates a notification `Receiver` for the local source URI.
`Receiver::stop` closes admission, detaches physical delivery and drains accepted
frames. Drop aborts instead. Publish and notification workers check decoded kind,
source and sink before user code, independently of physical routing hints. A
retained lease remains valid after listener or endpoint destruction. A publisher
can also consume a checked Publish route using `Publisher::from_route`, reusing
an existing template's reserved state.

## Request and response

`Endpoint::rpc_client<Req, Resp>` reserves a client for an exact method before any
call. Its `call` returns a checked response of the separately selected `Resp` type.
`Endpoint::rpc_server<Req, Resp, H>` starts a `RequestHandler` worker for a local
method resource in `0x0001..=0x7FFF`, with a bounded first-contact reply cache.
These use the prepared RPC state and status-only handling described above.

## Runtime setup and allocation measurements

Initialize each runtime worker's UUID generator and deferred-waker storage before
counting traffic, and do the same for every binding delivery thread. On a
current-thread Tokio runtime, deferred-waker storage belongs to a `block_on`
scheduler entry: warmup and measurement must remain in that same entry. Endpoint
creation reserves SDK state; do not use a new client's first call as warmup. Count
initialization, header writing/parsing, handlers, pending waits and rejection on
every participating thread. Observers and application handlers must honor their
own allocation policy; their costs cannot be subtracted from a whole-process count.

## Errors and unsafe boundaries

Invalid metadata, absent agreement, wrong identity and invalid method filters return
typed SDK errors. Borrowing checks identity, representation, length, alignment and
field bits. Role paths use those checked operations. The separate expert transport
borrow API requires its explicit per-call safety proof; a provenance label is not
a safe attestation witness.
