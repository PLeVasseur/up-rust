# Asynchronous integration

Implementation guidance; see the [index](README.md) for its status. It maps the
executor-independent contract of `req~ssr-async-seam~3` onto Rust and records the
evidence plan for the planned general async layer (working name `up-rust-async`).
Core stays synchronous (`req~ssr-crate-sync-path~1`): futures, tasks and pinning
live above it.

## The contract in Rust terms

Each contract property maps to a Rust mechanism, with a note on what the mechanism must and must not do.

- **Progress step.** Rust mechanism: `Future::poll`.
  - `poll` must not run a blocking wait, a middleware wait or `Overflow::Wait`; an operation that needs one stays synchronous.
  - On a serialized route that declares its allocation, a `poll` that decodes or drops an owned value can wait inside the declared allocator, as the route's policy states.
  - Each non-waiting guarantee limits its own operation and not the other's (`req~ssr-heap-guarantees~1`). On a route that requires non-waiting acquisition, no allocation in a `poll` waits, or the route is refused by name. On a route that requires non-waiting release, a drop in a `poll` that could wait becomes pending cleanup.
  - Acquisition only: a route that requires non-waiting acquisition and declares a release that may wait is admitted where the acquisition is established, and a `poll` that drops an owned value may wait in that release.
  - Release only: a route that requires non-waiting release and declares an allocation that may wait is admitted where the release is established, and a `poll` that decodes may wait in that allocation.
- **Notification arranged before pending.** Rust mechanism: Register the task's `Waker` with the seam before the final readiness check, then return `Poll::Pending`.
  - Readiness is authoritative; a wake is a hint and the future rechecks.
- **Concurrent waiters.** Rust mechanism: One registration per pending operation, sized from the admitted capacity.
  - A single per-route cell that a second waiter overwrites does not meet this.
- **Abandonment.** Rust mechanism: Dropping the future.
  - Before submission: nothing sent.
  - After: nothing recalled; owners return or stay charged.
  - The SDK's own work in dropping the future neither waits nor panics.
  - On a route that requires non-waiting release, an owned value's destruction that could wait moves to the release service as pending cleanup, never into the future's drop or onto the executor.
  - On a route that does not, the owned value is destroyed in the future's drop, under the route's declared policy, and the report says that this can wait.
- **Forgotten operation.** Rust mechanism: `mem::forget` of the future is safe.
  - Storage validity must not depend on `Drop`: no stored callback may borrow the future's stack; the slot stays charged.
- **Deadlines.** Rust mechanism: The SDK's acceptance predicate, using its own clock.
  - A scheduler timer only re-polls; Tokio's and Embassy's generic timeout wrappers can report an inner future ready past their deadline.
- **Native ownership.** Rust mechanism: A native TX loan never crosses `.await`.
  - An RX borrow stays tied to its retained owner while suspended.
- **Native resources.** Rust mechanism: The exact executor's `Waker` clone, wake and drop, task and timer paths.
  - Rust's safe `Waker` API promises no heap-free, panic-free or bounded behaviour; arbitrary wakers can run other code.
- **Service.** Rust mechanism: SDK-owned deadline, expiry and deferred-release service.
  - The application gains no service loop.
  - Pending cleanup never runs inline on a cooperative executor worker (a Tokio worker thread, an Embassy executor) that progress, timers or other returns need.
  - On one cooperative executor with no preempting or separate service context, non-waiting release is available only for releases that cannot wait. A composition that needs pending cleanup is refused by name if the package cannot isolate it from the contexts and resources that other work needs.
  - A route that does not require non-waiting release, including one that requires only non-waiting acquisition, needs no release service for ordinary inline decoded-value destruction alone. That destruction runs where the value ends under the declared policy; it is not pending cleanup. A wait there delays that executor context, which the report discloses. Neither that declared wait nor the absence of a service for that destruction is a reason to refuse the route.
  - Independently, a binding defers the return of a retained transport owner when the underlying return can wait, such as on a port lock (`req~ssr-binding-frame-drop~6`). That return is pending cleanup even when the route requests no non-waiting release guarantee. It must meet the isolation rules of `req~ssr-release-lease-drop~4`; otherwise setup refuses the route by name before taking any value or owner that would need that isolation. Ordinary inline decoded-value destruction and isolated binding-deferred return can coexist on the same route.

`Pin`, `Waker`, `Context` and the seam's function names are mechanisms. The
normative text speaks of progress steps, notifications, submission and
acceptance events, so any language binding can keep it.

## What the frozen revision shows

Targeted observations at `dc7f22da2bf90d98d94d468621df26caadf853ca`, recorded by the
application and async research and spot-checked against the cited source lines; gaps,
not a source verdict:

