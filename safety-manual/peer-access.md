# Only authorized processes reach the transport, and peers are faulty, never malicious

`uman~sdk-aou-peer-access~1`

Covers:
- aou~seooc-peer-access~4

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- Strict peer listings, integrity refusals and all proposed faulty-peer containment are not claimed available at this baseline.
- Known binding failures remain failures, including publisher exposure.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* restrict access to every transport's resources (shared-memory segments, sockets, service names) to the processes deployed to use them, with OS permissions or equivalent.
>
> * *The fault model.* Within that, this SEooC treats every peer as faulty, never malicious. A peer may send any content, at any time and in any volume, and may choose any source URI, and the requirements hold against all of it. A faulty peer includes one that writes values its own middleware never writes, into shared memory it can write.
> * *A peer below a reader's level.* A peer below a reader's level may write its own shared memory at any moment: the reader then takes the copy route (`+req~ssr-integrity-zero-copy~4+`), and the requirements hold.
> * *Two-way shared state.* On a binding whose ports write shared state both ways, such as iceoryx2, every process on one service runs at one integrity level; a lower-integrity consumer takes a copy route, and the binding refuses any other at setup, by name. Each level runs in its own iceoryx2 domain, and its processes under their own OS user, so that a faulty lower process cannot open, map or remove a higher level's shared memory.
> * *Zero-copy routes.* On a zero-copy route, the element relies on the writer, developed to at least the reader's level, never writing a sample it has sent.
> * *Out of scope.* Only deliberate exploitation, such as engineering a peer-ID hash collision, is out of scope, as security.
> * *Message IDs.* Message IDs are unique only to a stated residual (`+req~ssr-ids-unique~4+`), and they are not secret, so nothing may rely on them for authentication.
> * *Strict mode.* In strict mode, the report lists each route's declared peers and the shared-memory permissions its binding requests, and warns of an ASIL receive route that admits undeclared peers.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Restrict access to transport resources.**
  - Allow access to service names, sockets and shared-memory resources only to deployed processes.
  - Verify OS users, permissions and namespaces.
  - On iceoryx2 separate integrity levels into distinct domains and users.
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
