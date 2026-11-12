# Wire Metadata Fixtures

This directory indexes the checked-in [wire metadata vectors](../../golden/wire-metadata/).

`tests/wire_metadata_conformance.rs` exercises public codec APIs with constructed
and malformed metadata. `tests/wire_metadata_golden.rs` separately verifies both
manifests, byte lengths, decoded-byte SHA-256 and exact hex-file SHA-256, then decodes
the stored bytes and compares their fixture semantics and re-encoded bytes.

- no-payload metadata
- standard-payload metadata
- private-use-payload metadata
- wrong magic
- unknown metadata layout id
- unsupported version
- wrong selected wire id
- unknown selected wire id
- payload-family mismatch
- malformed length
- explicit contract-defined zero, unknown/reserved/private identifiers
- trailing bytes

Missing manifests, missing vectors, count changes and hash mismatches fail.
Ordinary tests never write fixtures, even if `UP_GOLDEN_UPDATE` is accidentally set.
To deliberately regenerate both profiles from the repository root:

```sh
UP_GOLDEN_UPDATE=1 cargo test --locked --all-features --test wire_metadata_golden -- --ignored --exact regenerate_wire_metadata_goldens
```

Review any byte change and commit vectors and hash-bound manifests together with
its owning representation change. The current fixture-private identifiers are
local test declarations, not public registry assignments.
