# Implementation traceability for the up-rust SEooC requirements

**Non-normative.** This note is SDK-side traceability. The specification in
`up-spec/safety/` defines what the SDK must satisfy: the element's own rules in
`seooc-up-rust/`, and its transport-layer and communication-layer items in
`up-l1/` and `up-l2/`. The SDK traces to it. Nothing here is a requirement, a
rationale or evidence, and the specification does not depend on this note.

## What this note holds

- The sentences below were written as part of the specification's rationales and
  verification notes, and cited SDK source locations, internal names or test
  names. They were moved here so that the specification cites no implementation.
- Each sentence is kept as it was written, under the item it belonged to. Paths
  are relative to the SDK repository.
- They describe the SDK as it stood when they were written, at the snapshot paired
  with up-spec commit `ed7b60b5939788d1cf41fcf281676d9ad9dfa629`. They are not
  re-verified here and can go stale as the code moves.
- The authoritative links from code to requirements are the OFT coverage tags in
  the code and tests (`[impl->req~…]`, `[utest->req~…]`). Use this note to find
  where to look, then check the tags and the source.
- Each heading under "Items" keeps the revision its sentences were written
  against. Where a revision has risen since, the table in the next section gives
  the current one.
- The file headings under "Items" follow the specification's current layout,
  with paths relative to `up-spec/safety/`. The specification now files the items
  that concern a protocol layer under `up-l1/` and `up-l2/`. Each entry sits,
  unchanged, under the file that holds its item now. Filing an item elsewhere
  changed no ID, no revision and no statement, so no coverage tag moves.

## Element parts and tooling

The specification names the element's parts by crate. At the paired snapshot they were found here:

| Part | Location in the SDK |
| --- | --- |
| E1, the runtime path | `up-rust-core`: `transport/`, `wire/`, `communication::two_mode`, `frame`, `payload`, identifiers |
| E2, the lending and handoff runtime contracts | `up-rust-core::transport` |
| E3, startup memory | `up-rust-core::memory` |
| E4, platform | `up-rust-core::platform`, `signal` |

- The `heap-window` verification method uses the Rust guard `heap_guard` and the C-level observer `heap_guard::c_level`, both in `test-util`.
- The committed OFT coverage configuration is `.env.oft-current`. It keeps the nineteen SDK assumptions of use in the gating run through `SafetyGateUpRust`, and adds `SafetyManual` in the report-only run.

## Revisions raised by the hand-off mechanism correction

The specification's rationales described one mechanism for handing a retained
owner to a binding's deferred return, a lock-free queue served by the binding's
own worker, and did not say that it was an example. The corrected rationales
say that it is one mechanism among those that meet the statements. The two
statements did not change. Their revisions rose so that every tag and every
claim that rests on them is read against the corrected text:

| Earlier revision | Current revision |
| --- | --- |
| `req~ssr-release-lease-drop~4` | `req~ssr-release-lease-drop~5` |
| `req~ssr-binding-frame-drop~7` | `req~ssr-binding-frame-drop~8` |

- A binding that hands a retained owner over through such a queue met the
  earlier revisions and meets these. Nothing is added for it.
- The SDK's tags for `req~ssr-release-lease-drop` name revision 4. Each stays
  as it is until its code or test has been reviewed against the current
  statement, whose text is the same.
- `req~ssr-binding-frame-drop` is covered in each binding's repository, which
  this note does not inventory.

## Revisions raised by the owning-wire applicability correction

The specification now admits ordinary owning values on a serialized route and
gives a stronger resource guarantee only where a route requires it
(`req~ssr-heap-guarantees~1`, a new item). These revisions rose with it:

| Earlier revision | Current revision |
| --- | --- |
| `req~ssr-heap-none~2` | `req~ssr-heap-none~3` |
| `req~ssr-heap-declared~2` | `req~ssr-heap-declared~3` |
| `req~ssr-heap-slot-types~2` | `req~ssr-heap-slot-types~3` |
| `req~ssr-release-lease-drop~3` | `req~ssr-release-lease-drop~4` |
| `req~ssr-bound-wait~2` | `req~ssr-bound-wait~3` |
| `req~ssr-mode-init-failure~3` | `req~ssr-mode-init-failure~4` |
| `req~ssr-mode-bounded-startup~2` | `req~ssr-mode-bounded-startup~3` |
| `req~ssr-peer-failure-normal~2` | `req~ssr-peer-failure-normal~3` |
| `req~ssr-binding-frame-drop~4` | `req~ssr-binding-frame-drop~5` |
| `req~ssr-binding-bounded~2` | `req~ssr-binding-bounded~3` |
| `req~ssr-async-seam~2` | `req~ssr-async-seam~3` |
| `req~ssr-wire-heap-claim-tested~2` | `req~ssr-wire-heap-claim-tested~3` |
| `aou~seooc-qualified-bindings-and-wires~5` | `aou~seooc-qualified-bindings-and-wires~6` |

