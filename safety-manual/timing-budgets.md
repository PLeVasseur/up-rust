# Timing budgets are the integrator's

`uman~sdk-aou-timing-budgets~1`

Covers:
- aou~seooc-timing-budgets~5

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them. Revision 5 now includes the corrected event-only and response/TTL wording without a second Draft6 rise. Default mode remains permissive; future strict receive routes must declare a deadline or event_only. This documentation does not qualify the proposed monitor or response machinery.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* schedule the threads that call the element (priorities, CPU budgets, affinity), and chooses every deadline, TTL and wait timeout so that the item's fault tolerant time interval holds. It allows for the following:
>
> * *TTL resolution.* A frame is refused at the latest when its TTL, rounded up to a whole millisecond, plus one more millisecond has passed since the time its message ID records (`+req~ssr-qos-ttl~4+`). An ID made while the sender's wall clock could not be read records the last good time, rounded down to a multiple of 64 ms from the route's start, so its frame can be refused early.
> * *When an RPC failure is seen.* Where the element can report a failure, a call learns of it as soon as the element's answer arrives (`+req~ssr-peer-rpc-no-server~1+`, `+req~ssr-peer-rpc-undecodable-answered~4+`, `+req~ssr-streamer-rpc-error-response~3+`). A request that the E2E wire refuses is answered, final or advisory; only a corrupt one is not. On an RPC route with the E2E wire, a response the client refuses at arrival, including the Streamer's best-effort error response with no E2E header, gives the claimed call slot back to await another response (`+req~ssr-rpc-check-at-arrival~1+`). That refusal does not complete the call. A later acceptable response or protected final refusal answer can complete it before TTL; a call with no such later response or final answer ends at its TTL, naming the refusal. The TTL remains the guarantee when no earlier completion occurs.
> * *Fail-silent routes.* The Streamer's safe state is fail-silent per route, so a receiver sees a gated route as silence. An RPC client whose request arrives on a gated route gets the best-effort `+Unavailable+` response when the reverse route can carry it (`+req~ssr-streamer-rpc-error-response~3+`), and otherwise learns at its TTL. A client that uses the E2E wire refuses that response as unprotected and counts the refusal apart from corruption; it gives the slot back, and its call ends at TTL naming the refusal only if no later acceptable response or protected final answer completes it. Every safety-related receiver whose safety reaction depends on detecting silence *SHALL* declare a deadline. An explicit `+event_only()+` declaration permits a route without a deadline, but supplies no deadline-based silence or delay detection; the integrator *SHALL NOT* rely on that route's deadline monitor for such a reaction.
> * *Priority inversion through middleware locks.* iceoryx2, for example, holds its port lock for up to a wait's bound, and takes it briefly when a frame is dropped (its README).
> * *Watchdog and reuse.* The integrator's watchdog supervises every thread that calls the element, and detects one that makes no progress inside the element for longer than its period `+T_wd+`. `+T_wd+` times the highest reuse rate of any one call slot stays below 2^28^, and of any one peer-table entry below 2^22^ (`+req~ssr-atomic-windows~1+`). Its reaction restarts the process, or on an MCU the partition or OS-Application, that holds the element's memory, before the stalled thread can act inside the element again. The same bound covers the ready queue: a frame queued behind a producer that stalls before linking its own frame is delivered when that producer resumes. It also covers the wrap windows that remain: a stalled `+expire+` or `+fail_older+` that would end a later call early, and a stalled `+retire+` that would act as a spurious `+peer_lost+`.
> * *Retry loops.* Each compare-exchange loop in the element retries only after another write to the same reservation granule succeeded or, on load-linked/store-conditional cores, after an exception taken inside the loop. On a multi-core part, a write by another core anywhere in the granule counts. Its worst-case execution time under the integrator's interrupt load and contention is measured on the target with the other execution-time figures.
> * *Deadline detection.* A missed deadline is counted at the latest after the deadline, plus twice the route's arrival latency, plus the monitor's resolution, and is seen at the next counter read (`+req~ssr-qos-deadline~3+`). The arrival latency is the binding's declared one, plus the time a recording thread can be preempted between reading the clock and storing the arrival. The integrator's schedule bounds the second. The fault reaction time budget includes all of it.
> * *The monitor's wait bound.* A thread inside the element waits less than 2^28^ ticks of the shortest-scaled monitored route between reading the clock and storing an arrival or a check: under the watchdog above, `+T_wd+` below 0.27 s when a route uses the shortest admitted threshold (1.024 µs), and below 2.3 minutes at 1 ms.
>
> The element bounds its work per call in frames and iterations, not in time (`+req~ssr-bound-work~2+`). Execution time on the target is measured by the integrator, using the element's upper estimates once they are published. Default mode remains permissive and refuses no route solely for lacking a deadline. In strict mode every ASIL receive route declares a deadline or an explicit `+event_only()+`, with its absence of deadline-based silence and delay detection, and a receive route on the E2E wire does at every level; the Enforce state refuses one that declares neither. The report lists each route's detection bound and TTL against the reaction budget the integrator declares for it, and each transport's servicing and watchdog bounds; the Enforce state refuses a route whose bound exceeds its declared budget, and a declared servicing period longer than its transport's bound.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] For each route, record the item reaction interval, scheduling priorities/affinity, CPU load, binding arrival latency, recording-thread preemption bound, service period, monitor resolution, TTL, wait bound, counter-read period and application reaction time. Measure worst-case execution time under target interrupt load and contention; iteration bounds are not time bounds.
- [ ] Include TTL rounding (whole milliseconds plus the extra millisecond in the quoted requirement), synchronized clock error and early expiry from last-good-time quantization in the budget. Do not rely on those proposed ID/TTL details until the corresponding implementation is qualified.
- [ ] Supervise every calling thread. Establish T_wd × per-slot reuse rate < 2^28 and T_wd × per-peer-entry reuse rate < 2^22 for the proposed narrowed design. Restart the whole memory-owning unit before a stalled thread can resume; include ready-queue stalls and stale expire/fail_older/retire work.
- [ ] Check the proposed monitor inequalities separately: every in-element observation-to-store delay < 2^28 ticks of the shortest-scaled route; route servicing at least once per 2^29 ticks. Convert ticks to time from the actual implementation configuration; the quote’s example 0.27 s/2.3 min values are not target measurements.
- [ ] Budget middleware lock contention, frame release, retries, notification failures and fail-silent routes. Every safety-related receiver whose safety reaction depends on detecting silence must declare a deadline. Qualified peer-specific progress checks complement aggregate monitoring; they do not substitute for that deadline duty. An explicitly event-only route supplies no deadline-based silence/delay detection.
- [ ] Treat a refused RPC response as one refused copy: its slot is given back to await another response, and a later acceptable response or protected final refusal answer can complete the call. Only if neither completes it does TTL provide the outcome. Include unprotected Streamer errors in this budget; verify that behavior on the implementing release.

## Violation manifestations

The item may react too late despite bounded per-call work. A missed service/watchdog window can invalidate wrap/reuse assumptions. Queue traffic can continue while useful work is starved; lock delays can consume the reaction budget.

## Detection limits

The SDK does not establish WCET or a scheduler guarantee. Aggregate queue-arrival deadlines do not prove application consumption or each peer’s progress. An event-only declaration supplies no deadline-based silence/delay detection. The quoted default/strict/monitor behavior includes unimplemented requirements.
