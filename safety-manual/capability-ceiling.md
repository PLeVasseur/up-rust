# Data above ASIL B relies on measures outside the element

`uman~sdk-aou-capability-ceiling~1`

Covers:
- aou~seooc-capability-ceiling~1

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- This entry binds only where data above ASIL B crosses the element, or the element would run in an ASIL C or D process or partition.
- Its clauses were part of the integrity-levels assumption up to revision 5, and their wording is unchanged. The assignment of each transport's level stays in the integrity-levels entry.
- Strict acknowledgements and their reports require implementation/qualification. Do not infer them from this documentary coverage.
- The decomposition and coexistence duties hold for whatever end-to-end protection the system supplies. The SDK's own E2E wire is deferred.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> *Capability.*
> The element is developed for ASIL B capability, and no configuration claims more for it, whatever OS, middleware or compiler it runs on, so data above ASIL B that crosses the element relies on measures outside it.
>
> *Decomposition and interference.*
> End-to-end protection, whichever mechanism the system supplies, can carry part of an ASIL C or D requirement only within a decomposition the integrator argues (9-5), with independence shown by dependent failure analysis. It covers faults in the exchange of information, not the timing and memory interference the element could cause inside an ASIL C or D process: there the integrator shows freedom from interference at the higher ASIL's rigour, or keeps the element out of that partition. The usual pattern is an ASIL B process that relays to the C or D partition over a channel the integrator protects at C or D.
>
> *Strict mode.*
> In strict mode, the Enforce state refuses a transport or route at C or D unless `+above_capability+` names the integrator's measures, and refuses C or D data whose only protection is an end-to-end mechanism of ASIL B capability unless `+decomposition+` names the argument; the report lists both duties.
>
> *The level itself.*
> A level is assigned, and is a label, as `+aou~seooc-integrity-levels~6+` states.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Protect C/D information outside the element.**
  - For C/D information, identify external protection and the independence argument for any decomposition.
  - E2E protection of exchanged data does not control execution-time or memory interference inside a C/D partition.
- [ ] **Keep the element out of a higher-integrity partition.**
  - Keep the element out of a higher-integrity partition unless the required interference argument is verified.
  - Where appropriate, use a B process and a separately protected channel to the C/D partition.
- [ ] **Review declarations against evidence.**
  - Review proposed above_capability and decomposition declarations against real evidence before using a future Enforce implementation.
  - Labels and acknowledgements cannot raise demonstrated capability.

## Violation manifestations

- Data whose assumed integrity exceeds its protection can be delivered as if it were protected.
- The element can interfere, in time or memory, with a C or D partition that holds it.

## Detection limits

- Setup metadata cannot prove independence or a decomposition.
- The quoted ASIL B capability describes the specification target, not certification of this SDK snapshot.
- No OS, middleware or compiler raises it.
