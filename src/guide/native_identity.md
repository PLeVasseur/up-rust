# Native identity and deployment agreement

There is one payload-encoding registry. `PayloadEncoding` identifies the payload
encoding, not the enclosing frame format. Public Protobuf ID 2 still needs a
schema contract; a deployment-private ID can specify an agreed native format.
A [`NativeProfile`](crate::NativeProfile) records that encoding and the admitted
native representations. It neither serializes values nor creates a second registry.

Application developers derive `StablePayload` and use native application roles.
Deployment owners provision one profile and the matching peer configuration before
activation. See the [typed guide](crate::guide::typed) for native application
examples.

```text
payload encoding F100 ── agreed native format, shared by different types
native token          ── DoorState or SeatState's agreed complete representation
selected wire         ── encoding of the enclosing frame, not this payload
```

Enable `native-identity` for these APIs. Selected-wire capabilities imply the
feature; message-only default builds do not activate the runtime `sha2` dependency.

A native payload needs more than equal byte size. Its target, complete layout,
field semantics and peer agreement determine whether a representation is the
expected one. Payload validity, alignment and backing-storage lifetime are separate
checks; a matching short token does not authorize an unchecked reference.

## Target-qualified descriptors and tokens

[`NativeTarget`](crate::NativeTarget) records the compiler target triple
and pointer width. [`NativeRepresentation`](crate::NativeRepresentation) includes
that target, a deployment-stable name/revision, byte order, size/alignment and
ordered nested fields with offsets and repetition counts. Constructors reject
empty or duplicate names, overlap, arithmetic overflow, invalid layout and
mixed-target fields.

Canonical descriptor version 2 includes target identity. Its token is the first
four SHA-256 bytes interpreted little-endian, under token-derivation version 2.
The full descriptor remains authoritative; profiles reject short-token collisions.
These formats/derivations are provisional and versioned pending the dedicated
wire/native-profile specification. They do not allocate global registry entries.
Version 1 descriptors are rejected rather than silently interpreted as version 2.

The checked canonical decoder bounds lengths/counts by remaining input, caps
input at `MAX_NATIVE_DESCRIPTOR_BYTES` and nesting at 128, then reapplies layout
validation. Exported data is target-specific: another target can legitimately have
a different descriptor even when scalar sizes happen to match. An imported target
description is metadata, not proof that the data can be borrowed on this host.

Native profiles in descriptor version 2 are single-target. Different compiler
triples cannot agree the same native profile even when particular layouts happen
to coincide. Pointer width, scalar alignment and field semantics remain part of
the full qualification. Use a serialized representation for mixed-target routes
instead of relabelling native descriptors.

`usize` and `isize` are stable fields only within that target qualification. Prefer
fixed-width integers for types mirrored by another language. Their widths are not
portable, and a target-qualified descriptor does not authorize borrowing bytes
that use a different ABI.

```rust
use up_rust::{NativeByteOrder, NativeRepresentation, NativeTarget};

let descriptor = NativeRepresentation::new(
    "example.DoorState", 1, 4, 4, NativeByteOrder::native(), vec![],
)?;
assert_eq!(descriptor.target(), &NativeTarget::current());
let restored = NativeRepresentation::from_canonical_bytes(&descriptor.canonical_bytes())?;
assert_eq!(descriptor, restored);
# Ok::<(), up_rust::NativeProfileError>(())
```

## Stable payloads and borrowing

`zero-copy-transport` enables `StablePayload` and `StablePayloadInit` derives,
stable-container payload codecs and receive/transmit loan contracts. A stable
layout needs `repr(C)` or `repr(transparent)`, explicit padding and no drop glue.
`#[stable_payload(type_name = "...", revision = N)]` names the descriptor; a crate
that renames the SDK dependency adds `crate = "path"`, and both derives then emit
that path instead of `::up_rust`.
The derive's field validators reject invalid bool and char representations;
typed borrowing additionally checks the descriptor, exact size and alignment.
An immutable agreement resolves the complete type separately from these memory
checks. Bindings retain the backing storage for every outstanding lease.
The in-memory transmit helpers return `ResourceExhausted` for impossible sizes
or allocation failure; bindings define their own physical allocation policy.

```rust
# #[cfg(feature = "zero-copy-transport")]
# {
use std::mem::MaybeUninit;
use up_rust::{StablePayload, StablePayloadInit};
#[repr(C)]
#[derive(Debug, StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.State", revision = 1)]
struct State { value: u32 }
#[repr(align(8))]
struct Storage([MaybeUninit<u8>; 4]);
let mut storage = Storage([MaybeUninit::uninit(); 4]);
let state = State::init(&mut storage.0)?.value(42).finish();
assert_eq!(state.value, 42);
# }
# Ok::<(), up_rust::UWireError>(())
```

## Linked inventory

Enable `native-inventory` to register concrete application types for target-side
descriptor export. Generic types use `register_native_type!(ConcreteInstantiation)`.
The defining crates must be linked into the exporter; a Cargo dependency alone
does not make an unlinked crate discoverable.

```rust
# #[cfg(feature = "native-inventory")]
# {
use up_rust::{native_inventory::registered_native_types, StablePayload};
#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "example.InventoryState", inventory)]
struct InventoryState { value: u32 }
assert!(registered_native_types()?.iter().any(|value| value.name() == "example.InventoryState"));
# }
# Ok::<(), up_rust::NativeProfileError>(())
```

Enumeration sorts by stable name/revision and rejects duplicate registrations or
callback target/identity disagreement. It constructs descriptors on demand,
neither application values nor a runtime-global payload-ID allocation map.

