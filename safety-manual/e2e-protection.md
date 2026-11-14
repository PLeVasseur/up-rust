# The integrator decides which routes need end-to-end protection

`uman~sdk-aou-e2e-protection~1`

Covers:
- aou~seooc-e2e-protection~6

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them. Revision 6 now quotes the corrected strict receive rule and conditional TTL outcomes without a second Draft6 rise. The accepted pre-adoption accounting rule is exact when proven, otherwise explicitly unknown; implementation, qualification and documentary gate admission remain pending.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* decide which routes carry data that needs end-to-end protection, and applies protection there: the E2E wire `+E2e<W>+`, or a mechanism of the integrator's own. Where the E2E wire is used, the integrator:
>
> * declares each route's senders in deployment data, from which both sides derive the data IDs with the element's function (`+req~ssr-e2e-data-id~4+`), naming no wildcard, and making each receiver's declared senders equal to its route's declared peers; keeps the derived IDs distinct wherever a misaddressed frame can travel; and sets `+resync_frames+` where the default does not fit;
> * *(b)* declares a sender's starts increasing only where every start of that sender has a greater start value than every earlier start's: a wall clock kept across restarts and boots, with restarts more than a millisecond apart, or a counter it keeps across restarts and boots and supplies at construction. Otherwise, it leaves the declaration at its default;
> * runs one live sender per data ID at a time; builds each E2E value from its own role's addressing; puts at most one E2E wire on a route; and writes no wire of its own that shares one E2E value between roles;
> * declares a deadline on each such route, within the item's fault tolerant time interval, because the wire's timeout is the route's deadline, or accepts that delay and silence on that route are undetectable. In default mode the element refuses no route for lacking a deadline;
> * uses one sending thread per publish and request route, and one decoding thread per E2E route, at a time: the consuming thread on a pull binding, or one delivery thread on a push binding, as the binding's README states;
> * accepts what the wire leaves unprotected: the message header (source, sink, type, ID, TTL, priority and token), and faults that no end-to-end check can see, such as a sender that puts wrong data under a correct check. On an RPC route, a response echoes its request's data ID, counter and incarnation, with its message type, result and status inside the CRC, and the client accepts only an exact match with its own call (`+req~ssr-e2e-rpc-echo~1+`). The Streamer's best-effort error responses carry no E2E header, so an E2E client refuses them and gives the slot back; absent a later acceptable response or protected final answer, it learns of the failure at the call's TTL;
> * accepts the residuals:
> ** *(b)* two equal incarnations across a restart, at about 2^-32^ per restart with entropy that differs at each start;
> ** *(b)* one undetected corruption of a frame's header, at the CRC's undetected-error rate. A counter corrupted upward makes the receiver refuse the sender's later frames, until the counter passes the corrupted value or the sender restarts. For a sender declared increasing, an incarnation corrupted upward also retires the live sender, which is then refused as stale until it restarts, and it stays the newest;
> ** a replayed run of a displaced incarnation's old frames, after two faults;
> ** an RPC call whose every response copy is refused, which ends at its TTL, naming the refusal;
> ** in default mode, a late run of more than `+resync_frames+` requests, all inside their TTL, which is adopted and executed: for a client not declared increasing, from any incarnation but the reference; for one declared increasing, from one older than the retired one. The live client then pays `+resync_frames+` refused calls, answered. After a re-adoption, a replay can re-execute more requests inside the window;
> ** a request more than 63 counters late, refused as out of the window with an advisory answer, whose call ends at its TTL naming the refusal if no later acceptable response or protected final answer completes it;
> ** a false final answer across a server restart, behind a Streamer, which takes three faults at once;
> ** the TTL and the message ID outside the CRC, so the TTL's bound on a late run needs the two wall clocks to agree within the TTL (`+aou~seooc-clock~2+`);
> ** in strict mode, a client locked out by a false declaration of increasing starts: told, and counted at the server, until the server restarts;
> ** a request duplicated more than once, whose extra answers can overflow the client's response queue; the route's overflow policy then drops and counts the extra frame, and if that frame is the real response and no later acceptable response or protected final answer completes the call, it ends at its TTL, naming the repeat;
> ** where a client declares several servers for one method, an answer from another declared server to a misaddressed copy, which passes.
>
> Nothing in the element requires end-to-end protection on any route, including routes at an ASIL integrity level. In strict mode every receive route on the E2E wire declares a deadline or `+event_only()+`; the clients of an ASIL RPC route are declared increasing, from a counter kept across restarts and boots, never the wall clock, which the integrator makes durable before use and keeps per sender construction, never per boot, and after replacing a client ECU the integrator restarts its servers. On that strict ASIL RPC request route, the receiver refuses every incarnation older than the newest adopted, counts the refusal and answers under the final/advisory proof in `+req~ssr-e2e-refusal-answered~1+`; it never re-adopts an older incarnation through a run (`+req~ssr-e2e-resync~4+`). The persistent counter declaration alone does not provide that receive rule; and on an ASIL route over a transport the integrator classifies as a black channel, the route uses the E2E wire or names the integrator's own protection. The report lists each E2E route's senders, data IDs, `+resync_frames+`, start declarations with their duty, thread duties and figures, and warns of an unclassified transport.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] Perform a route-by-route fault analysis and choose qualified end-to-end protection where needed. E2e<W> is a proposed wire here; use no deployment claim based on its presence in the frozen specification. Optional protection does not remove the integrator’s duty.
- [ ] For the proposed E2E design, record exact non-wildcard senders and receiver peers, derived data IDs and their collision domain, resync_frames, one live sender per data ID, role-specific addressing, and at most one E2E wrapper per route.
- [ ] Permit a declaration of increasing starts only with evidence across every construction/restart/boot. For future strict ASIL RPC, use a durable per-sender construction counter, not wall time; persist before use and restart servers after replacing a client ECU.
- [ ] Plan one sending thread per publish/request route and one decoder per route, using the binding’s documented threading model. Supply deadlines where detecting silence matters; an explicit event-only choice accepts the lack of deadline-based silence/delay detection. Any other required detection needs an independently qualified mechanism.
- [ ] Review each quoted residual explicitly: incarnation collision, undetected corruption, displaced-incarnation replay, refused RPC copies, default-mode late adoption/re-execution, the 63-counter window, server-restart final-answer residual, unprotected TTL/ID and clock agreement, false increasing-start lockout, duplicate-response overflow and multiple-server misaddressing.
- [ ] Require actual protection of request/response correlation and protected final/advisory responses before using those behaviors. Account separately for corrupt-request silence and unprotected Streamer errors; never count a refused copy as necessarily terminating an otherwise completable call.
- [ ] For a future strict deployment, record black-channel classification or external protection. Verify that the strict ASIL RPC receiver refuses, counts and answers every incarnation older than the newest adopted, without starting/extending a run or re-adopting it. A persistent start counter alone does not implement that rule. Check final/advisory non-delivery proof and retain the default-mode residual separately. Treat the never-arrived-before-adoption figure as exact only when its retained distinct-arrival history proves it. Read count and completeness consistently; unknown is different from zero, any cumulative unknown contribution keeps the total incomplete, and saturation gives a lower bound. Define and qualify the item's reaction to unknown. Do not stop recovery or impose counter/incarnation lifetime limits to preserve this diagnostic; verify bounded history and no runtime allocation, freeing or waiting in the implementing release.

## Violation manifestations

Misconfiguration can cause lockout, replayed request execution, silent delay, wrong-peer acceptance or refused calls that reach TTL. Counter or incarnation corruption can suppress good traffic. CRC-valid but semantically wrong payload remains possible.

## Detection limits

E2E does not authenticate a sender, protect a shared-memory race, cover all outer header fields, or validate an application’s payload meaning. No unimplemented E2E behavior is credited. The accepted exact-or-unknown rule does not reconstruct evicted history. It changes neither lost/late/pending definitions nor replay/recovery rules, and does not supply implementation evidence.
