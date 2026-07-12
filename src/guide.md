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

## Wire author

1. Implement payload codec traits for the types the wire carries.
2. define and register the wire and payload-family identities;
3. associate types with the wire through `UWirePayload`;
4. choose or implement a metadata codec;
5. run golden, wrong-identity, wrong-family, malformed-envelope, and round-trip
   conformance tests.

The external `up-wire-xcdrv2-rust` crate is the reference for adding a wire
without adding transport-specific code.

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
