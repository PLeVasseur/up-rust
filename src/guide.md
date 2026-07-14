# Architecture and walkthroughs

This guide connects the public entry points. Start with the audience that
matches the code you are writing.

## Application author

1. Build a `UMessage` with `UMessageBuilder`, or use a role from
   `communication` when its feature is enabled.
2. Send through a configured `UTransport`; applications do not implement
   transports or choose wire identities per message.
3. For selected-wire stable payloads, use the typed helpers on the configured
   adapter. The selected wire and metadata profile were fixed when the link was
   constructed.
4. Verify ordinary role behavior with the local transport examples. Verify
   cross-transport behavior with the Streamer transport matrix.

## Physical transport author

There are two implementation seams:

```text
semantic native frame                  encoded selected-wire core
UZeroCopyTransportImpl                 UZeroCopyTransportCore
validated UFrameMetadata               encoded metadata bytes
direct family API                      UWireTransport composes wire + codec
```

Use the semantic seam when the transport API itself exposes the library's
native frame model. Use the encoded core when the physical technology should
remain ignorant of metadata and payload codecs. Product transports may expose
both when they support both use cases, but documentation must not describe one
as the other.

The zero-copy lifecycle is mechanical: validate a requested layout, loan TX
storage, commit it, deliver RX storage behind an immutable lease, and reclaim it
after release. The generic layer owns wire identity, decoding, validation, and
typed initialization. The governing requirements are in
`up-spec/up-l1/transport_families.adoc`.

### Minimum transport-family contract

A binding declares classic, owned-frame, and behavioral zero-copy support
independently. Owned-frame support does not imply loan-backed storage. A
behavioral zero-copy claim means the documented operation uses transport-owned
TX loans and RX leases; it does not by itself mean native shared memory, typed
borrowing, or end-to-end no-copy. Unsupported families fail explicitly rather
than hanging or degrading to a different family.

For classic and owned carriage, preserve three payload states: absent, present
with zero bytes, and present with nonempty bytes. Presence is not inferred from
length. Encoded metadata and payload regions are opaque to the physical core.
If a technology mirrors routing fields in native headers, validate those fields
against decoded metadata before callback or forwarding; route from validated
source/sink sideband rather than decoding transport-specific meaning from
payload bytes.

### Listener lifecycle and readiness

Registration completes only when the binding can state what readiness means for
that technology. Discovery-backed transports should expose a bounded peer or
subscription readiness result. They must not use duplicate application sends as
a readiness protocol. An absent peer returns a bounded, observable status.

Listener dispatch preserves the binding's documented ordering and is bounded by
an explicit queue, worker, or backpressure policy. Successful unregister is a
quiescence boundary: a callback already running may complete if the binding says
so, but no later callback may begin. Cancellation wakes blocking receive/poll
operations, workers expose health, shutdown joins them, and native entities are
deleted deterministically so a transport can be recreated in the same domain.

Polling is an acceptable native fallback when its wait is wakeable or bounded,
each iteration bounds take and dispatch work, failures become health/status
signals, and drop cannot leave an unjoined worker.

## Wire author

1. Write down the governing representation/version, supported subset, byte
   order, framing markers, options, padding, canonical-byte and complete-input
   rules before implementing the codec.
2. Implement `PayloadCodec`, `EncodePayload`, `DecodePayload`, and, when ordered
   reader input is supported, `ReadDecodePayload`.
3. Define selected-wire and payload-family identities. Compact codes are unique
   within their own namespace. Codes in the experimental range remain
   literal-labeled and must not be described as registered or stable.
4. Implement `UWire` on a marker and associate each payload type through
   `UWirePayload<T>`. The associated codec can be a separate type; it is not
   required to be the marker.
5. Choose a documented metadata profile. `NativePrefixFrameMetadataCodec` is
   the canonical field-block profile. Free `with_*_native_prefix` constructors
   are ecosystem conveniences, not requirements on `UWire`.
6. Run wrong-wire, wrong-payload-family, unknown-layout, malformed-envelope,
   exact/short/overlong/oversized-reader, independent-vector, trailing-byte,
   golden, and round-trip tests.

`PayloadFormat` already receives a blanket `PayloadCodec` implementation. Do
not add a second `PayloadCodec` implementation for the same format marker; Rust
will correctly reject the overlap with E0119. Put encode/decode capability on
the existing marker or use a distinct codec type and select it through
`UWirePayload<T>`.

The external XCDRv2, Arrow, and OMGIDL crates demonstrate different profiles
without adding transport-specific codec code. Their implementation details do
not override the trait contracts. In particular, a `ReadDecodePayload`
implementation allocates only after applying a finite limit, consumes exactly
the declared length, distinguishes early EOF, probes one additional byte to
reject overlong input, and returns errors rather than panicking.

