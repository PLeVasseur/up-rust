# Waivers are approved with an interference argument

`uman~sdk-aou-waiver-approval~2`

Covers:
- aou~seooc-waiver-approval~3

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- Strict waiver evidence checks are prospective.
- Existing permit admission is not evidence of the final strict approval/report contract.
- Revision 3 keeps the duty that every permit brings, with revision 2's wording. The iceoryx2 permit and what a failed allocation does on iceoryx2 are now in `aou~seooc-iceoryx2-limits~1`, which the iceoryx2 binding's safety manual states.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> For each permit a binding ships, the deployment's owner *SHALL* review the allocation it covers, file a waiver record approved by someone other than its author, and argue how interference from that allocation is excluded: by partition, by qualification, or by analysis.
>
> * *Review again.* The waiver *SHALL* be reviewed again when the middleware version or the deployment changes.
> * *Heap sizing.* The deployment *SHALL* size the heap the permit draws on for the bound it states. For iceoryx2, `+aou~seooc-iceoryx2-limits~1+` names the permit and what a failed allocation under it does.
> * *Strict mode.* In strict mode, the Enforce state refuses a waiver that does not name its interference argument's kind and a reference, and the report lists each waiver with its approver and its argument.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Record and review each permit.**
  - For each compiled-in middleware heap permit, record its exact ID/revision, middleware version, affected operations, allocation bound, deployment, author and an independent approver.
  - Review the interference argument (partition, qualification or analysis) and reference its evidence.
- [ ] **Size the affected heap.**
  - Size the affected heap for worst-case peer changes and concurrent load, including failure behavior.
  - For iceoryx2, the binding's own entry names the permit and the abort on a failed allocation.
- [ ] **Re-review on change, and keep the registers apart.**
  - Re-review the waiver when middleware or deployment changes.
  - Keep a waiver register distinct from accepted anomalies and open failures.
  - Never use a waiver to permit heap use in SDK/binding runtime code or to excuse panic/corruption.
- [ ] **Verify the future Enforce gate before relying on it.**
  - For a future Enforce release, verify the declared argument kind/reference and independent approval are checked and appear in its report before relying on that gate.

## Violation manifestations

- An undersized heap fails an allocation under the permit. On iceoryx2 that aborts the process during a peer change.
- A rubber-stamped waiver can leave allocation interference unbounded.
- A stale waiver can omit a new middleware allocation path.

## Detection limits

- A waiver declaration does not measure interference or establish sufficient heap.
- Process death must be handled by the external supervisor.
- Acceptance of an anomaly is not acceptance of an open failure.
