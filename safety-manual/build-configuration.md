# The ASIL binary is built from its claimed graph, with the ASIL profile

`uman~sdk-aou-build-configuration~2`

Covers:
- aou~seooc-build-configuration~5

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them. This package changes no Rust build or artifact. Strict compile-time exclusion and build-report checks remain implementation work. Revision 5 judges the binary by its claimed safety dependency graph and exact toolchain scope, and leaves code outside that graph, such as the up-rust compatibility facade, async integrations and the classic Streamer, to ordinary use. A crate is in or out of the graph by what it actually brings into the image, not by its name.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* build the ASIL binary so that, at run time, it depends only on its claimed safety dependency graph: `+up-rust-core+`, the ASIL Streamer crate where it is used, the bindings, wires and platform backends its configuration qualifies, and any optional layer its configuration admits with its own evidence (`+req~ssr-crate-core-only-binary~2+`). It *SHALL NOT* link code from outside that graph into the binary's image. A crate belongs to the graph by what it actually brings into the image, with its features and target-selected paths, never by its name: a compatibility re-export is judged by the dependencies it brings, and code outside the graph, such as the `+up-rust+` compatibility facade, an async integration or the classic Streamer, keeps its ordinary use, which claims no assurance scope. It *SHALL* use the exact compiler release, target, library modules and options that its configuration records for the release, with the supplier's qualification evidence for that scope where the claim needs it; the minimum supported Rust version, 1.88 today, and the availability of an API are not that evidence. It *SHALL* use the ASIL profile: `+panic = "abort"+` and `+overflow-checks = true+`. In strict mode's locked build, `+--cfg up_rust_strict+` in the integrator's build configuration allows only the Enforce state, and refuses a crate or route that the declared graph does not admit, naming its replacement (`+req~ssr-strict-lock~2+`). Strict mode also refuses an ASIL process built with unwinding, and `+assert_asil_build!()+` gives any ASIL binary the Streamer binary's check; its report lists overflow checks as a checklist item.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] Inventory the final ASIL binary's actual runtime dependency graph and configuration: core, any ASIL Streamer crate, the qualified bindings, wires and platform backends, and only those optional layers that the configuration admits with their own evidence (none at this revision). State that graph with the configuration and check the final image against it, including features and target-selected paths. Judge each crate by what it brings into the image: a compatibility facade, an async integration or a classic Streamer crate stays out because of the code its graph brings, and a re-export is judged by the dependencies it actually brings.
- [ ] Pin the exact compiler release, target, library modules, options and features that the configuration records, with the supplier's qualification evidence for that scope where the claim needs it (MSRV and API availability are not that evidence); record panic=abort and overflow-checks=true for the shipped artifact. Check the actual build configuration and linked binary, including dependency feature unification.
- [ ] Do not rely on a proposed up_rust_strict lock or assert_asil_build! macro until implemented and verified. When available, qualify both intended acceptance and rejection of crates outside the declared graph and of unwinding; keep overflow settings in the deployment checklist.

## Violation manifestations

An unwanted runtime dependency can introduce heap use or incompatible recovery. Unwinding can run destructors in a runtime heap window; unchecked overflow can change behavior.

## Detection limits

A source profile does not prove the shipped artifact used it. Configuration flags alone do not qualify all transitive code or imply ASIL certification.
