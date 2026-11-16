# The integrator decides which routes need end-to-end protection

`uman~sdk-aou-e2e-protection~2`

Covers:
- aou~seooc-e2e-protection~7

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

Revision 7 separates two groups of duties:

- Duties that hold now: deciding which routes need end-to-end protection, supplying and justifying the integrator's own protection there, and the black-channel and decomposition rules.
- Duties that bind only a route that selects the SDK's E2E wire.

That wire is deferred and not part of this delivery.

- No wrapper duty applies to a current route.
- Its fourteen requirements keep their identifiers and revisions without implementation, gate or coverage credit.
- For a later route on the wire, the accepted pre-adoption accounting rule is exact when proven, otherwise explicitly unknown.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* decide which routes carry data that needs end-to-end protection, and *SHALL* supply protection on each of them and justify it for the item.
>
> * *In this delivery.* In this delivery that protection is the integrator's own mechanism: the SDK's E2E wire `+E2e<W>+` is deferred and not part of it. The integrator states which faults its mechanism detects, the boundary it covers and the residuals it leaves, and its analysis of the route rests on that statement.
> * *Above ASIL B.* Data above ASIL B that relies on such protection stays within the decomposition and coexistence duties of <<aou-seooc-capability-ceiling,Data above ASIL B relies on measures outside the element>>.
> * *Adds, and replaces nothing.* End-to-end protection adds to the element's own duties and replaces none of them: RPC correlation, TTL, ownership, and the refusal and error handling of every route hold whether a route is protected or not, and message IDs are not authentication (xref:../up-l1/deployment.adoc#aou-seooc-peer-access[Only authorized processes reach the transport, and peers are faulty, never malicious]).
> * *Not required by the element.* Nothing in the element requires end-to-end protection on any route, including routes at an ASIL integrity level.
> * *Strict mode.* In strict mode, on an ASIL route over a transport the integrator classifies as a black channel, the route names the integrator's own protection or, once the E2E wire is delivered and selected for it, uses that wire; the report lists the protection named for each route, and warns of an unclassified transport.
>
> The duties that follow bind only a route that selects the SDK's E2E wire, once that wire is delivered. A route protected by the integrator's own mechanism, or not protected, is bound by none of them. Where the E2E wire is selected for a route, the integrator:
>
> * declares each route's senders in deployment data, from which both sides derive the data IDs with the element's function (xref:e2e-wire.adoc#req-ssr-e2e-data-id[One data ID per sender and route]), naming no wildcard, and making each receiver's declared senders equal to its route's declared peers; keeps the derived IDs distinct wherever a misaddressed frame can travel; and sets `+resync_frames+` where the default does not fit;
> * declares a sender's starts increasing only where every start of that sender has a greater start value than every earlier start's: a wall clock kept across restarts and boots, with restarts more than a millisecond apart, or a counter it keeps across restarts and boots and supplies at construction. Otherwise, it leaves the declaration at its default;
> * runs one live sender per data ID at a time; builds each E2E value from its own role's addressing; puts at most one E2E wire on a route; and writes no wire of its own that shares one E2E value between roles;
> * declares a deadline on each such route, within the item's fault tolerant time interval, because the wire's timeout is the route's deadline, or accepts that delay and silence on that route are undetectable. In default mode the element refuses no route for lacking a deadline;
> * uses one sending thread per publish and request route, and one decoding thread per E2E route, at a time: the consuming thread on a pull binding, or one delivery thread on a push binding, as the binding's README states;
> * accepts what the wire leaves unprotected: the message header (source, sink, type, ID, TTL, priority and token), and faults that no end-to-end check can see, such as a sender that puts wrong data under a correct check. On an RPC route, a response echoes its request's data ID, counter and incarnation, with its message type, result and status inside the CRC, and the client accepts only an exact match with its own call (xref:e2e-wire.adoc#req-ssr-e2e-rpc-echo[An E2E response answers exactly its call]). The Streamer's best-effort error responses carry no E2E header, so an E2E client refuses them and gives the slot back; absent a later acceptable response or protected final answer, the call ends at its TTL, which the client observes when it next services or waits on the call;
> * accepts the residuals:
> ** two equal incarnations across a restart, at about 2^-32^ per restart, the chance that 32 bits of initialization entropy repeat where that entropy meets its stated contract (xref:platform.adoc#req-ssr-platform-entropy[Initialization entropy comes from a declared source]);
> ** one undetected corruption of a frame's header, at the CRC's undetected-error rate. A counter corrupted upward makes the receiver refuse the sender's later frames, until the counter passes the corrupted value or the sender restarts. For a sender declared increasing, an incarnation corrupted upward also retires the live sender, which is then refused as stale until it restarts, and it stays the newest;
> ** a replayed run of a displaced incarnation's old frames, after two faults;
> ** an RPC call whose every response copy is refused, which ends at its TTL, naming the refusal;
> ** in default mode, a late run of more than `+resync_frames+` requests, all inside their TTL, which is adopted and executed: for a client not declared increasing, from any incarnation but the reference; for one declared increasing, from one older than the retired one. The live client then pays `+resync_frames+` refused calls, answered. After a re-adoption, a replay can re-execute more requests inside the window;
> ** a request more than 63 counters late, refused as out of the window with an advisory answer, whose call ends at its TTL naming the refusal if no later acceptable response or protected final answer completes it;
> ** a false final answer across a server restart, behind a Streamer, which takes three faults at once;
> ** the TTL and the message ID outside the CRC, so the TTL's bound on a late run needs the two wall clocks to agree within the TTL (<<aou-seooc-clock,The platform provides the clocks>>);
> ** in strict mode, a client locked out by a false declaration of increasing starts: told, and counted at the server, until the server restarts;
> ** a request duplicated more than once, whose extra answers can overflow the client's response queue; the route's overflow policy then drops and counts the extra frame, and if that frame is the real response and no later acceptable response or protected final answer completes the call, it ends at its TTL, naming the repeat;
> ** where a client declares several servers for one method, an answer from another declared server to a misaddressed copy, which passes.
>
> On a route that selects the E2E wire, strict mode adds these rules: every such receive route declares a deadline or `+event_only()+`; the clients of such an ASIL RPC route are declared increasing, from a counter kept across restarts and boots, never the wall clock, which the integrator makes durable before use and keeps per sender construction, never per boot, and after replacing a client ECU the integrator restarts its servers.
>
> On such a strict ASIL RPC request route, the receiver refuses every incarnation older than the newest adopted, counts the refusal and answers under the final/advisory proof in xref:e2e-wire.adoc#req-ssr-e2e-refusal-answered[A refused request is answered, final or advisory]; it never re-adopts an older incarnation through a run (xref:e2e-wire.adoc#req-ssr-e2e-resync[A restart resynchronizes, stale frames are refused, and no arrival order locks a sender out]). The persistent counter declaration alone does not provide that receive rule.
>
> The report lists each E2E route's senders, data IDs, `+resync_frames+`, start declarations with their duty, thread duties and figures.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Decide which routes need protection, and supply it.**
  - Perform a route-by-route analysis of exchange-of-information faults and decide which routes need end-to-end protection.
  - For each, supply the integrator's own protection, record the faults it detects, the boundary it covers and the residuals it leaves, and justify it for the item.
  - The SDK's E2E wire is not available in this delivery. Optional protection does not remove the integrator's duty.
- [ ] **Classify each transport.**
  - Classify each transport as trusted or as a black channel.
  - For every ASIL route over a black channel, name the protection supplied.
  - For data above ASIL B, keep the decomposition and coexistence arguments that the [capability-ceiling](capability-ceiling.md) entry requires.
  - Keep the element's RPC correlation, TTL, ownership and error handling in the analysis: protection replaces none of them, and message IDs are not authentication.
- [ ] **Only for a route that selects the deferred E2E wire, once it is delivered: record its declarations.**
  - Record exact non-wildcard senders and receiver peers, derived data IDs and their collision domain, resync_frames, one live sender per data ID, role-specific addressing and at most one E2E wrapper per route.
- [ ] **Only for such a route: starts, threads and deadlines.**
  - Permit a declaration of increasing starts only with evidence across every construction, restart and boot.
  - For a strict ASIL RPC route on the wire, use a durable per-sender construction counter, not wall time, persisted before use, and restart servers after replacing a client ECU.
  - Plan one sending thread per publish/request route and one decoder per route under the binding's documented threading model.
  - Supply a deadline where detecting silence matters. An explicit event-only choice accepts the lack of deadline-based silence/delay detection.
- [ ] **Only for such a route: review each quoted residual explicitly.**
  - The residuals are: incarnation collision, undetected corruption, displaced-incarnation replay, refused RPC copies, default-mode late adoption/re-execution, the 63-counter window, the server-restart final-answer residual, unprotected TTL/ID and clock agreement, false increasing-start lockout, duplicate-response overflow and multiple-server misaddressing.
  - Require actual protection of request/response correlation and protected final/advisory responses before relying on them.
  - Account separately for corrupt-request silence and unprotected Streamer errors.
  - Never count a refused copy as necessarily ending an otherwise completable call.
- [ ] **Only for such a route, in strict mode: verify the receive rule and the figures.**
  - Verify that the strict ASIL RPC receiver refuses, counts and answers every incarnation older than the newest adopted, without starting/extending a run or re-adopting it. A persistent start counter alone does not implement that rule.
  - Check the final/advisory non-delivery proof and keep the default-mode residual separate.
  - Read the never-arrived-before-adoption figure as exact only when its retained distinct-arrival history proves it, unknown otherwise, with saturation as a lower bound.
  - Verify bounded history with no runtime allocation, freeing or waiting in the implementing release.

## Violation manifestations

- A route that needs protection and has none, or whose protection misses the faults it is relied on to detect, can deliver corrupted, repeated, delayed or misaddressed data unnoticed.
- On a later route with the E2E wire, misconfiguration can cause lockout, replayed request execution, silent delay, wrong-peer acceptance or refused calls that reach TTL.
- On such a route, counter or incarnation corruption can suppress good traffic.
- CRC-valid but semantically wrong payload remains possible.

## Detection limits

- The element cannot see whether a route needs protection or whether the integrator's protection works.
- End-to-end protection does not authenticate a sender, protect a shared-memory race, cover all outer header fields, or validate an application's payload meaning.
- No unimplemented E2E behavior is credited.
- The accepted exact-or-unknown rule does not reconstruct evicted history, changes neither lost/late/pending definitions nor replay/recovery rules, and supplies no implementation evidence.
