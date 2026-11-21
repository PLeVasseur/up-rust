# Conformance intake catalogue

This generated, versioned view accompanies the adopted up-spec revision
`f18913c6ed062dc6d6c3d7a73dec7a33433b39e4`. It does not change that specification.
The paired SDK documents are `0786fa6e3cd7dbe12c8c10a47d2b581914854b29`.

All current implementation and evidence assessments are **unassessed**;
applicability, component revision, target, configuration and profile remain
unknown. Historical implementation and evidence gaps remain visible. The 328
obligations without follow-ups are not passes. Portfolio filters select routing
candidates, not supported deployment tuples.

| Record view | Count |
| --- | ---: |
| [Adopted obligation inventory](obligations.md) | 504, including 226 safety-page rows |
| [Historical follow-ups](followups.md) | 271 |
| [Observation cases](cases.md) | 12 |
| [Pending transfers](transfers.md) | 90 destinations from 84 entries |

[Recorded identities](identities.md) distinguish public documents, historical
source-bound receipts and private unaccepted candidates. Streamer, E2E and
additional wires retain separate deferred scopes. Proposed references, including
FU-218's unmatched dispatcher identifier, are not adopted requirements.

The [interactive HTML](index.html) embeds all normalized data and the exact
documentary input snapshots it displays. Download/open it in a browser for
filters and pointer-aware record details; these Markdown pages work in repository
viewers without JavaScript. No separate site or server is required.

The [input manifest](INPUT-MANIFEST.v2.json) pins all 19 files under `records/`
by byte length and SHA256. The records are immutable documentary exports; their
product source/evidence locators are historical strings, not bundled or verified
evidence. Retaining a digest does not establish product conformance. Generated
excerpts and the HTML viewer derive from those records, not a manually maintained
second matrix. Held maintainer-proposal packets are not included or adopted.

Reproduce with `python3 -B generate.py --check`. All input bytes are verified in
the same required read pass before normalization. With absent outputs,
`python3 -B generate.py` creates them; existing outputs must already match.
Changed records require a deliberately new version, review and publication.
The generator reads only this directory, uses the Python standard library, and
runs no product tests. [Generation receipt](GENERATION-RECEIPT.json).

Manual/registry and OFT failures retain their original scope. Component
assessments, applicability and actual component-owned filings remain open.
This catalogue establishes no safety qualification or runtime guarantee.
