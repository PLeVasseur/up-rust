# The platform provides the clocks

`uman~sdk-aou-clock~1`

Covers:
- aou~seooc-clock~2

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them. This manual does not qualify any clock source or promise the future strict startup warning. The integrator must validate its real target.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The platform *SHALL* provide a monotonic clock and a wall clock. The wall clock is synchronized with every peer's time base, to within an error the integrator includes in its TTL and deadline choices, before the element's TTL decisions are relied on. UUIDv7 message IDs and TTL are defined in wall time. The TTL bound on an RPC request holds only while the client's and the server's wall clocks agree within the TTL. In strict mode, the report warns when the wall clock at setup reads earlier than the element's release date, a sign that it is not yet synchronised, and lists the time-sync source where one is declared.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] Identify the target monotonic clock and wall clock, their failure behavior and synchronization source. Qualify clock reads and wait behavior on the deployed backend.
- [ ] Before relying on TTL decisions, bound client/server and peer wall-clock disagreement and include that error in TTL/deadline choices. Define the response to sync loss, jumps or an unreadable wall clock.
- [ ] Retain startup synchronization evidence and runtime monitoring for a readable but incorrect wall clock. Check proposed startup release-date warnings only after that feature is implemented; they are not a synchronization proof.

## Violation manifestations

A skewed readable clock can reject useful data early or accept stale data. An unreadable clock can cause refusal; a failed monotonic source can invalidate waits and deadline budgets.

## Detection limits

A successful wall-clock read proves neither correctness nor synchronization. RPC TTL guarantees require the stated clock agreement. Proposed bounded last-good-time generation is not current qualification.