No code or test tag moves with these revisions, and none gains coverage from
them. A tag that names an earlier revision stays as it is until its code or test
has been reviewed against the current statement.

When the specification was last published before this correction (up-spec commit
`a4805bcfba227b3c4a2fb14aeacc8cc9d1d50372`), OFT's report-only run listed these
tags in the SDK. Each already named a revision older than the one current then:

| Requirement | Tags in the SDK | Revision they name |
| --- | --- | --- |
| `req~ssr-async-seam` | 9 `impl`, 5 `utest` | 1 |
| `req~ssr-mode-init-failure` | 1 `impl`, 15 `utest` | 2 |
| `req~ssr-peer-failure-normal` | 2 `impl`, 6 `utest` | 1 |
| `req~ssr-release-lease-drop` | 1 `impl`, 4 `utest` | 2 |
| `req~ssr-heap-none` | 2 `utest` | 1 |
| `req~ssr-heap-declared`, `req~ssr-heap-slot-types`, `req~ssr-bound-wait`, `req~ssr-mode-bounded-startup` | none | none |

- The new item `req~ssr-heap-guarantees~1` has no tag.
- The binding and wire items (`req~ssr-binding-frame-drop`, `req~ssr-binding-bounded`, `req~ssr-wire-heap-claim-tested`) are covered in each binding's and each wire's repository, which this note does not inventory.
- The safety-manual entry for `aou~seooc-qualified-bindings-and-wires` covers revision 6.

## Revisions raised by the responsibility refinement

The specification now states a duty that binds only one transport, or only some
deployments, as an item of its own. No obligation changed. These revisions rose:

| Earlier revision | Current revision |
| --- | --- |
| `aou~seooc-peer-access~4` | `aou~seooc-peer-access~5` |
| `aou~seooc-integrity-levels~5` | `aou~seooc-integrity-levels~6` |
| `aou~seooc-qualified-bindings-and-wires~6` | `aou~seooc-qualified-bindings-and-wires~7` |
| `aou~seooc-waiver-approval~2` | `aou~seooc-waiver-approval~3` |
| `aou~seooc-deployment-data~5` | `aou~seooc-deployment-data~6` |
| `req~ssr-binding-frame-drop~5` | `req~ssr-binding-frame-drop~6` |
| `req~ssr-binding-no-panic~4` | `req~ssr-binding-no-panic~5` |
| `aou~seooc-timing-budgets~6` | `aou~seooc-timing-budgets~7` |

These items are new, each with text that an earlier revision above held:

| New item | Its text came from |
| --- | --- |
| `aou~seooc-capability-ceiling~1` | `aou~seooc-integrity-levels~5` |
| `aou~seooc-resource-guarantees~1` | `aou~seooc-qualified-bindings-and-wires~6` |
| `aou~seooc-iceoryx2-level-separation~1` | `aou~seooc-integrity-levels~5`, `aou~seooc-peer-access~4` |
| `aou~seooc-iceoryx2-limits~1` | `aou~seooc-qualified-bindings-and-wires~6`, `aou~seooc-waiver-approval~2`, `aou~seooc-deployment-data~5` |
| `req~ssr-binding-iceoryx2-panic-paths~1` | `req~ssr-binding-frame-drop~5`, `req~ssr-binding-no-panic~4` |

- No code or test tag moves with these revisions, and none gains coverage from
  them. A tag that names an earlier revision stays as it is until its code or
  test has been reviewed against the current statement.
- The two binding rules and the new iceoryx2 requirement are covered in each
  binding's repository, which this note does not inventory.
- The SDK's safety manual has an entry for each of the first two new
  assumptions. The iceoryx2 binding owns the other two.
- `aou~seooc-timing-budgets~7` rose for a clarified clause, not for a split:
  allowing for a middleware lock in a budget accepts no frame drop that waits
  for it. An assumption of use carries no code or test tag, so this revision
  leaves none stale. The two requirements that cite it,
  `req~ssr-qos-deadline~4` and `req~ssr-atomic-windows~2`, keep their revisions.
- The rows under "Names that the specification describes instead" cite the
  current revisions.

## Revisions raised by the binding panic-path allocation

The specification's rules of every binding name no middleware. One binding's
allowed panic paths, and the setup check for its data segments, are stated with
that binding.

| Earlier revision | Current revision |
| --- | --- |
| `req~ssr-binding-no-panic~5` | `req~ssr-binding-no-panic~6` |
| `req~ssr-binding-frame-drop~6` | `req~ssr-binding-frame-drop~7` |
| `req~ssr-binding-iceoryx2-panic-paths~1` | `req~ssr-binding-iceoryx2-panic-paths~2` |

This item is new, with text that an earlier revision above held:

