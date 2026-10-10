# URI storage and received parts

`UUri` owns its authority. With `zero-copy-transport`, that authority uses bounded
inline storage and the URI occupies 136 bytes. Other builds retain `String`
storage and occupy `size_of::<String>() + 8` bytes (32 on a 64-bit target).
Successful part construction and cloning allocate nothing with inline storage.
Serialization still returns an owned string.

Use the typed wire constructor when rejecting received input must not allocate:

```rust
use up_rust::{UUri, UUriPartsError};

let uri = UUri::try_from_wire_parts("vehicle", 0x1234, 1, 0x8001)?;
assert_eq!(uri.authority_name(), "vehicle");
assert_eq!(
    UUri::try_from_wire_parts("vehicle/x", 0x1234, 1, 0x8001),
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
