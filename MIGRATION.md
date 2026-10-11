# Migrating from 0.9 to 0.10

## URI storage and rejection

`zero-copy-transport` selects inline URI authority storage; default builds retain
`String`. Use `UUri::try_from_wire_parts` and its Copy `UUriPartsError` for received
parts that must reject without allocating. `try_from_parts` retains its owned
validation errors. Every build now rejects IPvFuture authorities and authority
arguments followed by a path, query or fragment. See the URI guide for examples.
`UCode::from_i32` and `UUID::is_valid_u64_pair` provide allocation-free wire-value
checks; their legacy error-returning counterparts remain available.

With `zero-copy-transport`, `TokenBuffer` reserves an explicit byte capacity at
setup and preserves the prior token on an oversized update. It redacts Debug and
wipes owned bytes on overwrite, clear and Drop. `RxHeader` instead borrows received
token bytes; its lifetime is tied to the received slice.
Prepared native receive uses `RxAdmission<P>` and `RxLease<P, F>` over a binding's
retained `Frame`. The generic `ZeroCopyHandler` receives only frames that passed
identity and representation admission. Manual unsafe stable-payload implementations
must also exclude interior mutability; a typed borrow cannot permit writes through
the immutable source bytes.

Prepared native RPC uses `PreparedRpcClient<C, Req, Resp>` and
`PreparedRpcServer<C, Resp>` with explicit pending-call and reply-cache capacities.
Implement `RequestHandler` to handle the retained request asynchronously and then
initialize the response synchronously. Correlation is armed before publication;
request and response types may differ. Cancellation releases the pending slot,
and errors use fixed-size `CallError` values. See the stable-native communication
guide for token, first-contact and status-only response behavior.

The `communication::zero_copy` roles now require `PreparedTransportCore`. Its
`Endpoint` uses an adapter over `Arc<C>`. Choose the payload type, topic/destination,
TTL and capacities when creating a publisher or notifier; `publish_stable` and
`notify_stable` are synchronous and take borrowed `MessageFields`. Replace the old
per-message `CallOptions`/resource arguments with `SendConfig` at creation.
`StableRx<C, P>::payload` returns the checked type selected at registration.
Replace dynamic zero-copy role listeners with generic `ZeroCopyHandler` values;
keep the returned `Receiver` or `Subscription` and stop/unsubscribe it explicitly.
RPC creation selects both request and response types and uses `RequestHandler`
instead of a request-listener facade. Service rollback/teardown failures remain
explicit, and dropping a subscription only closes its local worker.

Opaque prepared receive uses `HeaderAdmission` and `HeaderHandler` rather than
allocating an owned metadata view per callback. `HeaderLease` retains byte storage,
borrows token/traceparent, and exposes no typed payload reference. Its filtered
registration preserves partial source filters and sinkless-only `None` behavior.
`OpaqueTemplate` prepares bounded exact-address routes; `forward` preserves an
admitted payload identity, including reserved values, and refuses wire/family/layout
relabeling. `send_bytes` is the separate originating operation and rejects reserved
payload IDs. Both explicitly copy the opaque payload to the egress loan once.

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

## Frame metadata and payload codec interfaces

Frame carriers use `UFrameMetadata` for semantic metadata and implement
`UFrameView::has_payload` explicitly; an empty byte slice is not sufficient to
determine absence. The metadata builder validates message structure, and metadata
decoders validate raw fields before exposing the decoded value. Projections to
classic attributes reject fractional/overflowing TTL values rather than clamp them.

`PayloadCodec`, `PayloadLayout` and `ReadDecodePayload` separate encoding identity,
byte layout and bounded decoding from transport storage. `UWireError` distinguishes
missing payload/encoding, unsupported encoding, malformed bytes, insufficient output
space and structured `SchemaMismatch` diagnostics. The new
[frame metadata guide](src/guide/frame_metadata.md) contains compile-tested examples.

