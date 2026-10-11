# Eclipse uProtocol Rust library

This is the [uProtocol Language Library](https://github.com/eclipse-uprotocol/up-spec/blob/main/languages.adoc)
for Rust. Source builds use the protocol definitions pinned by the `up-spec`
submodule. The [migration guide](MIGRATION.md) describes the development API's
explicit payload-encoding identity and opaque carriage rules.

The crate can be used to

* implement uEntities that communicate with each other using the uProtocol [Communication Layer API](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l2/api.adoc) over one of the supported transport protocols.
* implement support for an additional transport protocol by means of implementing the [Transport Layer API](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/up-l1/README.adoc).

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

Most developers will want to use the Communication Level API and its default implementation which are provided by the `communication` module. Please refer to the [examples](./examples/) for inspiration how to use this crate.

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
The same implementer feature provides a bounded `RegistrationInbox` and one
generic async `RegistrationHandler` worker per `Registration`. Storage and tasks
are created at setup; synchronous pushes use an explicit overflow policy. Stop
drains earlier admissions, while Drop cancels queued work. The
[registration guide](src/guide/registrations.md) explains lifecycle and blocking.

For application facades, enable `owned-frame-transport` with `usubscription`.
`communication::owned::Endpoint` creates Publisher, Subscriber, Notifier,
RpcClient and RpcServer values with the existing role semantics. The
[owned guide](src/guide/owned.md) covers setup, validated frames, notification
receive, RPC and error handling; the subscriber needs a subscription service.

Convert complete messages with `frame::metadata::try_project_umessage_to_frame_metadata`
and `try_project_frame_to_umessage`, using their `_with_native_agreement` variants
for native identity. These check payload presence and the carried encoding together.
Attribute-only conversion uses `UAttributes::try_project_to_frame_metadata()`;
there is no separate encoding override. See the owned-frame guide for migration.
Frames and messages share the protobuf-independent `UMessageType`/`UPriority`
semantics. Their explicit encoded-code methods preserve selected-wire and ABI bytes.

Selected-wire adapters compose an encoded transport core with a wire marker and
metadata codec. Enable `selected-wire-user-api` for application construction and
`transport-implementer-api` for encoded-core contracts, with the required frame
family. Use crate-root imports; no compatibility import modules are needed.
The adapter checks identities and decoded frame validity before exposing receive
results or invoking application listeners.

### Selected-wire identity

`native-identity` enables target-qualified descriptors, independent 32-bit type
tokens and immutable deployment agreement. Selected-wire capabilities imply it;
default message-only builds do not require the `sha2` runtime dependency. See the
[native identity guide](src/guide/native_identity.md) for checked descriptors,
private-ID tables and explicit ID-zero contracts under the provisional policy.
The lightweight `NativeTypeToken` and metadata field remain available in default
builds for opaque carriage. Classic projection of a carried token requires the
agreement-aware APIs; ordinary projection refuses to silently remove it.

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
The same feature exposes `TxHeaderParts`, caller-buffer header writers and
`parse_header` for allocation-free in-place metadata validation. `RxHeader`
borrows token/traceparent bytes from the validated input; payload admission remains
a separate check. See the [frame metadata guide](src/guide/frame_metadata.md).
`UFrameFieldsMetadataCodec::native_prefix` obtains the agreed framing prefix at
setup using the same registered/private identity encoding as the whole codec.
`TokenBuffer` provides setup-reserved token storage with allocation-free updates,
oversize rejection that retains the previous value, redacted Debug and owned-byte
wiping on overwrite/clear/drop. The [stable-native guide](src/guide/zero_copy.md)
distinguishes that owned storage from tokens borrowed from received frames.
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
deployment artifacts, append-only allocation and independently provisioned admission.

`up-l2-zero-copy` adds stable-native Publisher, Notifier and RPC facades under
`communication::zero_copy`; add `usubscription` for Subscriber. An endpoint uses an
explicitly agreed `StableContainerWire` adapter and URI provider. The
publisher/notifier hold typed routes and send synchronously; listeners own generic
workers. Subscription setup rollback and unsubscribe preserve local/service errors,
and received values remain valid after the registration is removed. RPC clients
select request and response types separately and reserve their pending capacity.
`Publisher::from_route` reuses a template-prepared Publish route. The
[stable-native guide](src/guide/zero_copy.md) covers loan initialization, receive
leases, response correlation, method filters, checked borrowing and runtime warmup.

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

## License

The crate is published under the terms of the [Apache License 2.0](LICENSE).

## Contributing

Public API items must have documentation and public types must support `Debug`.
Unsafe blocks need a concrete safety justification, and public unsafe functions
need a `# Safety` contract. The repository enforces these rules with its Rust and
Clippy lint configuration. The minimal notification example now also enables
`protobuf-support`; each example declares the features it uses in Cargo.toml.

Contributions are more than welcome. Please refer to the [Contribution Guide](CONTRIBUTING.md).