| New item | Its text came from |
| --- | --- |
| `req~ssr-binding-iceoryx2-static-segments~1` | `req~ssr-binding-no-panic~5` |

- No SDK code or test tag names one of these items, so none moves. They are
  covered in each binding's repository, which this note does not inventory.
- A tag there that names an earlier revision stays as it is until its code or
  test has been reviewed against the current statement.

## Names that the specification describes instead

The specification's statements describe these interfaces by what they do. The
SDK identifiers they stood for at the paired snapshot are:

| Item | Wording in the statement | SDK identifier |
| --- | --- | --- |
| `aou~seooc-qualified-bindings-and-wires~7` | the binding contract suite | `assert_binding_contract` |
| `aou~seooc-qualified-bindings-and-wires~7` | the wire contract suite | `assert_wire_contract` |
| `aou~seooc-qualified-bindings-and-wires~7` | the QoS contract suite, and its deadline checks | `run_qos_contract` |
| `aou~seooc-qualified-bindings-and-wires~7` | an arrival reported through the runtime port | `RuntimePort::arrived` |
| `aou~seooc-qualified-bindings-and-wires~7` | the arrival-report latency a binding declares in its QoS support | `QosSupport::arrival_reports` |
| `aou~seooc-qualified-bindings-and-wires~7` | the runtime port's receive-failure report | `RuntimePort::receive_failed` |
| `aou~seooc-application-reacts~4` | the route counters' difference operation | `RouteCounters::since` |
| `aou~seooc-build-configuration~5` | the element's ASIL build check, placed in an application's own binary | `assert_asil_build!()` |
| `aou~seooc-timing-budgets~7`, `req~ssr-atomic-windows~2` | a stalled expiry of a call | `expire` |
| `aou~seooc-timing-budgets~7`, `req~ssr-atomic-windows~2` | a stalled failure of a peer's older calls | `fail_older` |
| `aou~seooc-timing-budgets~7`, `req~ssr-atomic-windows~2` | a stalled retirement of a peer entry | `retire` |
| `aou~seooc-timing-budgets~7`, `req~ssr-atomic-windows~2` | a spurious peer loss | `peer_lost` |

## Status remarks moved out of the specification

These remarks described the SDK as it stood, inside statements, violation notes,
the summary table or rationales. They are kept here as they were written, and
the specification now states the rule without them.

**`req~ssr-atomic-windows~2`** — Every narrowed value states its window

- "At this revision every supported profile, host and MCU alike, keeps these values at full width". Only the words "At this revision" left the statement. The obligation stays in it: every supported profile keeps those values at full width.
- Rationale: "The list in the statement describes the representation that the supported profiles share at this revision". Only the words "at this revision" left it.

**`aou~seooc-timing-budgets~7`** — Timing claims rest on a supported configuration, and the item's budgets are the integrator's

- Of a representation that keeps the narrowed generations: "which no current profile's does".
- Of the one-word tick representation: "which at this revision only the model tests do".
- "A full-width monitor, the runtime monitor of every current profile, has no such bound".
- Summary table: "where a configuration keeps the predecessor narrowed values, which no current profile does".

**`aou~seooc-application-reacts~4`** — The application reacts to what the element reports

- Of the one-word tick representation: "which at this revision only the model tests do".
- Summary table: "where a route's monitor uses the one-word tick, which no current profile's runtime monitor does".
- Rationale: "Every current profile's runtime monitor is full width (`req~ssr-atomic-windows~2`), so the duty binds no current configuration."

**`aou~seooc-target-environment~4`** — The target environment is verified

- "The current SDK profiles are macOS ARM64, Linux x86-64, Linux ARM64, Windows x86-64, Cortex-M7 and RV32IMAC; QNX 7.1 and 8.x are future work, separate from each other, whose backend exists only as source."
- "The element ships backends for Linux and macOS, and one for QNX that exists only as source: no toolchain or device has run it."

**`aou~seooc-build-configuration~5`** — The ASIL binary is built from its claimed graph, with the ASIL profile

- Of the minimum supported Rust version: "1.88 today".

**`aou~seooc-supervisor-restart~4`** — A supervisor outside the process restarts it (violation note)

- Of metadata changed in exclusive steps: "as at this revision".

**`aou~seooc-clock~3`** — The platform provides the clocks (violation note)

- Of a generator that keeps a 64-bit counter and the full last good time: "as every supported profile's does at this revision".

## Items

### `seooc-up-rust/assumptions-of-use.adoc`

**`aou~seooc-waiver-approval~3`** — Waivers are approved with an interference argument (after the statement)

- *Rationale.* A waiver records that the deployment accepts an allocation; it is not the argument that the allocation cannot interfere, which the deployment still has to make (`core/src/transport/waiver.rs`: a permit "is not approval").

**`aou~seooc-target-environment~4`** — The target environment is verified (after the statement)

