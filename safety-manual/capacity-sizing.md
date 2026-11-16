# Capacities are sized for the worst case

`uman~sdk-aou-capacity-sizing~2`

Covers:
- aou~seooc-capacity-sizing~3

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- Revision 3 has the application declare each route's semantic worst-case load.
- The element computes its capacities from that load, including asynchronous registrations, retained owners, deferred releases and deferred destructions, stating what each figure measures.
- The computation (MEM-1) and the reporting implementation are not established yet and still need qualification.
- The statement now cites the raised revision of the lease-drop requirement, with no other change.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* declare the item's worst-case semantic load for each route: its traffic, payload and token sizes, retention, peers and calls in flight.
>
> * *What the element computes.* The element computes from those declarations the implementation capacities it needs, slots, queue depth, held messages, peer and call tables, registration storage for pending asynchronous operations, the backlog of retained owners, deferred releases and deferred destructions, and startup memory, states what each figure measures, and checks them at setup (xref:startup-memory.adoc#req-ssr-memory-computable[Startup storage is computable before start]); the integrator does not derive the element's internal sizes, and the supported configuration's package states the sizes that depend on its scheduler, allocator or stacks.
> * *Exhaustion.* In runtime mode exhaustion is a refusal, not a failure of the element; capacity that a release still pending holds is exhausted the same way, and fixed capacity promises neither its recovery nor sustained throughput within a time unless the supported configuration's package establishes the release progress on which such a claim relies (xref:release.adoc#req-ssr-release-lease-drop[A lease gives back its slot and nothing else]). Whether the item tolerates that refusal is the integrator's question.
> * *Strict mode.* In strict mode, the report lists the startup memory each setup computes, and every route's capacities.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Declare the worst-case semantic load.**
  - Declare each route's worst-case semantic load: traffic, payload and token sizes, retention, peers and calls in flight.
  - Once the element's computation is available, review the slots, queue depths, retained-owner and deferred-release backlog, peer entries and startup memory it derives. Do not derive internal sizes by hand.
  - Check what each computed figure measures: usable capacity, reserved storage, retained backing, transient setup memory or stack. A live-object count is not the retained or physical footprint.
- [ ] **Include the hard cases in the load.**
  - Include burst overlap, late/cancelled RPC traffic, held receive loans, releases that stay pending, restart/churn, overflow policy and bounded wait behavior.
  - Do not treat a measured average release delay as a bound: a claim that capacity recovers within a time needs the release progress that the supported configuration's package establishes.
  - Record numeric capacity calculations and the load assumptions behind them.
- [ ] **Exercise each capacity boundary.**
  - Exercise each capacity boundary and its refusal/recovery path on the final target.
  - Decide the item's safe response to exhaustion. Do not expand a pool at runtime.
  - On a native path, and on any route whose heap class is none, nothing allocates from the heap at runtime. A serialized route allocates only as its declared heap class states and setup admits.
  - On a route that does not require non-waiting release, a value's destruction can wait where the value ends, and its slot stays charged until it finishes. Size for that, or require the guarantee.
- [ ] **Review the computed figures.**
  - Review actual computed startup memory and capacities when a future strict report is available.
  - A configuration listing does not validate the capacity calculation.

## Violation manifestations

- Exhaustion refuses work and can cause loss of function, deadline misses or RPC expiry.
- Too-small retained/expired connection budgets can violate middleware lifetime assumptions.

## Detection limits

- The SDK cannot establish the item's worst-case load or determine whether refusal is safe.
- The required bounded refusal behavior still needs product qualification on affected paths.
