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

Please refer to the [examples](./examples/) for owned-buffer and zero-copy wire-format usage.
The [native frame migration guide](./docs/native-frame-migration.md) explains how to move from generated `UMessage` envelopes to `UOwnedFrame`, `UMessageBuilder`, and serializer-neutral payloads.

`UMessageBuilder` provides native builder ergonomics for `UOwnedFrame` construction without reintroducing generated message envelopes:

```rust
use up_rust::{UMessageBuilder, UUri};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let topic = UUri::try_from("//my-vehicle/4210/1/B24D")?;
let frame = UMessageBuilder::publish(topic).build_with_raw_payload(vec![0x01, 0x02])?;
assert_eq!(frame.payload_bytes(), &[0x01, 0x02]);
# Ok(())
# }
```

Use `build_with_serializable::<MyWireFormat, _>(&value)` to create frames with custom serializer-neutral payloads. Protocol Buffers remain available as optional payload support through the `protobuf-wire` feature. uSubscription service DTO payloads use the generated `up-core-api` protobuf DTOs when the `protobuf-wire` feature is enabled, while the transport envelope remains a native frame.

Transport trait defaults match the mainline `UTransport` shape: optional receive/listener methods return `UNIMPLEMENTED` unless a transport implements them. Push-oriented transports should implement listener registration; transports that support true pull receive should implement `receive_owned` directly. Routing code that needs to work with owned and zero-copy endpoints can use `UTransportEndpoint` to wrap `UOwnedTransport` or true `UZeroCopyTransport` implementations.

Use `UMessageBuilder` or `UFrameMetadata::try_*` constructors for checked metadata construction. The shorter `UFrameMetadata::{publish, notification, request, response}` constructors are unchecked convenience helpers for tests, adapters, and cases that validate separately. Native domain types such as `UUri` and `UUID` keep their fields private; use constructors and accessors instead of struct literals.

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

## License

The crate is published under the terms of the [Apache License 2.0](LICENSE).

## Contributing

Contributions are more than welcome. Please refer to the [Contribution Guide](CONTRIBUTING.md).