- *Rationale.* `core/src/platform/mod.rs`: backends by `cfg`; QNX "has never been compiled for or run on a real target".

**`aou~seooc-integrator-platform~4`** — A custom platform keeps the platform's rules (after the statement)

- *Rationale.* `core/src/platform/mod.rs`: runtime code takes a `&'static dyn Platform`, and `NATIVE` exists only on Linux, macOS and QNX.

**`aou~seooc-build-order~2`** — Applications build every transport and role before the last transition (after the statement)

- *Rationale.* Once a transport that shares startup memory enters runtime mode, the caller's handle places nothing more, and the memory is sealed for every handle when the last setup sharing it has entered runtime mode (`StartupMemory` in `core/src/memory.rs`).

### `seooc-up-rust/bounded-work.adoc`

**`req~ssr-bound-wait~2`** — Every bounded wait has a logical deadline (after the statement)

- A QoS wait gives up after its declared timeout and reports failure (`Overflow::Wait` in `core/src/transport/qos.rs`).
- A carrier egress refuses it at setup, because the carrier's worker would be the one waiting (`core/src/transport/carrier.rs`).
- A route with nothing to report stays quiet, since a service tick that woke waiters regardless would make idle waiters wake each other for ever (`TransportRuntime::service` in `core/src/transport/mode.rs`).

**`req~ssr-bound-no-cross-process-lock~1`** — No lock shared with another process (after the statement)

- In the code, the Linux backend's waits are private to the process: it uses no shared-memory futex and no robust list (`core/src/platform/linux.rs`).

### `seooc-up-rust/build.adoc`

**`req~ssr-build-specified-functions-only~1`** — The ASIL build contains only specified functions (after the statement)

- `up-rust-core` has hidden test hooks today, such as `__admit_for_tests` in `core/src/transport/envelope.rs` and `__slice_for_tests` in `core/src/memory.rs`, which an ASIL image would contain unless they are removed or specified.

### `seooc-up-rust/crate-isolation.adoc`

**`req~ssr-crate-isolated~1`** — The safety crate cannot be tainted (after the statement)

- *Rationale.* Cargo turns a crate's feature on for the whole build once any crate asks for it, so only a crate with no features and no dependencies cannot have `std` switched on inside it by another (`core/src/lib.rs`).
- CI checks the manifest and builds the crate against a sysroot that holds only `core` (`core/src/lib.rs`, `tools/ci/check_core_gates.py`).
- With no `alloc`, the crate's URIs keep their authority inline (`UUri` in `core/src/uri/mod.rs`), so this crate does not allocate a separate heap-owned authority representation.

**`req~ssr-crate-sync-path~1`** — Traffic is synchronous or polled (after the statement)

- CI checks that `core::task`, `core::future` and `core::pin` are absent from the crate (`core/src/lib.rs`, `tools/ci/check_core_gates.py`).

**`req~ssr-crate-core-only-binary~2`** — An ASIL binary needs no crate outside its claimed graph at run time (after the statement)

- An ASIL application depends directly on `up-rust-core` and imports its `up_rust_core::` paths (`core/src/lib.rs`).

### `seooc-up-rust/e2e-wire.adoc`

**`req~ssr-e2e-wraps-any-wire~4`** — The E2E wire wraps any wire and leaves its bytes alone (after the statement)

- Verification:: test (the sequenced wire contract on `E2e<Xcdr2>` and on `E2e` over a test wire; a control that `assert_wire_contract` still refuses the E2E wire, with its existing message; a byte comparison showing the inner encoding unchanged inside the frame; the `WireInfo` fields, which are the identity, the payload encoding, the maximum encoded length and its exactness; `Xcdr2` and the test wire passing `assert_wire_contract`), heap-window

**`req~ssr-e2e-survives-hops~1`** — Protection survives every allowed hop (after the statement)

- *Rationale.* A carrier forwards a frame's identifiers and payload bytes unchanged (`core/src/transport/carrier.rs`, `req~ssr-streamer-byte-identity~1`), so a check over those bytes at the receiving endpoint covers every hop before it.

### `seooc-up-rust/errors.adoc`

**`req~ssr-refuse-no-panic~1`** — No panic on runtime paths (after the statement)

- No waiver can cover a panic: a waiver covers allocation inside middleware only (`core/src/transport/waiver.rs`).
- The crate denies `clippy::panic`, `clippy::unwrap_used` and `clippy::indexing_slicing` (`core/src/lib.rs`), and the workspace's lint table denies `indexing_slicing` (`Cargo.toml`).

**`req~ssr-error-fixed-size~1`** — Errors are fixed-size diagnoses (after the statement)

- *Rationale.* Runtime errors are `Copy` and carry no heap data, so reporting a refused send cannot touch the heap (`core/src/transport/error.rs`).
- `up-rust` turns only initialization errors (`StartupError`) into a `UStatus` with text (`core/src/transport/error.rs`).

