# An integrator-supplied platform keeps the platform's rules

`uman~sdk-aou-integrator-platform~1`

Covers:
- aou~seooc-integrator-platform~3

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them. No platform implementation/test is added or qualified here. Strict evidence refusal/reporting is prospective.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> On a target with no shipped backend, the integrator *SHALL* supply the platform:
>
> * a runtime platform that keeps the rules of `+req~ssr-platform-contract~1+`: no heap, no panic, no thread creation and no global state that needs first-use setup; a monotonic clock that never fails; a wall clock that reports a failed read instead of panicking; and a wait that never loses a wake;
> * an initialization-only platform for prefault, and for entropy that differs at each start of the process and is reported as failed rather than replaced by a fixed value.
>
> It passes the platform contract test in `+test-util+` on that target. In strict mode, the Enforce state refuses an integrator platform that names no contract-test evidence, and the report lists the entropy duty and the platform contract test.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] Implement the runtime platform for the target with no heap activity, panic, thread creation or first-use global initialization. Supply a monotonic clock that never fails, a wall clock that reports failure and a wait/wake protocol that loses no wake.
- [ ] Keep prefault and start-distinct entropy in initialization. Report entropy failure; never substitute a fixed seed. Exercise startup entropy failures and runtime wake/clock races separately.
- [ ] Run the platform contract on the actual target, preserving toolchain/backend/configuration identity and results. Review ordering, interrupt constraints, timing and stack use as part of the platform qualification.
- [ ] For a future strict release, associate the platform declaration with its real contract evidence and inspect the report; a reference without successful applicable evidence is insufficient.

## Violation manifestations

Lost wakes can stall traffic; a failed monotonic source invalidates waits; lazy runtime initialization can allocate; repeated entropy can repeat identities or weaken restart assumptions.

## Detection limits

Contract tests cover observed cases and do not prove all target schedules or OS behavior. The SDK cannot validate the quality of an external entropy source merely from a successful return.
