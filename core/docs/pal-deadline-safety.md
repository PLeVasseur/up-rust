# Source proof for the host deadline successor

This proof describes the internal host deadline backend in this SDK. The
Linux GNU x86-64/ARM64 extension is an unintegrated candidate; section8 supplies
its additional CPU/ABI obligations.
Its algorithm and atomic wrapper received independent source review before
composition with the private PAL seam and host ID generator. The selected
Apple Clang profile is stated below. Source review does not establish finite
report timing, another platform, a native transport route or ASIL qualification.
The full pinned Google skill was read before authorship. In particular this
argument follows [model.proof], [fact.classify], [conv.anti-gullibility],
[conv.field-invariant], [conv.safety-usable], [hazard.send-sync],
[hazard.edge-domain], [doc.hidden-obligations], and [rej.miri-passes].

## 1. Authority and proof boundaries

**Rust axioms:** the Reference's type-layout and undefined-behavior chapters,
and the standard library's UnsafeCell, Send, Sync and atomic module contracts.
The authoritative contracts are the [Rust Reference layout rules](https://doc.rust-lang.org/reference/type-layout.html),
[undefined behavior](https://doc.rust-lang.org/reference/behavior-considered-undefined.html),
[UnsafeCell](https://doc.rust-lang.org/stable/core/cell/struct.UnsafeCell.html),
[Send](https://doc.rust-lang.org/stable/core/marker/trait.Send.html),
[Sync](https://doc.rust-lang.org/stable/core/marker/trait.Sync.html), and
[atomics](https://doc.rust-lang.org/stable/core/sync/atomic/index.html).
repr(C) establishes field order/offset rules; repr(align(16)) raises alignment;
UnsafeCell has the inner representation and permits interior mutation, but
neither erases exclusive-reference rules nor prevents races. All u64 bit
patterns are valid. Send permits transfer; Sync permits shared references across
threads. Atomic ordering supplies modification/coherence and release/acquire
relations, not a physical visibility deadline. No LLVM behavior is a Rust axiom.

**Selected dependency lemma C-ATOM:** Apple Clang 17.0.0
(clang-1700.6.3.2), arm64-apple-macosx11.0.0, Apple M1 baseline, implements its
GCC-compatible generic __atomic_load and __atomic_compare_exchange as documented.
[Clang LanguageExtensions](https://clang.llvm.org/docs/LanguageExtensions.html)
explicitly documents the GCC-compatible builtins; the
[GCC builtin contract](https://gcc.gnu.org/onlinedocs/gcc/_005f_005fatomic-Builtins.html) permits arbitrary same-sized types for the four generic
non-arithmetic operations. weak=false is strong (no spurious failure), success
acq_rel/failure acquire has its documented meaning. __atomic_always_lock_free
is a compile-time admission, not an optimistic runtime fallback. The Rust and C
code participate in the same process/thread memory-ordering execution through
their synchronous FFI calls. Compiler/ABI correctness is an explicit platform
trust premise, not derived from passing tests or a Rust-language layout rule.

**Project semantic premise:** SDK 4b5d187 deadline.rs and report.rs define
forward-only full-u64 origins, at most one due credit per origin, full-u128
saturation, sticky health, no live reset, and a delayed independent legacy u32.
MonitorConfig is closed and enforces shift<=53, cutoff<=2048. These are concrete
source invariants, not hidden obligations on safe application callers. Arbitrary
safe scalar samples (old, equal, zero, MAX) remain accepted as inputs and are
handled by the existing forward-sample rule. The clock/ingress/timing properties
needed to interpret physical gaps remain separate assumptions to qualify.

**Evidence only:** actual host compilation, deterministic schedules, the real
SDK adapter controls, maximum controls, no-panic link, allocator observation,
C layout assertions, IR and disassembly. None replaces the following proof.
Changed-source review must challenge both the premises and each preservation
step. No Miri claim is made for the C-backed primitive.

## 2. FFI and storage kernel (all unsafe product sites)

The unsafe product boundary is Atomic128 in
`src/platform/deadline_atomic128.rs`, paired with `src/atomic128.c`. Its storage is repr(transparent) over
UnsafeCell<Words>, with Words repr(C,align(16)) containing low:u64, high:u64.
Rust const assertions enforce size=16, alignment=16, offsets=0/8. C hpr3_word
has exactly those fields, size, alignment and offsets, enforced by _Static_assert
**equality**, not >=. The fields occupy all 16 bytes: no padding is read or
compared. There is no C _Atomic object whose separate initialization,
representation, lifetime, or address identity must be guessed. Rust constructs
ordinary initialized integer storage, and the chosen compiler's generic atomic
builtins perform every shared access to that storage.

The SDK build script compiles the shim for aarch64-apple-darwin (arm64,
minimum macOS11.0), x86_64-unknown-linux-gnu (-mcx16), and
aarch64-unknown-linux-gnu (-march=armv8-a+lse). Linux x86-64 additionally requires
an explicit Rust cmpxchg16b minimum-ISA target profile, as described in section8;
its CPUID query is not an automatic system-wide admission mechanism. The Linux
profiles also require the local CPU/OS witness in section8. C rejects a non-lock-free
16-byte primitive at compilation. Unsupported CPUs refuse monitor setup, with
no lock fallback. Other deadline backends remain unchanged and unqualified.
The complete SDK still requires independent qualification of all six profiles.

| Operation | Preconditions and enforcement | Postcondition |
|---|---|---|
| Atomic128::new | Two owned u64 values; no caller pointer, allocation or callback. Writes initialized Words inside its cell. | Exact-layout live storage; no retained foreign address. |
| Atomic128::load / hpr3_load | &self supplies liveness for the whole call; private field invariant supplies 16 initialized writable aligned bytes within one allocation. UnsafeCell::get preserves provenance; C receives a raw cell pointer, never a manufactured &mut. Distinct initialized u64 output locals are uniquely borrowed and disjoint from self. | One acquire whole-word observation, both outputs written, no pointer retained or ownership changed. |
| Atomic128::cas / hpr3_cas | Same cell liveness/layout/mutation invariant; four by-value integers. All overlapping shared accesses have the identical width and use the same builtin family. | One strong acq_rel CAS; failure acquire observes a different whole value. No spurious result, callback, allocation, free or unwind. |
| unsafe impl Send | Only initialized integer representation moves; no thread-affine resource, guard, address-dependent state, destructor or foreign retention. Rust borrowing excludes concurrent access during a move. | Moving the unshared cell preserves its representation and atomic-access invariant. |
| unsafe impl Sync | No safe method exposes the words/raw address. Every concurrent mutation stays within UnsafeCell and uses whole-word atomics. There are no overlapping non-atomic/mixed-width accesses. | Shared callers cannot create a race or invalid Rust reference through this wrapper. |

C uint64_t arguments/results through pointers match Rust u64 on the admitted
AAPCS64/Darwin ABI. The C comparison result is int and Rust i32, confirmed in
this profile's LLVM signature; it is not a foreign arbitrary-bit Rust bool.
Only synchronous extern C functions are called. C invokes no callback, throws
no exception, retains no pointer, and transfers no allocation ownership.

The hpr3_layout test query has no pointer precondition and returns one integer.
The resource harness's unsafe GlobalAlloc implementation forwards the exact
System allocator contracts; its comments prove each call separately. The
no-panic harness's undefined symbol is a link oracle, not a callable runtime
fallback. A successful link removes every reference to it. The harness's unique
main symbol belongs only to that dedicated executable. These harness unsafe
sites do not enter product source.

## 3. Semantic representation and injectivity

Let M = u64::MAX. Stable states are S(o,c), and a pending due marker is D(o,t).
A marker always has o<M and o<=t. t>o means a due arrival, t=o means a due check.
No count credit exists for a non-due arrival; that operation changes the stable
word with one CAS. The entire due descriptor is installed in **one** origin CAS.
There is no separate FREE/reserved proposal, pointer, owner ticket or reusable
slot. Both timestamps remain full 64-bit integers.

The exact word encoding is:

| Logical state | Two 64-bit words |
|---|---|
| S(o,false), all o | (o,o) |
| S(o,true), o<M | (o+1,o) |
| S(M,true), representable though unreachable from normal due semantics | (M,1) |
| D(o,t), o<t | (o,t) |
| D(o,o), o<=M-2 | (o+2,o) |
| D(M-1,M-1) | (M,0) |

The strict upper triangle, diagonal, first lower diagonal and second lower
diagonal are disjoint. (M,0)/(M,1) are outside those diagonals for the fixed
64-bit domain M>3. Thus every permitted marker/stable state has a unique word.
The decode order implements these classes. No addition wraps in this domain;
wrapping operators merely make the Rust expression total and remove unreachable
panic branches. The two exceptional encodings prevent the old terminal alias.
D(o,M) is ordinary upper-triangle data, including o=M-1.

Stable states advance strictly in lexicographic (origin,counted) order, with
false<true. A check changes S(o,false) to S(o,true); an arrival has t>o and
changes to S(t,false). Neither can repeat a stable pair. A completed due at o
precludes another due at o: a check leaves counted=true until a later arrival,
and a due arrival itself advances origin. Therefore due IDs **g=o+1** are in
1..=M, strictly increase, and never repeat. ID0 denotes setup. A due at M is
impossible because no u64 sample exceeds M, exactly as in the normative source.
The service is not retired at a new limit: reports and missing-health updates
still operate at M and old/equal samples retain their original no-op meaning.

The counter is physically two full 64-bit payload limbs. Each limb is stored
alongside a full 64-bit due ID in its own 128-bit atomic. No count bit is used as
a tag, and no count width or caller population is shortened. The full result is
low | (high<<64), including every u128 bit and MAX. Tests can seed any full-u128
count before publication; the production constructor seeds zero. Safety does
not depend on a mission-duration assumption or a finite caller registry.

## 4. Publication and exact-once credit induction

Inductive stable invariant I:
- origin is S(o,c), with every previous due descriptor fully committed;
- low/high tags agree on the last due ID (or both zero at setup);
- the two payload limbs encode exactly the saturated total of committed credits;
- exhausted is true iff a committed increment beyond MAX has occurred;
- missing is independent, sticky unavailable-arrival evidence;
- every prior stable pair and due ID is permanently behind the current state.

**Expected old state / publication.** observe acquires one stable word, applies
the unchanged due predicate to it, and strong-CASes that **exact** word to either
S(sample,false) for a non-due arrival or D(old,target) for a due operation.
A due branch has old.counted=false and sample>old.origin, so its marker domain
and ID are valid. Losing the CAS cannot publish any descriptor or increment.
An old proposer resuming after arbitrary transitions compares the old complete
word, which never recurs, and fails. It then recomputes from current state.
Only a successful marker installer returns true and later mirrors the legacy
credit. A helper may finish first without changing that initiating result.

**Active-marker invariant II.** No other descriptor may install while this
marker is current. Other observers help it. Low can be old or tagged g; high
can be old or tagged g. High tagged g certifies that low tagged g and the
necessary exhausted store precede it. Later tags are observable only after
this marker was finalized and later markers were installed. Every acquiring
helper starts after the preceding stable publication, so a pre-g limb it reads
cannot be an arbitrary earlier generation: it is the complete preceding
credit's limb, unless this marker has already started/completed.

**Low credit.** A helper first acquires high. If high.tag>=g the credit is
already complete (or superseded). Otherwise it acquires low. If low.tag<g,
that pair is the exact pre-credit value if its CAS can still succeed. When
both payloads are MAX it release-latches exhausted **before** attempting low's
CAS and keeps low at MAX; otherwise it uses low.wrapping_add(1). The CAS matches
both old payload and full tag, and installs tag g. Only one helper can win.
If it fails, strong CAS guarantees another helper changed that word; acquiring
the reload cannot move backwards behind that failed observation. Its tag is
now at least g. There is no retry-until-owner loop.

A saturated observation cannot spuriously latch exhausted: high was read after
the acquired marker/previous finalization, then low was read; a successful
pre-g observation of both MAX describes this marker's already-maximal count.
If the helper is suspended before its sticky store, the same due marker has
already required the overflow, even if other helpers finished it meanwhile.
The store never depends on its later CAS succeeding.

**High credit.** With low.tag==g, carry is exactly (new_low==0). Saturation
retains low=MAX, so it never creates a carry. A true carry has old_high<MAX.
The helper CASes its original full high pair to (high.saturating_add(carry),g).
Only one succeeds; others' expectations fail even when payloads are unchanged,
because the full tag changed. If this helper sees low.tag>g, later work already
completed both limbs for g; it need not write high. There is no lost carry.
The acquiring low read orders the pre-low exhaustion store into high's release.

**Finalization.** Every path to the final origin CAS has either observed
high.tag>=g, observed low.tag>g, or attempted high's CAS after low.tag==g.
A failed high CAS means high advanced to >=g; a successful one commits g.
Thus all paths establish that g's full credit and latch are complete. The
exact D(o,t)->S(t,t==o) CAS is the logical due operation's linearization point.
It release-publishes the full credit before another marker can be installed.
All helpers compute the same result, and a stale finalizer can only fail.
There is no phase cleared before publication and no premature marker recovery.
I is re-established, including equality-target checks and terminal targets.

## 5. Suspension, reuse and all full-width edges

A helper may stop before/after every load, sticky store or CAS and resume after
any allowed number of later transitions. Its marker carries both full immutable
timestamps, and every write matches either that exact never-repeated marker or
a full limb pair whose due ID is older than its descriptor. Once g completed,
no later limb ever has an ID below g again. A paused helper that has not loaded
low sees >=g and skips credit; one holding a stale low pair loses its CAS; one
holding low=g and stale high loses high's CAS; a finalizer loses the marker CAS.
An old high>=g observation is already sufficient for completion. The only
unconditional write is the sticky, already-justified exhaustion store.

This argument is independent of how many subsequent due IDs have passed; it
uses strict order, not elapsed time, parity, modulo equality, a watchdog, caller
slots, or a stopped-helper reclamation rule. The finite timestamp domain is
inherited from the nonwrapping base clock. There is no new retirement before
its maximum and no assumption that a stale caller resumes before some wrap.
Tags never wrap because no legal due origin is MAX.

At count MAX-1 the low increment reaches MAX with no overflow and exhausted
stays false. At count MAX a later due credit retains MAX and sets exhausted.
At a low-limb carry the high limb increases exactly once; every u128 boundary
(32,64,96,128 bits) follows the same two-limb arithmetic. There is never an
expression u128::MAX+1. Stale helpers cannot lower tags even at saturation,
where payloads may be identical across arbitrarily many credits.

## 6. Coherent in-call report and planning snapshot

report acquires stable origin A, then both limbs, exhausted, missing, then
origin B. A visible marker is helped. Unequal words cause a retry. A==B with A
stable cannot hide any completed transition because stable states do not recur.
If a limb/exhaustion read observes a write from a newer marker, its acquire is
ordered after that marker's release publication; the subsequent origin load
cannot read the old stable state before that marker. Thus A==B excludes a new
limb paired with an old origin, torn limbs, and a new exhaustion flag paired
with a pre-MAX count. A's acquire similarly puts all preceding committed limbs
before the reads. Choose the missing flag's load inside this stable interval as
the report's linearization point. Concurrent missing stores are included or
excluded at that point; no arbitrary stale/eventual substitution is made.

A marker may exist physically during a reader interval, but it cannot expose a
partial result as a valid stable collect. A reader can logically precede an
in-flight update whose writes it did not observe; it cannot return an impossible
combination. report does not initiate a new check, read a clock, or return Busy
or a contention None. Its helping only finishes an already-admitted operation.

count_schedule makes one origin-word observation. If it finds a marker, it
helps that descriptor at most once and returns Recheck; it does not retry to
obtain a stable state. The service planner maps Recheck to ServiceNow, preserving
the previous completed-pass sample and preventing sleep. A stable observation
supplies the full origin/counting-bit state without count/health fields. The call
uses at most seven wide-atomic invocations plus an optional latch store, subject
to the primitive's progress contract. This bounds algorithmic work only, not
physical latency or service/publication J. Arithmetic follows the base:
first=(origin>>shift + cutoff + 1)*2^shift; maintenance=previous+configured.
For shift<=53/cutoff<=2048, first<2^66; maintenance<2^65. Widening is to u128
before addition/multiplication. Total wrapping operators equal mathematical
arithmetic throughout that domain; no endpoint is silently narrowed. The
existing service notification handshake must cover an arrival after this read.

## 7. Complete lifecycle and edge-domain audit

Standalone construction is checked/fallible; actual SDK construction consumes
its closed MonitorConfig and, for these hosts, a checked CPU witness. Unpublished state can move freely. CoherentDeadline
arm requires &mut self. The existing DeadlineSlot permanently publishes an
ArmedDeadline through startup memory; runtime users cannot regain that exclusive
borrow, reset a live counter, fabricate a marker, or expose cell pointers.
No new public application API, mode exception or unsafe integrator duty is added.

Every new product type has no Drop implementation, allocation, callback, timer,
thread registration, retained foreign handle or implicit cleanup. Borrowing
keeps synchronous FFI storage live. Forgetting a value/handle cannot release a
guard needed for safety, and no destructor is needed to finish a marker. Native
slot lifetime and deferred return outside this record remain G1/G2 obligations.

The four mandatory [hazard.edge-domain] cases:
1. **Zero size/length:** FFI always accesses a fixed nonzero 16-byte object and
   two u64 locals, not a caller-supplied slice. No zero-length/null exception,
   allocation, ZST, or `len-1` computation exists.
2. **Alignment/casts:** exact 16/16 size/alignment and offsets are compile-time
   checked on both sides; cell pointers preserve allocation provenance. No
   integer-to-pointer conversion or signed length cast exists. All narrowing
   of u128 counts to limbs is deliberate lossless split/reassembly, not identity
   truncation. Unsupported target pairs are refused for this candidate.
3. **Final stepping:** no pointer arithmetic, pointer stride, out-of-allocation
   projection, reference reconstruction, or loop endpoint pointer exists.
4. **Forget/drop:** no lifetime guard, background borrowed pointer, callback
   retention or drop-based synchronization exists. Setup-only seeds require
   exclusive &mut and are absent in production. Leaking a value cannot permit
   later use of deallocated borrowed storage.

The runtime path contains no panic operation on its admitted inputs; the strict
release link with overflow checks on confirms no panic branch remains in the
exercised host entry graph. Failed earlier links are preserved, not passes.
There is no runtime heap/free in this fixed metadata algorithm; measured
allocator observations and no-helper C symbols supplement source inspection.
This does not establish physical WCET, J, IPC robustness, middleware behavior,
complete CallSnapshot, general SDK panic freedom, or another architecture.

## 8. Linux candidate CPU admission and fallible setup

### Atomic admission

AtomicProfile has one private unit field. Its only product constructor calls
hpr3_admit; Copy/Clone can only duplicate a genuine witness. admit_if(false)
refuses without calling C. The true branch still checks the actual CPU, so a
safe boolean cannot forge admission. Atomic128::new requires the witness before
creating storage; every HostDeadline field and test seed receives it. HostDeadline
retains it for rearm, which requires exclusive access. No runtime CPU query or
foreign thread registration is added. Tests use an expect only in cfg(test).

hpr3_admit is compiled in its own translation unit without CX16 or LSE flags.
On x86-64, the build script first requires `cmpxchg16b` in Cargo's target-feature
metadata. Select it with `-C target-feature=+cmpxchg16b` or a target-cpu baseline
that includes CX16. The resulting executable requires CX16 on **every CPU on
which the process may execute**, for its whole lifetime, including migration or
hotplug. This is the ordinary minimum-ISA deployment requirement of that compiler
target, not a new caller affinity or concurrency restriction. SDK build admission
enforces the selected target; it cannot inspect or certify the deployment.
The generic x86-64 build without that target feature is refused at build time.

GCC-compatible cpuid.h __get_cpuid then verifies leaf1 and ECX bit13
(CMPXCHG16B) on the currently executing CPU only. CPUID is baseline x86-64;
the wide primitive uses Clang19 -mcx16. This probe is a local sanity check of
the selected profile, **not** proof that other eligible CPUs support CX16. The
compiler's minimum-ISA environment supplies that premise. A successful probe
must never be advertised as unrestricted generic-x86 automatic admission.
See [rustc target-feature](https://doc.rust-lang.org/rustc/codegen-options/index.html#target-feature)
and [target-cpu](https://doc.rust-lang.org/rustc/codegen-options/index.html#target-cpu)
for the selected compiler contract; these are compiler/deployment dependencies,
not Rust language axioms or hardware qualifications established by this package.
On Linux ARM64, getauxval(AT_HWCAP) tests HWCAP_ATOMICS; the primitive uses
Clang18 -march=armv8-a+lse. The AArch64 Linux ELF HWCAP contract supplies this
CPU feature availability across scheduler migration. Admission errors/absent flags yield
false; no guessed capability or libatomic fallback. Apple ARM64 retains its
existing baseline premise. See Linux getauxval(3), the AArch64 Linux ELF HWCAP
contract (HWCAP_ATOMICS), and Intel CPUID leaf1 ECX.CMPXCHG16B specification.
References: https://man7.org/linux/man-pages/man3/getauxval.3.html ;
https://docs.kernel.org/arch/arm64/elf_hwcaps.html ;
https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html .
Compiler dependency contract remains Clang GCC-compatible generic atomic builtins:
https://clang.llvm.org/docs/LanguageExtensions.html and
https://gcc.gnu.org/onlinedocs/gcc/_005f_005fatomic-Builtins.html .

The ABI is Linux x86-64 SysV / Linux ARM64 AAPCS64: uint64_t/u64 limbs,
int/i32 comparison result, raw pointer arguments, synchronous extern C calls.
The exact16-byte size/alignment and offsets0/8 assertions are retained on both
sides. UnsafeCell storage is writable even for hpr3_load (which may lower to
CMPXCHG16B); no Rust references to the words, partial atomics or non-atomic
shared accesses are introduced. No callback, pointer retention, ownership
transfer, destructor, allocation or free occurs in the wide primitive.

### Fallible setup and lifetime

CoherentDeadline::try_new obtains admission before constructing HostDeadline.
PreparedTiming::prepare propagates None as StartupError::InvalidConfig before
publication/activation of any monitor. An already prepared worker is still under
its existing initialization rollback guard. No new runtime failure outcome,
caller/thread cap, wait/Busy result, hidden spin lock or silent backend fallback
is added. Unmonitored routes do not require this backend. Non-host targets retain
their old constructor through an always-Some adapter; no MCU mechanism changes.

The four edge cases from the pinned unsafe-rust-review skill remain explicit:
fixed nonzero16-byte storage; exact alignment/no signed-pointer casts; no pointer
stepping; no destructor or forget-sensitive guard underpins atomic safety.
Witness lifetime is the process under the selected platform contract: explicit
whole-process CX16 target baseline on x86-64, ELF HWCAP_ATOMICS on Linux ARM64,
and the Apple ARM64 baseline. CPUID alone does not establish that lifetime.
Sources—not tests or disassembly—must justify this platform dependency lemma.

## Windows x86-64 MSVC target candidate

This port uses the same Clang generic atomic-builtins dependency contract and
exact two-u64 storage representation as the reviewed Linux x86-64 port. It
does not require Microsoft cl.exe to implement `__int128`: this source has no
`__int128` and is compiled by Clang for the MSVC target ABI. Clang and llvm-lib
are explicit build tools; they do not become runtime Rust dependencies.

The Windows compiler profile explicitly enables static unwind tables with
`-funwind-tables`. Microsoft's [x64 exception-handling contract](https://learn.microsoft.com/en-us/cpp/build/exception-handling-x64?view=msvc-170)
requires function-table and unwind information for code that saves nonvolatile
registers or changes its stack. The CX16 functions and baseline CPUID admission function save RBX.
The Windows admission TU uses explicit `cpuid` inline assembly with EAX, EBX,
ECX and EDX outputs and matching leaf/subleaf inputs. The EBX output makes
RBX clobbering visible to Clang so its Win64 frame lowering can preserve it
with static unwind coverage. Neither generic `cpuid.h` nor Apple Clang17
`__cpuidex` suffices here: both hide a temporary RBX change inside xchg pairs.
CPUID reads no caller memory; leaf0 supplies the maximum basic leaf, then leaf1
ECX bit13 supplies the local CX16 check. The baseline TU has no CX16 flag.
This dependency obligation applies to arbitrary-PC stack walking as well as
language exceptions; the absence of callbacks or C++ throws does not discharge
it. Compiler qualification inspects every generated Windows C function, including
`hpr3_admit`, and checks each generated prologue against its COFF
`.pdata`/`.xdata` ranges and unwind codes, and binds those objects into the
captured archive and Rust library. The tables are static data: no dynamic
function registration, runtime allocation, callback or new import is required.

The Rust executable must select `cmpxchg16b` in its minimum ISA. Every CPU on
which the process can execute must meet that compiler-target contract. CPUID
is a local sanity check, not a migration guarantee. The baseline admission
object receives no `-mcx16`; the wide object does, and a static assertion rejects
any compiler that cannot implement the exact 16-byte access lock-free. The
MSVC-target COFF archive is produced by llvm-lib. Both Rust and C independently
assert size and alignment exactly16 and field offsets0/8. All concurrent
access to each UnsafeCell object uses the same 16-byte builtins. Initial integer
writes occur before shared publication. No additional Windows object, address
registration, heap, hidden lock, reference creation or destructor is introduced.

The reviewed generation/helping/coherent-report state machine is unchanged.
This port does not export a Windows Native platform or compile the separate
WaitOnAddress candidate. The core can be built and its deadline algorithm checked
independently of that OS backend; no safe caller is given the unaccepted foreign
operation. Windows wait safety (WP-ATOMIC-1), final linking/execution, native
platform constructors, worker activation, finite J, resource qualification and
ASIL acceptance remain separate. A Windows core build is not a complete Windows
SDK/platform support claim.