**`req~ssr-error-route-report~2`** — Each route reports what it got (after the statement)

- *Rationale.* Each report names its permit through its heap class (`RouteReport` in `core/src/transport/report.rs`), and the runtime keeps the waivers it recorded, so each permit joins to the waiver that admitted it.

### `seooc-up-rust/heap.adoc`

**`req~ssr-heap-none~2`** — No heap in runtime mode on a native path (after the statement)

- Runtime mode works instead in startup memory, placed during initialization and never freed (`core/src/memory.rs`: "Objects placed here are never dropped or freed").

**`req~ssr-heap-lazy-forced~1`** — Lazy state is forced before runtime mode (after the statement)

- In the code, the platform layer needs no state with first-use setup, and message IDs need no thread-local generator (`core/src/platform/mod.rs`, `core/src/transport/ids.rs`).

**`req~ssr-heap-declared~2`** — The heap class is declared, with no default (after the statement)

- *Rationale.* A claim of no heap use can be checked only if it is stated, so `WireInfo::new` takes the heap class of both directions, with no default (`core/src/wire/mod.rs`).
- The transition refuses such a native route with `StartupError::RouteNeedsHeapAtRuntime`, which carries the route and the wire's reason (`core/src/transport/error.rs`).

**`req~ssr-heap-slot-types~2`** — Decoded types run no destructor on a heap-free route (after the statement)

- A receive route's startup check refuses such a type with the reason "the decoded type owns heap memory" (`core/src/transport/inbound.rs`).

### `seooc-up-rust/memory-safety.adoc`

**`req~ssr-unsafe-inventory~3`** — Every `unsafe` is inventoried and justified (after the statement)

- Each site states those obligations, and the workspace lints deny an `unsafe` block or function without that documentation (`Cargo.toml`).
- A CI gate fails when the code's `unsafe` sites and the checked-in inventory disagree (`tools/ci/check_unsafe_inventory.py`).

**`req~ssr-unsafe-wire-untrusted~1`** — No wire can break the element's memory safety (after the statement)

- *Rationale.* A wire is a plain safe trait (`Wire` in `core/src/wire/mod.rs`), so its code can be wrong without any `unsafe`, and the element cannot rely on what it reports.
- Encoding goes through a checked writer that keeps the count of the bytes written itself, so a safe wire cannot claim more initialized bytes than it wrote (`core/src/wire/writer.rs`).

**`req~ssr-unsafe-binding-untrusted~1`** — No binding written in safe code can break the element's memory safety (after the statement)

- The binding contracts are safe traits today, `BindingSetup`, `LendingRuntime`, `HandoffRuntime` and `Frame`, and only `PreparedTxStorage` is an `unsafe trait` (`core/src/transport/binding.rs`, `core/src/transport/mod.rs`). up-spec's iceoryx2 chapter asks the same of a Rust payload's author, through iceoryx2's `ZeroCopySend` (the iceoryx2 transport), which iceoryx2 declares as an `unsafe trait` ([`zero_copy_send.rs` in iceoryx2 0.9.2](https://github.com/eclipse-iceoryx/iceoryx2/blob/806442b5273d87380511bb5ffcab316d129af31c/iceoryx2-bb/elementary-traits/src/zero_copy_send.rs)).

### `seooc-up-rust/platform.adoc`

**`req~ssr-platform-contract~2`** — Every platform backend keeps the platform's rules (after the statement)

- *Rationale.* `core/src/platform/mod.rs`, the trait's stated rules: "no heap, no panic, no thread creation, no global state that needs first-use setup, and `wait` must not lose a wake".

**`req~ssr-platform-init-only~1`** — Entropy and prefault stay in initialization (after the statement)

- *Rationale.* `core/src/platform/mod.rs`: "`PlatformInit::runtime` hands runtime code a value that cannot reach back".
- Both services belong to setup: entropy seeds the message-ID generators once, and prefault makes memory resident before anything is placed in it, so that no write in runtime mode takes a page fault (`core/src/platform/mod.rs`).

**`req~ssr-platform-entropy~2`** — Initialization entropy comes from a declared source (after the statement)

- *Rationale.* Entropy is read once, to seed the message-ID generators (`PlatformInit::entropy` in `core/src/platform/mod.rs`), so a fixed or repeated seed could repeat IDs from one start to the next.
- The shipped backends read the OS's source: `getrandom` on Linux, `getentropy` on macOS and `/dev/urandom` on QNX (`core/src/platform/linux.rs`, `core/src/platform/macos.rs`, `core/src/platform/qnx.rs`).

**`req~ssr-platform-classified~1`** — Every runtime behaviour is classified (after the statement)

- The platform layer is the only part of the crate that talks to an operating system (`core/src/platform/mod.rs`), which keeps the two kinds apart in the code.

