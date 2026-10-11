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

## Explicit operation contracts

[`NativeProfileMode::ContractDefined`](crate::NativeProfileMode::ContractDefined)
uses payload ID 0 with an explicit operation/subscription contract. There is no
automatic fallback from a missing table allocation. Equal-sized values alone do
not establish that contract. Route composition must preserve its agreed identity
and any additional policy required when identity evidence is lost across a hop.

## Separate payload and wire identities

Payload encoding IDs and selected-wire/family/layout identities are different
namespaces. A payload-private range assigns no wire compact IDs. Private wire references need literal identity;
compact-only matching is limited to the registered first-wave SDK pairs. See the
[wire guide](crate::guide::wires) for that compatibility contract. Provisioned
agreement is not remote attestation and is not inferred from received payload bytes.
