# Processes are isolated by the OS

`uman~sdk-aou-process-isolation~1`

Covers:
- aou~seooc-process-isolation~2

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them. Revision 2 records the already-written strict reporting duty honestly. The statement itself is unchanged from frozen revision 1; neither revision proves that the report or warning is implemented.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> Each process using the element *SHALL* run in its own address space, under an OS that enforces memory protection, so one process cannot write another's private memory. Shared memory between processes *SHALL* exist only where a binding maps it, with the permissions the binding requests. ASIL and QM software *SHALL NOT* share a process unless the integrator makes and verifies a coexistence argument for that process. In strict mode, the report lists the transports that share each process, and warns of every QM transport in an ASIL process, with the coexistence duty that follows.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] Document the process/partition map and every shared-memory region, owner and access permission. Use OS memory protection and separate identities; verify that a faulty process cannot write another process’s private memory.
- [ ] List every QM and ASIL component in each process, including bindings, callbacks and middleware. Separate them unless a reviewed, verified coexistence argument covers memory and timing interference.
- [ ] Check the actual deployment mappings and privileges, not only declared integrity labels. Record each intended shared-memory mapping and compare it with the binding’s requested permissions. For an implementing strict release, compare the report of every co-resident transport with this inventory and resolve each QM/coexistence warning against the reviewed argument.

## Violation manifestations

Unauthorized writes can corrupt another process, while co-resident code can corrupt private state or starve safety work. A permissive shared mapping bypasses the intended isolation.

## Detection limits

The SDK cannot detect a breach of OS protection or validate a coexistence argument. A future strict warning is a configuration aid, not evidence of isolation.
