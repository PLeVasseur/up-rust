# Each transport's integrity level is assigned correctly

`uman~sdk-aou-integrity-levels~2`

Covers:
- aou~seooc-integrity-levels~5

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them. Integrity admission, strict acknowledgements and their reports require implementation/qualification. Do not infer them from this documentary coverage. Revision 5 states the decomposition and coexistence duties for whatever end-to-end protection the system supplies; the SDK's own E2E wire is deferred.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* give each transport one integrity level in the deployment data: QM, A, B, C or D, in that order. Every route on that transport carries the level (`+req~ssr-integrity-declared~3+`). On iceoryx2 a transport runs at its process's level. Each process declares the level it was built to, and is developed to at least that level. The element is developed for ASIL B capability, and no configuration claims more for it, whatever OS, middleware or compiler it runs on, so data above ASIL B that crosses the element relies on measures outside it. End-to-end protection, whichever mechanism the system supplies, can carry part of an ASIL C or D requirement only within a decomposition the integrator argues (9-5), with independence shown by dependent failure analysis. It covers faults in the exchange of information, not the timing and memory interference the element could cause inside an ASIL C or D process: there the integrator shows freedom from interference at the higher ASIL's rigour, or keeps the element out of that partition. The usual pattern is an ASIL B process that relays to the C or D partition over a channel the integrator protects at C or D. In strict mode, the Enforce state refuses a transport or route at C or D unless `+above_capability+` names the integrator's measures, and refuses C or D data whose only protection is an end-to-end mechanism of ASIL B capability unless `+decomposition+` names the argument; the report lists both duties. A declared level is a label checked at setup: it neither raises the element's capability nor qualifies the process that declares it.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] Assign one of QM/A/B/C/D to every transport and record the development level of every process. Ensure every route inherits its transport’s level and every iceoryx2 service/process topology is consistent.
- [ ] For C/D information, identify external protection and the independence argument for any decomposition. E2E protection of exchanged data does not control execution-time or memory interference inside a C/D partition.
- [ ] Keep the element out of a higher-integrity partition unless the required interference argument is verified. Where appropriate, use a B process and a separately protected channel to the C/D partition.
- [ ] Review proposed above_capability and decomposition declarations against real evidence before using a future Enforce implementation; labels and acknowledgements cannot raise demonstrated capability.

## Violation manifestations

Misdeclared levels can admit an unsafe sharing topology or deliver data whose assumed integrity exceeds its protection. Shared-state writers can interfere in both directions.

## Detection limits

Setup metadata cannot detect a peer lying about its level, prove its development process, or prove independence. The quoted ASIL B capability describes the specification target, not certification of this SDK snapshot; no OS, middleware or compiler raises it.
