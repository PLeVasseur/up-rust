# The ASIL binary is built from the safety crates, with the ASIL profile

`uman~sdk-aou-build-configuration~1`

Covers:
- aou~seooc-build-configuration~4

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them. This package changes no Rust build or artifact. Strict compile-time exclusion and build-report checks remain implementation work.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* build the ASIL binary so that, at run time, it depends only on `+up-rust-core+`, the ASIL Streamer crate where it is used, and the bindings and wires it uses. It *SHALL NOT* link a QM crate: no `+up-rust+`, no `+up-rust-tokio+`, no classic Streamer. It *SHALL* use the toolchain versions and target listed for the release (stable and 1.88.0 today), and the ASIL profile: `+panic = "abort"+` and `+overflow-checks = true+`. In strict mode's locked build, `+--cfg up_rust_strict+` in the integrator's build configuration allows only the Enforce state, and makes the QM-only runtime crates fail to compile, each naming its replacement: `+up-rust+`, `+up-rust-tokio+`, the wires that use the heap, and the classic Streamer. Strict mode also refuses an ASIL process built with unwinding, and `+assert_asil_build!()+` gives any ASIL binary the Streamer binary's check; its report lists overflow checks as a checklist item.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] Inventory the final ASIL binary’s runtime dependency graph: direct core, any ASIL Streamer crate and qualified bindings/wires only. Exclude up-rust, up-rust-tokio and classic Streamer runtime dependencies from that path.
- [ ] Pin release toolchains, target and features; record panic=abort and overflow-checks=true for the shipped artifact. Check the actual build configuration and linked binary, including dependency feature unification.
- [ ] Do not rely on a proposed up_rust_strict lock or assert_asil_build! macro until implemented and verified. When available, qualify both intended acceptance and rejection of QM crates/unwinding; keep overflow settings in the deployment checklist.

## Violation manifestations

An unwanted runtime dependency can introduce heap use or incompatible recovery. Unwinding can run destructors in a runtime heap window; unchecked overflow can change behavior.

## Detection limits

A source profile does not prove the shipped artifact used it. Configuration flags alone do not qualify all transitive code or imply ASIL certification.
