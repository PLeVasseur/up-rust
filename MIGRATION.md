# Migrating from 0.9 to 0.10

## Feature and accessor names

Update direct feature selections in Cargo.toml and dependent-crate feature
forwarding. The old feature names are removed without aliases:

| Previous feature | Replacement | Capability |
| --- | --- | --- |
| `up-core-types` | `up-core-api` | Generated core-service protobuf APIs and conversions; implies `protobuf-support`. |
| `up-l2-api` | `communication-api` | Communication Layer traits, independently of the default role implementations. |

The hand-written public types use accessor names without `get_`. Generated
protobuf APIs retain their own generator-defined naming conventions.

| Previous accessor | Replacement |
| --- | --- |
| `UStatus::get_code` | `UStatus::code` |
| `UStatus::get_message` | `UStatus::message` |
| `UStatus::get_message_or_default` | `UStatus::message_or_default` |
| `UStatus::get_details` | `UStatus::details` |
| `UAny::get_type_url` | `UAny::type_url` |
| `UAny::get_value` | `UAny::value` |
| `UUID::get_time` | `UUID::time` |

The returned values and serialization are unchanged by these source-level renames.
For example:

```rust
use up_rust::{UAny, UCode, UStatus, UUID};

let detail = UAny::new("example.com/error".to_owned(), b"opaque".as_ref());
let status = UStatus::new(UCode::DataLoss, Some("invalid response"), Some(vec![detail]));
assert_eq!(status.code(), UCode::DataLoss);
assert_eq!(status.message(), Some("invalid response"));
assert_eq!(status.message_or_default("fallback"), "invalid response");
let detail = status.details().first().expect("one detail");
assert_eq!(detail.type_url(), "example.com/error");
assert_eq!(detail.value(), b"opaque");
let _creation_time_millis = UUID::build().time();
```