**`req~ssr-atomic-width~2`** — No target needs an atomic width it lacks (after the statement)

- *Rationale.* A requirement that every protocol word be a 32-bit atomic, in one implementation on every target, avoided operations that some supported cores lack, but it also forbade the full-width host deadline monitor, which uses 16-byte atomics where the CPU provides them and refuses setup where it does not (`core/docs/pal-deadline-safety.md`, `core/src/platform/deadline_atomic128.rs`), and it did not by itself keep identity full width.

**`req~ssr-atomic-windows~2`** — Every narrowed value states its window (after the statement)

- The list above is the representation at this revision: the host profiles' deadline backend (`core/docs/pal-deadline-safety.md`), the deadline record of the MCU profiles and of every other non-host build, a 64-bit origin and a 128-bit count in one exclusive record (`core/src/transport/metadata/deadline.rs`), the peer and call tables and the generator (`core/src/transport/metadata.rs`, `core/src/transport/ids.rs`), the pools (`core/src/transport/pool.rs`, `core/src/transport/rpc.rs`) and the counters (`core/src/transport/report.rs`); the one-word monitor survives only as a model under test (`core/src/transport/deadline_monitor.rs`).

### `seooc-up-rust/qos.adoc`

**`req~ssr-qos-no-downgrade~2`** — QoS is honored or refused, never weakened (after the statement)

- Verification:: test (the QoS contract suite on real iceoryx2 and on in-memory bindings; iceoryx2 tests `t01` to `t14`; the native lanes; a route that requires a timing bound or an assurance scope its configuration does not establish is refused by name; the same route without that requirement is admitted on a configuration that has no timing or assurance package)

**`req~ssr-qos-deadline~4`** — Deadlines are monitored (after the statement)

- The runtime monitor keeps full-width origins and a saturating full-width count on every profile: on the host profiles through the wide-atomic backend, whose source argument covers arbitrary suspension of a helper without a watchdog or a wrap window (`core/docs/pal-deadline-safety.md`), and on the MCU profiles and every other non-host build through one exclusive record of a 64-bit origin and a 128-bit count (`core/src/transport/metadata/deadline.rs`); the one-word tick representation remains only as a model under test (`core/src/transport/deadline_monitor.rs`).

**`req~ssr-qos-ttl~4`** — Expired messages are not processed (after the statement)

- A sender that cannot read its clock reuses the last good time in its IDs, so they keep working (`core/src/transport/ids.rs`), and the statement discloses the early refusal this can cause.

**`req~ssr-qos-consistent-declarations~1`** — Two declarations of one route agree (after the statement)

- *Rationale.* The refusal names the setting `QosSetting::PeerMismatch` (`core/src/transport/qos.rs`).

### `seooc-up-rust/release.adoc`

**`req~ssr-release-inert-drop~1`** — Dropping a runtime handle does nothing (after the statement)

- Each handle type carries a compile-time assertion that it needs no drop, such as the one on `TransportRuntime` in `core/src/transport/mode.rs`.

### `seooc-up-rust/runtime-modes.adoc`

**`req~ssr-mode-two-types~1`** — Two modes, as two types (after the statement)

- Non-generic types let a forwarder such as the Streamer hold transports chosen from configuration at run time in one list (`core/src/transport/mod.rs`).

**`req~ssr-mode-no-setup-at-runtime~1`** — Setup is impossible in runtime mode (after the statement)

- `TransportSetup::enter_runtime` consumes the setup value, so no setup call can be written afterwards (`core/src/transport/mode.rs`).

**`req~ssr-mode-no-traffic-before-runtime~1`** — Traffic is impossible before runtime mode (after the statement)

- In the code, every traffic operation lives on a handle activated from a `TransportRuntime`, and a role's setup value has no way to send (`core/src/transport/mode.rs`, `core/src/communication/two_mode.rs`).

**`req~ssr-mode-backstop~1`** — Run-time refusal only where types cannot reach (after the statement)

- *Rationale.* Types keep the modes apart inside Rust, but they cannot follow a call through a C ABI, and every transport's runtime has the same type (`core/src/transport/mode.rs`), so those cases need a check when the call is made.
- Activating a role against another runtime returns `StartupError::WrongTransport`, and a placement after the seal returns `RegionSealed` (`core/src/transport/error.rs`, `core/src/memory.rs`).

**`req~ssr-mode-entry~5`** — Entering runtime mode is one all-or-nothing operation (after the statement)

- All or nothing holds for each transport, not for a group: a transport that has entered runtime mode cannot be taken back (`req~ssr-mode-one-way~1`), so a group's later refusal does not roll it back, and a refusal ends the process for a restart (`enter_runtime_all` in `core/src/transport/mode.rs`).

**`req~ssr-mode-entry-admission~1`** — Every route is admitted before runtime mode (after the statement)

