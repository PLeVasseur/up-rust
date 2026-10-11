# Provisional deployment agreement

## Ownership and versions

The SDK defines native representations and identity matching. This package defines
the reference JSON artifacts and generation/admission workflow. JSON format 1 is
explicitly provisional. Canonical descriptor and token-derivation versions are 2,
including the actual compiler triple and pointer width. Older or unknown versions
are rejected; no implicit migration reinterprets old canonical bytes.

Artifacts use schema identifiers `uprotocol.native-inventory.provisional`,
`uprotocol.deployment-agreement.provisional` and
`uprotocol.native-deployment.provisional`. Each carries `format_version:1` and
`provisional:true`. These file conventions are not public registry assignments.

Content hashes are SHA-256 over compact UTF-8 JSON with recursively lexicographically
ordered object keys, integer/bool/string encodings from serde_json, array order
retained and the root `content_sha256` field omitted. Hashing first normalizes the
declared model, including explicit empty optional
identity sections; whitespace and input object-property order are not identity.
Generation orders inventory types by name/revision, native assignments by ID, and declaration/membership lists
deterministically. The full content hash covers every declaration, not just the
short native token. File reads have a 16 MiB input bound; descriptors have the SDK's
independent checked bound. File output is also capped at 16 MiB. Output paths must
be new and are fsynced after writing. Inventories, native allocations and each
identity-declaration list are capped at 4096 entries; manifests cap endpoints and
routes at 4096 and each route caps its endpoint membership at 4096.

## Actual target inventory

Applications opt concrete stable types into the SDK's linked inventory. The derive
`#[stable_payload(type_name = "example.DoorState", inventory)]` registers a concrete
type; `register_native_type!(GenericType<u64>)` registers an explicit instantiation.
Link every defining crate into the exporter. Cargo dependency presence alone does
not prove a registration reached the executable. Pass the required name/revision
keys to `InventoryArtifact::from_linked` so omissions fail immediately.

The inventory contains the exporting executable's actual target and complete
canonical descriptor bytes in standard padded Base64, full descriptor hashes and
independent 32-bit tokens. Import verifies each field against the canonical bytes;
changing only the target label or hash cannot turn a host export into a cross export.
Generate and run the exporter for the intended target. Different targets may have
different descriptors even when their layouts happen to coincide. No remote
compiler attestation is claimed by these configuration files.

## Append-only private allocations

`generate` recommends Table mode. Its default F100-F7FF band is overridable within
F000-FFFF. It is an allocation policy for one deployment, not a numeric wire-ID
partition. Explicit seeds can preserve already coordinated F101/F102 assignments.
Seeds are allowed only at genesis and must name actual inventory types.

New identities receive IDs in sorted name/revision order after the prior high-water
mark, skipping explicitly reserved private payload declarations. Old assignments
are never renumbered. Removing a type retires its reservation; adding other types
cannot reuse it. Re-adding the identical name/revision/full descriptor reactivates
the same assignment. A changed layout under an existing name/revision is rejected;
an explicitly new revision is a new identity and receives a new ID. This preserves
IDs through feature toggles without allowing semantic reassignment.

Successors keep domain, target and band, advance exactly one generation, bind the
actual previous content hash and retain every old reservation/descriptor. Duplicate
IDs/type keys, changed descriptors, short-token collisions and exhaustion fail.
`verify --previous` checks the imported successor against the actual prior ledger.
The full current generation is provisioned independently for runtime admission.

The published shape contracts are in `schemas/native-inventory-v1.schema.json`,
`schemas/deployment-agreement-v1.schema.json` and
`schemas/deployment-manifest-v1.schema.json`. Schema validation is separate from
semantic checks such as hashes, canonical descriptor agreement and append-only history.

## Two identity namespaces in one artifact

The `native` section is explicitly `table` or `contract_defined`. Tables contain
private payload encoding assignments. `identities.private_payloads` records other
private payload reservations, such as serialized codec formats, without pretending
they are native layouts. Those IDs cannot overlap native assignments or tombstones.

`identities.wires` is optional and records a separate `wire`, `payload_family` or
`metadata_layout` namespace for each literal, compact alias, owner, declaration
version and matching form. `registered_compact` is accepted only for an exact pair
in the SDK's namespace-specific first-wave list. Every private/unregistered entry
uses `literal`. Two private literals may have the same alias; that does not permit
compact-only matching or make their meanings interchangeable. Previously declared
ownership/meaning reservations cannot be silently removed or redefined in a chain.
Wire literals are limited to 65535 Unicode scalar values, matching JSON Schema's
`maxLength`. A selected codec can impose a lower encoded-byte limit on its carrier.
Before configuring a selected wire, `AgreementArtifact::require_wire` checks its
actual SDK identity pairs. Registered first-wave pairs may be omitted from the
optional list; each private pair requires an explicit literal declaration.

`contract` explicitly creates an ID 0 operation agreement for one inventoried type.
It is never selected because a table lookup failed. Byte size/alignment and a
short token alone do not establish application-type agreement.

## Distribution and activation

An independently provisioned deployment manifest names each endpoint's relative
artifact locator, expected full content hash, target, domain and generation, plus
all endpoints on each route. `profile_root` locates the immutable deployment files.
Absolute/parent-relative paths and symlinks escaping that root are rejected.

`LoadedDeployment::load` reads each declared artifact and checks it against that
endpoint's provisioned expectation. It verifies every route's full content agreement
before admission. A missing peer is not replaced with the local table. Calling
`agreement(route, local, peer)` also rejects self-substitution and endpoints outside
the declared route. The same manifest contract applies to direct app-to-app pairs.

Typed applications additionally call `require_local_types` with the required
application identities before constructing endpoints. This compares actual linked
compiler-target descriptors to active provisioned entries. Opaque routers can
validate a foreign representation target without claiming a local typed borrow.
Payload validity, alignment and backing lifetime remain separate SDK/binding checks.

Keep the loaded generation immutable while frames are in flight. Provision a new
manifest/artifact generation, verify it independently, then activate replacement
routes and drain the old generation. Startup reporting includes manifest hash,
agreement hash, endpoint/route IDs, target, domain and generation. Controllers compare
the independently emitted process receipts with the provisioned expectations;
this is configuration agreement, not a network handshake or cryptographic attestation.

## Diagnostics and commands

The six subcommands are `generate`, `contract`, `verify`, `verify-inventory`,
`manifest` and `admit`; each flag has `--help` text. Success exits 0, an artifact or
I/O operation failure exits 1, and invalid command-line usage exits 2. I/O errors
name the concrete file/directory and preserve the underlying source. Collision
errors identify both type names/revisions and their short token. A successor must
retain its allocation band; changing bands requires a new deployment profile.
Genesis seeds and `--previous` cannot be supplied together.