## Selected-wire API ownership

Wire contracts use crate-root imports under `wire-implementer-api`; there is no
redundant `wire_implementer_api` compatibility module. `UFrameFieldsMetadataCodec`
names the metadata field-block codec and owns layout identity 2. The selected
`UWire` marker owns the wire/family identity and format version, independently
of storage or a whole-frame envelope. `UWire::REQUIRES_NATIVE_AGREEMENT` names
the native proof requirement rather than a locally configured profile.

The whole-frame interface is named `UFrameEnvelope` in the envelope source, which
is compiled only with its owning frame-carrier capability. `UProtocolNativeWire`
and `ProtobufWire` remain selected-wire markers. No wire identity bytes or
metadata-layout value changes accompany these naming choices. Layout compact
value 1 remains an internal SDK reservation with no decoder, not a public constant
or a payload-registry assignment.

## Metadata conformance fixtures

Metadata codec implementations provide `encoded_frame_metadata_len` and
`encode_frame_metadata_into`; the owned `encode_frame_metadata` now has a default
implementation using them. The canonical codec keeps its direct vector writer and
offers an allocation-free output path into caller storage. Field-block users can
use `frame_metadata_fields_len` and `encode_frame_metadata_fields_to`; short output
is a typed error and does not partially overwrite the destination. The native
prefix also has a caller-buffer setup writer, `native_prefix_into`.

Metadata vector manifests are required and bind both the decoded bytes and exact
hex files with SHA-256. Verification decodes the stored vectors. Setting
`UP_GOLDEN_UPDATE` on a normal test run is an error; regeneration requires the
explicitly ignored updater described in the fixtures README. No field-block or
classic-attribute bytes change merely to add these integrity checks.

## Owned frames and listener admission

Enable `owned-frame-transport` for validated owned payload storage and its listener/
transport contracts. Unchecked frames carry `Unvalidated` and must pass `validate`
before they can be sent. Payload encoding and payload presence are checked together.
`InMemoryOwnedTransport` is public only under explicit `test-util`; cfg(test)
does not change the public export surface.

Enable `transport-implementer-api` for `ListenerAdmission`, `NativeCallbackGuard`
and `ListenerQuiesceError`; `util` implies this feature for existing utility users.
`stop` closes callback admission, while `cancel` plus `quiesce` supports bounded
native teardown. A timeout never revokes retained storage. Native callback guards
must stay on their entering thread and are not `Send`.

## Selected-wire transport adapters

The asynchronous `UZeroCopyTransportCore`/`UZeroCopyUninitTransportCore` and public
`UZeroCopyTransportImpl`/extension family are replaced by `PreparedTransportCore`.
Prepare physical addresses and limits once, implement synchronous `loan` and
`publish`, and attach a synchronous `FrameSink` to setup-owned receive state.
`PreparedTxLoanSpec` is replaced by a prepared route plus per-message
`TxHeaderParts`/`TxFrameLayout`; metadata is written directly into storage.
`PreparedTxStorage` implementations are unsafe: document exclusive, disjoint,
stable backing storage and preservation of initialized bytes across all methods.
These are distinct obligations from checked lengths, allocation policy and callback
quiescence, and let the SDK safely construct the initialized-loan proof.

Replace `UZeroCopyListener` and `UEncodedZeroCopyListener` trait objects with
generic `ZeroCopyHandler` or `HeaderHandler` workers. Adapter registration returns
an owned `WireRegistration`; retain it for detachment retry and explicit draining.
Pull-style consumers can attach a preallocated inbox and admit each received frame
through `RxAdmission` or `HeaderAdmission`. A selected wire is still chosen by
`UWireTransport`; use its preparation/registration methods instead of the removed
`USelectedWireZeroCopyTransport` marker. `PayloadInit` replaces the former
`USelectedWireStablePayloadInit` argument on typed sends.