`payload_layout` is a measurement operation. A variable-length codec may need a
probe serialization or traversal and may do equivalent work again during
`encode_payload`. Benchmark these as separate phases; never label layout probing
as zero-cost.

### Feature and import doors

Use one role feature rather than relying on feature unification from a larger
workspace:

```text
ordinary selected-wire user: selected-wire-user-api
wire author:                 wire-implementer-api
physical transport author:   transport-implementer-api
typed uninitialized TX user: selected-wire-user-api + zero-copy-uninit
communication roles:         communication
```

For a local checkout, the corresponding minimal dependency entries are:

```text
user:      up-rust = { path = "../up-rust", default-features = false, features = ["selected-wire-user-api"] }
wire:      up-rust = { path = "../up-rust", default-features = false, features = ["wire-implementer-api"] }
transport: up-rust = { path = "../up-rust", default-features = false, features = ["transport-implementer-api"] }
```

For a remote dependency, replace `path` with the approved repository and exact
revision without changing the feature list. A stable version may be used only
after that version and its identity policy are actually published.

An ordinary selected-wire user imports configured adapter/user traits from the
crate root and does not import core implementation traits. A wire author imports
wire identities, marker/mapping traits, metadata-codec traits, and payload codec
traits. A transport author imports encoded core traits and physical buffer/lease
contracts. Compile each recipe in an independent crate with default features
disabled; a recipe that works only because another workspace member enables a
feature is invalid.

### Error categories

Public boundaries preserve stable categories even when detailed internal errors
differ:

| Failure | Public status category |
| --- | --- |
| malformed caller input or selected-wire payload | invalid argument |
| unsupported family, wire, profile, or operation | unimplemented |
| unavailable backend, peer, or discovery state | unavailable or not found, as documented by the binding |
| bounded queue, loan, listener, or storage exhaustion | resource exhausted |
| violated lifecycle invariant or unexpected implementation failure | failed precondition or internal |

`UWireError` provides codec diagnostics and currently converts to invalid
argument at generic wire boundaries. A transport must not flatten backend,
exhaustion, or lifecycle statuses into codec errors. Tests inject each public
category at the boundary where it is promised.

### Manual selected-wire TX sequence

Prefer typed selected-wire helpers. A low-level initialized-loan escape hatch
must perform the same steps, in order:

1. Compute `C::payload_layout(&value)` and retain its exact length/alignment.
2. Stamp metadata with `metadata.with_payload_encoding(C::payload_encoding())?`.
3. Create `UTxLoanSpec::payload(metadata, layout.len(), layout.align())?`.
4. Await `transport.loan_tx(spec)`.
5. Call `verify_tx_buffer_payload_layout` before exposing storage to the codec.
6. Encode exactly into `buffer.payload_mut()` and propagate the codec error.
7. Commit only through `transport.send_zero_copy(buffer).await`.

Do not commit after a missing encoding, undersized buffer, alignment mismatch,
partial initialization, or failed codec write. Computing a layout and writing
directly into a loan avoids an intermediate owned payload in this helper, but it
does not prove the codec itself performs no copies or that the route is
end-to-end no-copy.

## Stable-payload author

1. Define an explicit `#[repr(C)]` layout with no implicit padding.
2. Derive `StablePayload`, and derive `ByteBackedStablePayload` only when every
   byte pattern is valid for every field.
3. Derive `StablePayloadInit` to obtain the typestate initializer.
4. Initialize every field and explicit padding slot, call `finish()`, and return
   its completion proof from the higher-ranked initialization closure.

The proof constructor is private. Typestate makes incomplete completion
unavailable, and the higher-ranked closure prevents a proof tied to one call
from escaping or being substituted into another. Field writes use the
centralized checked write kernels; unsafe codec/borrow contracts remain
separate expert boundaries.

## How to know the implementation is done

- application roles: local transport examples and role tests;
- semantic zero-copy: `InMemoryZeroCopyTransport` and payload-contract tests;
- encoded cores and wires: wire adapter conformance and golden tests;
- stable layouts: derive UI tests, Miri, exact-byte fixtures, and selected-wire
  round trips;
- cross-transport composition: the Streamer endpoint-profile matrix.

The matrix denominator is derived from independent source profiles, sink
profiles, roles, and wire identities. Each generated row is either supported and
passing or has a predeclared structural unsupported reason. Missing rows and
unknown reasons are failures; retries and duplicate sends are not documentation
substitutes for deterministic readiness.