## Agree native representations once per deployment

[`NativeProfile`](crate::NativeProfile) binds one native payload encoding to a set
of complete representations. The independent structural token remains 32-bit. Peers must agree
on the domain, generation and full content, not just on a convenient compact value.

DoorState and SeatState can have the same size and alignment and still be distinct
application types. Their stable names and full descriptors preserve that distinction:

```rust
use std::sync::Arc;
use up_rust::{NativeByteOrder, NativeProfile, NativeProfileAgreement,
    NativeProfileError, NativeRepresentation, PayloadEncoding};

// The caller supplies independently loaded, provisioned peer configuration.
fn agree_with_peer(peer: &NativeProfile) -> Result<NativeProfileAgreement, NativeProfileError> {
let door = NativeRepresentation::new("example.DoorState", 1, 4, 4,
    NativeByteOrder::native(), vec![])?;
let seat = NativeRepresentation::new("example.SeatState", 1, 4, 4,
    NativeByteOrder::native(), vec![])?;
let local = Arc::new(NativeProfile::new("example.vehicle", 1,
    PayloadEncoding::private_use(0xF100), [door.clone(), seat.clone()])?);
let agreement = NativeProfileAgreement::new(local, peer)?;
let identity = agreement.identity_for(&door)?;
assert_eq!(identity.encoding(), agreement.identity_for(&seat)?.encoding());
assert_ne!(identity.token(), agreement.identity_for(&seat)?.token());
assert_eq!(agreement.verify_representation(identity.encoding(), identity.token(), &seat),
    Err(NativeProfileError::IdentityMismatch));
Ok(agreement)
}
```

The private ID in this example is not a built-in default or a public assignment.
Deployment owners coordinate its meaning; private use is valid for production.
Immutable agreement objects retain their generation for in-flight work. A newer
generation is prepared and checked independently before activating a replacement.

## When the contract explicitly assigns zero

Use the same constructor with `PayloadEncoding::CONTRACT_DEFINED`. Its meaning
must be explicitly agreed by the deployment contract. Zero is not a fallback or
wildcard: distinct representation tokens still identify types.

```rust
use std::sync::Arc;
use up_rust::{NativeByteOrder, NativeProfile, NativeProfileAgreement,
    NativeProfileError, NativeRepresentation, PayloadEncoding};

fn agree_with_peer(peer: &NativeProfile) -> Result<NativeProfileAgreement, NativeProfileError> {
let door = NativeRepresentation::new("example.DoorState", 1, 4, 4,
    NativeByteOrder::native(), vec![])?;
let seat = NativeRepresentation::new("example.SeatState", 1, 4, 4,
    NativeByteOrder::native(), vec![])?;
let local = NativeProfile::new("example.vehicle", 1,
    PayloadEncoding::CONTRACT_DEFINED, [door.clone(), seat.clone()])?;
// Independently supplied peer content must match before activating an endpoint.
let agreement = NativeProfileAgreement::new(Arc::new(local), peer)?;
assert_eq!(agreement.identity_for(&door)?.encoding(), PayloadEncoding::CONTRACT_DEFINED);
assert_eq!(agreement.identity_for(&seat)?.encoding(), PayloadEncoding::CONTRACT_DEFINED);
assert_ne!(agreement.identity_for(&seat)?.token(), agreement.identity_for(&door)?.token());
Ok(agreement)
}
```

Canonical profile version 3 contains one encoding and a token-sorted representation
set; descriptors and tokens use derivation version 2. The provisional deployment
tool rejects artifacts in any other format version; regenerate configuration
rather than relabeling agreement bytes or hashes.

## Native frames and protobuf messages

The lightweight `NativeTypeToken` and metadata codecs preserve all u32 values even
without `native-identity`; this feature gates interpretation and agreement, not
opaque forwarding. Token presence requires encoding presence. Changing the
encoding alone while a token is present rejects; clearing payload identity clears
both fields.

The protobuf `UAttributes` has no native-token field. Converting a native frame to
that message shape fails explicitly rather than silently dropping identity.
Native frame codecs preserve both encoding and token. Serialized/token-free
messages still use `try_project_frame_to_umessage` and
`try_project_umessage_to_frame_metadata` for migration; received encoding IDs
remain opaque. The reverse conversion never invents a native token, even when an
encoding is recognized or a profile contains only one type. Never infer identity
from the expected Rust type, byte size or payload contents.

## Retain the agreed generation

`StableContainerWire` uses an explicitly supplied immutable agreement. Construct
the adapter through `with_native_agreement` or the stable-container convenience
constructor; the ordinary constructor rejects native-required wires at codegen.
Receive views and registered listeners retain their configured generation.
`native_agreement()` exposes it, and checked borrowing rejects a different supplied
generation. Typed TX helpers verify identity while borrowing the configured adapter.
Storage owners and callback quiescence remain separate lifetime contracts.

The `up-native-profile` workspace tool supplies inventory, agreement and deployment
manifest handling. The source checkout's
`tools/up-native-profile/docs/deployment-agreement.md` describes the provisional
files and independently provisioned peer expectations.
Applications check required linked descriptors and private wire declarations before
activation. SDK agreements neither discover peers nor attest a remote producer.

## Separate payload and wire identities

Payload encoding IDs and selected-wire/family/layout identities are different
namespaces. A payload-private range assigns no wire compact IDs. Private wire references need literal identity;
compact-only matching is limited to the registered first-wave SDK pairs. See the
[wire guide](crate::guide::wires) for that compatibility contract. Provisioned
agreement is not remote attestation and is not inferred from received payload bytes.