- *Rationale.* The SDK owns admission, as it owns the mode state machine and the startup slots (`core/src/transport/binding.rs`), so every binding's routes meet the same checks.
- A route that cannot be served then fails setup by name instead of failing its first message: a refused carrier leg, for one, reports the route, the binding and the reason (`StartupError::CarrierRefused` in `core/src/transport/error.rs`).

**`req~ssr-mode-entry-clock~2`** — The wall clock is read before runtime mode (after the statement)

- `TransportSetup::enter_runtime` reads the clock once before the transition, so a clock that cannot be read fails setup instead of every later message (`core/src/transport/mode.rs`).

**`req~ssr-mode-entry-no-latent-refusal~1`** — Nothing is refused at runtime for a reason setup already knew (after the statement)

- Setup refuses, for example, a wire with no size bound when the route also has no `max_payload` bound (`StartupError::PayloadUnbounded`) and a variable-size wire on a binding that cannot carry a short payload (`TruncationNotSupported`).

**`req~ssr-mode-one-way~1`** — There is no way back (after the statement)

- The code has no such path: a transport that started cannot be taken back out of runtime mode, and a refusal means the process exits and is restarted (`enter_runtime_all` in `core/src/transport/mode.rs`).

**`req~ssr-mode-restart~1`** — A restart is an initialization (after the statement)

- A binding that cannot recover its predecessor refuses setup (`core/src/transport/mode.rs`).

**`req~ssr-mode-no-shutdown~1`** — No shutdown in runtime mode (after the statement)

- In the code, liveness comes from the service tick and the transport, never from a destructor (`TransportRuntime::service` in `core/src/transport/mode.rs`).

**`req~ssr-mode-classic-excluded~1`** — Classic routes never enter runtime mode (after the statement)

- *Rationale.* The classic API allocates in the SDK's own code: `UTransport` is an async trait, and the compatibility path keeps its async-trait allocation (`src/utransport.rs`, `src/wire_transport/owned.rs`).
- A waiver covers allocation inside a binding's middleware only, never in SDK or binding code (`core/src/transport/waiver.rs`), so no waiver can admit a classic route.

### `seooc-up-rust/startup-memory.adoc`

**`req~ssr-memory-static~2`** — ASIL-path storage is static or caller-provided (after the statement)

- *Rationale.* The ASIL crate is `no_std` with no `alloc` (`core/src/lib.rs`), so its storage can come only from a `static` region or from bytes the caller provides (`StartupRegion` and `StartupMemory::new` in `core/src/memory.rs`).
- Every placement goes through `StartupMemory::reserve`, which checks bounds, alignment and exclusivity (`core/src/memory.rs`).

**`req~ssr-memory-computable~3`** — Startup storage is computable before start (after the statement)

- *Rationale.* An integrator who places startup memory in a static region or a linker section has to size it before the process starts, and a request that does not fit is refused only at startup (`StartupMemory::new` in `core/src/memory.rs`).

### `seooc-up-rust/waivers.adoc`

**`req~ssr-waiver-required~1`** — Middleware heap needs a named waiver (after the statement)

- The transition refuses a missing waiver with `StartupError::MiddlewareHeapNeedsWaiver`, which names the route and the permit (`core/src/transport/error.rs`).

**`req~ssr-waiver-scope~1`** — A waiver covers middleware and nothing else (after the statement)

- *Rationale.* The element can answer only for its own code, so a waiver covers only what middleware does inside itself, for the operations and the bound its permit states (`core/src/transport/waiver.rs`).

**`req~ssr-waiver-record~1`** — Waivers are recorded and can be found (after the statement)

- *Rationale.* A binding ships a permit that states the use it needs, and a deployment files a waiver against it, so accepting the allocation is the deployment's recorded decision (`core/src/transport/waiver.rs`).

### `up-l1/bindings.adoc`

**`req~ssr-binding-lazy-forced~1`** — Lazy state is forced, and threads are idle, before running (after the statement)

- The binding contract states the same rule, and `assert_binding_contract` checks it (`core/src/transport/binding.rs`).

**`req~ssr-binding-peer-identity~1`** — Peer identity is stable, and incarnations rise (after the statement)

- *Rationale.* A stable identity with a rising incarnation lets the element tell a restarted peer from its earlier self, and refuse a frame from the earlier one as stale (`ReceiveError::StalePeer` in `core/src/transport/error.rs`).
- A binding with no peer identity reports `PeerRef::UNKNOWN`, and then only deadlines can show that a peer has gone (`core/src/transport/binding.rs`).

**`req~ssr-binding-bounded~2`** — A binding bounds its own work and its waits (after the statement)

- A carrier ingress refuses a borrowed delivery, because its bytes live only for that call and a carrier may not stage a copy (`core/src/transport/binding.rs`).

**`req~ssr-binding-predecessor-cleanup~1`** — A binding cleans up after a dead predecessor, in initialization (after the statement)

