# up-native-profile

This workspace package owns **provisional, versioned** native inventory and
deployment-agreement artifacts. It is a reference implementation, not a normative
registry assignment or a finalized language-neutral wire/profile specification.

The SDK supplies target-qualified representation, token and agreement APIs. This
tool supplies deterministic private-ID allocation, immutable reservations,
artifact serialization and independently provisioned deployment admission.
The [deployment agreement guide](docs/deployment-agreement.md) documents the
formats, default allocation policy, exporter linkage and startup sequence.

## Workflow

1. Build an application-linked exporter for the actual target. Use the library's
   `InventoryArtifact::from_linked` with required type keys, then `write_artifact`.
   A generic tool binary cannot discover unlinked downstream types.
2. Feed that inventory to `generate`, optionally supplying explicit genesis seeds
   or an actual previous ledger. The default native band F100-F7FF is an
   overridable deployment recommendation, not a global registry partition.
3. Provision a manifest naming every endpoint's artifact path, expected full hash,
   target/domain/generation and complete route membership. Distribute the immutable
   artifacts before activation.
4. Load and verify the manifest/artifacts, check required linked application types,
   and obtain agreement for the actual declared local/peer endpoints. Emit the
   loaded identity in startup evidence. Direct application pairs use the same rule.
   `AgreementArtifact::require_wire` also checks private selected-wire declarations
   before constructing the selected adapter.

```text
cargo run -p up-native-profile -- generate --inventory inventory.json \
  --domain example.vehicle --generation 1 --output agreement-1.json
cargo run -p up-native-profile -- verify --agreement agreement-1.json
cargo run -p up-native-profile -- generate --inventory inventory-next.json \
  --domain example.vehicle --generation 2 --previous agreement-1.json \
  --output agreement-2.json
cargo run -p up-native-profile -- verify --agreement agreement-2.json --previous agreement-1.json
cargo run -p up-native-profile -- admit --profile-root deployment \
  --manifest deployment/manifest.json --route direct --local publisher --peer subscriber
```

`single-type` is an explicit separate choice for one exact ID 0 representation,
without URI/operation binding or an operation argument. A missing table allocation
never falls back to it. `verify-inventory --expect-target ...`
checks an export's target; it does not relabel or regenerate a host layout.
Run cross-compiled exporters on their target or a compatible user-mode emulator.
The [deployment guide](docs/deployment-agreement.md) explains single-target native
profiles and coordinated remedies for token collisions and exhausted ID bands.

```text
cargo run -p up-native-profile -- single-type --inventory inventory.json \
  --domain example.vehicle --type-name example.DoorState --type-revision 1 \
  --output single-type-1.json
```

## SDK ownership and compatibility

The SDK owns and reviews this package. Package version `0.1.0` is compatible with
SDK `0.10.0-SNAPSHOT`, expressed by its exact SDK dependency.
It remains `publish = false` until an authorized coordinated release with up-rust
and up-rust-macros. Release notes must name the compatible SDK; version numbers
need not coincide. Downstream exact Git revisions remain necessary until publication.

Provisional format 1 has no compatibility window: unsupported `format_version`
and native mode values produce named errors, and deployments regenerate. The
retired `contract_defined` mode and `contract --operation` command have no aliases.
`single_type` uses canonical native-profile mode tag 3 without an operation label;
Table artifact bytes and descriptor/token version 2 are unchanged.

## Policy and validation

Native allocations and optional private serialized-payload reservations share one
payload-ID namespace. Wire/family/layout declarations are separate namespaces in
the same artifact. Private wire identities require literals; registered compact
forms must match the SDK's actual first-wave pairs.

Outputs use new paths and are never overwritten. Preserve prior ledgers and use
their content hash when generating a successor. Retired assignments remain
reserved; only the identical identity may reactivate its original ID.

Public documentation is denied when missing, and production unsafe code is
forbidden. Run current/MSRV tests, strict Clippy, Rustdoc and doctests for this
package. Its guide is also rendered as `up_native_profile::guide`.

