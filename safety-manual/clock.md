# The platform provides the clocks

`uman~sdk-aou-clock~2`

Covers:
- aou~seooc-clock~3

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them. This manual does not qualify any clock source or promise the future strict startup warning. Revision 3 requires a time provider with a stated contract, forbids uptime as wall time and treats a failed monotonic read as fail-closed. The integrator must validate its real target.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The platform *SHALL* provide a monotonic clock and a wall clock through a time provider whose supported configuration states their units, resolution, monotonicity, adjustment and failure behaviour (`+req~ssr-platform-contract~2+`). The wall clock is synchronized with every peer's time base, to within an error the integrator includes in its TTL and deadline choices, before the element's TTL decisions are relied on. UUIDv7 message IDs and TTL are defined in wall time, and uptime *SHALL NOT* stand in for it: a target without a synchronized wall clock supplies one through its provider, or setup refuses (`+req~ssr-mode-entry-clock~2+`). The TTL bound on an RPC request holds only while the client's and the server's wall clocks agree within the TTL. In strict mode, the report warns when the wall clock at setup reads earlier than the element's release date, a sign that it is not yet synchronised, and lists the time-sync source where one is declared.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] Select the configuration's time provider and cite its package's stated contract and evidence for the monotonic and wall clocks, their units, resolution, adjustment and failure behavior, and the clock reads and waits on the deployed backend. Confirm that the deployment's clock source and synchronization match it; derive that evidence yourself only for a custom provider or a context the package does not cover. Never let uptime stand in for wall time.
- [ ] Before relying on TTL decisions, bound client/server and peer wall-clock disagreement and include that error in TTL/deadline choices. Define the response to sync loss, jumps or an unreadable wall clock.
- [ ] Retain startup synchronization evidence and runtime monitoring for a readable but incorrect wall clock. Check proposed startup release-date warnings only after that feature is implemented; they are not a synchronization proof.

## Violation manifestations

A skewed readable clock can reject useful data early or accept stale data. An unreadable clock can cause refusal; a failed monotonic read ends dependent bounded operations as expired, and a monotonic clock at the wrong rate invalidates waits and deadline budgets.

## Detection limits

A successful wall-clock read proves neither correctness nor synchronization. RPC TTL guarantees require the stated clock agreement. The quoted 64 ms and ±4.36-year last-good-time figures belong to a generator with a 32-bit sequence; the generator at this revision keeps the full last good time and a 64-bit counter. None of this is current qualification.
