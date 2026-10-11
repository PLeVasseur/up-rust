# Native identity and deployment agreement

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
The full descriptor remains authoritative; tables reject short-token collisions.
These formats/derivations are provisional and versioned pending the dedicated
wire/native-profile specification. They do not allocate global registry entries.
Version 1 descriptors are rejected rather than silently interpreted as version 2.

The checked canonical decoder bounds lengths/counts by remaining input, caps
input at `MAX_NATIVE_DESCRIPTOR_BYTES` and nesting at 128, then reapplies layout
validation. Exported data is target-specific: another target can legitimately have
a different descriptor even when scalar sizes happen to match. An imported target
description is metadata, not proof that the data can be borrowed on this host.

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

## Prefer a private-ID table

[`NativeProfileMode::Table`](crate::NativeProfileMode::Table) is the recommended
deployment choice. It assigns private 16-bit payload encoding IDs to complete
representations. The independent structural token remains 32-bit. Peers must agree
on the domain, generation and full content, not just on a convenient compact value.

DoorState and SeatState can have the same size and alignment and still be distinct
application types. Their stable names and full descriptors preserve that distinction:

```rust
use std::sync::Arc;
use up_rust::{NativeByteOrder, NativeProfile, NativeProfileAgreement,
    NativeProfileError, NativeProfileMode, NativeProfileTable, NativeRepresentation, PayloadEncoding};

// The caller supplies independently loaded, provisioned peer configuration.
fn agree_with_peer(peer: &NativeProfile) -> Result<NativeProfileAgreement, NativeProfileError> {
let door = NativeRepresentation::new("example.DoorState", 1, 4, 4,
    NativeByteOrder::native(), vec![])?;
let seat = NativeRepresentation::new("example.SeatState", 1, 4, 4,
    NativeByteOrder::native(), vec![])?;
let table = NativeProfileTable::new([
    (PayloadEncoding::private_use(0xF101), door.clone()),
    (PayloadEncoding::private_use(0xF102), seat.clone()),
])?;
let local = Arc::new(NativeProfile::new("example.vehicle", 1, NativeProfileMode::Table(table))?);
let agreement = NativeProfileAgreement::new(local, peer)?;
let identity = agreement.identity_for(&door)?;
assert_eq!(agreement.verify_representation(identity.encoding(), identity.token(), &seat),
    Err(NativeProfileError::IdentityMismatch));
Ok(agreement)
}
```

The example uses explicit assignments for clarity. Applications must coordinate
their allocations across the deployment; private use is valid for production.
Immutable agreement objects retain their generation for in-flight work. A newer
generation is prepared and checked independently before activating a replacement.

## Explicit single-type agreement

[`NativeProfileMode::SingleType`](crate::NativeProfileMode::SingleType) uses payload
ID 0 for one exact [`NativeSingleType`](crate::NativeSingleType) representation.
It is deployment-scoped, not bound to a URI, RPC method or subscription. A profile
cannot choose multiple types by operation; prefer Table for that deployment.
There is no automatic fallback from a missing allocation. Equal-sized values do
not establish agreement. Route composition preserves its identity and any explicit
policy required when identity evidence is lost across a hop.

```rust
use std::sync::Arc;
use up_rust::{NativeByteOrder, NativeProfile, NativeProfileAgreement,
    NativeProfileError, NativeProfileMode, NativeRepresentation, NativeSingleType, PayloadEncoding};

fn agree_with_peer(peer: &NativeProfile) -> Result<NativeProfileAgreement, NativeProfileError> {
let door = NativeRepresentation::new("example.DoorState", 1, 4, 4,
    NativeByteOrder::native(), vec![])?;
let seat = NativeRepresentation::new("example.SeatState", 1, 4, 4,
    NativeByteOrder::native(), vec![])?;
let local = NativeProfile::new("example.vehicle", 1,
    NativeProfileMode::SingleType(NativeSingleType::new(door.clone())))?;
// Independently supplied peer content must match before activating an endpoint.
let agreement = NativeProfileAgreement::new(Arc::new(local), peer)?;
assert_eq!(agreement.identity_for(&door)?.encoding(), PayloadEncoding::CONTRACT_DEFINED);
assert!(agreement.identity_for(&seat).is_err());
Ok(agreement)
}
```

Canonical mode tag 3 and JSON `single_type` replace the former operation-labelled
mode. Old `contract_defined` artifacts fail by name; deployments regenerate rather
than alias them. Table bytes and descriptor/token-derivation version 2 are unchanged.

## Carriage and classic projection

The lightweight `NativeTypeToken` and metadata codecs preserve all u32 values even
without `native-identity`; this feature gates interpretation and agreement, not
opaque forwarding. Token presence requires encoding presence. Changing the
encoding alone while a token is present rejects; clearing payload identity clears
both fields.

Use `frame::metadata::try_project_frame_to_umessage_with_native_agreement` to
check the carried ID/token before classic projection. The reverse
`try_project_umessage_to_frame_metadata_with_native_agreement` restores a token
only for an ID in the agreed mapping. Plain projection refuses to discard a token;
unknown classic IDs remain opaque without an invented native identity.

## Retain the agreed generation

`StableContainerWire` uses an explicitly supplied immutable agreement. Construct
the adapter through `with_native_agreement` or the stable-container convenience
constructor; the ordinary constructor rejects native-required wires at codegen.
Receive views and registered listeners retain their configured generation.
`native_agreement()` exposes it, and checked borrowing rejects a different supplied
generation. Typed TX helpers verify identity while borrowing the configured adapter.
Storage owners and callback quiescence remain separate lifetime contracts.

The `up-native-profile` workspace tool supplies inventory, allocation and deployment
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
