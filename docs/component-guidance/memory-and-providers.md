# Memory layers and providers

Implementation guidance; see the [index](README.md) for its status. It covers the
optional `up-rust-alloc` and `up-rust-std` layers and independent memory-provider
crates such as `up-alloc-cortex-m-rust` and `up-alloc-riscv-rust`.

## Layer boundaries

- `up-rust-core` owns checked placement, layout arithmetic, capacity, ownership
  and the setup/runtime transition. It stays `no_std`, without `alloc`, without
  dependencies and without Cargo features (`req~ssr-crate-isolated~1`).
- `up-rust-alloc` is an optional allocator-aware layer above core. It is
  experimental on Rust 1.100 and depends only on core. It obtains setup memory
  through an explicitly selected allocator and hands regions to core, which checks
  and owns them like static regions (`req~ssr-memory-static~2`).
- `up-rust-std` is an optional host convenience that obtains setup regions from
  the global allocator and delegates every rule to core. At the frozen SDK
  revision it depends directly on core and keeps the 1.88 minimum supported
  version. A proposed `std -> alloc -> core` graph is not current until it is
  implemented, and it must not silently raise std's minimum compiler.
- A provider is an independent crate selected by the application through public
  APIs. No SDK fork, private access, privileged registration or built-in provider
  list is needed, and selecting a provider admits no runtime resource or safety
  claim by itself.
- None of these layers is in a claimed ASIL graph at this revision
  (`req~ssr-crate-core-only-binary~2`). ASIL-path storage stays static or
  caller-provided. An optional layer can join a later claimed graph only with its
  own allocated requirements and evidence, and it must not bypass or reimplement
  core's checks.

## The provider contract record

Each supported provider, in each configuration a package supports, states:

| Field | What it says |
| --- | --- |
| Allocation domain | Which backing it serves, and that no request falls back silently to another domain |
| Requests admitted | Sizes, alignments, zero sizes and zero-sized types, and any request it refuses |
| Ownership and lifetime | How ownership of a block or region transfers, how long it stays valid, and what forgetting a handle does |
| Exhaustion | Whether a refused request returns an error or can panic or end the process, on every path: allocation, growth, shrink, destruction and deallocation, dependencies included |
| Release | Whether a release can wait (for example, behind a lock); if so, the dependency it waits for, whether it holds that dependency or anything public allocation needs while it waits, and whether its own mechanism returns by a finite deadline (a timer or a worker around it does not); and whether release, destruction or deallocation of any block it issued can panic or end the process. An admitting route needs evidence that none can panic or abort (`req~ssr-heap-slot-types~2`), and a release that can wait is admitted only as isolated pending cleanup (`req~ssr-release-lease-drop~3`) |
| Reclamation | Whether released blocks are reused, or retained permanently, and what growth leaves claimed |
| Contexts | Threads and interrupt contexts it may be called from, the synchronization participants it relies on, and which of them a waiting release could hold or occupy |
| Evidence | Exact source, toolchain, target and configuration of its evidence, its known limits and, for a release that can wait, the isolation argument and any established release progress |

This record is what `req~ssr-heap-declared~2` uses for a serialized route's declared policy and what `req~ssr-mode-init-failure~3` uses during setup. Its statements come from the provider's own contract, not from the shape of the call.

- In Rust 1.100, the standard `Allocator` trait's `allocate` returns a `Result`, but its documentation permits an implementation to abort or panic on exhaustion instead.
- A `try_*` wrapper therefore cannot establish recoverable exhaustion for a provider whose contract does not promise it.
- The published `linked_list_allocator` 0.10.5 documents a panic when its setup storage is too small; a provider that wraps it checks the size before that call.
- Stating the panic does not make the call admissible (`req~ssr-mode-init-failure~3`).

A provider used unchanged in a supported package is the package maintainers'.
A custom provider is an explicit extension with its own evidence, in the same
way as a custom platform. No current claimed configuration admits provider-backed
storage, so no assumption of use binds an integrator to a provider proof today.

## Placement and live ownership

Core places an object only after checking that its storage lies within the
region, is aligned and sized for it and is owned by no other live object, and a
refused placement exposes no object (`req~ssr-memory-static~2`). The region source
or provider hands each region over exclusively, for the life of the process.
Exclusivity is a property of live ownership, not of numeric addresses: a region
may reuse addresses of an earlier owner that has ended.

The independently rechecked REGION-001 allegation, an all-safe path to two live
overlapping regions, is rejected: exclusive reference transfer and the consumer's
ownership defeat it. `RegionSource` does not need to become unsafe or sealed on
that premise. REGION-DOC-001, a prose correction about historical addresses,
remains a separate documentation item.

