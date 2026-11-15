# Peer rates on shared routes are the integrator's

`uman~sdk-aou-peer-rates~1`

Covers:
- aou~seooc-peer-rates~2

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- Revision 2 includes the corrected observation/progress duties and strict report obligation.
- It supplies no new quota implementation, counter evidence or product acceptance.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> On routes that several peers share, the integrator *SHALL* bound each peer's message rate, by deployment and scheduling, or accept that one faulty peer can fill a route's depth and so starve the others there.
>
> * *What the element does.* The element applies the route's overflow policy and counts each discard it or its binding can observe; losses entirely inside middleware that the binding cannot observe are explicitly disclosed in the route's report (`+req~ssr-qos-overflow~3+`, `+req~ssr-error-no-silent-loss~3+`). It has no per-peer quota.
> * *Peer-specific progress.* Where the item's safety reaction requires detecting a particular peer's lack of progress, the integrator *SHALL* supply peer-specific progress expectations and application checks, or deployment and scheduling measures that meet that requirement; an aggregate route deadline does not supply that detection.
> * *Strict mode.* In strict mode, the report warns of an ASIL route with several declared senders and no declared per-peer rate, and lists each shared route with what one faulty peer can starve there.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Bound each peer's rate, or accept the consequence.**
  - For every multi-sender route, document per-peer message-rate/burst assumptions and how deployment/scheduling bounds them, or explicitly accept that a faulty sender may monopolize depth.
  - No per-peer quota is supplied by this proposal.
- [ ] **Detect a missing peer where the item needs it.**
  - Supply application-level per-peer progress checks, independent deadlines or another verified mechanism where missing service from one peer must be detected.
  - Budget their observation and reaction delay.
- [ ] **Review overflow and discard observability.**
  - Review route overflow behavior and the binding's discard observability.
  - Count only observable discards, and retain declared middleware blind spots.
  - Do not infer every middleware eviction from the presence of a counter field.
- [ ] **Qualify the shared route under flooding.**
  - Qualify the shared route with one continuously flooding peer and another whose useful frames are starved.
  - Check that the item's own mechanism detects the required failure even while aggregate arrivals continue.
  - When the future strict report exists, inspect its no-declared-rate warning and description of what a peer can starve. A warning is not an enforced quota.

## Violation manifestations

- A peer can fill a shared route and starve others.
- Aggregate arrival deadlines may remain healthy.
- Aggregate drops may show congestion without identifying the missing peer.

## Detection limits

- Only observable discards can be counted. Middleware loss beyond the binding's observation is disclosed separately.
- No guarantee of every middleware discard count or aggregate-deadline starvation detection is made.
- Declared rates do not constrain a faulty peer unless external enforcement actually does so.
