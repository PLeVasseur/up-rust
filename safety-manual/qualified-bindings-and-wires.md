# Bindings and wires pass their contracts

`uman~sdk-aou-qualified-bindings-and-wires~4`

Covers:
- aou~seooc-qualified-bindings-and-wires~7

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- No binding README uman coverage is claimed by this SDK manual. Binding owners must provide it separately.
- Revision 5 bound qualification to exact versions and configuration and to the evidence state each record has.
- Revision 6 adds one duty: where the item relies on a stronger resource guarantee on a serialized route, the integrator requires it by name.
- Revision 6 also describes the contract suites and the runtime port's reports instead of naming SDK identifiers. The identifiers are in the SDK's implementation traceability note.
- Revision 7 keeps the qualification duty, with revision 6's wording. The specification states it with its transports, in `up-spec/safety/up-l1/deployment.adoc`.
- The duty that revision 6 added is now the [resource-guarantees](resource-guarantees.md) entry, with its wording.
- The iceoryx2 limits, anomalies and known failures are now `aou~seooc-iceoryx2-limits~1`. The iceoryx2 binding's safety manual states it, and this manual has no entry for it. A deployment on another transport reads that transport's own README for its limits.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* use on the ASIL path only bindings and wires that pass the contract suites (the binding contract suite, the QoS contract suite and the wire contract suite) on the integrator's own target, at the exact versions and configuration whose evidence its supported configuration records, and *SHALL NOT* take evidence of one state, compiled, linked, exercised, source-reviewed, composition-verified or qualified, for another.
>
> * *32-bit targets.* On a 32-bit target, those are only the bindings and wires whose contracts pass on that target.
> * *Pull bindings and deadlines.* A pull binding used for a route with a deadline reports an arrival through the runtime port only with evidence that a frame entered the route's queue, or that the queue's overflow policy discarded one, and within the arrival-report latency it declares in its QoS support. It never reports one on a wake hint alone. It reports each failure to read its own queue through the runtime port's receive-failure report. Its deadline checks of the QoS contract suite pass, on a subscriber and on a carrier ingress.
> * *Response routes.* A push binding delivers one response route from one thread at a time, which the contract checks. A pull binding needs nothing: the element serializes the pulls of a response route itself.
> * *Cores without coherent atomics.* A route between cores whose atomics are not coherent with each other (`+aou~seooc-target-environment~4+`) uses a transport whose qualification covers that.
>
> The integrator accepts the limits and known anomalies that each binding's README states (`+req~ssr-binding-readme~1+`). For a deployment that uses iceoryx2, `+aou~seooc-iceoryx2-limits~1+` lists them, and lists apart that binding's known failures, which are not accepted limits.
>
> A wire or binding from elsewhere with `+unsafe+` code of its own, such as a wire over a C codec, is a separate element, and the integrator qualifies its `+unsafe+` (`+req~ssr-unsafe-inventory~3+`). The bindings this project ships keep their own inventories under the same gate (`+req~ssr-binding-unsafe-inventory~1+`).
>
> In strict mode, the Enforce state refuses a binding or wire that is not on the integrator's qualified list, an ASIL process that reaches a deviation not acknowledged at its current revision, and any known failure, whatever is acknowledged. The report lists each binding's README limits, deviations and known failures.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Maintain a qualified binding and wire list.**
  - Maintain a qualified binding/wire list keyed by version, target, toolchain, features, configuration and contract evidence.
  - Record each piece of evidence by its state (compiled, linked, exercised, source-reviewed, composition-verified or qualified) without promoting one to another.
  - Require binding, QoS and wire contracts on the actual target, including response-thread serialization and subscriber/carrier arrival/deadline behavior.
- [ ] **Inspect queue-arrival evidence.**
  - Arrivals require queue admission or overflow discard, not a wake hint.
  - Queue-read failures need separate reporting.
  - Validate the declared latency and loss-observability capability.
  - Do not claim every middleware discard is counted.
- [ ] **Read each binding README and its records.**
  - Separate accepted anomalies, open known failures and heap waivers by ID/revision.
  - For iceoryx2, complete the binding's own entry for `aou~seooc-iceoryx2-limits~1`, which lists them.
- [ ] **Do not approve deployments affected by known failures.**
  - Do not approve a deployment affected by a binding's known failure merely by acknowledging it.
  - Verify each fix and its affected paths before removing the restriction.
- [ ] **Qualify external unsafe code, and mind the target.**
  - Qualify external unsafe code as its own element and retain each shipped binding's unsafe inventory.
  - For non-coherent inter-core routes use a transport qualified to synchronize those cores.
  - On a 32-bit target, use only bindings and wires whose contracts pass on that target.

## Violation manifestations

- An unqualified binding can lose arrivals, misreport counters, block frame release, panic on a peer fault, corrupt shared state or allocate without a valid waiver.
- These manifestations can violate the item even if a mock contract passed.

## Detection limits

- A suite pass is limited to its exact bindings and target. It does not erase known failures or establish all deployed loads.
- Strict known-failure refusal is required but not claimed implemented by this manual.
- The absence of a failure report does not show all paths were exercised.
