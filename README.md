# Eclipse uProtocol Rust library

This is the [uProtocol Language Library](https://github.com/eclipse-uprotocol/up-spec/blob/main/languages.adoc)
for Rust. Source builds use the protocol definitions pinned by the `up-spec`
submodule. The [migration guide](MIGRATION.md) describes the changes from the
published 0.9 API.

The crate can be used to

* implement uEntities that communicate with each other using the uProtocol [Communication Layer API](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l2/api.adoc) over one of the supported transport protocols.
* implement support for an additional transport protocol by means of implementing the [Transport Layer API](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l1/README.adoc).

## Quick start

Applications use the role handles in `communication::application`: enable the
`typed-communication` feature. These small programs are the place to begin; `test-util`
supplies their in-memory carriage:

```sh
cargo run --example hello_publisher --features typed-communication,test-util
cargo run --example hello_rpc --features typed-communication,test-util
cargo run --example hello_notify --features typed-communication,test-util
```

`examples/hello_publisher.rs` in full:

```rust
use protobuf::well_known_types::wrappers::StringValue;
use std::sync::Arc;
use up_rust::communication::application::{Deployment, PublishOptions};
use up_rust::{InMemoryOwnedTransport, StaticUriProvider, UEntityId};

up_rust::declare_operations! {
    event DOOR_STATUS: StringValue = 0x8001;
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    Ok(())
}
```

A real program passes its binding's transport to `Deployment::with_defaults`
instead of the in-memory one. Generated protobuf types need no schema mapping; for your
own types use `json_codec!` or the other paths in the
[application guide](src/guide/applications.md), which also covers typed addressing and
role subsets. A program with several roles should let `role_set!` own them, so that
failed or cancelled setup keeps what it acquired and cleanup stays bounded and retryable:

```rust,ignore
up_rust::declare_operations! {
    pub event STATUS: StringValue = 0x8001;
    pub rpc SET_LEVEL: UInt32Value => UInt32Value = 1;
}

up_rust::role_set! {
    pub mod roles {
        status: Publisher<StringValue>,
        set_level: RpcClient<UInt32Value, UInt32Value>,
    }
}

async fn start(setup: &mut roles::Setup, local: &BoundDeployment<ProtobufProfile>,
               peer: &dyn LocalUriProvider) -> Result<(), SerializedError> {
    let slots = setup.begin()?; // one attempt; a repeat is refused
    slots.status.store(STATUS.publisher(local, PublishOptions::default())?);
    slots.set_level.acquire(SET_LEVEL.client(local, peer, OperationOptions::default().rpc)).await?;
    Ok(())
}
```

`begin` returns the slots once and each owner is stored before the next await;
`setup.close(deadline)` cleans up, and `setup.finish()?` promotes a complete set to typed
`Roles`. `examples/user_first_door.rs` is the advanced lifecycle example, showing
retained cleanup after a lost unsubscribe acknowledgement; it is not the place to start.

Going further: exact codec, envelope, transport and native-versus-serialized changes
are in the [migration guide](MIGRATION.md), and binding authors should read
[implementing a transport](src/guide/transports.md) and enable the
`transport-implementer` feature.

The application roles have been exercised mainly on in-process fixtures. Native paths
have only been run on test fixtures, SOME/IP is not yet admitted as a supported binding,
and other bindings have not been rebuilt against this API.

## Using the Crate
<!--
`uman~up-language-using~1`
Covers:
- req~up-language-documentation~
-->
For the published 0.9 API, add the crate to the `[dependencies]` section of Cargo.toml:

```toml
[dependencies]
up-rust = { version = "0.9" }
```

The 0.10 development API described by this checkout requires an explicit source
revision or local dependency; it is not supplied by the 0.9 dependency above.

The classic Communication Layer API (`communication` feature), with its default implementations `SimplePublisher`, `SimpleNotifier`, `InMemorySubscriber`, `InMemoryRpcClient` and `InMemoryRpcServer`, remains supported; `examples/simple_publish.rs`, `examples/simple_notify.rs` and `examples/simple_rpc.rs` show it, and the [compatibility guide](src/guide/compatibility_communication.md) has recipes. New applications should start from the application roles above.

The `cloudevents` feature maps messages to and from Protobuf Event Format
`CloudEvent`s. Its `CloudEventAttributeValue` and nested `cloud_event` types are
exported beside `CloudEvent` for use in consumer signatures.

## Provisional Interfaces

Frame APIs, metadata field codecs and the opt-in fixed ABI are provisional while
their representation/native-profile specification is developed. Formats carry
explicit versions and require peer agreement; a version change requires coordinated
endpoint updates and regeneration of affected artifacts. These local SDK formats
do not establish normative registry assignments. See the
[frame metadata guide](src/guide/frame_metadata.md) for contracts and examples.

`owned-frame-transport` provides `UOwnedFrame`, `UOwnedTransport` and
`UOwnedListener`. Frames are validated before entering the public send/receive
surface; test transports require the explicit `test-util` feature.
`transport-implementer-api` provides listener admission and bounded quiescence,
also enabled by `util`. Admission closure, callback completion and retained storage
remain separate contracts, described in the transport implementation guide.
Native callback admission never blocks and may run on any callback thread.
The same implementer feature provides a bounded `RegistrationInbox` and one
generic async `RegistrationHandler` worker per `Registration`. Storage and tasks
are created at setup; synchronous pushes use an explicit overflow policy. Stop
drains earlier admissions, while Drop cancels queued work; a stop from the
worker's own handler returns the nonterminal `RegistrationStopError::CalledFromWorker`,
and `RegistrationStopError::is_terminal` classifies stop failures. The
[registration guide](src/guide/registrations.md) explains lifecycle and blocking.
New owned transports can implement `serialized_dispatch::StaticOwnedTransport` or
`StaticOwnedCore` with plain `async fn` methods, without `#[async_trait]`, so the
SDK stores each send future inline instead of boxing it.

For the application roles, see the [quick start](#quick-start) and the
[application guide](src/guide/applications.md). Add `up-l2-zero-copy` for roles that
initialize native middleware loans in place through an explicit
`NativeLoanCore`. `communication::advanced::typed` exposes the underlying static
engines (`examples/typed_door.rs` and the [typed guide](src/guide/typed.md)). Classic
message support remains through `ClassicCarriage` and the original
UMessage/UTransport APIs.

Convert complete messages with `frame::metadata::try_project_umessage_to_frame_metadata`
and `try_project_frame_to_umessage`. These check payload presence and the carried
encoding together. Native-token-bearing frames cannot project to the protobuf
`UMessage` shape: it has no place to preserve that identity. Use native frames instead.
Attribute-only conversion uses `UAttributes::try_project_to_frame_metadata()`.
The [native identity guide](src/guide/native_identity.md) describes these
projections and their token rules.
Frames and messages share the protobuf-independent `UMessageType`/`UPriority`
semantics. Their explicit encoded-code methods preserve selected-wire and ABI bytes.

Selected-wire adapters compose an encoded transport core with a wire marker and
metadata codec. Enable `selected-wire-user-api` for application construction and
`transport-implementer-api` for encoded-core contracts, with the required frame
family. Use crate-root imports. The adapter checks identities and decoded frame
validity before exposing receive results or invoking application listeners.

### Selected-wire identity

`native-identity` enables target-qualified descriptors, independent 32-bit type
tokens and immutable deployment agreement. Selected-wire capabilities imply it;
default message-only builds do not require the `sha2` runtime dependency. See the
[native identity guide](src/guide/native_identity.md) for checked descriptors,
deployment agreement under the provisional policy. One `NativeProfile` binds an
agreed native encoding to a set of complete representations. Separate tokens identify
those representations without allocating a payload encoding per type.
Applications use typed routes; deployment setup owns the profile. Explicit encoding
zero uses the same representation rules. This uses the existing registry, not a
second registry or serializer.
Native profiles are single-target. The native identity guide explains target
qualification; the deployment tool guide covers executed exporters and remedies
for token collisions or incompatible representation history.
The lightweight `NativeTypeToken` and metadata field remain available in default
builds for opaque carriage. Projection to the protobuf `UMessage` shape refuses
to discard a carried token; encoding alone cannot reconstruct one.

Payload encoding, selected wire, payload family and metadata layout use separate
namespaces. Private selected-wire identities require their agreed literals;
compact-only matching is limited to registered first-wave SDK pairs. The metadata
codec owns its layout identity. Use canonical crate-root imports such as
`UWire`, `WireIdentity` and `UFrameFieldsMetadataCodec` with `wire-implementer-api`.
The [wire identity guide](src/guide/wire_identity.md) explains the namespace and
private-alias rules under the same provisional policy.

Checked-in [metadata fixtures](tests/fixtures/wire_metadata/README.md) are verified
against hash-bound manifests; normal tests do not regenerate them. Deliberate
representation changes use the explicit ignored updater and include reviewed
vector/manifest changes together.

`zero-copy-transport` adds checked stable payload borrows and initialized/two-phase
transmit loans, and implies `native-identity`. Typed codec APIs (including bounded
reader decode) require `native-identity`, implied by selected-wire features.
`StablePayload` derives fixed layouts with explicit padding and checked field bits;
`StablePayloadInit` provides write-once initialization. Bindings still own storage,
native lifetime and physical copy behavior. See the [native guide](src/guide/native_identity.md).
`native-inventory` registers concrete linked types for target-side descriptor export.
The zero-copy feature also stores URI authority bytes inline. The
[URI guide](src/guide/uris.md) explains the typed, allocation-free rejection API
and the default build's retained `String` storage. Every build rejects IPvFuture
authorities and authority arguments with trailing paths, queries or fragments.
Upstream protobuf type/instance identifiers are checked separately on receipt;
the existing packed Rust parts API and inline layout remain unchanged.
URI text now uses `/type/instance/version/resource`; migrate legacy three-segment
configuration strings and zero-instance defaults as described in the URI guide.
The same feature exposes `TxHeaderParts`, caller-buffer header writers and
`parse_header` for allocation-free in-place metadata validation. `RxHeader`
borrows token/traceparent bytes from the validated input; payload admission remains
a separate check. See the [frame metadata guide](src/guide/frame_metadata.md).
`UFrameFieldsMetadataCodec::native_prefix` obtains the agreed framing prefix at
setup using the same registered/private identity encoding as the whole codec.
`TokenBuffer` provides setup-reserved token storage with allocation-free updates,
oversize rejection that retains the previous value, redacted Debug and owned-byte
wiping on overwrite/clear/drop. That owned storage is distinct from the tokens an
`RxHeader` borrows from received frames.
`RxAdmission<P>` prepares a stable type's representation, complete identity and
agreement once. Its `RxLease<P, F>` retains the binding's `Frame`, checks header
and payload presence/identity before typed access, and validates length, alignment
and field bits without allocating error strings. Generic `ZeroCopyHandler` workers
receive only admitted leases; storage provenance remains descriptive evidence.
Prepared transmit uses `TxTemplate<P>` and `TxRoute<C, P>` over a
`PreparedTransportCore`. Route/fan-out/storage capacities are reserved at setup;
synchronous sends encode headers and initialize payloads directly in the loan.
Routes can also check and encode caller-built semantic metadata using
`prepare_metadata`/`send_metadata`, borrowing its strings without cloning them.
`HeaderAdmission`/`HeaderLease` support in-place opaque carriage without imposing
a native type agreement. Bounded `OpaqueRoute` forwarding preserves payload IDs
and tokens under the same wire/family/layout identities with one explicit egress
payload copy; originating reserved IDs is rejected separately.
Asynchronous callback admission tracks nested context on the polling stack;
registration workers own their concrete handler futures and bounded inboxes.
Selected-wire `WireRegistration` handles combine decoded filtering with physical
detachment, retain failed detach handles for retry, and drain on explicit stop.
Implementing `PreparedTxStorage` requires an explicit unsafe ownership contract:
stable disjoint storage and preservation of every byte initialized by the SDK.
Owned RPC listeners use weak transport references to avoid registry cycles;
entered callbacks retain the sender only through handler and reply completion.
Field-block and selected-wire codecs also offer exact-length, caller-slice writers;
short output is rejected before mutation and canonical metadata bytes are preserved.
Generated array initializers support bulk in-place repeated patterns, with a
fixed-size empty-pattern error and no payload-sized temporary. Stable fields are
duplicable plain data, excluding ownership-sensitive handles and uniqueness state.
The payload-contract fixture features ship eight hash-bound stable byte vectors.
Their core/full catalog digests include the stored-vector manifest; ordinary tests
validate stored bytes, and regeneration requires an explicitly selected updater.
LiDAR fixture geometry uses frozen coefficient bits so exact validation does not
depend on platform-specific transcendental rounding.
The local/CI policy runs prepared allocation checks on both compilers in debug and
release, auditing every required result row and the benchmark's allocating control.
It also runs the serialized dispatch allocation controls in both profiles.
The explicit `test-util` feature exposes `PoolCore` as an in-process conformance
implementation, with owned-lease provenance rather than a claim of native SHM.
Prepared RPC clients reserve pending slots and credential storage, and servers
reserve their reply-route caches. They arm correlation before publication and
separate asynchronous request handling from synchronous reply initialization.
Status-only responses retain payload absence and request correlation.
Server dispatch checks the decoded destination before admitting a client route.
In-memory TX constructors report impossible sizes/allocation failure as
`ResourceExhausted`; backend capacity and allocation behavior belong to bindings.

`StableContainerWire` requires explicit `with_native_agreement` construction.
Selected-wire receive views retain that immutable generation; checked borrowing
is the ordinary path. The [wire guide](src/guide/wires.md) states the per-call
unsafe producer/bit-validity obligations for the expert receive path.
The [up-native-profile tool](tools/up-native-profile/README.md) owns versioned
deployment artifacts, representation history and independently provisioned admission.
Its `generate` command binds an explicit native encoding to the target inventory.
The SDK-owned tool states compatibility in its exact SDK dependency and remains unpublished.

`up-l2-zero-copy` adds the prepared stable-native engines under
`communication::zero_copy` (`communication::advanced::zero_copy` with
`typed-communication`) that native application roles and deployment bindings
compose. Publishers and notifiers hold typed routes and send synchronously,
receivers own generic workers, and RPC clients select request and response types
separately and reserve their pending capacity. They use an explicitly agreed
`StableContainerWire` adapter, and received values remain valid after their
registration is removed. `Publisher::from_route` reuses a template-prepared Publish
route. Applications reach these engines through `communication::application` or
`communication::advanced::typed` with `Native<C>`, which also own subscription service
coordination. The [native loan guide](src/guide/zero_copy.md) lists the binding
contracts.

## Building from Source

For source-level feature and accessor changes, see the
[0.9 to 0.10 migration guide](MIGRATION.md).

<!--
`uman~up-language-building~1`
Covers:
- req~up-language-documentation~1
-->

First, the repository needs to be cloned using:

```sh
git clone --recurse-submodules https://github.com/eclipse-uprotocol/up-rust.git
```

The `--recurse-submodules` parameter is important to make sure that the git submodule referring to the uProtocol type definitions is being initialized in the workspace. The proto3 files contained in that submodule define uProtocol's basic types and are being compiled into Rust code as part of the build process.
If the repository has already been cloned without the parameter, the submodule can be initialized manually using `git submodule update --init --recursive`.

The crate can then be built using the [Cargo package manager](https://doc.rust-lang.org/cargo/) from the root folder:
<!--
`impl~use-cargo-build-system~1`
Covers:
- req~up-language-build-sys~1
- req~up-language-build-deps~1
-->

```sh
cargo build
```

The crate has some (optional) _features_ as documented in [lib.rs](src/lib.rs).

APIs that require one exact resource can accept `ExactUUri`, a checked URI without
wildcard components. The [transport implementation guide](src/guide/utransport.md#exact-uri-arguments)
shows construction and borrowing; ordinary `UUri` values remain the filter type.

VSCode can be instructed to build all features automatically by means of putting the following into `./vscode/settings.json`:

```json
{
  "rust-analyzer.cargo.features": "all"
}
```

### Generating API Documentation

The API documentation can be generated using

```sh
cargo doc --no-deps --all-features --open
```

### Repository Checks

Run the checked-in gates locally with
`bash tools/ci/gate.sh --configuration current --all` and
`bash tools/ci/gate.sh --configuration msrv --all`.
The [CI guide](tools/ci/README.md) lists the required tool versions, generated
policy workflow, separate link/feature jobs and test-log artifact contract.
Checks reject missing targets and incomplete test summaries. Timeout cleanup
terminates the command's remaining process group even when its leader exits first.
The independent feature gate uses an isolated source snapshot and an audited
derived lock; the CI guide documents its offline setup and retained check results.
Dependency validation is explicitly locked/offline, with cache preparation kept
separate from the validation command.

## License

The crate is published under the terms of the [Apache License 2.0](LICENSE).

## Contributing

Public API items must have documentation and public types must support `Debug`.
Unsafe blocks need a concrete safety justification, and public unsafe functions
need a `# Safety` contract. The repository enforces these rules with its Rust and
Clippy lint configuration. The minimal notification example now also enables
`protobuf-support`; each example declares the features it uses in Cargo.toml.

Contributions are more than welcome. Please refer to the [Contribution Guide](CONTRIBUTING.md).