## Partial setup and retained backing

The requirement `req~ssr-mode-init-failure~3` asks every setup refusal to leave each acquired
resource and initialized value with one stated disposition. The current
mechanisms, as the memory research recorded them at the frozen revision, already
differ. A disposition is acceptable only when it is the stated one and also meets
that item's ownership and failure duties; stating it is not that proof:

| Mechanism at the frozen revision | Disposition on refusal |
| --- | --- |
| `try_box_in` in `up-rust-alloc` | Returns the untouched value and allocator to the caller |
| `StartupMemory::place` in core | Drops the input value; earlier successful placements stay placed and are never dropped |
| `up-rust-std` region growth | Retains every issued region for the life of the process, including one that lost a concurrent growth race |
| The fixed allocator's growth and shrink | Retains blocks it has handed out; history stays claimed |

`StartupMemory::place` drops a refused input in setup code. That meets
initialization's rules only for an input whose destruction is established not to
panic or abort and to finish within setup's bound (`req~ssr-mode-bounded-startup~2`);
otherwise the input has to be handed back to the caller or refused before it is
taken. Setup never hands a refused value to the release service, and retaining
startup storage is no licence to leak an owning value whose destructor would wait. The frozen revision records the drop,
not that proof, so this stays source work.

No refusal promises a transactional rollback of an already entered transport
(`req~ssr-mode-entry~5`), and none may free storage that a successful owner can
still reach. A same-process retry on sealed memory is refused
(`req~ssr-memory-sealed~3`); recovery is a restart.

## Honest resource figures

The requirement `req~ssr-memory-computable~3` asks each figure to say what it measures. The std
helper's `MemoryReport` at the frozen revision counts what was linked; it excludes
unlinked race-loser allocations, excess `Vec` capacity and allocator overhead.
Report it as such, never as the retained backing or the physical footprint.
Sizes that depend on a scheduler, an allocator's metadata or a stack belong to the
supported configuration's package. Stack sizing remains a final-image obligation
(`aou~seooc-target-environment~4`).

## Native paths

Allocator-aware collections are setup tools. A fixed-region allocator does not
admit `Vec` or `Box` operations on a native runtime path: growth, shrink,
deallocation and element destruction inside a pre-obtained region are still
allocator activity (`req~ssr-heap-none~2`). A serialized route may allocate through
its declared provider, and every value it admits is released once when its
ownership ends (`req~ssr-heap-slot-types~2`); a destruction that could wait is
moved off the drop that ends it. Only the wait moves: the release service runs
the same destruction, so an owning type or provider is admitted only with
evidence that its destruction cannot panic or abort (next section). Returning a
native loan or retained sample to its middleware frees no heap and never frees
the backing it lives in. A deferred native return is pending cleanup too: it
acquires and frees no heap and frees no backing, on whichever thread completes it.

## Cleanup admission

`req~ssr-heap-slot-types~2` admits an owning type on a serialized route only
where setup establishes that destroying each value and releasing what it owns,
through every destructor, allocator and dependency it reaches, cannot panic or
abort, at once or on the release service. Otherwise setup refuses the route,
naming the type, wire, binding or provider, before it takes any value. A
destruction that can wait is also admitted only as isolated pending cleanup
(below). This is a behavioural admission rule. The SDK's memory safety must never depend on it.

| Evidence that can establish it | Note |
| --- | --- |
| The value has no destructor (`!core::mem::needs_drop::<T>()`) | A type fact; nothing is released. This admits such types without a proof record; it is not a ban on the others |
| The drop glue consists only of SDK-owned types, the selected wire's own types and standard containers, and its releases go only to the route's declared provider | Maintainer proof work for a supported wire and provider, pinned to their versions. The provider's record (above) must establish that deallocating its own blocks cannot panic or abort |
| A caller-defined type with its own `Drop`, or a custom provider | An explicit extension with its own evidence for that exact type, provider and set of contexts, supplied through public setup APIs and shown in the startup report. Acceptance reviews that evidence, not the presence of a record |

None of the following establishes it:

- `catch_unwind`. It cannot catch an abort, does not stop the panic from
  happening and does not show that the destruction finished.
- `panic = "abort"`. It turns the panic into a process end.
- A `Result` or `try_*` interface. It concerns allocation exhaustion, not a
  destructor's behaviour.
- A safe marker-trait implementation on its own.
- A blanket refusal of types that need dropping. It would refuse ordinary
  serialized owners whose destruction is shown safe.
