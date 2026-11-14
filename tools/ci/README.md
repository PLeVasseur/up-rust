# Repository checks

Install the generator's pinned Python dependency from `requirements.txt`. The
checked-in shell gate itself uses the standard library only.

```sh
python3 -m pip install -r tools/ci/requirements.txt
python3 tools/ci/generate.py --check
python3 -m unittest discover -s tools/ci -p 'test_*.py'
bash tools/ci/gate.sh --configuration current --all
bash tools/ci/gate.sh --configuration msrv --all
```

`policy.yaml` defines the toolchain roles, exact command vectors, groups, bounded
timeouts and test-completion formats. `generate.py` emits `gate.sh` and parallel
jobs in `.github/workflows/check.yaml`. Edit policy inputs, run the generator
without `--check`, and review both generated outputs. `--check` never writes files.
`workflow-extra.yaml` preserves independent network link checking and the existing
feature-powerset workflow; those separate jobs are not included in the shell's
`--all` set. Each generated role/group invokes exactly the shell command usable
locally, for example `--configuration current --group tests`.

Current stable Rust owns formatting, strict Clippy, strict documentation generation,
runtime/doctests and dependency policy. MSRV Rust 1.88.0 checks all targets and runs
the same library, doctest and URI/UUID TCK targets. Library and doctest checks retain
the minimal, core/protobuf and all-feature configurations. Both deny compiler/rustdoc
warnings. Install cargo-deny 0.19.0 for the current dependency group. All Cargo
project checks use the existing lock; configure Cargo offline when the dependency
cache is already populated. Neither build jobs nor workspace test threads are
capped by product policy.

The manual/reusable `RUST_TOOLCHAIN` input overrides only the current compiler;
the release workflow forwards it explicitly. Local gates accept the same
environment variable, for example `RUST_TOOLCHAIN=1.95.0 bash tools/ci/gate.sh
--configuration current --group check`. Setup, Cargo and extra jobs use the same
selection; each gate prints actual compiler/Cargo versions. MSRV stays fixed.

The dependency check explicitly uses `cargo deny --offline --locked --all-features
check`; cargo-deny's advisory fetching does not follow CARGO_NET_OFFLINE. Prepare
the project and advisory caches separately when network access is available:

```sh
cargo fetch --locked
cargo deny --locked fetch
```

The hosted dependency job performs this setup before invoking the same offline
gate. Local runs can reuse their existing caches; a missing cache fails rather
than fetching during validation or changing the owning lock.

Each example is also checked with its exact declared feature set on both compiler
roles. This catches a missing `required-features` entry that an all-feature build
would hide, including the notification example's `protobuf-support` dependency.

The `abi` group compiles the standalone Rust ABI mirror with the selected compiler
and compiles/runs the C11 and C++17 layout probes. A C/C++ compiler is required;
this is host layout conformance, not a native middleware integration trial.

Current runs reject any inherited `UP_RUST_SKIP_TRYBUILD` value, including `0`.
MSRV sets that variable explicitly for version-sensitive compiler diagnostics.
`RUSTC_BOOTSTRAP` is rejected for either compiler. Checks do not silently skip
missing targets; add new targets and their applicability to the policy when their
implementation is introduced. Empty or incomplete test summaries are failures.
Every announced test binary/block must have its own successful summary and matching
test count; an earlier successful binary cannot hide a later incomplete one.
Current UI checks additionally require actual `tests/ui/` case output. Each command
reports its elapsed time, timeout and exit status in a `CI_CHECK` line so local
cold-run timings can be compared with the configured per-check limits.

Each command owns a separate POSIX process group. On timeout, `run_check.py` sends
TERM to that group, allows at most five seconds for it to exit, then sends KILL to
remaining members even if the original leader has already exited. Reaping the
leader is separately bounded. Timeout returns 124; a cleanup failure also fails
the check. The regression suite includes a real TERM-resistant descendant and
missing-script, nonzero-exit and empty-test controls.

The `native-callback-ui` check runs on the current compiler only and verifies the
checked-in diagnostic showing that a native callback guard is not `Send`. MSRV's
policy explicitly omits this current diagnostic; its runtime/admission tests still
run. Direct invocation with the skip variable verifies the actual MSRV version and
prints a named skipped-diagnostic message rather than consuming current stderr.

