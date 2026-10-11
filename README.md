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
The canonical field block supports caller-slice output. In-place zero-copy headers
borrow token/traceparent, reject malformed input with fixed-size errors and report
their initialized extents. Header/token storage policies remain distinct from
payload interpretation and binding-owned physical storage.
Selected-wire metadata codecs expose exact-length and caller-slice encoding,
including a setup prefix writer. These reproduce the same canonical metadata
bytes while keeping wire/family/layout identities independent of payload encoding.

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

### Selected-wire identity

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

## Building from Source

With `zero-copy-transport`, `UUri` stores its bounded authority inline rather than
allocating on URI construction/cloning. Default builds retain String storage.
`UUri::try_from_wire_parts` returns Copy `UUriPartsError` diagnostics; both forms
reject IPvFuture hosts and trailing authority components. The [URI guide](src/guide/uris.md)
explains layout, UTF-8 bounds and legacy conversion behavior.
`UUID::is_valid_u64_pair` and `UCode::from_i32` provide fixed-size scalar validation
without constructing error strings.
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