- A new `unsafe` API asserting the behaviour. `unsafe` marks memory-safety
  contracts, not behavioural promises.
- A built-in list of providers. Admission stays open to any provider or type that
  brings its evidence through public APIs.

The memory-safety defences are required in every case and grant no permission:

- **Unwinding.** A panicking destructor, which is a defect, still cannot cause a
  second destruction or slot reuse.
- **Forgetting.** `mem::forget` leaves the slot charged
  (`[hazard.edge-domain]`).
- **Partial initialization.** A failed decode leaves nothing half-owned.
- **Reentrancy.** A destructor is caller-adversarial code that may call back into
  the SDK, so the service holds no broken invariant while one runs
  (`[dep.caller-adversarial]`, `[evidence.caller-code]`).
- **Thread transfer.** A value handed to the service crosses threads, so it must
  be `Send` (`[hazard.send-sync]`). A type that is not `Send` is destroyed at once,
  where its destruction cannot wait, or refused. `Send` is necessary, not
  sufficient: the value, its allocator and its backing must stay valid for as
  long as the release may stay pending, which no `Drop` elsewhere can guarantee
  (`[hazard.edge-domain]`).
- **Service failure.** A stalled or failed service leaves its values charged and
  shows as capacity pressure. It never frees a slot early.

The release service is created during initialization and runs for the life of the process.

- Its hand-off is lock-free and preallocated for every value that can be pending.
- It acquires no heap memory and needs no call from the application to make progress.
- It holds ownership awaiting destruction, not unread traffic, so it is not a second receive queue.
- It is distinct from a binding's deferred return of native retained samples, which frees no heap.
- The exact iceoryx2 port-lock anomaly keeps its bounded scope (`req~ssr-binding-frame-drop~4`) and is never a general destructor exception.

### Waiting releases are pending cleanup

Panic-free admission and waiting are separate questions. A release that waits
behind its allocator's own lock cannot see a deadline from inside that call, and
running it on another thread does not change that. `req~ssr-release-lease-drop~3`
therefore treats a handed-off destruction, and a binding's deferred return, as
*pending cleanup*:

- **No completion deadline.** The operation that ended the value reports its
  outcome on time (`req~ssr-bound-wait~2`); nothing promises when the release
  completes. A timer or timeout around a release that cannot be cancelled is not
  a completion bound.
- **Ownership, not completion.** Ownership passes to the service exactly once.
  The charge and the completed-release count change only when the release
  finishes. A stalled or partly completed release is never retried as a fresh
  destruction, and no observer revokes it, reuses its storage, reports it
  complete or frees its backing.
- **Isolation.** While a release waits, it must not hold or occupy anything that
  public operations, drops and hand-offs, arrival, deadline, expiry and
  peer-failure accounting, health and capacity reports, or the return of an
  unrelated owner need. Check it transitively: allocator and allocation-domain
  locks, middleware port locks, the service's dispatch and wake paths, shared
  exclusion and reentrant callbacks. A separate thread is not by itself
  isolation, and one blocked release must not take the only worker that
  unrelated returns or deadlines need.
- **Capacity, not throughput.** Pending values stay charged, and real exhaustion
  is the route's ordinary truthful refusal. Fixed capacity contains a release
  that never completes; it does not keep throughput going. A claim that capacity
  recovers within a time needs release progress and sizing established by the
  package; a measured average delay is not a bound.
- **No unbounded computation.** The service's own control work and bookkeeping
  are bounded, and the admitted waiting dependency is named. A destructor that
  loops for ever in caller code is not admitted because a worker exists.

Practical Rust design paths, all still to be built and proved:

| Owner and provider | Design path |
| --- | --- |
| No drop glue, or a release that cannot wait | Destroy inline on the drop; the service is not involved |
| A `Send` owner whose release may wait behind its provider's lock | Hand the owner itself through the lock-free hand-off to a service context that waits only for that lock and holds nothing else. One context per independent dependency (an allocation domain, a port) keeps one stalled release from stopping unrelated returns. A non-blocking attempt that requeues instead of waiting is an alternative only where the provider offers one; the standard `Allocator::deallocate` has no such variant |
| An owner that is not `Send`, or is tied to a borrow | Destroy inline only where its release cannot wait; otherwise refuse by name. A stack borrow cannot be extended by relying on `Drop` |
| An allocation that may outlive a forgotten or stalled cleanup | The owner keeps its allocator and backing alive itself, for example through a process-lifetime handle, never through a borrow that the service could outlive |
| A provider whose release holds a lock that public allocation also needs while it waits | Not admitted under this contract without a mechanism that keeps public allocation bounded; ordinary QM use of that provider is unaffected |

