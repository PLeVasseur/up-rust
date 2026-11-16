# The application reacts to what the element reports

`uman~sdk-aou-application-reacts~2`

Covers:
- aou~seooc-application-reacts~4

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- Revision 4 places the application's report reads outside the element's detection bound J.
- It keeps the servicing duty only for a one-word tick monitor, which no current profile's runtime monitor uses.
- The statement was reworded in place: it describes the counters' difference operation instead of naming an SDK identifier, and no longer remarks on the present implementation. Its meaning is unchanged.
- The accepted E2E pre-adoption rule is exact when proven, otherwise explicitly unknown.
- Documentary gate admission and runtime qualification remain pending.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The application *SHALL* handle every error the element returns, and reads the counters it relies on, within its own fault reaction time.
>
> * *What the element reports.* The element reports every refusal and loss it can see. A loss it cannot see, such as a drop-oldest eviction inside the middleware, is declared in that route's report instead (xref:errors.adoc#req-ssr-error-no-silent-loss[No silent loss, and each loss counted once]). For such losses, and for silence, the application uses available deadline and end-to-end observations only within their declared detection limits; neither promises to detect every hidden loss.
> * *Shared routes, and routes without a deadline.* Another peer can keep a shared route's aggregate deadline refreshed while one peer is starved, so required peer-specific progress detection follows <<aou-seooc-peer-rates,Peer rates on shared routes are the integrator's>>. A route without a deadline, including an explicit `+event_only()+` route, supplies no deadline-based silence or delay detection.
> * *The reaction is the application's.* The element does not decide the item's reaction. Reading a route's report is the application's own observation: its delay adds to the item's reaction, outside the element's detection bound (xref:qos.adoc#req-ssr-qos-deadline[Deadlines are monitored]), and the element places no servicing duty on the application for that bound.
> * *Reading counters.* The application takes differences of the flow counters (`+carried+`, `+busy+`, `+parked+`) between reads fewer than 2^32^ increments apart. It reads a fault counter at 2^32^ − 1 as saturated: at least that many, which the route counters' difference operation reports as saturated, not as a difference.
> * *Keeping up.* A route's deadline measures arrivals at its queue, not the application's reads. An application that needs to know whether it keeps up *SHALL* read the observable discard counters and account for the route's disclosed unobservable-loss limits; zero counts alone do not establish that no frame was lost.
> * *Bad data.* Bad data is reported by its own refusal counters, which the application acts on: a peer that sends only frames refused when read shows there, and on an E2E route in E2E's figures, not in `+deadline_missed+`.
> * *One-word tick monitors.* Where a route's monitor uses the one-word tick representation, the application services that route at least once per 2^29^ of its monitor's ticks, 2^18^ of its thresholds; a thread blocked in the transport's `+wait+` meets this, because `+wait+` runs the service between inner waits of at most 2^28^ ticks of the shortest-scaled route on the transport. A full-width monitor needs no such servicing to keep its count.
> * *Strict mode.* In strict mode, the report lists for each route the counters the application reads and what each means, the E2E figures included.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Map errors and counters to reactions.**
  - Map every returned error and observable refusal/loss counter to an application reaction and maximum reaction time.
  - Record counter sampling intervals and demonstrate error handling under target fault injections.
- [ ] **Read counters within their windows.**
  - For proposed wrapping flow counters, compare reads fewer than 2^32 increments apart.
  - Treat a fault counter at 2^32−1 as saturated, not a recoverable exact difference.
  - Verify the final since behavior rather than assuming the future API exists.
- [ ] **Separate queue arrival from useful consumption.**
  - Read observable dropped_oldest/dropped_newest and refusal counters.
  - Examine each binding's explicit unobservable-loss declaration.
  - Do not infer losslessness from zero counts.
- [ ] **Use peer-specific progress expectations where individual service matters.**
  - Traffic from another peer can keep the aggregate deadline fresh while a peer starves.
  - Bad data refused at read can still have counted as a queue arrival.
  - A route without a deadline, including event_only, supplies no deadline-based silence or delay detection.
  - Neither deadline nor E2E observations promise to detect every hidden loss.
- [ ] **Service, read delay and E2E figures.**
  - Only where a route's monitor uses the one-word tick representation (no current profile's runtime monitor does), schedule service within 2^29 ticks / 2^18 thresholds and verify wait-driven servicing on the actual implementation. A full-width monitor needs none.
  - Count the application's own read delay in the item's reaction, not in the element's bound.
  - On a later route with the E2E wire, read the never-arrived-before-adoption count with its completeness and distinguish unknown from zero. Any unknown contribution leaves a cumulative total incomplete even after later exact contributions, and saturation is a separate lower-bound condition.
  - Define and qualify a reaction to unknown within the item's fault reaction time using other qualified observations as needed.

## Violation manifestations

- Ignored errors, saturated counters treated as ordinary differences, or infrequent reads can conceal loss.
- An application can fall behind while the route's arrival deadline continues to be satisfied.

## Detection limits

- Unobservable middleware drops cannot be reconstructed from absent counters.
- Aggregate deadlines do not detect all peer starvation or validate payloads.
- E2E figures are not implemented evidence here.
- The item's reaction remains external to the SDK.
