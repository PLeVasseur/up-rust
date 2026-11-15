# Bindings and wires pass their contracts

`uman~sdk-aou-qualified-bindings-and-wires~2`

Covers:
- aou~seooc-qualified-bindings-and-wires~5

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- Named records in the quote retain their classifications.
- No binding README uman coverage is claimed by this SDK manual. Binding owners must provide it separately.
- Revision 5 binds qualification to exact versions and configuration and to the evidence state each record has.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The integrator *SHALL* use on the ASIL path only bindings and wires that pass the contract suites (`+assert_binding_contract+`, the QoS contract, `+assert_wire_contract+`) on the integrator's own target, at the exact versions and configuration whose evidence its supported configuration records, and *SHALL NOT* take evidence of one state, compiled, linked, exercised, source-reviewed, composition-verified or qualified, for another.
>
> * *32-bit targets.* On a 32-bit target, those are only the bindings and wires whose contracts pass on that target. The iceoryx2 binding claims no 32-bit target, because iceoryx2 0.9 needs 64-bit atomics and its bare-metal port is a proof of concept.
> * *Pull bindings and deadlines.* A pull binding used for a route with a deadline reports an arrival through `+RuntimePort::arrived+` only with evidence that a frame entered the route's queue, or that the queue's overflow policy discarded one, and within the latency it declares in `+QosSupport::arrival_reports+`. It never reports one on a wake hint alone. It reports each failure to read its own queue through `+RuntimePort::receive_failed+`. Its `+run_qos_contract+` deadline checks pass, on a subscriber and on a carrier ingress.
> * *Response routes.* A push binding delivers one response route from one thread at a time, which the contract checks. A pull binding needs nothing: the element serializes the pulls of a response route itself.
> * *Cores without coherent atomics.* A route between cores whose atomics are not coherent with each other (`+aou~seooc-target-environment~4+`) uses a transport whose qualification covers that.
>
> The integrator accepts the limits and known anomalies that each binding's README states (`+req~ssr-binding-readme~1+`). For a deployment that uses iceoryx2, these include:
>
> * allocation on a peer join or restart, under the waiver (`+UPW-iceoryx2-1+`);
> * sizing of the expired-connection buffer: a receive route is refused at setup unless twice its retained budget fits in it;
> * the service depth, which now comes from deployment data (`+req~ssr-binding-depth-from-deployment~1+`);
> * the port mutex's panic path, on lock and on unlock, which no peer can reach (`+UPD-IOX2-0001+`, revision 2);
> * the two `+blocking_send+` `+unwrap+`s, which only a failing OS clock call reaches (`+UPD-IOX2-0002+`);
> * iceoryx2's other accepted panic paths: two process-private invariants; a descriptor that other code in the process closes while a socket is set up; `+close()+` failing on a departed peer's descriptor; and a sample counter's overflow on 64-bit targets, at over 580 years at 10^9^ sends per second (`+UPD-IOX2-0003+`, `+UPD-IOX2-0004+`, `+UPD-IOX2-0005+`);
> * heap exhaustion during a peer change, which aborts, within the deployment's heap sizing under `+UPW-iceoryx2-1+` (`+UPD-IOX2-0006+`);
> * arrival reports from its receive thread, within that thread's period plus its stated pass bound, on a drained notification of the route's data ID together with a sample in the queue or taken since the previous pass, or on the queue turning non-empty;
> * a sender preempted between `+send+` and its notification delays the report, unless the queue was empty at the previous pass; a notification that fails at the sender while the queue holds samples leaves an arrival unreported, so a miss may be counted for a gap no longer than the deadline, and the sender counts the failure;
> * while a new publisher's data connection is being set up, its notification can be reported while older samples are queued;
> * iceoryx2 0.9.2 only logs a data connection that cannot be set up, under its default handler, and retries it only when the publisher list changes. So the binding installs a degradation handler on each port at setup: a failed connection is counted as `+receive_failed+`, or on the sending route, and the route then reports only a queue that turned non-empty, until the publisher list changes and an update runs clean; a queue that a healthy second publisher keeps non-empty meanwhile shows misses;
> * the event IDs are read from the event service's static config, and a route whose lifecycle events share the data ID, or whose data ID is above the service's `+event_id_max_value+`, is refused at setup.
>
> The README also says that a frame drop can wait for the port mutex. That is not an accepted limit, because `+req~ssr-binding-frame-drop~4+` forbids it. It is the binding's known failure `+UPF-IOX2-0001+`, open until the binding returns each dropped frame to iceoryx2 through its deferred return as pending cleanup, charged and isolated as `+req~ssr-release-lease-drop~3+` requires; a thread of its own is not by itself that isolation.
>
> iceoryx2's other known failures are not accepted limits either, because `+req~ssr-binding-no-panic~4+` forbids them: its index bug on a connection change (`+UPF-IOX2-0002+`), until iceoryx2 fixes the index; its event-deadline panics, on QM transports only, since ASIL deployments turn those deadlines off, until iceoryx2 fixes them (`+UPF-IOX2-0005+`); and the paths through which a peer that writes values its own iceoryx2 never writes can panic its survivors, until iceoryx2 validates what peers write (`+UPF-IOX2-0004+`).
>
> The wait thread's descriptor-set panics (`+UPF-IOX2-0006+`) remain until its own poll loop replaces that set, or a reviewed equivalent with a source-backed argument is adopted in its place, and unsafe name and queue-size configuration (`+UPF-IOX2-0007+`) remains until setup refuses it.
>
> The expired-buffer failure is fixed by the twice-budget setup refusal; it is recorded separately as fixed, not as an accepted anomaly.
>
> A wire or binding from elsewhere with `+unsafe+` code of its own, such as a wire over a C codec, is a separate element, and the integrator qualifies its `+unsafe+` (`+req~ssr-unsafe-inventory~3+`). The bindings this project ships keep their own inventories under the same gate (`+req~ssr-binding-unsafe-inventory~1+`).
>
> In strict mode, the Enforce state refuses a binding or wire that is not on the integrator's qualified list, an ASIL process that reaches a deviation not acknowledged at its current revision, and any known failure, whatever is acknowledged; while iceoryx2's known failures stand, that refuses each affected route at ASIL, including publishers affected by shared-state faults. The report lists each binding's README limits, deviations and known failures.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Maintain a qualified binding and wire list.**
  - Maintain a qualified binding/wire list keyed by version, target, toolchain, features, configuration and contract evidence.
  - Record each piece of evidence by its state (compiled, linked, exercised, source-reviewed, composition-verified or qualified) without promoting one to another.
  - Require binding, QoS and wire contracts on the actual target, including response-thread serialization and subscriber/carrier arrival/deadline behavior.
