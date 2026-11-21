# Specification pairing — November 21, 2026

The SDK mirrors the complete tree of published up-spec commit
`fc32a01fe7ec772940c143bce4f0973896a9decb`, tree
`f37601079155bef80609bdb3b1bb673e25362ba1`, from
`PLeVasseur/up-spec`, branch `codex/20261112-api-redesign-backup`.
`CORE-SOURCE-PIN.json` records that identity. The prior SDK documentary pair was
`0786fa6e3cd7dbe12c8c10a47d2b581914854b29` with normative up-spec
`f18913c6ed062dc6d6c3d7a73dec7a33433b39e4`. The intervening portable
catalogue at `1bb360852a2922bfdb0309b71e2069ac3e50716e` was not separately
mirrored; it is included in this exact tree.

The adopted merge preserves fourteen Gemini Neon/xhigh authoring commits and
two separately attributed root Astra review repairs. Independent Gemini review,
Astra whole-candidate review and a fresh bounded recheck accepted candidate
`2ae1a56d46ab81e69d379391e13d7f5c9d67cced`. The merge tree equals that
candidate exactly; the pre-round checkpoint remains available.

Ordinary behavior has canonical homes under the language-neutral protocol
chapters; `sdk/` and mirrored safety-layer folders are removed. Assurance
profiles, element boundaries and assumptions remain under `safety/`, with Rust
mechanisms mapped separately. The amendments admit narrowly scoped startup
provisioning and serialized owned-value runtime adapters, while preserving the
no-helper path, native runtime guarantees, family applicability, distinct claim
rules and integrator assumptions. They do not admit whole optional crates,
executors, general application allocation or platform-label qualification.

Current reading entry points:

- [Contract families and applicability](../../up-spec/contract-generations.adoc).
- [Ownership](../../up-spec/basics/ownership.adoc), [resources](../../up-spec/basics/resources.adoc), and [configurations](../../up-spec/basics/configurations.adoc).
- [Transport runtime](../../up-spec/up-l1/runtime.adoc) and [Rust mapping](../../up-spec/languages/rust.adoc).
- [Assurance profile](../../up-spec/safety/profiles/up-rust-asil-b.adoc), [element boundary](../../up-spec/safety/elements/up-rust/README.adoc), and [assumptions of use](../../up-spec/safety/elements/up-rust/assumptions-of-use.adoc).
- [Historical conformance intake](../../up-spec/conformance/README.md) and [future assessments](../../up-spec/conformance/assessments/README.md).

This is documentary adoption only. Runtime sources, original source/evidence
pins, compile receipts and test/implementation tags retain their existing scope.
No implementation, safety or target qualification follows. The manual and trace
note retain historical material until reconciliation; their old paths and
revision tables are not the current normative map.

Reconcile all 34 deliberately revised requirement/assumption identities and the
242 moved/revised identities, including ten retired non-item anchors. Use the
exact component revision, target, route, provider, build and selected function
scope; separate applicability, behavior and evidence. Existing component owners
retain implementation and evidence work, including startup memory, serialized
cleanup, QNX/bare-metal lifecycle/reset contracts and URI handling. Supplier
evidence does not discharge integrator assumptions. Source-level unsafe proofs
remain required where applicable.

Document checks render 66 pages without warnings/errors and verify 4,443 local
links. Assessment-schema positive/negative controls pass. The original spec-only
OFT observations remain exit 1 with 489 defects; unchanged trace metadata permits
reuse but no fresh trace pass is claimed. Older SDK manual and OFT failures stay
bound to their original observations; this pairing does not rerun or erase them.

All 31 historical conformance files are preserved relative to the round baseline.
The 504-obligation intake, 271 follow-ups, 90 pending transfer destinations and
787 unassessed component slots remain historical intake, not current assessments.
The separate successor assessment index is empty. The 142 held maintainer
proposals are not adopted or sent by this pairing. Runtime reconciliation and
actual evidence-based assessments remain downstream.
