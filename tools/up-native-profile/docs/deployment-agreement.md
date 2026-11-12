# Provisional deployment agreement

## Ownership, shape and versions

The SDK owns native representation, token and profile matching. This tool owns
reference JSON artifacts and generation/admission, not public registry assignments
or a normative wire format. Package `0.1.0` pins SDK `0.10.0-SNAPSHOT` and remains
unpublished pending an authorized coordinated SDK/macros/tool release.

The artifact schema names are `uprotocol.native-inventory.provisional`,
`uprotocol.deployment-agreement.provisional` and
`uprotocol.native-deployment.provisional`. All require `provisional:true`.
Agreement JSON uses **format_version 2**; inventory and manifest JSON remain
**format_version 1**. SDK canonical profile bytes are version 3; canonical
descriptors and token derivation stay version 2. Unsupported versions fail with
the actual and supported versions. Old agreements must be regenerated from the
actual inventory, not rehashed or relabelled as new-source qualification.

The agreement's `native` section contains `encoding_id` and `types`. An entry in
`types` contains `state` and the complete `descriptor` only. There are no mode tags,
per-type encodings, allocation bands or seeds. The encoding is explicitly chosen
once for the profile. Zero has the same multi-type semantics as a private encoding.
Missing encodings never select zero implicitly.

Schema contracts live in `schemas/native-inventory-v1.schema.json`,
`schemas/deployment-agreement-v2.schema.json` and
`schemas/deployment-manifest-v1.schema.json`. Schema checking alone does not verify
canonical descriptor bytes, full hashes, actual history or SDK identity agreement.

Content hashes are SHA-256 over compact UTF-8 JSON with recursively lexicographically
ordered object keys, serde_json integer/bool/string encodings, array order retained
and root `content_sha256` omitted. Hashing normalizes the declared model, including
explicit empty optional identity lists and an absent previous hash as null.
Whitespace and object-property input order are not identity. Inventory and catalogue
types sort by name/revision; generation also sorts identity declarations. The full
artifact hash covers all declarations and retired entries, not just short tokens.

Reads and writes cap at 16 MiB. Descriptors have the SDK's independent checked
bound. Inventories, complete active/retired catalogues and each declaration list
cap at 4096 entries. Manifests cap endpoints, routes and each route's members at
4096. Output paths must be new and are fsynced; existing generations are not
overwritten. JSON duplicate fields and unknown fields are rejected.

## Actual target inventory

Applications register concrete stable types with
`#[stable_payload(type_name = "example.DoorState", inventory)]` or explicitly
register an instantiation with `register_native_type!(GenericType<u64>)`.
Link every defining crate into the exporter. A Cargo dependency alone does not
prove the registration reached the executable. Pass required name/revision keys
to `InventoryArtifact::from_linked` so omissions fail.

The inventory carries the actual compiler triple, byte order, pointer width,
complete canonical descriptors in standard padded Base64, full descriptor hashes
and independent 32-bit type tokens. Import verifies all fields against the
canonical bytes and rejects noncanonical input. Target labels cannot turn host
layouts into cross-target representations. No remote compiler attestation is claimed.

Run a cross-compiled exporter on the target or a compatible user-mode emulator
with that target's loader/libraries. `verify-inventory --expect-target` checks the
exported triple. Cross compilation alone is not an executed export. Native routes
remain single-target; use serialized codecs for mixed-target routes.

## Shared encoding and immutable catalogue

`generate --inventory ... --domain ... --encoding ...` selects every inventoried
representation. The encoding is decimal or hexadecimal, either private
`0xF000..=0xFFFE` or explicit `0`. Registered/non-private values and future-reserved
`0xFFFF` fail. `0xF100` in examples is a deployment choice, not a built-in identity.
Every active representation uses this same encoding; its structural token
discriminates the type. Encoding, size or expected Rust type cannot replace a token.