`selected-wire-user-api` supplies adapter construction and receive views;
`transport-implementer-api` supplies encoded-core contracts. Enable the owning
frame family as well. Public imports are canonical at the crate root; the
`selected_wire_user_api` and `transport_implementer_api` compatibility modules are
not provided. Public exports do not change under cfg(test).

`UWireTransport::new` and ordinary convenience constructors accept wires without
a native-agreement requirement. A marker setting `REQUIRES_NATIVE_AGREEMENT` cannot
bypass that requirement through an ordinary constructor. Received metadata must
match wire, family and layout before an owned frame is returned or a listener is
called. Registration wrappers are scoped to the adapter instance.

## Owned communication roles

Owned RPC request listeners no longer keep their transport alive through a registry
cycle. They upgrade a weak sender reference on callback entry and retain it through
the reply. A callback after transport destruction does not invoke the handler.
Endpoint unregistration remains explicit while the transport is live.

`communication::owned` provides `Endpoint` and the Publisher, Subscriber, Notifier,
RpcClient and RpcServer facades over `UOwnedTransport`. Enable `owned-frame-transport`
and `usubscription`; the classic communication traits keep their APIs. Owned
listeners project representable frames to `UMessage` before calling existing
`UListener` handlers. Notification receive uses the notifier's listening methods.
Typed selected-wire operations additionally require the adapter capability.

## Native representation agreement

Enable `native-identity` to use `NativeRepresentation`, `NativeTypeToken` and
`NativeProfileAgreement`; selected-wire capabilities enable it automatically.
Message-only default builds do not activate the runtime SHA-256 dependency.
Native descriptors bind target, layout and field semantics. Their independent
32-bit tokens do not allocate payload IDs or by themselves authorize typed access.
Use a checked private-ID table or an explicit ID-zero contract, with independently
supplied peer configuration. The native identity guide covers generation retention
and canonical descriptor limits.

## Carried native tokens

Frame metadata carries `Option<NativeTypeToken>` independently of the payload
encoding. The opaque type is available at the crate root and in `frame::metadata`
without `native-identity`; the enabled `frame::native` module reexports that same
type. Descriptor/hash/agreement APIs remain opt-in. Presence bit 8 carries all 32
token bits, including zero, and fixed ABI storage is at offset 28.

Plain classic projection rejects token-bearing frames. The `with_native_agreement`
projection variants require the feature, check identity before token removal and
restore only identities known to the agreement. Unrelated unknown IDs stay opaque.
The field-block/fixed-ABI profiles retain token-free bytes, size 928/alignment 8;
peers must coordinate supported profile versions and reject unknown presence bits.

## Stable payloads and typed codec context

Generated array setters additionally expose `<field>_fill_pattern`, which repeats
borrowed stable object representations in place without invoking Clone. Empty
patterns on nonempty arrays return `StablePayloadPatternError::EmptyPattern`.
Manual stable-field implementations must permit plain-data representation
duplication; ownership-sensitive handles and uniqueness-dependent state are
excluded. This is the same copyability needed for serialized stable carriage.

Typed payload codecs now require `native-identity` (implied by selected-wire features).
`PayloadCodec::payload_identity(agreement)` replaces a context-free encoding method;
fixed codecs return `PayloadIdentity::Fixed`, while native codecs require an agreed
complete representation. `verify_metadata` checks encoding and native token together.
Reader decoding and `UFrameView::decode_payload_from_reader_as` require an explicit
`PayloadDecodeLimit`. Ordinary message/protobuf APIs and opaque frame/token carriage
remain independent of descriptor hashing.

Enable `zero-copy-transport` for stable payload derives, loan/lease traits and typed
borrowing. Derives require `repr(C)` or `repr(transparent)`, explicit padding and no
drop glue. Use `revision`/`REVISION` for descriptor revisions. Tuple initializers
use `field_0`, `field_1`, etc.; array setters borrow or fill elements in place.
Checked borrowing validates representation, exact length, alignment and field bits
before returning a reference. The explicitly unsafe lane retains its caller contract.
Enable `native-inventory` only when concrete linked types should be exported.