## Commands, limits and errors

The six subcommands are `generate`, `single-type`, `verify`, `verify-inventory`,
`manifest` and `admit`. Each accepts `--help` with the purpose of every flag.
Exit status is **0** for success, **1** for artifact/I/O failure and **2** for
invalid command-line usage. Errors use their human-readable `Display`: I/O errors
name the concrete path and retain the source; collision errors name both types and
their short token. `--seeds` conflicts with `--previous`, including an empty seed
file. A successor keeps its table band; a band change needs a new deployment profile.

Artifact reads and writes are limited to 16 MiB. Inventories, allocations and each
identity-declaration list are limited to 4096 entries. Manifests allow at most
4096 endpoints/routes and 4096 members per route. Wire literals allow 65535 Unicode
scalar values, matching the schema; a codec can impose a lower encoded-byte limit.
The pinned base64 dependency preserves the qualified standard-alphabet/padding
behavior of descriptor artifacts. This package remains `publish = false`.

## Worked two-endpoint manifest

This example uses the linked fixture exporter on `x86_64-unknown-linux-gnu` with
all its fixture features. Generate input 1 with the commands below; inputs 2–4 are
the provisioner's explicit files. A different target or type set needs its own
export and hashes. The hash in endpoint expectations is the generated agreement's
**content_sha256 field**, not a hash of its pretty-printed file bytes.

### Input 1: inventory.json (exporter output)

```json
{
  "schema": "uprotocol.native-inventory.provisional",
  "format_version": 1,
  "provisional": true,
  "descriptor_version": 2,
  "token_derivation_version": 2,
  "target": {"triple": "x86_64-unknown-linux-gnu", "endianness": "little", "pointer_width": 64},
  "types": [
    {
      "key": {"name": "example.AirState", "revision": 0},
      "canonical_descriptor_base64": "dXByb3RvY29sLm5hdGl2ZS1yZXByZXNlbnRhdGlvbi52MgAYAAAAAAAAAHg4Nl82NC11bmtub3duLWxpbnV4LWdudUAQAAAAAAAAAGV4YW1wbGUuQWlyU3RhdGUAAAAABAAAAAAAAAAEAAAAAAAAAAEBAAAAAAAAAAUAAAAAAAAAdmFsdWUAAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAB4ODZfNjQtdW5rbm93bi1saW51eC1nbnVAAwAAAAAAAAB1MzIAAAAABAAAAAAAAAAEAAAAAAAAAAEAAAAAAAAAAA==",
      "descriptor_sha256": "13e429f25074b80114bb44add7e5edfd0653b1de238ed3dd945099cf17c5c99a",
      "native_type_token": 4062831635
    },
    {
      "key": {"name": "example.DoorState", "revision": 0},
      "canonical_descriptor_base64": "dXByb3RvY29sLm5hdGl2ZS1yZXByZXNlbnRhdGlvbi52MgAYAAAAAAAAAHg4Nl82NC11bmtub3duLWxpbnV4LWdudUARAAAAAAAAAGV4YW1wbGUuRG9vclN0YXRlAAAAAAQAAAAAAAAABAAAAAAAAAABAQAAAAAAAAAFAAAAAAAAAHZhbHVlAAAAAAAAAAABAAAAAAAAABgAAAAAAAAAeDg2XzY0LXVua25vd24tbGludXgtZ251QAMAAAAAAAAAdTMyAAAAAAQAAAAAAAAABAAAAAAAAAABAAAAAAAAAAA=",
      "descriptor_sha256": "f11bc9b3f81bbd86840eb256612fb679258a2ec3c6b678cb3d55d7f8e6a18359",
      "native_type_token": 3016301553
    },
    {
      "key": {"name": "example.SeatState", "revision": 0},
      "canonical_descriptor_base64": "dXByb3RvY29sLm5hdGl2ZS1yZXByZXNlbnRhdGlvbi52MgAYAAAAAAAAAHg4Nl82NC11bmtub3duLWxpbnV4LWdudUARAAAAAAAAAGV4YW1wbGUuU2VhdFN0YXRlAAAAAAQAAAAAAAAABAAAAAAAAAABAQAAAAAAAAAFAAAAAAAAAHZhbHVlAAAAAAAAAAABAAAAAAAAABgAAAAAAAAAeDg2XzY0LXVua25vd24tbGludXgtZ251QAMAAAAAAAAAdTMyAAAAAAQAAAAAAAAABAAAAAAAAAABAAAAAAAAAAA=",
      "descriptor_sha256": "21a336a09a3161e166fe2d94b9dc82be8758f8fc7557451d402ba88da556ed78",
      "native_type_token": 2687935265
    }
  ],
  "content_sha256": "485a0f661b1aeb6ba8c354c5c2d1234ef1a53e58893d40f80a8003e9533ad7b4"
}
```

