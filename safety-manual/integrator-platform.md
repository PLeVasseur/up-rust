# A custom platform keeps the platform's rules

`uman~sdk-aou-integrator-platform~2`

Covers:
- aou~seooc-integrator-platform~4

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- No platform implementation/test is added or qualified here.
- Strict evidence refusal/reporting is prospective.
- Revision 4 makes a custom platform an explicit extension: routine supported use selects a maintainer package instead, and entropy comes from a declared source with a stated contract.
- This entry applies only to a custom platform, for a target or configuration that no package supports.
- A team that uses a supported configuration unchanged has nothing to do here, and target-environment covers its checks.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> A supported configuration's platform backend is its maintainers' (<<aou-seooc-target-environment,The target environment is verified>>), and an application team that selects one supplies no backend of its own. An integrator that supplies a platform for a target or configuration that no package supports *SHALL* treat it as an explicit extension, with its own authoring contract and evidence boundary, and supply:
>
> * a runtime platform that keeps the rules of xref:platform.adoc#req-ssr-platform-contract[Every platform backend keeps the platform's rules]: no heap, no panic, no thread creation and no global state that needs first-use setup; clock reads that report a failure instead of panicking or substituting another time base; a wait that never loses a wake; and the contracts that a package states, written down for this platform;
> * an initialization-only platform for prefault, and for entropy from a declared source with its stated contract, reported as failed rather than replaced by a fixed value (xref:platform.adoc#req-ssr-platform-entropy[Initialization entropy comes from a declared source]);
> * a source-level safety argument for its synchronization and its atomic or exclusion mechanism, covering every participant that can reach that mechanism's storage (xref:platform.adoc#req-ssr-atomic-width[No target needs an atomic width it lacks]).
>
> It passes the platform contract test in `+test-util+` on that target.
>
> In strict mode, the Enforce state refuses an integrator platform that names no contract-test evidence, and the report lists the entropy duty and the platform contract test.
<!-- aou-statement:end -->

## Integrator actions

Where the item uses a custom platform, the integrator SHALL complete and retain the following checklist for it; it does not apply to a supported configuration used unchanged:

- [ ] **Implement the runtime platform.**
  - Implement the runtime platform for the target with no heap activity, panic, thread creation or first-use global initialization.
  - Supply clock reads that report failure instead of panicking or substituting another time base, and a wait/wake protocol that loses no wake.
  - Write down the contracts a package would state.
- [ ] **Keep prefault and entropy in initialization.**
  - Keep prefault and entropy in initialization, from a declared source with its stated collision probability or persistent construction.
  - Report entropy failure. Never substitute a fixed seed.
  - Exercise startup entropy failures and runtime wake/clock races separately.
- [ ] **Run the platform contract on the target.**
  - Run the platform contract on the actual target, preserving toolchain/backend/configuration identity and results.
  - Review ordering, interrupt constraints, timing and stack use as part of the platform qualification.
- [ ] **Associate the declaration with real evidence.**
  - For a future strict release, associate the platform declaration with its real contract evidence and inspect the report.
  - A reference without successful applicable evidence is insufficient.
- [ ] **Argue the synchronization mechanism at source level.**
  - Give the platform's synchronization and atomic or exclusion mechanism a source-level safety argument that covers every participant that can actually reach its storage, interrupts and DMA included where the deployment has them.

## Violation manifestations

- Lost wakes can stall traffic.
- A failed clock read that is not handled fail-closed invalidates waits.
- Lazy runtime initialization can allocate.
- Repeated entropy can repeat identities or weaken restart assumptions.

## Detection limits

- Contract tests cover observed cases and do not prove all target schedules or OS behavior.
- The SDK cannot validate the quality of an external entropy source merely from a successful return.