The `wire-constructor-codegen` probe runs on both compiler roles. It obtains the
exact SDK rlib from Cargo JSON build output, invokes rustc with code generation on
a native-required constructor fixture, requires the specific E0080 assertion,
and builds/runs an otherwise identical control with the requirement false.
`cargo check` alone does not evaluate this generic constructor assertion.

Both compiler roles run native descriptor/agreement tests and doctests with only
`native-identity`. The dependency check inspects Cargo's normal/build graph to
prove default message-only use excludes `sha2`, while explicit native identity
and selected-wire codec capabilities include it. Dev-only fixture hashing does
not count as a runtime dependency.

The native-only library check includes the complete available codec and descriptor
suite. Workspace library checks also execute the derive crate's error-branch tests.
Both compilers run inventory/duplicate-registration tests and exactly one dedicated
`native_path_zero_alloc` run with eight test threads; allocation is not included
in a second integration battery. Counts cover allocation, release and reallocation
through the `heap_guard` module.
Minimal zero-copy/inventory checks qualify their feature closures. Current alone
executes the stable-payload and owned-frame diagnostic batteries, including the
tuple initializer pass case; MSRV explicitly omits current compiler diagnostics.

Default/core SDK checks select `-p up-rust` so tool/fixture dependencies cannot
unify native features into a supposedly minimal SDK build. All-feature workspace
checks still cover every member, including the deployment tool's library tests.
The selected-wire UI verifies privacy and explicit unsafe calls on current Rust;
the constructor codegen probe checks the real stable wire and executes its explicit
agreement constructor on both compilers.

`native-tool-cli` prebuilds the real exporter and tool, captures Cargo's executable
paths/hashes, then runs the Rust `CARGO_BIN_EXE` CLI test through all five subcommands.
Its unique result directory retains commands, output streams and the ten generated
artifact inputs/outputs. Direct CLI-test invocation without the prebuilt exporter
uses an isolated Cargo target directory to avoid waiting on the parent's target lock.

Both roles execute stable-native communication tests with the minimal
`up-l2-zero-copy` feature and again with `usubscription` for subscriber interaction.
The in-memory encoded core drives publish, notification and correlated RPC delivery;
tests check typed payloads and owned-lease provenance. These are SDK role contracts,
with native storage/copy and worker acceptance remaining in the binding's gates.

Both roles also run the typed and application role suites: `typed-application`,
`user-first-application`, `user-first-bindings`, `user-first-recovery`,
`native-retained-cleanup`, `independent-roles`, `independent-native-roles` and
`owned-rpc-join`. They share one feature union that is a superset of every
target's `required-features`, so one SDK build serves them. They exercise SDK
contracts over in-process carriage and `PoolCore`, not installed middleware.
`serialized-dispatch` runs the dispatch allocation controls, which count
calling-thread allocations around owned send dispatch, with their declared
feature set; `serialized-dispatch-release` repeats them in the release profile.
The compile-fail fixtures `tests/ui/serialized_dispatch_*.rs` and
`tests/ui/dispatch_controls_*.rs` have no in-repository trybuild harness and are
not run by this policy. The `typed_door`, `user_first_door` and
`serialized_inline_transport` examples are checked like the other examples. The
`hello_publisher`, `hello_notify` and `hello_rpc` examples are run, not only checked,
because they are the first programs a newcomer executes.

Both compiler roles also run the harness-free prepared allocation gate in debug and
release. The auditor requires every named subject exactly once, all-thread counters,
zero allocation calls and bytes, and final completion; a missing/duplicate row is a
failure. The bounded release benchmark requires ten observations of all fourteen
subjects, zeros for its seven prepared-path subjects, and a positive heap-allocation
control. These include full header construction and encoding; no smoke override
can satisfy acceptance. Setup warms every runtime/delivery thread, and a new RPC
client's first contact stays inside the counting window.
RPC windows also wait for bounded publication-accounting completion inside the
measurement. A received reply can precede the server's final telemetry update on
another worker; all exact reply/count assertions remain required.

