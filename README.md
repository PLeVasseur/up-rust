# Eclipse uProtocol Rust library

This is the [uProtocol v1.6.0-alpha.7 Language Library](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/languages.adoc) for the Rust programming language.

The crate can be used to

* implement native, serializer-neutral uProtocol frame producers and consumers.
* implement support for transport protocols using owned-buffer or zero-copy frame APIs.

## Using the Crate
<!--
`uman~up-language-using~1`
Covers:
- req~up-language-documentation~
-->
The crate needs to be added to the `[dependencies]` section of the `Cargo.toml` file:

```toml
[dependencies]
up-rust = { version = "0.10" }
```

Please refer to the [examples](./examples/) and [payload codec notes](./docs/payload-codecs.md) for owned-buffer, zero-copy, and stable-container payload-codec usage.
The [native frame migration guide](./docs/native-frame-migration.md) explains how to move from generated `UMessage` envelopes to `UOwnedFrame`, `UFrameBuilder`, and serializer-neutral payloads, including a side-by-side owned/zero-copy transport API matrix.

Stable-container payloads use two derive levels: `#[derive(StablePayload)]` for stable identity and RX borrowing, and `#[derive(StablePayload, ByteBackedStablePayload)]` when the type is also used with safe stable-container TX or owned/raw encode. Prefer the checked `ByteBackedStablePayload` derive over manual `unsafe impl up_rust::payload::ByteBackedStablePayload` except for expert FFI/codegen payloads.

The crate root keeps the common native-frame types available for short examples. For larger codebases, prefer the role-focused modules `frame`, `payload`, `frame_wire`, `transport`, `zero_copy`, and `prelude` to make simple application code and advanced transport code easier to separate.

`UFrameBuilder` provides native builder ergonomics for `UOwnedFrame` construction without reintroducing generated message envelopes:

```rust
use up_rust::{UFrameBuilder, UUri};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let topic = UUri::try_from("//my-vehicle/4210/1/B24D")?;
let frame = UFrameBuilder::publish(topic).build_with_raw_payload(vec![0x01, 0x02])?;
assert_eq!(frame.payload_bytes(), &[0x01, 0x02]);
# Ok(())
# }
```

Use `build_with_serializable::<MyPayloadCodec, _>(&value)` to create frames with custom serializer-neutral payloads. Protocol Buffers remain available as optional payload support through the `protobuf-wire` feature. CloudEvents mapping is available through the optional `cloudevents` feature. uSubscription service DTO payloads use the generated `up-core-api` protobuf DTOs when the `protobuf-wire` feature is enabled, while the transport envelope remains a native frame.

Transport trait defaults match the mainline `UTransport` shape: optional receive/listener methods return `UNIMPLEMENTED` unless a transport implements them. Push-oriented transports should implement listener registration; transports that support true pull receive should implement `receive_owned` directly. Zero-copy transmit metadata is fixed when a loan is reserved, leaving the payload bytes as the mutable zero-copy surface. Routing code that needs one owned-frame facade over owned and zero-copy transports can use `transport::UOwnedFrameEndpoint`; wrapping a zero-copy transport crosses a copy boundary and is not end-to-end zero-copy forwarding.

Payload presence is explicit: `build()` creates a frame with no payload and no `PayloadEncoding`, while `build_with_raw_payload(Vec::new())` creates a present empty raw payload with standard raw-byte encoding metadata.

Use `UFrameBuilder` or `UFrameMetadata::try_*` constructors for checked metadata construction. The shorter `UFrameMetadata::{publish, notification, request, response}` constructors are unchecked convenience helpers for tests, adapters, and cases that validate separately. Native domain types such as `UUri` and `UUID` keep their fields private; use constructors and accessors instead of struct literals.

Tests can enable the `test-util` feature for supported `mockall` mocks and in-memory owned/zero-copy transport fakes under `up_rust::test_util`.

## Building from Source
<!--
`uman~up-language-building~1`
Covers:
- req~up-language-documentation~1
-->

First, the repository needs to be cloned using:

```sh
git clone https://github.com/eclipse-uprotocol/up-rust.git
```

No generated schema build step is required for the default native frame API. Enabling `protobuf-wire` runs build-script code generation for the checked-in `up-spec/up-core-api` protobuf definitions using a vendored `protoc` binary.

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

### Verification

Run the focused stable-payload compile-fail tests with:

```sh
cargo test --locked --test stable_payload_trybuild
```

The stable-container Miri runner uses the nightly Miri component and covers the
default stable payload path plus the feature-gated unsafe payload hatches. It
disables Miri isolation because `UFrameMetadata::publish` builds a UUID from
`SystemTime::elapsed`, which uses `clock_gettime`; Miri rejects that clock call
when isolation is enabled.

```sh
scripts/run-miri-stable-container.sh
```

Equivalent explicit command:

```sh
rustup +nightly component add miri
cargo +nightly miri setup
MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test --test stable_container_miri
MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test --features unsafe-stable-payload-init --test stable_container_miri
MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test --features unsafe-uninit-payload-bytes --test stable_container_miri
MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test --features expert-unsafe-payloads --test stable_container_miri
```

## License

The crate is published under the terms of the [Apache License 2.0](LICENSE).

## Contributing

Contributions are more than welcome. Please refer to the [Contribution Guide](CONTRIBUTING.md).
