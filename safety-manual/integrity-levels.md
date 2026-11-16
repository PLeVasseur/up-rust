# Each transport's integrity level is assigned correctly

`uman~sdk-aou-integrity-levels~3`

Covers:
- aou~seooc-integrity-levels~6

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- Integrity admission and its reports require implementation/qualification. Do not infer them from this documentary coverage.
- Revision 6 keeps the assignment of each transport's level, with revision 5's wording. The specification files it with the transports, in `up-spec/safety/up-l1/deployment.adoc`.
- The duties for data above ASIL B, and for the element inside an ASIL C or D process or partition, which revision 5 held, are the [capability-ceiling](capability-ceiling.md) entry, with their wording.
- The sentence about iceoryx2 is now `aou~seooc-iceoryx2-level-separation~1`. The iceoryx2 binding's safety manual states it, and this manual has no entry for it.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* give each transport one integrity level in the deployment data: QM, A, B, C or D, in that order. Every route on that transport carries the level (`+req~ssr-integrity-declared~3+`). Each process declares the level it was built to, and is developed to at least that level.
>
> * *One transport's restrictions.* What one binding adds to the assignment is stated with that binding (for iceoryx2, `+aou~seooc-iceoryx2-level-separation~1+`).
> * *Above ASIL B.* The duties for a level or for data above ASIL B are `+aou~seooc-capability-ceiling~1+`.
> * *A level is a label.* A declared level is a label checked at setup: it neither raises the element's capability nor qualifies the process that declares it.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Assign and record levels.**
  - Assign one of QM/A/B/C/D to every transport and record the development level of every process.
  - Ensure every route inherits its transport's level.
  - On iceoryx2, follow the binding's own entry for the service and process topology as well.
- [ ] **Treat a level as a label.**
  - Labels and acknowledgements cannot raise demonstrated capability.
  - For a level or data above ASIL B, or where the element runs inside an ASIL C or D process or partition, complete the [capability-ceiling](capability-ceiling.md) entry.

## Violation manifestations

- Misdeclared levels can admit an unsafe sharing topology or deliver data whose assumed integrity exceeds its protection.
- Shared-state writers can interfere in both directions.

## Detection limits

- Setup metadata cannot detect a peer lying about its level or prove its development process.