- [ ] **Inspect queue-arrival evidence.**
  - Arrivals require queue admission or overflow discard, not a wake hint.
  - Queue-read failures need separate reporting.
  - Validate the declared latency and loss-observability capability.
  - Do not claim every middleware discard is counted.
- [ ] **Read each binding README and its records.**
  - Separate accepted UPD anomalies, open UPF failures and UPW heap waivers by ID/revision.
  - The expired-buffer failure UPF-IOX2-0003 is recorded as fixed, not an accepted anomaly.
- [ ] **For iceoryx2, verify its sizing and limits.**
  - Verify retained-budget/expired-buffer sizing, deployment service depths, event/data-ID separation and maximum, connection degradation behavior, arrival-thread period and notification/preemption limits.
  - Keep event-deadline restrictions and allocation bounds explicit.
- [ ] **Do not approve deployments affected by known failures.**
  - Do not approve deployments affected by frame-drop blocking, shared-state faults (publishers included), connection-change index failures, wait-thread descriptor faults or unsafe setup configuration merely by acknowledging them.
  - Verify each fix and its affected paths before removing the restriction.
  - For the wait-thread descriptor faults, accept only the binding-owned poll loop or a reviewed, source-backed equivalent, with every claimed eliminated panic site shown unreachable.
- [ ] **Qualify external unsafe code, and mind the target.**
  - Qualify external unsafe code as its own element and retain each shipped binding's unsafe inventory.
  - For non-coherent inter-core routes use a transport qualified to synchronize those cores.
  - iceoryx2 0.9 has no 32-bit target claim here.

## Violation manifestations

- An unqualified binding can lose arrivals, misreport counters, block frame release, panic on a peer fault, corrupt shared state or allocate without a valid waiver.
- These manifestations can violate the item even if a mock contract passed.

## Detection limits

- A suite pass is limited to its exact bindings and target. It does not erase known failures or establish all deployed loads.
- Strict known-failure refusal is required but not claimed implemented by this manual.
- The absence of a failure report does not show all paths were exercised.