Standard containers qualify only when their whole element and provider destruction graph meets admission; neither "standard" nor an `Allocator` implementation shows timing or panic freedom.

- Supported provider and worker packages are maintainer work: the SDK, binding, platform and provider maintainers establish the composition and package the check and the named refusal.
- Integrators select a supported configuration and declare its load; they do not design worker pools, audit lock graphs or promise how arbitrary callbacks behave.
- A custom extension brings evidence for its exact type, provider and contexts through public APIs, and a type, trait or record name is never that evidence.
- An arbitrary lock-based provider without the evidence stays unsupported for this contract, while ordinary supported allocating and QM use remains available.

None of these mechanisms is implemented at the frozen revision. The existing
fixed-provider and owning-value experiments and the Tokio and Embassy probes show
only their recorded feasibility; they do not show this isolation.

The documentary requirement is the admission obligation and its refusal.

- The mechanism is source work that is not implemented at the frozen revision: the evidence record, the named refusal, the service, its capacity and its isolation.
- Source acceptance needs a per-type, per-provider and per-context proof covering reentrancy, unwinding, forgetting, failure and stall, capacity, the isolation of waiting releases, exactly-once transfer, lifetime under forgetting, allocation identity and layout, and the scheduler or worker composition.
- Tests and Miri supplement that argument; they do not replace it (`[rej.miri-passes]`).

## Unsafe obligations

A provider's `unsafe impl` of `Allocator` must prove, with the pinned skill:

- returned blocks fit the requested layout and alignment, including `0 < size < align` (`[hazard.edge-domain]`);
- deallocation and `grow`/`shrink` receive only blocks this allocator issued, with matching layouts;
- zero-size requests honour the trait's contract;
- provenance and exclusivity of every block;
- and `Send`/`Sync` only where concurrent use is actually synchronized (`[hazard.send-sync]`).

A lifetime-bound allocator borrow must not let a block escape the allocator's lifetime; Rust rejects that escape at compile time (experiment MEM-E02).

## Feasibility evidence and its limits

All on exact Rust 1.100.0 (`0ef1bc386059596dada8fb65b50425aab2ea637a`),
aarch64-apple-darwin, with no feature gates; drivers forbid unsafe code.

| Experiment | Observation | Limit |
| --- | --- | --- |
| MEM-E01 | Staged custom-allocator construction over unchanged copied core memory and alloc kernels destroys two initialized values once on an injected refusal, three once on owner drop; backing stays claimed | Reduced host wrapper, not a whole SDK build, transport setup rollback or serialized runtime destruction |
| MEM-E02 | A box borrowed from an allocator cannot outlive it: E0515 | A type-system illustration only |
| MEM-E03 | Real `linked_list_allocator` 0.10.5 admits seven 32-byte, 32-aligned blocks from 256 bytes, then refuses with a stable used count; a one-byte setup panics as documented | One host control; no target, interrupt or timing claim |

One driver compile failed with E0277 before correction; its source and log are
kept. The seven earlier allocator controls are reused at their own recorded
scope. Both published providers are thin wrappers over one fixed-region
implementation, local and not `Sync`; they are not board, RTOS or interrupt
integrations, and compile or link success does not qualify them.

## Acceptance checks still to run

- Build a separate ordinary consumer that selects a provider through public APIs
  only, and check that every allocator-aware constructor receives the selected
  allocator.
- Inject a refusal at each provider and placement boundary during setup; check
  the stated disposition of every value and region, that no setup destruction
  runs for a value whose destruction is not established, that none waits beyond
  setup's bound and that nothing is handed to the release service.
- Refuse by name, before any value is taken, a route whose owning type, wire,
  binding or provider lacks established cleanup evidence. For an admitted type,
  count destructor runs at once and on the release service. Stall the service and
  check charged slots, capacity pressure and no early reuse.
- Hold one release pending indefinitely behind its dependency. Check that the
  ending operation reports its outcome on time; that bounded operations,
  deadline and expiry accounting, health and capacity reports and the returns of
  owners that do not need that dependency progress; and that a route whose
  waiting release cannot be isolated is refused by name.
- Exercise repeated refused setups, growth histories and lost races, and compare
  them with what the report says it measures.
- Source-review each provider and the alloc and std layers against the record
  above with the pinned skill; compile-fail checks for lifetime escape and
  unsupported cross-thread use.
- Keep core-only operation working with identical checked outcomes, and keep
  std at its current minimum compiler until a justified dependency change.
