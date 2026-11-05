# Frame metadata and representations

`UFrameMetadata` is the shared semantic model for frame carriers. Builders validate
addressing, correlation, priority and TTL rules. Payload presence is a separate
carrier property: `UFrameView::has_payload` must distinguish absence from a present
empty payload, and the carrier checks it against the declared encoding.

```rust
use up_rust::{PayloadEncoding, UFrameMetadata, UUri};
use up_rust::frame::codec::{encode_frame_metadata_fields, decode_frame_metadata_fields};

let topic = UUri::try_from_parts("vehicle", 0x0001_1234, 1, 0x8001)?;
let metadata = UFrameMetadata::publish(topic)
    .with_payload_encoding(PayloadEncoding::RAW)
    .build()?;
let fields = encode_frame_metadata_fields(&metadata)?;
assert_eq!(decode_frame_metadata_fields(&fields)?, metadata);
# Ok::<(), Box<dyn std::error::Error>>(())
```

That block contains metadata, not the payload bytes. The field codec is
variable-length; the opt-in `UFrameMetadataAbiV1` representation is a fixed
928-byte, alignment-8 structure for explicitly agreed cross-language boundaries.
Fixed capacities are representation limits, not limits of the semantic model.
Conversions fail instead of truncating. These SDK-defined representation formats
are provisional; they are not normative native-profile registry assignments.
`encode_frame_metadata_fields_into` appends the same bytes to an existing vector,
preserving its prefix and avoiding an intermediate field vector in an outer codec.
If encoding fails, discard the appended region before reusing that buffer.

For fixed caller-owned storage, use `frame_metadata_fields_len` followed by
`encode_frame_metadata_fields_to`. It produces the identical field block without
allocating output. Short storage returns `UFrameFieldsError::OutputTooShort`
before writing and the bytes after the encoded extent remain unchanged.
At the selected-wire boundary, `UWireMetadataCodec::encoded_frame_metadata_len`
and `encode_frame_metadata_into` provide the same operation including the prefix.
`UFrameFieldsMetadataCodec::native_prefix_into` writes only that prefix into a
caller-owned setup buffer. The owned-vector codec remains available for owned
carriage; its output has the same bytes and independent wire/family/layout IDs.

## In-place zero-copy headers

With `zero-copy-transport`, `TxHeaderParts` borrows URI, token and traceparent
inputs instead of constructing per-message owned strings. `header_len` determines
the required prefix/length/field-block extent, and `write_header_uninit` initializes
that extent in caller-owned storage. `write_header` accepts an initialized slice.
The caller obtains the selected-wire prefix at setup through
`UFrameFieldsMetadataCodec::native_prefix(context)`, which includes the codec's
layout and the selected wire/family identities without encoding a probe message. The underlying
field bytes match the existing canonical codec; payload placement remains with
the frame carrier.

`parse_header` validates the prefix, lengths, UTF-8, scalar values and message-role
invariants before applying `HeaderLimits::max_token_len`. Its `RxHeader` borrows
token and traceparent from that same immutable input. This avoids a separately
allocated string and keeps the view's lifetime tied to the received bytes.
Headers retain the independent native type token and distinguish an absent token
from a present empty token. Debug output reports access-token length, not contents.

`UFrameHeaderError` is a Copy error on success and rejection paths. The zero-copy
header profile caps traceparent at 63 bytes. An owned semantic model or another
representation may have different capacities. The writer returns the token range
so an owner can wipe its own reusable buffer; that range does not grant permission
to mutate shared native storage while another receiver retains it.

These functions validate metadata only. The carrier must still validate payload
presence/length, and typed admission must validate identity, alignment and field
bits before borrowing a payload.

## Reproducible payload fixtures

The opt-in `payload-contract-fixtures` module has six core stable fixtures;
`payload-contract-large-fixtures` adds the LiDAR and camera fixtures. Their complete
sequence-17, little-endian `.bin` representations are stored under
`tests/golden/payload-contract/`. `vectors.json` binds all eight file hashes,
lengths, alignments and type names. The canonical core/full fixture catalogs include
that manifest's SHA-256, so `fixture_manifest_sha256_hex()` also commits to the
stored bytes. Normal tests check the stored data rather than treating regenerated
bytes as the oracle. An explicit ignored updater is documented in the corpus README.

The digest identifies this fixture corpus and enabled catalog, not a payload
encoding assignment or a peer's native agreement. The stored byte order is an
explicit test profile; it does not make native representations portable across
unagreed targets.

LiDAR geometry uses frozen trigonometric coefficient bits, preserving the stored
fixture while avoiding per-call platform-dependent transcendental rounding. The
corpus README documents its separate explicit native-host updater; stored vectors
and digest pins must still verify without silently regenerating them.

## Preserve opaque native tokens

`NativeTypeToken` and its frame field are available without `native-identity`:
forwarding a value does not need descriptor hashing or deployment agreement.
Presence is separate from its numeric value. A token requires a payload encoding.

```rust
use up_rust::{NativeTypeToken, PayloadEncoding, UFrameMetadata, UFrameMetadataError, UUri};
use up_rust::frame::codec::{decode_frame_metadata_fields, encode_frame_metadata_fields};

let topic = UUri::try_from_parts("vehicle", 0x0001_1234, 1, 0x8001)?;
let metadata = UFrameMetadata::publish(topic)
    .with_payload_encoding(PayloadEncoding::CONTRACT_DEFINED)
    .with_native_type_token(NativeTypeToken::from_u32(u32::MAX)).build()?;
let restored = decode_frame_metadata_fields(&encode_frame_metadata_fields(&metadata)?)?;
assert_eq!(restored.native_type_token(), Some(NativeTypeToken::from_u32(u32::MAX)));
assert!(matches!(restored.try_project_to_attributes(),
    Err(UFrameMetadataError::FieldNotRepresentable { field: "native_type_token", .. })));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Plain classic projection rejects token-bearing metadata. Agreement-aware
projections require `native-identity` and verify the carried pair before removing
the token; see [native identity](crate::guide::native_identity). This is distinct
from payload bit-validity, alignment and backing-lifetime checks.

Payload codecs describe encoding identity and byte layout while bindings own
storage. A bounded reader rejects short or overlong input, and impossible declared
lengths return a typed error rather than driving an unbounded allocation/read.
Schema-aware codecs can report structured diagnostics without depending on a
particular schema library in the SDK:

```rust
use up_rust::{UCode, UStatus, UWireError};

let mismatch = UWireError::SchemaMismatch {
    field_index: Some(2),
    expected: "unsigned integer".into(),
    actual: "text".into(),
};
assert!(matches!(&mismatch, UWireError::SchemaMismatch { field_index: Some(2), .. }));
let status = UStatus::from(mismatch);
assert_eq!(status.code(), UCode::InvalidArgument);
```

The field index is zero-based; `None` denotes a schema-wide mismatch, such as a
field-count difference. Expected/actual descriptions are diagnostics, not a new
schema namespace or a substitute for codec-specific validation.
