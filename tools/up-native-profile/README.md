# up-native-profile

This workspace package owns **provisional, versioned** target inventories,
deployment agreements and independently provisioned admission manifests. It is
SDK-owned reference tooling, not a registry assignment or finalized wire standard.

One agreement selects **one explicit native encoding** and the complete native
representations that may use it. Tokens disambiguate types; an encoding cannot
identify a type. There are no modes, bands, seeds or per-type encoding assignments.
The [deployment guide](docs/deployment-agreement.md) covers history and admission.

## Workflow

1. Link application types into an exporter for the actual target. Call
   `InventoryArtifact::from_linked` with required name/revision keys, then
   `write_artifact`. An unlinked type cannot be discovered by the generic CLI.
2. Run `generate` with that inventory, domain and an explicit `--encoding`:
   private `0xF000..=0xFFFE` or `0`. No encoding is chosen by default. Explicit
   zero admits many types under the same rules; it is not a fallback or mode.
3. Provision each endpoint's approved full agreement hash, target, domain,
   generation and relative file locator in a manifest with complete route members.
   Distribute each independently named artifact before activation.
4. Load the manifest and all artifacts, check required linked types and private
   wire declarations, then obtain agreement for the declared local/peer pair.
   A missing peer is an error, never a reason to substitute the local profile.

```text
cargo run --locked -p up-native-profile -- generate --inventory inventory.json \
  --domain example.vehicle --generation 1 --encoding 0xF100 --output agreement-1.json
cargo run --locked -p up-native-profile -- verify --agreement agreement-1.json
cargo run --locked -p up-native-profile -- generate --inventory inventory-next.json \
  --domain example.vehicle --generation 2 --encoding 0xF100 \
  --previous agreement-1.json --output agreement-2.json
cargo run --locked -p up-native-profile -- verify --agreement agreement-2.json \
  --previous agreement-1.json
cargo run --locked -p up-native-profile -- generate --inventory inventory.json \
  --domain example.zero --encoding 0 --output zero-1.json
```

`0xF100` is only an example deployment-private agreement, not a built-in assignment.
The API has the same single generation path:

```rust,ignore
let agreement = AgreementArtifact::generate(
    &inventory, "example.vehicle".to_owned(), 1, 0xF100,
    None, IdentityDeclarations::default(),
)?;
let profile = agreement.profile()?;
assert_eq!(profile.encoding().id(), 0xF100);
assert_eq!(profile.representations().count(), inventory.types.len());
```

## History and artifact versions

The native JSON section is `{"encoding_id":61696,"types":[...]}`. Each type has
only `state` (`active` or `retired`) and its complete `descriptor`. Generation
selects every inventoried type, retains removed descriptors as retired and
reactivates only an identical name/revision/descriptor. A new revision is a new
identity, not a new encoding. Duplicate keys, descriptor redefinition and token
collisions, including with retired types, fail. Successors keep domain, target
and encoding, advance exactly one generation and pin the actual previous hash.
The active descriptor set must reproduce the pinned inventory's content hash.

Deployment agreement JSON is **format 2**; inventory and manifest JSON remain
**format 1**. SDK profile canonical bytes are version 3; descriptor/token derivation
stays version 2. The schemas live in `schemas/`. Reject and regenerate old agreement
formats from the actual target inventory; do not translate hashes or reuse old
qualification. Removed fields and commands have no aliases. Ordinary protobuf
conversion does not carry native tokens: this tool does not reconstruct them or
offer native roundtrips through that old shape.

Other private payload declarations cannot reuse the shared native encoding, even
when all its types are retired. Wire/family/metadata identities occupy separate
namespaces; equal numeric aliases do not conflict with a payload encoding. Private
wires require exact literal declarations, while compact registered references must
match the SDK's namespace-specific pairs. Previous declarations cannot be removed
or redefined in a successor.

## Provision and admit

At provisioning time, author `endpoints.json` as an array of
`DeploymentEndpoint` objects from the approved generation. Each object has `id`,
`agreement_path`, `content_sha256`, `target`, `domain` and `generation`. The hash
must come from the approved format-2 artifact, not this document or an old capture.
For example, `sender` and `receiver` use distinct locators `sender.json` and
`receiver.json` beneath `deployment/`. Do not derive peer expectations from a
local runtime profile. The route input for that direct pair is:

```json
[{"id":"direct","endpoints":["sender","receiver"]}]
```

After distributing the endpoint files and saving this input as `routes.json`:

```text
cargo run --locked -p up-native-profile -- manifest --endpoints endpoints.json \
  --routes routes.json --output deployment/manifest.json
cargo run --locked -p up-native-profile -- admit --profile-root deployment \
  --manifest deployment/manifest.json --route direct --local sender --peer receiver
```

`admit` checks configuration, not payload validity or network peer authentication.
Typed applications additionally call `LoadedDeployment::require_local_types` and
`AgreementArtifact::require_wire` before activation. Actual bytes still need SDK
identity, target, validity, alignment and lifetime checks. Cross-compiled exporters
must run on their target or a compatible emulator; `verify-inventory --expect-target`
checks the export without relabelling a host representation.

## Commands, bounds and qualification

The five commands are `generate`, `verify`, `verify-inventory`, `manifest` and
`admit`. Exit codes are 0 for success, 1 for artifact/I/O failure and 2 for invalid
CLI usage. Errors name the path or colliding type identities. Outputs must be new
files and are fsynced; reads/writes have a 16 MiB bound. Inventories, catalogues,
each declaration list, endpoints, routes and each route's members cap at 4096.
Descriptors also have the SDK's independent bound. Absolute paths, parent traversal
and symlinks escaping the provisioned profile root are rejected.

This unpublished `0.1.0` tool pins SDK `0.10.0-SNAPSHOT`; coordinated release notes
must name the compatible SDK and macros. Qualification requires current/MSRV
package tests, CLI integration, strict Clippy, Rustdoc and doctests. CLI integration
accepts `UP_NATIVE_PROFILE_EXPORTER` pointing to an already built actual linked
exporter; otherwise it builds a fixture in an isolated temporary target directory.
Supply that exporter when build/storage orchestration is parent-owned.