## Agreement-retaining selected-wire adapters

Use `StableContainerWire` and construct its adapter with
`UWireTransport::with_native_agreement` or `into_stable_container_transport(agreement)`.
Ordinary `new` rejects native-required wires during code generation. The redundant
`with_wire_and_metadata_codec` constructor is removed; ordinary wires use `new`.
`into_parts` includes the optional retained agreement as its fourth element.

`UHasWire::native_agreement` and `UFrameView::native_agreement` expose configured
context; `UWireRx::try_from_encoded_with_native_agreement` retains it with received
storage. A successor generation cannot reinterpret an old retained view. Typed TX
helpers verify identity against the adapter's agreement. The explicit unsafe borrow
lane may omit field-bit validation only under its documented producer proof; target,
representation, size, alignment and metadata checks remain.

The `up-native-profile` workspace tool creates and verifies deployment artifacts.
Its I/O errors retain the failing path and source; CLI exit codes are documented in
the tool README. Default/core SDK checks select the SDK package independently so
tool dependencies cannot conceal missing feature requirements.

## Stable-native communication roles

Enable `up-l2-zero-copy` for the module-qualified Publisher, Notifier, RpcClient and
RpcServer facades; add `usubscription` for Subscriber. `Endpoint` pairs the stable
adapter and URI provider. These roles use `StableContainerWire`, checked receive
leases and generated TX initialization; other wires use the transport API.
Notification receive belongs to Notifier. RPC server register/unregister rejects
non-method resources before calling the core. A response listener sends through
the existing adapter and preserves request correlation.

`encode_frame_metadata_fields_into` appends into caller-owned metadata storage;
it preserves the existing prefix. On failure the appended region may be partial,
so callers must restore their original length before reusing the buffer. The
representation bytes remain identical to standalone encoding.

## Checked message and attribute projections

Replace `UMessage::to_frame_metadata(encoding)` and
`UMessage::to_frame_metadata_unencoded()` with
`frame::metadata::try_project_umessage_to_frame_metadata(&message)`.
The message already carries its optional encoding; the projection checks it
against actual payload presence without an override or separate empty-message path.
Use the `_with_native_agreement` variant to reconstruct native identity through an
explicit deployment agreement.

Replace attribute `to_frame_metadata` methods and the public
`try_project_attributes_to_frame_metadata` function with
`UAttributes::try_project_to_frame_metadata()`. This attribute-only entry cannot
validate payload storage; use the complete-message function whenever available.
Frames without payload keep absent encoding; present-empty payloads keep their
encoding. Opaque IDs and bytes remain unchanged.

The reverse functions `try_project_frame_to_umessage` and its agreement-aware
variant, and `UFrameMetadata::try_project_to_attributes` and its agreement-aware
variant, retain their checked contracts. Fractional/overflowing TTLs still fail
rather than truncate. Wire bytes, canonical metadata layouts and the fixed ABI
are unchanged.

## Shared semantic enums

Replace `FrameMessageKind` with `UMessageType` and `FramePriority` with `UPriority`.
Frame metadata, prepared headers, routes and communication roles now use those
unconditional, protobuf-independent core enums. The four
`from_legacy_type`/`to_legacy_type` and
`from_legacy_priority`/`to_legacy_priority` converters are removed: no conversion
is necessary between identical semantic types.

Use `wire_code()`/`from_wire_code()` for encoded kinds and priorities. Never use
Rust discriminants as wire codes. Absent priority remains `None`, encoded as zero;
reserved kind/priority codes remain invalid. Encoded values, classic API signatures,
metadata goldens and the fixed ABI remain unchanged. Core types gain `Eq`/`Hash`;
priority also gains `Ord`, preserving its existing CS0-through-CS6 ordering.
