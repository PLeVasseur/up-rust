# URI storage and received parts

`UUri` owns its authority. With `zero-copy-transport`, that authority uses bounded
inline storage and the URI occupies 136 bytes. Other builds retain `String`
storage and occupy `size_of::<String>() + 8` bytes (32 on a 64-bit target).
Successful part construction and cloning allocate nothing with inline storage.
Serialization still returns an owned string.

The upstream protobuf URI represents the entity type and instance as separate
16-bit identifiers in `ue_type_id` and `ue_instance_id`. The existing Rust parts
constructors and inline ABI retain the packed `u32`: type in the low 16 bits,
instance in the high 16 bits. Protobuf conversion checks each received identifier
before packing; serialization emits the separate fields. The type/instance
accessors and allocation-free storage contract are unchanged. This preserves
the existing implementation, not compatibility with the former protobuf layout.

URI strings use four hexadecimal path segments in the order
`/type/instance/version/resource`, for example `up://vehicle/1234/ABCD/1/8001`.
Each identifier must fit in 16 bits, with instances in `1..=0xFFFF`, the version
in 8 bits, and the resource in 16 bits. Constructors reject a zero instance with
`UUriPartsError::ZeroEntityInstance`; migrate packed default-instance identifiers
from `0x0000_TYPE` to `0x0001_TYPE` explicitly. The previous three-segment
packed-ID string format is rejected; migrate
stored URI strings and configuration values rather than silently reinterpreting
them. Existing `try_from_parts("vehicle", 0xABCD_1234, 1, 0x8001)` callers remain
valid and serialize to the four-segment form. Protobuf is also a changed wire
schema, not a transparent legacy-wire compatibility mode.

Use the typed wire constructor when rejecting received input must not allocate:

```rust
use up_rust::{UUri, UUriPartsError};

let uri = UUri::try_from_wire_parts("vehicle", 0x0001_1234, 1, 0x8001)?;
assert_eq!(uri.authority_name(), "vehicle");
assert_eq!(
    UUri::try_from_wire_parts("vehicle/x", 0x0001_1234, 1, 0x8001),
    Err(UUriPartsError::AuthorityHasTrailingComponent),
);
# Ok::<(), UUriPartsError>(())
```

Both authority defects are rejected in every build: IPvFuture literals, and an
authority argument followed by a path, query or fragment. The legacy
`try_from_parts` and `verify_authority` APIs retain their owned validation error
strings. Converting `UUriPartsError` to that legacy error may allocate. A default
build may also allocate its successful `String` authority.

Allocation-free wire validation helpers also include `UCode::from_i32` for defined
status codes and `UUID::is_valid_u64_pair` for the UUID version/variant bits. A
decoder still needs its structural, message-role and representation checks before
forming a typed view; these helpers do not replace them.
