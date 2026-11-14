# Waivers are approved with an interference argument

`uman~sdk-aou-waiver-approval~1`

Covers:
- aou~seooc-waiver-approval~2

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them. Strict waiver evidence checks are prospective. Existing permit admission is not evidence of the final strict approval/report contract.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> For each permit a binding ships (for iceoryx2, `+UPW-iceoryx2-1+`), the deployment's owner *SHALL* review the allocation it covers, file a waiver record approved by someone other than its author, and argue how interference from that allocation is excluded: by partition, by qualification, or by analysis. The waiver *SHALL* be reviewed again when the middleware version or the deployment changes. The deployment *SHALL* size the heap the permit draws on for the bound it states: on iceoryx2 an allocation that fails during a peer change aborts the process (`+UPD-IOX2-0006+`). In strict mode, the Enforce state refuses a waiver that does not name its interference argument's kind and a reference, and the report lists each waiver with its approver and its argument.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] For each compiled-in middleware heap permit, record its exact ID/revision, middleware version, affected operations, allocation bound, deployment, author and an independent approver. Review the interference argument (partition, qualification or analysis) and reference its evidence.
- [ ] Size the affected heap for worst-case peer changes and concurrent load, including failure behavior. Treat UPW-iceoryx2-1 as a middleware heap waiver; review UPD-IOX2-0006’s abort residual separately.
- [ ] Re-review the waiver when middleware or deployment changes. Keep a waiver register distinct from accepted anomalies and open failures. Never use a waiver to permit heap use in SDK/binding runtime code or to excuse panic/corruption.
- [ ] For a future Enforce release, verify the declared argument kind/reference and independent approval are checked and appear in its report before relying on that gate.

## Violation manifestations

An undersized heap can abort during a peer change. A rubber-stamped waiver can leave allocation interference unbounded. A stale waiver can omit a new middleware allocation path.

## Detection limits

A waiver declaration does not measure interference or establish sufficient heap. Process death must be handled by the external supervisor. Acceptance of an anomaly is not acceptance of an open failure.
