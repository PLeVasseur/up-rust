# Applications build every transport and role before the last transition

`uman~sdk-aou-build-order~1`

Covers:
- aou~seooc-build-order~2

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- No mode API or lifecycle implementation changes here.
- The entry describes the integrator's duty against the frozen specification only.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> An application *SHALL* build every transport and every role it will use before the last of the transports that share one startup memory enters runtime mode; for the Streamer, before it calls `+enter_runtime_all+`.
>
> A setup kept alive without ever entering runtime mode is a defect of the application.
>
> In strict mode, the Enforce state refuses a setup still alive and not entered when `+enter_runtime_all+` returns, and the report lists every such setup.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Build everything before the last transition.**
  - Construct every transport and role sharing a startup-memory owner before the last participating transport enters runtime.
  - Keep an explicit initialization inventory and activation order.
  - The Streamer's enter_runtime_all contract is future Stage7 work.
- [ ] **Account for every setup object.**
  - Account for every setup object, including optional/error branches.
  - Do not leave a setup permanently alive and unentered.
  - On setup/transition failure follow whole-process recovery, not runtime reconfiguration.
- [ ] **Test the lifecycle.**
  - Test the final lifecycle ordering and failure paths.
  - Verify traffic is unavailable before runtime and setup unavailable after runtime.
  - Do not rely on a proposed strict post-entry report to fix an incorrect construction sequence.

## Violation manifestations

- Premature activation can prevent creating required roles or strand setup resources.
- A never-entered setup can retain startup state and leave a partially available system.

## Detection limits

- A list of objects does not prove every initialization branch enters or exits correctly.
- Strict detection of leftover setups and the Streamer activation/report mechanism are not supplied by this manual.
