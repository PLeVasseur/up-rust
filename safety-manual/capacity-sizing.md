# Capacities are sized for the worst case

`uman~sdk-aou-capacity-sizing~1`

Covers:
- aou~seooc-capacity-sizing~2

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them. Revision 2 records the computed-startup-memory and route-capacity reporting duty. The statement is byte-identical to frozen revision 1, but its revision accounting is corrected; the reporting implementation still needs qualification.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* size every capacity for the item's worst-case load: slots, queue depth, held messages, calls in flight, peer tables, payload and token sizes, and startup memory. In runtime mode exhaustion is a refusal, not a failure of the element. Whether the item tolerates that refusal is the integrator's question. In strict mode, the report lists the startup memory each setup computes, and every route's capacities.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] List worst-case concurrent slots, queue depths, retained messages, in-flight calls, peer entries, payload sizes, token sizes and startup memory for every route and shared resource.
- [ ] Include burst overlap, late/cancelled RPC traffic, held receive loans, restart/churn, overflow policy and bounded wait behavior. Record numeric capacity calculations and the load assumptions behind them.
- [ ] Exercise each capacity boundary and its refusal/recovery path on the final target. Decide the item’s safe response to exhaustion; do not expand a pool or allocate from the heap at runtime.
- [ ] Review actual computed startup memory and capacities when a future strict report is available. A configuration listing does not validate the capacity calculation.

## Violation manifestations

Exhaustion refuses work and can cause loss of function, deadline misses or RPC expiry. Too-small retained/expired connection budgets can violate middleware lifetime assumptions.

## Detection limits

The SDK cannot establish the item’s worst-case load or determine whether refusal is safe. The required bounded refusal behavior still needs product qualification on affected paths.
