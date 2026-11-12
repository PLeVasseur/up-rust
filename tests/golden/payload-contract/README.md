# Stable payload fixture vectors

Each `.bin` is the complete stable representation of one representative fixture
at sequence 17, including explicit padding and every payload byte. The corpus is
little-endian; its manifest records each type, extent, alignment and SHA-256.
These are fixture bytes, not normative codec assignments or native type IDs.

`vectors.json` binds all eight files. The canonical core/full fixture catalogs
include the digest of that manifest, and normal tests pin the catalog digests and
check the stored bytes against the fixture validators and generated representations.
Tests fail when files/manifests are missing or changed. Normal tests never update.

The explicit updater requires both the environment opt-in and an exact ignored
test selection:

```sh
UP_PAYLOAD_VECTOR_UPDATE=1 cargo test --locked --all-features \
  --test payload_contract_vectors regenerate_payload_contract_vectors \
  -- --ignored --exact --nocapture
```

Review every changed vector and manifest, then deliberately update the pinned
core/full catalog digests printed by the updater in `tests/payload_contract.rs`.
Run normal core and all-feature conformance tests afterward. Regeneration is not
normal test acceptance. Large LiDAR/camera semantic validation requires
`payload-contract-large-fixtures`; hashes for all eight stored files are checked
even in the core-only fixture test.

LiDAR coordinate construction uses frozen f32 trigonometric coefficient bits under
`src/bench_fixtures/payload_contract/lidar_geometry.rs`, retaining the original
native-host fixture bytes without relying on per-call transcendental rounding.
The separate explicit updater is:

```sh
UP_PAYLOAD_VECTOR_UPDATE=1 cargo test --locked --all-features \
  --test payload_contract_vectors regenerate_lidar_geometry_coefficients \
  -- --ignored --exact --nocapture
```

Run it on the declared native fixture target, format the generated Rust source,
and verify every existing stored vector afterward. It does not regenerate vector
files or authorize changed fixture bytes/digests.
