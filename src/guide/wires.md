# Adding a wire format

A wire format teaches uProtocol to carry one payload encoding — CDR,
Arrow IPC, your in-house format — over every capable transport at once.
The whole job is one crate: a marker type, a codec, and identity
constants. In outline:

the runnable demo below is the complete outline — a marker
type carrying identity constants, a codec that measures then writes in
place, and the association binding payload types to that codec.

## Why three traits?

The split is load-bearing, and the demo shows each part earning its
keep:

* [`UWire`](crate::UWire) is the wire's **identity** — the constants two
  peers negotiate on ("are these bytes even mine to decode?"). One
  identity, forever.
* [`PayloadCodec`](crate::PayloadCodec) is one **encoding
  implementation** — a name plus the payload encoding it produces.
* `UWirePayload<T>` is the **binding**: "on this wire, type `T` uses
  that codec." It is separate because one wire identity routinely serves
  many payload types — a `LidarWire` can carry a `PointCloud` through a
  fixed-layout codec and a `StatusBlob` through a raw-bytes codec, one
  negotiated identity, two codecs — which a merged trait cannot express.
  Downstream dispatch also relies on the codec being a nameable type:
  generic wire routing constrains on codec equality, which only works
  when the codec is a type, not a method set folded into the marker.

For the common all-in-one case the marker implements everything and
binds `Codec = Self`, as the demo does —
[`bind_wire_self_codec!`](crate::bind_wire_self_codec) writes those
binding lines for you. (A trait-level default or blanket impl was
considered and rejected: the default form is unstable Rust, and a
blanket impl would forbid, by coherence, a wire binding a different
codec for some payload type — the case this split exists to support.)

Before the checklist, a demo wire **running end to end** — identity and
codec — carrying a `u32` little-endian:

```rust
# #[cfg(feature = "wire-implementer-api")]
# fn main() -> Result<(), up_rust::payload::UWireError> {
use up_rust::payload::codec::{DecodePayload, EncodePayload, PayloadCodec, PayloadLayout};
use up_rust::payload::UWireError;
use up_rust::{PayloadEncoding, UWire, WireIdentity};

struct DemoU32Wire;

impl UWire for DemoU32Wire {
    // Experimental compact codes (0x8000..=0xFFFE): literal-labeled, unregistered.
    const WIRE_ID: WireIdentity = WireIdentity::new("guide.demo.u32-le", 0x8042);
    const PAYLOAD_FAMILY_ID: WireIdentity = WireIdentity::new("guide.demo.u32", 0x8042);
    const METADATA_LAYOUT_ID: WireIdentity =
        WireIdentity::new("uprotocol.native-prefix.v1", 0x0001);
    const FORMAT_VERSION: u16 = 1;
}

impl PayloadCodec for DemoU32Wire {
    fn codec_name() -> &'static str { "demo-u32-le" }
    fn payload_encoding() -> PayloadEncoding { PayloadEncoding::RAW }
}

impl EncodePayload<u32> for DemoU32Wire {
    fn payload_layout(_value: &u32) -> Result<PayloadLayout, UWireError> {
        PayloadLayout::new(4, 4) // fixed size, four-byte alignment
    }
    fn encode_payload(value: &u32, dst: &mut [u8]) -> Result<(), UWireError> {
        dst.copy_from_slice(&value.to_le_bytes());
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
assert_eq!(DemoU32Wire::WIRE_ID.compact_id(), 0x8042); // experimental range
# Ok::<(), up_rust::payload::UWireError>(())
# }
# #[cfg(not(feature = "wire-implementer-api"))]
# fn main() {}
```

The steps, in the order that avoids rework:

1. Write down the governing representation/version, supported subset, byte
   order, framing markers, options, padding, canonical-byte and complete-input
   rules before implementing the codec.
2. Implement `PayloadCodec`, `EncodePayload`, `DecodePayload`, and, when ordered
   reader input is supported, `ReadDecodePayload`.
3. Define selected-wire and payload-family identities. Compact codes are unique
   within their own namespace. Codes in the experimental range must stay
   literal-labeled and must not be described as registered or stable.
4. Implement `UWire` on a marker and associate each payload type through
   `UWirePayload<T>`. The associated codec can be a separate type; it is not
   required to be the marker.
5. Choose a documented metadata profile. `NativePrefixFrameMetadataCodec` is
   the canonical field-block profile.
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

`payload_layout` is a measurement operation. A variable-length codec may need a
probe serialization or traversal and may do equivalent work again during
`encode_payload`. Benchmark these as separate phases; never label layout probing
as zero-cost.

## Why identities are strict

A wire identity is what lets a receiver reject bytes *before* handing
them to the wrong decoder: the adapter checks the `UPWM` prefix first,
so a CDR payload never reaches an Arrow decoder as garbage-in. That only
works if compact codes never collide — hence uniqueness within each
namespace. The experimental range (`0x8000..=0xFFFE`) exists so teams
can develop wires today without a registry; codes there are
literal-labeled precisely because nothing guarantees two projects chose
differently. When uProtocol operates an identity registry, registered
codes become stable and the experimental range stays what it is: a
sandbox.

Enable `wire-implementer-api` for the complete public authoring surface instead
of relying on feature unification from a larger workspace.
