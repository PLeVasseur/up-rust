# Migrating from 0.9 to 0.10

## URI storage and rejection

`zero-copy-transport` enables inline authority storage in `UUri`; default builds
retain String. Use `try_from_wire_parts` for allocation-free URI construction and
Copy `UUriPartsError` failures on that feature. Legacy constructors retain their
owned error-string conversion. IPvFuture hosts and authority suffixes containing
a path, query or fragment are rejected in every build, including protobuf source
and sink conversion. `UUID::is_valid_u64_pair` and `UCode::from_i32` support
allocation-free scalar rejection. See the URI guide for storage and layout details.

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

## Exact URI arguments

`ExactUUri` is available at the crate root for APIs requiring a URI with no
wildcard components. Convert an owned or borrowed `UUri` with `TryFrom` and handle
`UUriError::ValidationError` for wildcard input. Existing `UUri` filter APIs keep
their matching semantics; no implicit conversion narrows a wildcard filter.

## Extensible errors and UUID representation

`UUriError`, `ServiceInvocationError`, and `PubSubError` are non-exhaustive.
Downstream matches must retain a fallback arm for future variants rather than
assuming the current list is closed. Named variants keep their meaning.

`UUID::as_u64_pair` returns the most- and least-significant 64-bit words, the inverse
of `UUID::from_u64_pair`, without changing the UUID representation:

```rust
use up_rust::{UUriError, UUID};

fn classify(error: &UUriError) -> &'static str {
    match error {
        UUriError::SerializationError(_) => "syntax",
        UUriError::ValidationError(_) => "validation",
        _ => "other URI error",
    }
}
assert_eq!(classify(&UUriError::ValidationError("wildcard".into())), "validation");
let uuid = UUID::build();
let (most, least) = uuid.as_u64_pair();
assert_eq!(UUID::from_u64_pair(most, least).unwrap(), uuid);
```

## Payload identity and opaque carriage

`UPayloadFormat` is removed. Use the numeric `PayloadEncoding` identity without
equating structural validity with registry assignment or decoder support:

| Previous API | Replacement |
| --- | --- |
| `UPayloadFormat::{ProtobufWrappedInAny, Protobuf, Json, Someip, SomeipTlv, Raw, Text}` | `PayloadEncoding::{PROTOBUF_WRAPPED_IN_ANY, PROTOBUF, JSON, SOMEIP, SOMEIP_TLV, RAW, TEXT}` |
| `UPayloadFormat::Unspecified` for a present contract-defined payload | `PayloadEncoding::CONTRACT_DEFINED`, explicit ID 0; no payload uses `None` instead. |
| `UAttributes::payload_format`, `UMessage::payload_format` | `payload_encoding`, returning `Option<PayloadEncoding>` |
| `payload_format_unchecked` on a message | Prefer checked `payload_encoding`; `payload_encoding_unchecked` still requires presence. |
| `UAttributes::payload_format_unchecked` | Use `payload_encoding` and handle the optional identity explicitly. |
| `UPayload::new(bytes, format)`, `UPayload::payload_format` | `UPayload::new(bytes, encoding)`, `UPayload::payload_encoding` |
| `UMessageBuilder::build_with_payload(bytes, format)` | `build_with_payload(bytes, encoding)` |
| `UAttributesValidators::get_validator`, `get_validator_for`, `get_validator_for_attributes` | `validator`, `validator_for`, `validator_for_attributes` |
| Generated protobuf field `payload_format` | Optional `payload_encoding_id`, field 12, with uint32 wire storage and a 16-bit accepted range |

There is no `IMPLICIT` alias for `CONTRACT_DEFINED`. ID 0 displays as
`0 (contract-defined)` and requires an applicable contract; it is not absence or
an instruction to guess an encoding. `from_id` preserves every structurally valid
16-bit value, including unknown, unassigned, reserved and private identifiers.
`from_registry_entry` checks the assignment range; `private_use` checks the private
range. Neither constructor establishes an allocation or decoding agreement.

Builders reject origination of reserved IDs (`0xE000..=0xEFFF`) with
`UMessageError::AttributesValidationError`. Received/forwarded reserved identities
remain valid opaque carriage. `with_declared_payload_encoding` is the explicit
carrier/decorator operation for existing payload bytes; it does not turn an
absent payload into a present one. Payload and identifier must both be present
or both absent. A zero-length present payload is still present; mismatched
presence produces `UMessageError::PayloadError`.

```rust
use up_rust::{PayloadEncoding, UAttributesError, UMessageBuilder, UMessageError, UUri};

let topic = UUri::try_from_parts("vehicle", 0x1234, 1, 0x8001)?;
let present = UMessageBuilder::publish(topic.clone())
    .build_with_payload(Vec::new(), PayloadEncoding::CONTRACT_DEFINED)?;
assert_eq!(present.payload_encoding(), Some(PayloadEncoding::CONTRACT_DEFINED));
assert!(present.payload().unwrap().is_empty());
let reserved = PayloadEncoding::from_id(0xE000)?;
assert!(matches!(
    UMessageBuilder::publish(topic).build_with_payload("new payload", reserved),
    Err(UMessageError::AttributesValidationError(UAttributesError::ValidationError(_)))
));
let carried = present.with_declared_payload_encoding(reserved)?;
assert_eq!(carried.payload_encoding(), Some(reserved));
carried.validate()?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

CloudEvents output carries payload octets opaquely: binary data in Protobuf Event
Format, and `data_base64` rather than parsed `data` in JSON Event Format. `pformat`
preserves the identifier and the carriage media type is `application/octet-stream`.
JSON consumers must decode `data_base64` before parsing the application JSON, or
apply an explicit application-aware projection. Any present data property without
`pformat` is rejected regardless of its type. Legacy typed input remains a closed
set, not media-type inference or generic payload decoding.

TTL zero now consistently means no expiration (`None`); positive u32 millisecond
values are preserved without clamping. Requests still require a positive TTL.
Bindings with a smaller carrier range must report unrepresentability.
