# Frame metadata and representations

`UFrameMetadata` is the shared semantic model for frame carriers. Builders validate
addressing, correlation, priority and TTL rules. Payload presence is a separate
carrier property: `UFrameView::has_payload` must distinguish absence from a present
empty payload, and the carrier checks it against the declared encoding.

```rust
use up_rust::{PayloadEncoding, UFrameMetadata, UUri};
use up_rust::frame::codec::{encode_frame_metadata_fields, decode_frame_metadata_fields};

let topic = UUri::try_from_parts("vehicle", 0x1234, 1, 0x8001)?;
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

`frame_metadata_fields_len` computes the exact field-block length;
`encode_frame_metadata_fields_to` writes it directly into a caller-owned slice.
Short output returns a typed error before mutation and bytes beyond the encoded
extent are unchanged. `encode_frame_metadata_fields_into` appends to an existing
vector, while `encode_frame_metadata_fields` creates an owned vector.

At the selected-wire boundary, `UWireMetadataCodec::encoded_frame_metadata_len`
and `encode_frame_metadata_into` include the wire/family/layout prefix in that
caller-storage operation. `UFrameFieldsMetadataCodec::native_prefix` prepares
the prefix at setup without encoding a probe message; `native_prefix_into` writes
only that prefix to fixed setup storage. Private identities retain their agreed
literal, while registered identities can use their compact representation.

## In-place headers

With `zero-copy-transport`, `TxHeaderParts` borrows URI, token and traceparent
inputs. `header_len` computes a caller-agreed prefix/length/field-block extent;
`write_header_uninit` initializes it directly in existing storage. The writer
does not touch payload bytes or the surrounding alignment gap. `write_header`
accepts initialized output with the same checked semantics.

`parse_header` checks prefix, bounds, presence bits, UTF-8, scalar values and
message-role invariants before applying `HeaderLimits::max_token_len`. `RxHeader`
borrows token/traceparent from that same input. In-place rejection uses Copy
`UFrameHeaderError`, including invalid received authorities. Legacy owned decoders
may allocate their diagnostic strings. The prepared header token limit defaults
to 2048 bytes; explicit zero admits absent/empty tokens. Traceparent is capped at
63 bytes. Those capacities are independent of the fixed ABI's token storage.

Debug output redacts access-token content. The writer reports its initialized
header and token extents for the owning buffer's reclamation policy. Parsing
metadata does not establish payload type, byte validity or physical ownership.

The zero-copy URI's scalar/inline authority layout is checked against `UUriAbi`;
default String storage remains a separate representation.

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
