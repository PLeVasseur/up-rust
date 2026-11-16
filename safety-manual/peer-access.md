# Only authorized processes reach the transport, and peers are faulty, never malicious

`uman~sdk-aou-peer-access~2`

Covers:
- aou~seooc-peer-access~5

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- Strict peer listings, integrity refusals and all proposed faulty-peer containment are not claimed available at this baseline.
- Known binding failures remain failures, including publisher exposure.
- The specification states this assumption with its transports, in `up-spec/safety/up-l1/deployment.adoc`.
- Revision 5 keeps the integrator's duties of revision 4, with their wording.
- What a faulty peer may do is now defined once, as the faulty-peer model in `up-spec/safety/README.adoc`, with the same content. Read it with this entry. It lifts no security requirement of the protocol.
- The iceoryx2 domain and OS-user separation is now an assumption of its own, `aou~seooc-iceoryx2-level-separation~1`. The iceoryx2 binding's safety manual states it, and this manual has no entry for it.
- The refusal of a port at another level than its service is the SDK's, as its requirement is allocated. Revision 4 gave it to the binding. It is a refusal by name because the specification has every refusal at setup name its cause, the route where there is one, and the setter to change (`req~ssr-mode-init-failure~4`).

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* restrict access to every transport's resources (shared-memory segments, sockets, service names) to the processes deployed to use them, with OS permissions or equivalent.
>
> * *The fault model.* Within that, this SEooC treats every peer as faulty, never malicious. What a faulty peer may do is defined once, for every part, as the faulty-peer model of the safety area's shared concepts. Only deliberate exploitation, such as engineering a peer-ID hash collision, is out of scope, as security.
> * *Two-way shared state.* On a binding whose ports write shared state both ways, every process on one service runs at one integrity level; a lower-integrity consumer takes a copy route, and any other is refused at setup, by name (xref:integrity.adoc#req-ssr-integrity-zero-copy[Zero-copy only from a writer at or above the reader]). What one such binding adds is stated with that binding.
> * *Zero-copy routes.* On a zero-copy route, the element relies on the writer, developed to at least the reader's level, never writing a sample it has sent.
> * *Message IDs.* Message IDs are unique only to a stated residual (xref:../up-l2/interaction.adoc#req-ssr-ids-unique[Message IDs are unique]), and they are not secret, so nothing may rely on them for authentication.
> * *Strict mode.* In strict mode, the report lists each route's declared peers and the shared-memory permissions its binding requests, and warns of an ASIL receive route that admits undeclared peers.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Restrict access to transport resources.**
  - Allow access to service names, sockets and shared-memory resources only to deployed processes.
  - Verify OS users, permissions and namespaces.
  - On iceoryx2, follow the binding's own entry for domains and users as well.
- [ ] **Inventory writable shared objects.**
  - Inventory every writable shared object, including return/control structures used by subscribers.
  - Keep a single integrity level per two-way shared service, or use a separately qualified copying boundary.
- [ ] **Hold zero-copy writers to their duty.**
  - Require a zero-copy writer to stop modifying a sample after publication.
  - Exercise access restrictions and the declared copy boundary in deployment qualification.
  - A private payload copy alone does not validate middleware offsets or metadata.
- [ ] **Declare peers, and do not treat addressing as authentication.**
  - Declare peers at initialization.
  - Treat source URI equality and message IDs as routing/correlation data, never authentication.
  - Supply separate security controls if malicious peers are in the item's threat model.

## Violation manifestations

- A racing shared-memory write can cause undefined behavior even when all payload bit patterns are valid.
- Invalid middleware control data can affect publishers and subscribers.
- An undeclared or masquerading sender may inject apparently valid addressing.

## Detection limits

- The faulty-peer assumption does not cover deliberate exploitation.
- URI filters cannot authenticate a process, and IDs are not secrets.
- E2E checks cannot repair a memory race or prove OS access control.
