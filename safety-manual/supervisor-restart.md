# A supervisor outside the process restarts it

`uman~sdk-aou-supervisor-restart~2`

Covers:
- aou~seooc-supervisor-restart~4

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- Supervisor integration and deployment fault tests are integrator work.
- Strict restart declarations/reports are prospective. No automatic supervisor is supplied by this manual.
- Revision 4 adds the bounded reaction chain for an item that relies on a restart within a time.
- It also states that a restart revokes no storage a surviving peer retains.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* provide a supervisor outside every process that uses the element. It detects that a process has exited, crashed or been killed, and restarts it or takes the item to its safe state, under a restart policy the integrator sets (rate limit, escalation).
>
> * *No in-process recovery.* The element has no in-process recovery. In runtime mode a process leaves only by exit, crash or kill, and in the ASIL build a panic aborts the process (xref:build.adoc#req-ssr-build-panic-abort[The ASIL build aborts on panic and checks overflow]), which the supervisor handles as a crash.
> * *Setup errors.* The element is designed on the assumption that an application treats an initialization or transition error the same way, by exiting so that the supervisor restarts it, rather than by retrying setup in the process.
> * *Whole process or partition.* No thread or interrupt handler that calls the element is terminated on its own: the process, or on an MCU the partition or OS-Application that holds the element's memory, is restarted as a whole.
> * *Fresh initialization.* A restart is a fresh initialization: it revokes no storage that a surviving peer still retains, and needs no departure notice from the process that died.
> * *Reaction time.* Where the item relies on the restart or the safe state within a time, the integrator *SHALL* bound the whole chain, from the detection of the exit through the supervisor's decision and the restart or isolation to the physical safe state, against the allocated fault tolerant time interval, and *SHALL* argue the supervisor's own clock, resources and independence from the process it supervises; a supervisor that can share the failure it detects does not establish that bound.
> * *Strict mode.* In strict mode the integrator declares the restart path, a supervisor or, on an MCU, a partition restart, and the report lists it, or warns that none is declared.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Assign a supervisor outside each process or partition.**
  - Record the process it observes, its heartbeat/exit observation, restart rate limit, escalation and item safe-state action.
  - Test exit, crash, kill and an initialization/transition error in the deployment qualification.
- [ ] **Recover by restarting the whole unit.**
  - On initialization or transition failure, exit the entire process/partition and reconstruct its startup memory.
  - Do not retry setup in a half-initialized process or cancel an individual thread/interrupt context while it can hold SDK state.
- [ ] **Prove the supervisor survives the supervised fault.**
  - Prove the supervisor survives the supervised fault and can restart the whole memory-owning unit.
  - Include restart duration and escalation in the item reaction budget.
  - For a future strict deployment, record the restart-path declaration and check the resulting report once that mechanism is implemented.
- [ ] **Bound the reaction chain where the item relies on it.**
  - Where the item relies on the restart or the safe state within a time, bound detection, the supervisor's decision, restart or isolation and the physical safe state against the allocated FTTI.
  - Argue the supervisor's clock, resources and independence from the supervised process.
  - Otherwise record that no restart or reaction time is claimed.

## Violation manifestations

- An unobserved exit leaves the service silent.
- A restart loop can exhaust availability.
- Thread-only recovery can leave synchronization state held, causing persistent refusals or stalled traffic.
- Repeated setup can exhaust startup memory.

## Detection limits

- The process cannot report its own death.
- Deadlines, RPC expiry and binding peer events are secondary observations only where implemented and configured.
- Silence without an independent expectation is undetectable.
- A declared supervisor does not prove it works, and one that can share the failure it detects cannot establish a reaction bound.