The `prepared-core-runtime-minimal` check selects the SDK package with only
`zero-copy-transport,transport-implementer-api`. Protobuf-only fixture tests are
explicitly gated and are exercised by the all-feature library check. The external
wire consumer and minimal native-role doctests are also canonical checks on both
compilers; workspace feature unification cannot supply their missing capabilities.

Core/full payload fixture checks pin the catalog digest, which includes the
stored-vector manifest, verify all eight stored file hashes, and validate enabled
fixture types against bytes read from disk. Large fixtures remain explicitly gated
by their feature. The ignored vector updater is not a normal qualification check.

The `two-mode-release-order` check proves the order in which an RPC call slot is
given back, with no scheduler and no timing: disposal runs code the test controls,
a frame's `Drop` and the wake hook's clear event, and from there it tries to claim
the slot being released, from the same thread and from another, and must be refused.

## `unsafe` inventory

`unsafe_inventory.toml` lists every `unsafe` site in `core/`, `macros/`, `src/`,
`up-rust-tokio/`, `test-fixtures/`, `tests/`, `benches/`, `examples/`, `tools/`,
`abi/` and `build.rs`: each `unsafe` block,
`unsafe fn`, `unsafe impl`, `unsafe trait`, `unsafe extern` and `#[unsafe(...)]`.
A site is identified by its file, its enclosing item and its normalized text, so
moving code never disturbs the list and changing an `unsafe` site always does.
Each site names a row of the design's table of `unsafe` obligations, `[rows]` in
the file, or states a test-only reason.

`check_unsafe_inventory.py` fails on a new site, a listed site that has gone, a
site with no reason and a reason naming a row that does not exist. When it fails,
run it with `--update`, write a reason for each new entry and review that change
like the `unsafe` code itself. `--update` keeps existing reasons, and a new site
stays failing until it has one. A test-only reason is refused outside `test_paths`
and `#[cfg(test)]` items. The scanner is a lexer for the parts of Rust that can
hide a word, not a parser: comments, doc comments, strings and raw strings neither
create a site nor hide a real one. Line numbers are never part of an identity.

Other crates use the same script through path arguments:

```sh
python3 tools/ci/check_unsafe_inventory.py --root ../up-wire-xcdrv2 --paths src tests --require-zero
python3 tools/ci/check_unsafe_inventory.py --root ../up-transport-iceoryx2 --paths src tests --inventory unsafe_inventory.toml
```

Logs live in `${CARGO_TARGET_DIR:-target}/ci-logs/<configuration>/`. CI uploads the
current test group's logs as `test-results`, preserving the artifact name and
workflow output consumed by `release.yaml`. The artifact is readable command/test
output, not JUnit XML.

## Isolated feature combinations

The independent feature job uses a verified cargo-hack release and the policy's
current Rust. The runner accepts the qualified versions 0.6.45 and 0.6.46; other
versions fail closed until qualified.
For an offline local run with dependencies already cached:

```sh
python3 tools/ci/feature_gate.py --root "$PWD" --toolchain stable \
  --target "$PWD/../up-rust-feature-target" \
  --result "$PWD/../up-rust-feature-results"
```

Use a fresh result directory outside the checkout, separate from the reusable
target cache. The runner exports the exact source index and clean specification
gitlink, then removes dev-dependencies only in that snapshot. The owning lock seeds
an explicit offline setup resolution there. Every resulting package name, version,
source and checksum must already exist in the owning lock. Subsequent powerset
checks are locked and offline, and both the derived lock and owning source remain
unchanged. Staged local candidates are supported when worktree bytes match the index.

One owning-source content observation at entry and one at exit enclose all
isolated checks, including failure paths. Source identity and specification
checks remain required; no mutable-file metadata cache substitutes for content
verification. The result explicitly records this shared observation boundary.

The complete ordered progress inventory determines the actual combination count;
missing, duplicate or inconsistent cases fail. The result directory retains the
compiler/environment details, tested case count, isolated sources, derived lock
and command output for diagnosing failures. No product build-job or test-thread cap is added.
The shared bounded process-group cleanup also applies to feature commands. CI
uploads these records as `feature-results`, independently of `test-results`.
