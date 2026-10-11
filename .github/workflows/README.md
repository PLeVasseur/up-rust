# Which automations do we run?

This file is meant to provide an overview and explainer on what the up-rust workflow automation strategy is, and what the different workflow elements do.

The SDK check workflow uses the current and MSRV compiler versions declared in
[`tools/ci/policy.yaml`](../../tools/ci/policy.yaml): Rust 1.95.0 and 1.88.0.
Other workflows declare their toolchain separately; workflows using the
`RUST_TOOLCHAIN` variable retain their own default and override behavior.

At this time, there are three events that will initiate a workflow run:

## PRs and merges to main

[`check.yaml`](check.yaml) is generated from the repository's command policy.
Each compiler/group job invokes the same `tools/ci/gate.sh` command available
locally. Current Rust owns strict quality checks and compiler-diagnostic fixtures;
MSRV runs its declared build and runtime coverage. Policy does not cap workspace
build jobs or test threads. The dedicated allocation target uses eight threads.

Generate and verify both shell/YAML outputs with the commands in the
[CI guide](../../tools/ci/README.md). Missing targets, incomplete test summaries
and command timeouts fail. Timeout cleanup includes descendants that survive the
leader's exit. The independent dependency-policy workflow, link checking and
feature-powerset jobs retain their separate responsibilities.

The current test job publishes readable command logs as `test-results`, preserving
the artifact name consumed by [`release.yaml`](release.yaml); these are not JUnit
XML. Local runs establish local results, while hosted jobs report their own status.

## Release publication

We want exhaustive tests and all possible checks, as well as creation of license reports, collection of quality artifacts and publication to crates.io. This workflow pulls in other pieces like the build workflow. An actual release is triggered by pushing a tag that begins with 'v', else this workflow just generates and collects artifacts on workflow level. This will also publish to crates.io if the CRATES_TOKEN secret is set.

This is implemented in [`release.yaml`](release.yaml)

## Nightly, out of everyone's way

All the tests we can think of, however long they might take. For instance, we can build up-rust for different architectures - this might not really create many insights, but doesn't hurt to try either, and fits nicely into a nightly build schedule.

This is implemented in [`nightly.yaml`](nightly.yaml)

## uProtocol specification compatibility

The uProtocol specification is evolving over time. In order to discover any discrepancies between the currently implemented version and the changes being introduced to the specification, we perform a nightly check that verifies if the current up-rust code base on the main branch can be compiled and test can be run successfully using the most recent revision of the uProtocol specification.

This is implemented in [`latest-up-spec-compatibility.yaml`](latest-up-spec-compatibility.yaml)

## Further workflow modules

In addition to the main workflows described above, there exist a number of modules that are used by these main workflows. These live in the [uProtocol CI/CD repository](https://github.com/eclipse-uprotocol/ci-cd)
