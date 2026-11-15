# Deployment data is in range, verified and protected

`uman~sdk-aou-deployment-data~2`

Covers:
- aou~seooc-deployment-data~5

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- The values-only schema requirement remains uncovered. Prescribing the integrator's duty here is not evidence that all loader paths enforce it.
- Its inventory names E2E tables only for a route that selects the deferred E2E wire.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* produce the deployment data within the admissible ranges the element publishes, and verifies it for the item. The data covers:
>
> * routes and their QoS;
> * each transport's integrity level;
> * RPC client lists and expected peers;
> * capacities;
> * iceoryx2 service depths;
> * for each route that uses the E2E wire, its senders, from which its data IDs are derived, its `+resync_frames+`, and, *(b)*, for each sender, its start value's source and whether its starts are declared increasing;
> * the selection of compiled-in waivers.
>
> The integrator protects the data against unintended change between release and use, and delivers the Streamer's deployment data with an integrity code over its content, which the Streamer checks.
>
> Every peer that a route *SHALL* accept is declared at startup, because setup is impossible at runtime: adding an RPC client on a lending transport means restarting the server with new deployment data.
>
> In strict mode, the Enforce state refuses deployment data without a matching integrity code, and the report shows the data's digest, and what was declared and checked.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Keep a complete, reviewed deployment inventory.**
  - Version and review a complete deployment inventory: routes/QoS, transport integrity levels, expected peers and RPC clients, every capacity, iceoryx2 depths and selected compiled-in waiver IDs.
  - For a route that selects the deferred E2E wire, once it is delivered, also include its senders, data IDs, resync_frames and start sources.
- [ ] **Validate values, and accept values only.**
  - Validate all ranges and cross-references against the actual implemented schema.
  - Accept values only. Do not add scripts, expressions, dynamically loaded code or arbitrary waiver definitions.
- [ ] **Protect released data.**
  - Protect released data against unintended change through provisioning and startup.
  - Plan the Streamer integrity-code check and verify it on the future implementation.
  - A digest stored alongside mutable data alone does not establish authorized provenance.
- [ ] **Declare every peer and role before runtime.**
  - Declare every intended peer/role before runtime.
  - Change deployment through whole-process restart with new initialization data, including adding lending RPC clients.
  - Do not invent a runtime setup escape hatch.
- [ ] **Retain the release record.**
  - Retain the reviewed data, content identity, configuration owner and deployment-specific qualification with the release.

## Violation manifestations

- Wrong routes, capacities or peer lists can refuse legitimate traffic or admit wrong traffic.
- Unchecked data changes can invalidate the reviewed deployment.
- An undeclared client may be unreachable until restart.

## Detection limits

- Schema validation cannot decide whether values are safe for the item.
- The current manual supplies no completed deployment schema, loader integrity check or strict report.