- The application crate's publisher runs the synchronous publish to completion in
  its first `poll`, including a possible `Overflow::Wait`. It is documented, but it
  does not meet the progress property.
- In `up-rust-tokio`, a second `recv` on one route replaces the first task's waker,
  and `Recv::drop` clears the route's cell. The concurrent-waiter property needs
  per-operation registration ownership instead.
- `drive` derives a local `Instant::now() + ttl` after invoke and initial polling.
  Core's own deadline may still refuse a late response, so this alone is not a
  demonstrated deadline bug; the composed clock and acceptance proof is missing
  and must not be replaced by a fresh local duration.
- `service_loop` asks the application to spawn one task per transport. That must
  become SDK-owned service, not a correctness duty of the application.
- `core/src/transport/wake.rs` reserves opaque two-word storage and leaves
  arbitration to the async layer. Its ownership across every safe access path,
  callback replacement and cancellation needs a reviewed design. Nothing here
  classifies it as sound or unsound.

## Acceptance plan

Async API freeze needs working Tokio **and** Embassy integrations through the same
public seam, after the foundational repairs and before dependent Zenoh and MQTT5
work. No permanent Tokio-specific SDK crate is assumed. Controls, each with a
meaningful negative control:

1. Readiness before, during and after registration; spurious and coalesced wakes;
   a missing-recheck negative control.
2. Two and capacity-many waiters on one route and one call table; an older waiter
   abandoned after a newer registration; all remaining waiters progress.
3. Abandonment before first poll, after admission, before and after submission,
   during completion validation and after ready; a stalled release worker; a
   release held pending behind its dependency while other operations progress.
4. A forgotten pending and a forgotten completed operation: capacity stays
   charged, no dangling registration, no reuse.
5. A response accepted before expiry and observed late keeps its success; one that
   becomes acceptable after expiry is refused, also through a timeout wrapper that
   reports ready; early, spurious and late timers; clock steps and a failed read.
6. One real native owner and one allocating serialized value per role: no native
   TX loan at any `Pending`, exactly-once release or destruction, no serialized
   fallback.
7. Heap windows on native paths for the exact executor package; unsupported
   native async composition unavailable or refused by name.
8. Application polls stopped while arrivals, expiry and cancellations continue:
   SDK accounting stays coherent.
9. One release held pending indefinitely under each executor: progress steps,
   timers, expiry, notifications and the returns of independent owners continue
   on the executor's contexts; a configuration that would run that release on an
   executor worker is refused by name.
10. A route that requires only non-waiting acquisition, whose owned value's
    release waits in a `poll`, and a route that requires only non-waiting release,
    whose allocation waits in a `poll`: each admitted and reported, and neither
    refused over the guarantee it did not require.

Record host execution, MCU compile, link, emulator and physical-board evidence
separately. Ordinary serialized async needs no numerical J.

## Feasibility evidence and its limits

Rust 1.100.0, safe Rust only; the selected safe dependency `atomic-waker` 1.1.2's
registration contract is a dependency lemma.

| Experiment | Observation | Limit |
| --- | --- | --- |
| Portable seam probe | 11 controls pass: registration boundaries, wake after pending, two independent slots, cancellation before submission, while pending and after ready, duplicate and late completion, 128 concurrent cancel/complete schedules, a forgotten future, the logical acceptance predicate | Synthetic one-shot records: no payload owner, no slot reuse, no generations |
| Same seam on Tokio 1.54.1 and Embassy executor 0.10.0 | Both pass with real `Sleep` and `Timer` waking the producer; ownership credits returned once | Host aarch64 macOS only |
| Embassy task graphs | `thumbv7em-none-eabihf` and `riscv32imac-unknown-none-elf` type-check | No link, BSP, interrupt, timer driver or execution |
| Timeout ordering | Both generic wrappers accept an immediately ready inner future past their deadline | Shows why the SDK's own acceptance rule decides lateness |

These show feasibility only. They are not native retention, generation, heap
window, source-safety, timing, MCU, qualification or pending-cleanup isolation evidence, and the final
public SDK integrations still need their own acceptance evidence.

## Unsafe obligations

The seam's registration storage and any `unsafe` in the async layer are authored
and reviewed with the pinned skill. Apply `[conv.anti-gullibility]` (inspect the
code before the comment), `[doc.hidden-obligations]` (no unchecked precondition on
a safe API), `[dep.caller-adversarial]` (a caller's waker, timer or callback may
panic, reenter or allocate), `[hazard.edge-domain]` (in particular `mem::forget` of
a guard or future), `[hazard.send-sync]` and `[rej.miri-passes]`.