The API is `AgreementArtifact::generate(inventory, domain, generation, encoding_id,
previous, identities)`. Genesis uses generation 1 without a previous agreement.
Removing a type retires its complete descriptor. Reintroducing the identical
name/revision/descriptor reactivates it. A changed descriptor under the same
name/revision fails, including when that identity is retired. An explicitly new
revision is a new type identity under the same encoding.

Successors retain domain, complete target and encoding, advance exactly one
generation using checked arithmetic, pin the actual immediately previous full
content hash and retain every prior descriptor. There is no per-type ID high-water
mark. New identities sort by name/revision, even ahead of prior entries. Validation
checks every descriptor, including retired ones, rejects duplicate keys and short
token collisions across the entire catalogue, and reconstructs the active inventory
content hash to check `inventory_sha256`.

`verify --previous` checks an imported successor against the actual prior agreement;
without it the CLI refuses to verify a successor. Runtime admission instead checks
the independently approved full current artifact. Preserve prior generations for
history review. Changing domain, target or encoding needs a separately coordinated
deployment profile, not a fabricated successor. On a token collision, inspect both
full descriptors and coordinate a new application identity; never rewrite history.

## Separate identity namespaces

`identities.private_payloads` records other private-payload meanings, owners and
versions in the payload encoding namespace. Reusing the shared native encoding
fails, even for an empty active set or entirely retired catalogue. Other private
declarations do not allocate IDs for native types. Existing declarations cannot
be removed or redefined in a successor.

`identities.wires` uses separate `wire`, `payload_family` and `metadata_layout`
namespaces. A declaration includes a literal, compact alias, owner, version and
matching form. `registered_compact` requires an exact pair in the SDK's own
namespace-specific registry. Private/unregistered identities require `literal`.
Two private literals may share an alias; they remain different identities, and a
wire alias can equal the native payload encoding without conflict. Wire literals
cap at 65535 Unicode scalar values, matching the schema's `maxLength`; a codec may
impose a lower encoded-byte limit.

`AgreementArtifact::require_wire` checks the selected SDK wire/family/metadata
pairs before adapter setup. Registered pairs may omit declarations, while every
private pair requires an exact literal declaration. Removed mode fields and the
`single-type`/`contract` commands have no aliases. Ordinary protobuf projection
refuses token-bearing native frames; neither this tool nor the SDK infers a native
token from an encoding to preserve an old-message roundtrip.

## Independent distribution and activation

The independently provisioned manifest names every endpoint's relative artifact
locator, approved full hash, representation target, domain and generation, plus
complete membership for each route. Relative paths stay beneath `profile_root`;
absolute/parent traversal and symlinks escaping that root fail.

`LoadedDeployment::load` loads every declared endpoint artifact and checks its
independent expectation, then verifies full content agreement across each whole
route. A missing peer is not replaced by the local artifact. `agreement(route,
local, peer)` rejects self-substitution and undeclared members. Direct app-to-app
pairs obey the same rules. `admit` reports configuration admission, not an
authenticated network handshake or a payload-safety qualification.

Typed applications call `require_local_types` with required application identities.
It compares actual linked compiler-target descriptors to active provisioned types.
Opaque routers may retain foreign native identities without claiming local typed
access. SDK/binding checks still own carried encoding/token agreement, validity,
alignment, bounds and backing-storage lifetime.

Keep the loaded generation immutable while frames are in flight. Independently
verify and activate a newly provisioned manifest/artifact generation, then drain
the old routes. Startup evidence includes manifest/agreement hashes, endpoint/route
IDs, representation/executable targets, domain and generation. Compare those
receipts against approved expectations, not a freshly inferred local peer profile.

## Commands and diagnostics

The five commands are `generate`, `verify`, `verify-inventory`, `manifest` and
`admit`; each flag has `--help` text. Success exits 0, artifact/I/O failure exits 1
and invalid CLI usage exits 2. I/O diagnostics retain the concrete path and source;
token collisions name both full type keys and the short token. There is no default
native encoding. Removed band/seed/type-selection flags are rejected rather than
silently ignored. Examples in the README obtain new approved artifact hashes;
historical capture hashes are not forward-format examples or qualification.
