# Processes or partitions are isolated

`uman~sdk-aou-process-isolation~2`

Covers:
- aou~seooc-process-isolation~3

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- Revision 3 adds the MCU partition boundary and states what a separate address space does not protect.
- Neither it nor this entry proves that the strict report or warning is implemented.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> Each process using the element *SHALL* run in its own address space, under an OS that enforces memory protection, so one process cannot write another's private memory.
>
> * *MCUs.* On an MCU without such processes, each partition or OS-Application that holds the element's memory *SHALL* be protected by a memory protection unit or an equivalent mechanism that its configuration states, or the integrator *SHALL* state the boundary that the configuration has instead and make a coexistence argument for everything that shares it.
> * *Shared memory.* Shared memory between processes or partitions *SHALL* exist only where a binding maps it, with the permissions the binding requests. A separate address space protects private memory only: a lower-assurance process that shares memory with the element through a binding is not made harmless by running apart, and the faulty-peer model applies to it (`+aou~seooc-peer-access~4+`).
> * *ASIL and QM together.* ASIL and QM software *SHALL NOT* share a process or partition unless the integrator makes and verifies a coexistence argument for it.
> * *Strict mode.* In strict mode, the report lists the transports that share each process, and warns of every QM transport in an ASIL process, with the coexistence duty that follows.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Document the process and partition map.**
  - Document the process/partition map and every shared-memory region, owner and access permission.
  - Use OS memory protection and separate identities.
  - Verify that a faulty process cannot write another process's private memory.
- [ ] **List what shares each process.**
  - List every QM and ASIL component in each process, including bindings, callbacks and middleware.
  - Separate them unless a reviewed, verified coexistence argument covers memory and timing interference.
- [ ] **Check the actual deployment.**
  - Check the actual deployment mappings and privileges, not only declared integrity labels.
  - Record each intended shared-memory mapping and compare it with the binding's requested permissions.
  - For an implementing strict release, compare the report of every co-resident transport with this inventory and resolve each QM/coexistence warning against the reviewed argument.
- [ ] **On an MCU, record the boundary.**
  - On an MCU, record the memory protection regions, or the equivalent boundary, of each partition or OS-Application that holds the element's memory.
  - Where none exists, state the boundary the configuration has and make the coexistence argument for everything inside it.
  - Treat every process that shares memory through a binding as a possibly faulty peer, whatever its address space.

## Violation manifestations

- Unauthorized writes can corrupt another process.
- Co-resident code can corrupt private state or starve safety work.
- A permissive shared mapping bypasses the intended isolation.

## Detection limits

- The SDK cannot detect a breach of OS protection or validate a coexistence argument.
- A future strict warning is a configuration aid, not evidence of isolation.
