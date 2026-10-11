# Adding a wire format

A selected wire defines wire/family identities and payload codec associations.
The metadata codec owns the metadata layout. Transport adapters compose those
contracts with the binding's storage and delivery operations.
Enable `wire-implementer-api`, which includes `native-identity`, for these typed
codec contracts. Ordinary message and opaque frame carriage do not need it.
The example below defines the identities and a codec for a little-endian `u32`.

## Why three traits?

The traits have separate responsibilities:

* `UWire` declares wire and payload-family identities and the format version
  used for compatibility checks. The metadata byte layout is declared by the
  metadata codec (`UWireMetadataCodec::METADATA_LAYOUT_ID`), so one wire can be
  carried under different metadata profiles without changing the wire.
* [`PayloadCodec`](crate::PayloadCodec) identifies the encoding produced by
  a codec implementation.
* `UWirePayload<T>` selects the codec for payload type `T`. One wire can use
  different codecs for different payload types. The associated codec type also
  lets generic code require matching codecs when necessary.

A marker can implement the codec itself and select `Codec = Self` through
`UWirePayload<T>`. The [`bind_wire_self_codec!`](crate::bind_wire_self_codec)
macro generates that association. A blanket implementation would prevent a
wire from selecting a separate codec for another payload type.

This example encodes and decodes a `u32` in memory:

```rust
# #[cfg(feature = "wire-implementer-api")]
# fn main() -> Result<(), up_rust::UWireError> {
use up_rust::{DecodePayload, EncodePayload, PayloadCodec, PayloadLayout,
    PayloadEncoding, UWire, UWireError, WireIdentity};

struct DemoU32Wire;

impl UWire for DemoU32Wire {
    // Provisional private aliases: the agreed literal is carried and checked.
    const WIRE_ID: WireIdentity = WireIdentity::new("guide.demo.u32-le", 0x8042);
    const PAYLOAD_FAMILY_ID: WireIdentity = WireIdentity::new("guide.demo.u32", 0x8042);
    const FORMAT_VERSION: u16 = 1;
}

impl PayloadCodec for DemoU32Wire {
    fn codec_name() -> &'static str { "demo-u32-le" }
    fn payload_identity(_agreement: Option<&up_rust::NativeProfileAgreement>) -> Result<up_rust::PayloadIdentity, UWireError> {
        Ok(up_rust::PayloadIdentity::Fixed(PayloadEncoding::RAW))
    }
}

impl EncodePayload<u32> for DemoU32Wire {
    fn payload_layout(_value: &u32) -> Result<PayloadLayout, UWireError> {
        PayloadLayout::new(4, 4) // fixed size, four-byte alignment
    }
    fn encode_payload(value: &u32, dst: &mut [u8]) -> Result<(), UWireError> {
        let available = dst.len();
        let output = dst.get_mut(..4)
            .ok_or_else(|| UWireError::buffer_too_small(4, available))?;
        output.copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
}

impl<'a> DecodePayload<'a, u32> for DemoU32Wire {
    fn decode_payload(src: &'a [u8]) -> Result<u32, UWireError> {
        let bytes: [u8; 4] = src.try_into().map_err(|_| {
            UWireError::invalid_payload("demo-u32-le payload must be exactly 4 bytes")
        })?;
        Ok(u32::from_le_bytes(bytes))
    }
}

// Round-trip: measure, encode in place, decode.
let value: u32 = 0xDEAD_BEEF;
let layout = DemoU32Wire::payload_layout(&value)?;
let mut buf = vec![0u8; layout.len()];
DemoU32Wire::encode_payload(&value, &mut buf)?;
assert_eq!(DemoU32Wire::decode_payload(&buf)?, value);
assert_eq!(DemoU32Wire::WIRE_ID.compact_id(), 0x8042); // private alias
# Ok::<(), up_rust::UWireError>(())
# }
# #[cfg(not(feature = "wire-implementer-api"))]
# fn main() {}
```

Implementation steps:

1. Write down the governing representation/version, supported subset, byte
   order, framing markers, options, padding, canonical-byte and complete-input
   rules before implementing the codec.
2. Implement `PayloadCodec`, `EncodePayload`, `DecodePayload`, and, when ordered
   reader input is supported, `ReadDecodePayload`.
3. Define selected-wire and payload-family identities and record them in the
   deployment agreement. Production-private formats can be stable, long-lived
   contracts. Their compact aliases are not global assignments and cannot match
   without the agreed literal identity.
4. Implement `UWire` on a marker and associate each payload type through
   `UWirePayload<T>`. The associated codec can be a separate type; it is not
   required to be the marker.
5. Choose a documented metadata codec. `UFrameFieldsMetadataCodec` owns the
   field-block layout identity.
6. Run wrong-wire, wrong-payload-family, unknown-layout, malformed-envelope,
   exact/short/overlong/oversized-reader, independent-vector, trailing-byte,
   golden, and round-trip tests.

`PayloadCodecIdentity` already receives a blanket `PayloadCodec` implementation. Do
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
Pass an explicit `PayloadDecodeLimit`; it has no default or implicit integer
conversion. This bounds encoded input, not the decoded representation's allocations.

`payload_layout` is a measurement operation. A variable-length codec may need a
probe serialization or traversal and may do equivalent work again during
`encode_payload`. Benchmark these as separate phases; never label layout probing
as zero-cost.

## Why identities are strict

The adapter checks the `UPWM` prefix before metadata or payload exposure. Only
the registered first-wave literal/compact pairs use compact-only references:
native, protobuf and XCDRv2 wire/family IDs, and the field-block metadata layout.
The unsupported compact metadata value 1 is reserved, not registered.
Registration is specific to the wire, family or layout position;
choosing a low number for a different literal does not register it.
Agreement tooling can inspect the same namespace-specific pairs through
`WireIdentityNamespace::registered_identities` at the crate root; it must compare both the
literal and compact value, rather than trusting a declaration's registration flag.

Private wire/family/layout identities require literal matching. The metadata
encoder writes their literals, including for the provisional private alias band
`0x8000..=0xFFFE`. Two vendors can choose the same alias; different literals then
fail before the metadata body is parsed. Compatibility callbacks cannot bypass
the compact-only restriction. Any allowed literal aliases must be explicitly
defined by the configured wire's deployment agreement.

Payload encoding IDs (including normative private payload use in `0xF000..=0xFFFF`)
are a separate namespace. Their partition does not allocate wire IDs. Both kinds
of private identity belong in the same deployment agreement artifact, with their
namespaces, owners and versions recorded explicitly. The endpoint's selected
wire/context supplies the expected identity; equality does not establish remote
attestation or discover a peer's agreement automatically.

The README provisional-interface policy applies. A changed incompatible wire or
family gets a distinct identity; unsupported versions/layouts are rejected.

Enable `wire-implementer-api` for the complete public authoring surface instead
of relying on feature unification from a larger workspace.