### Input 2: identities.json

Both private identities used by `StableContainerWire` need literal declarations.
The registered metadata-layout pair can be omitted. These declarations do not
consume entries in the private payload-encoding band.

```json
{
  "private_payloads": [],
  "wires": [
    {"namespace": "wire", "literal": "org.eclipse.uprotocol.wire.stable-container", "compact_id": 32768, "owner": "example", "version": 1, "reference": "literal"},
    {"namespace": "payload_family", "literal": "stable-container", "compact_id": 32769, "owner": "example", "version": 1, "reference": "literal"}
  ]
}
```

### Input 3: endpoints.json

These expectations are authored before deployment from the approved generated
agreement. Runtime loading must not substitute one local file for a missing peer.

```json
[
  {
    "id": "sender", "agreement_path": "sender.json",
    "content_sha256": "d5490da411eb267b04fc4928824a2a9f23a8d6241c55d416cdc55ec14cb200a7",
    "target": {"triple": "x86_64-unknown-linux-gnu", "endianness": "little", "pointer_width": 64},
    "domain": "example.cli", "generation": 1
  },
  {
    "id": "receiver", "agreement_path": "receiver.json",
    "content_sha256": "d5490da411eb267b04fc4928824a2a9f23a8d6241c55d416cdc55ec14cb200a7",
    "target": {"triple": "x86_64-unknown-linux-gnu", "endianness": "little", "pointer_width": 64},
    "domain": "example.cli", "generation": 1
  }
]
```

### Input 4: routes.json

```json
[{"id": "direct", "endpoints": ["sender", "receiver"]}]
```

### Export, generate, verify and admit

Run from a fresh source-workspace directory with inputs 2–4 saved as shown:

```sh
mkdir deployment
cargo run --locked -p native-inventory-fixture-export --all-features -- \
  --output inventory.json --expect-target x86_64-unknown-linux-gnu \
  --require-type example.DoorState
cargo run --locked -p up-native-profile -- verify-inventory \
  --inventory inventory.json --expect-target x86_64-unknown-linux-gnu
cargo run --locked -p up-native-profile -- generate --inventory inventory.json \
  --domain example.cli --identities identities.json --output agreement.json
cargo run --locked -p up-native-profile -- verify --agreement agreement.json
cp -n agreement.json deployment/sender.json
cp -n agreement.json deployment/receiver.json
cargo run --locked -p up-native-profile -- manifest --endpoints endpoints.json \
  --routes routes.json --output deployment/manifest.json
cargo run --locked -p up-native-profile -- admit --profile-root deployment \
  --manifest deployment/manifest.json --route direct --local sender --peer receiver
```

Admission checks configuration. A typed endpoint also calls `require_local_types`
and `require_wire::<StableContainerWire, UFrameFieldsMetadataCodec>()` before
activation; received bytes never supply those expectations. The CLI integration
test runs the exporter and all six subcommands against generated artifacts on
both supported compiler roles through the repository gate.
