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

Current Rust 1.95.0 owns formatting, strict Clippy, strict documentation generation,
runtime/doctests and dependency policy. MSRV Rust 1.88.0 checks all targets and runs
the same library, doctest and URI/UUID TCK targets. Library and doctest checks retain
the minimal, core/protobuf and all-feature configurations. Both deny compiler/rustdoc
warnings. Install cargo-deny 0.19.0 for the current dependency group. All Cargo
project checks use the existing lock; configure Cargo offline when the dependency
cache is already populated. Neither build jobs nor workspace test threads are
capped by product policy.

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
in a second integration battery. Counts cover alloc, alloc_zeroed and realloc.
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
paths/hashes, then runs the Rust `CARGO_BIN_EXE` CLI test through all six subcommands.
Its unique proof directory retains commands, output streams and the nine generated
artifact inputs/outputs. Direct CLI-test invocation without the prebuilt exporter
uses an isolated Cargo target directory to avoid waiting on the parent's target lock.

Both roles execute stable-native communication tests with the minimal
`up-l2-zero-copy` feature and again with `usubscription` for subscriber interaction.
The in-memory encoded core drives publish, notification and correlated RPC delivery;
tests check typed payloads and owned-lease provenance. These are SDK role contracts,
with native storage/copy and worker acceptance remaining in the binding's gates.

Logs live in `${CARGO_TARGET_DIR:-target}/ci-logs/<configuration>/`. CI uploads the
current test group's logs as `test-results`, preserving the artifact name and
workflow output consumed by `release.yaml`. The artifact is readable command/test
output, not JUnit XML. Local command logs are not a source-provenance database.