- The binding contract's `recover_predecessor` runs in the constructor, before any route (`BindingSetup` in `core/src/transport/binding.rs`).

**`req~ssr-binding-unsafe-inventory~1`** — A binding this project ships inventories its own `unsafe` (after the statement)

- *Rationale.* A binding's `unsafe` code runs on the same runtime paths as the element's, so its obligations need the same record and the same gate (`tools/ci/check_unsafe_inventory.py`, `req~ssr-unsafe-inventory~3`).

### `up-l1/integrity.adoc`

**`req~ssr-integrity-any-bits~1`** — In-place types accept every bit pattern (after the statement)

- *Rationale.* The in-place payload contract in the code needs a validator that rejects every invalid bit pattern before a typed view exists (`StablePayload` in `core/src/payload/stable.rs`), and a check made once cannot hold while a faulty writer can still change the bytes.

**`req~ssr-integrity-checked-at-admission~1`** — In-place frames are checked once, at admission (after the statement)

- *Rationale.* In-place frames are borrowed through a checked typed view (`core/src/transport/inbound.rs`), and checking once, before any reader sees the frame, makes a malformed frame one counted refusal instead of an error at each reader.

**`req~ssr-integrity-carrier-crossing~2`** — Upward carrier crossings copy once, or are refused (after the statement)

- *Rationale.* A carrier keeps a frame's bytes unchanged and makes at most one copy, into a lending egress's loan (`core/src/transport/carrier.rs`), so that copy is the one place where a frame from a lower writer can be held still and checked.
- A byte-only egress receives the ingress bytes directly, with no loan to copy into (`core/src/transport/carrier.rs`).

### `up-l1/peers.adoc`

**`req~ssr-peer-failure-normal~2`** — Peer failure is normal input (after the statement)

- In the code, a peer dying or restarting is normal input to a transport (`core/src/transport/mod.rs`), and a frame from an earlier incarnation of a peer is refused as stale (`ReceiveError::StalePeer`).

**`req~ssr-peer-stale-rejected~3`** — Stale peers are rejected (after the statement)

- At this revision the peer table keeps each peer's full 64-bit identity and incarnation in one exclusive record and has neither limit (`core/src/transport/metadata.rs`, `core/src/transport/inbound.rs`), so the requirement states the outcome that every mechanism keeps instead.

### `up-l1/wires.adoc`

**`req~ssr-wire-no-panic~1`** — Encoding and decoding never panic (after the statement)

- The wire trait states the same rule: `encode` and `decode` must not panic on any input, and a malformed frame is `Err` (`core/src/wire/mod.rs`).
- The specification has raised this item to `req~ssr-wire-no-panic~2`: an input
  that is not an encoding in the wire's format is an error, and the format alone
  decides whether an empty input is an encoding, and of which value. The
  sentence above was written against revision 1. No SDK code or test tag names
  this item.
- Revision 2's verification states every expected result from the format and
  from the type decoded, never from the wire under test. The wire contract
  suite (`src/test_util/contract.rs`) is to be read against that before any tag
  cites revision 2: a check that takes its expectation from a sample's own
  encoding, or from that encoding's length, does not meet it.

### `up-l2/interaction.adoc`

**`req~ssr-peer-rpc-correlation~3`** — A response reaches only its own call (after the statement)

- At this revision the call table compares both request-ID words and the bound peer, and claims the slot, in one exclusive step (`begin_delivery` in `core/src/transport/metadata.rs`); a mechanism that claims before it compares keeps the recheck after the claim and its give-back instead.

**`req~ssr-ids-unique~4`** — Message IDs are unique (after the statement)

- The 32-bit figures above belong to a generator with a 32-bit sequence; the generator at this revision keeps a 64-bit counter and the full last good time (`core/src/transport/ids.rs`), so its period is 2^64^ IDs of one route, and the refusal bounds that this item requires of it are its configuration's to state.

### `up-l2/streamer.adoc`

**`req~ssr-config-exact-addresses~1`** — ASIL Streamer routes name exact source addresses (after the statement)

- The carrier primitives already require it of a lending leg: a lending route names one physical service, and a wildcard filter on one needs an explicit physical source or is refused (`CarrierRoute` in `core/src/transport/carrier.rs`).

**`req~ssr-streamer-byte-identity~1`** — Carriage keeps the bytes (after the statement)

- The carrier primitives hold to it: a byte-only egress receives the ingress bytes directly, and a lending egress takes one copy into its loan (`core/src/transport/carrier.rs`).

**`req~ssr-streamer-no-ingress-block~1`** — An egress never blocks an ingress (after the statement)

- *Rationale.* The carrier primitives ask for capacity before taking a frame, and can park a frame from a lending ingress in fixed storage, within the route's retained budget (`core/src/transport/carrier.rs`).
