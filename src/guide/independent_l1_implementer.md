# L1 wire and transport implementer

Enable the single `transport-implementer` feature for a binding: it turns on the
implementer contracts (`transport-implementer-api`), owned-frame and native-loan
families and selected-wire adapters. Compose the parts only if you need less. See
[implementing a transport](crate::guide::transports) for which trait to implement
and how to serve receive roles. The serialized send future's size bound is checked
when the generic is instantiated during codegen (`cargo build`), not by `cargo check`,
so build a binding in CI rather than only checking it.

Start with `examples/support/independent_layers.rs`: `selected_wire` composes a
real `UOwnedTransportCore`, a metadata codec, and independently provisioned wire
declarations. `CountedCarriage` is an actual `UOwnedTransportImpl` decorator;
it delegates the exact attachment owner instead of inventing a successful detach.
It is not an example physical middleware driver.

```rust,ignore
let carriage = selected_wire(core, envelope_codec, local_wire, &peer_wire)?;
```

The arguments are actual core/envelope objects and agreement data, not payload
codec markers. `OwnedWireContext::agree` checks wire, payload family, metadata
layout and version. Do not substitute local declarations for an independently
provisioned peer declaration. Schema agreement is still a separate obligation.

- Payload mapping implements `EncodePayload<Domain>`/`DecodePayload<Domain>`;
  see `ReadingProtobuf` and `Json` in `independent_schema.rs`.
- Full-wire metadata implements `UWireMetadataCodec`: layout identity, exact
  encoded length, bounded encode-into, and validated decode. Payload JSON is NOT
  a full-wire envelope. `OwnedWireTransport` performs their shared composition.
- Physical `UOwnedTransportCore` owns encoded storage, delivery and exact listener
  removal. `UOwnedTransportImpl` is the validated semantic-frame adapter surface.
  The sealed `UOwnedTransport` blanket implementation owns public validation.
  New implementations can instead write plain `async fn` methods for
  `serialized_dispatch::StaticOwnedCore` or `StaticOwnedTransport`, without
  `#[async_trait]`, wrapped in `InlineOwnedCore` or `InlineOwnedTransport`; the
  SDK then stores send futures inline, while `#[async_trait]` impls keep a boxed send.
- `attach_validated_owned_listener` must leave no registration on error OR
  cancellation. Preserve the real synchronous release capability; refuse scoped
  operation if the backend only supports incompatible asynchronous detach.
- Native backends implement `NativeLoanCore` with their real route, loan, RX and
  attachment types. Write in TX middleware storage and keep the retained RX owner;
  checked length/alignment/bit-pattern and identity admission remain mandatory.

Advanced: bounded headers, queue policy, codec context and native agreement are
deployment choices. A binding whose middleware owns request/response correlation,
such as SOME/IP sessions, implements `owned_rpc::OwnedRpc` and returns it from
`UOwnedTransportImpl::owned_rpc`. It then owns the exact record of every reserved
call and admitted request: refusals happen before side effects, dropping a call or
an unconsumed reply retires that record, and the SDK never falls back to its
generic listener path. No driver or physical proof is supplied here.
