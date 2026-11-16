# The target environment is verified

`uman~sdk-aou-target-environment~1`

Covers:
- aou~seooc-target-environment~4

## Status and scope

Draft integrator guidance for the revised assumption. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- This entry is new: earlier versions of this folder listed it without providing it.
- No target, toolchain, BSP or board is qualified by it.
- A supported configuration's package carries the target evidence. The integrator confirms the deployment fits it.
- Strict mode refuses an assurance scope only where one is explicitly required and the package does not establish it.
- An ASIL-labelled development configuration without an assurance package proceeds, reported as Draft and unassessed.
- The statement was reworded in place: it points to the profile list and to each package's named backend instead of listing the present ones, and cites the raised revision of the bindings-and-wires assumption. Its meaning is unchanged.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> The target *SHALL* be one that a supported configuration's package names, with the exact compiler release, target, options and library scope, OS or RTOS, BSP and hardware revision it was verified on.
>
> * *What verification is.* Upstream support for a target, a compiler qualification for another scope and a successful build are each distinct from that verification. The package's maintainers supply that verification, and the evidence for the conditions below, for the parts, cores, execution contexts and participants the package names; for a supported configuration the integrator confirms that the deployment matches them, and supplies an argument of its own only for a context or participant the package does not cover.
> * *Profiles.* The SDK's profiles are those that the SEooC's README lists, and a supported configuration's package names the profile it covers. A backend that exists only as source, which no toolchain or device has run, is not a profile. A profile is not a claim that every binding runs on it, and no profile carries an ASIL of its own.
> * *Atomics.* The target *SHALL* have 32-bit atomics with compare-and-swap; 64-bit atomics are not needed, and a target's backend keeps the element's semantics without an atomic width the target lacks (xref:platform.adoc#req-ssr-atomic-width[No target needs an atomic width it lacks]). The RH850, AURIX TriCore, Cortex-M3, Cortex-M4, Cortex-M7 and e200 class qualifies, and so does Cortex-M23 (ARMv8-M Baseline), which has 32-bit compare-and-swap: the element selects by `+target_has_atomic+`, never by a list of targets. Cores without compare-and-swap, such as Cortex-M0 and Cortex-M0+, do not.
> * *Atomic lowering.* Each atomic read-modify-write the element uses is atomic on the target's core. On RH850G3K the toolchain lowers compare-and-swap to `+CAXI+`, never to an `+LDL.W+`/`+STC.W+` loop, which that core executes as a plain load and store. On any other core it lowers to a sequence that is atomic on that core.
> * *Several cores.* Where one instance of the element, or a route's memory, spans several cores, these operations are coherent across those cores. Where the cores' data caches are not kept coherent by hardware, the element's memory is placed where they are, or where it is not cached. Parts whose cores' exclusive accesses are not shown to be coherent, such as STM32H7-class parts that pair a Cortex-M7 with a Cortex-M4, run a separate instance on each core, and a route between those cores uses a transport that synchronises them by its own means (xref:../up-l1/deployment.adoc#aou-seooc-qualified-bindings-and-wires[Bindings and wires pass their contracts]).
> * *Different ABIs.* Cores of different ABIs, such as a 64-bit Cortex-A beside 32-bit Cortex-R or Cortex-M cores, share no native representation: data between them takes a copying route, through a transport that synchronises the cores by its own means, and the element refuses a native wire between peers of different ABIs at setup (xref:../up-l1/peers.adoc#req-ssr-native-abi-agreed[A native representation never crosses an ABI]). For cores of one ABI that share memory, each core observes the other's atomic operations coherently. The element cannot detect a violation of that.
> * *Interrupt masks.* A private interrupt mask excludes only what it masks: an unmasked interrupt, a non-maskable interrupt, a fault handler or a DMA engine that the deployment actually has and that can reach the element's memory needs an argument of its own, from the package where it names that participant.
> * *Backends and custom platforms.* A supported configuration's package names the platform backend it uses. A target or configuration that no package supports needs a custom platform, an explicit extension (<<aou-seooc-integrator-platform,A custom platform keeps the platform's rules>>).
> * *Stacks and departures.* The integrator sizes the stacks of threads that call the element using its upper estimates, and analyses where the environment the element was verified in departs from the target.
> * *Strict mode.* In strict mode, the Enforce and Locked states apply the target, dependency, ownership, known-failure and deviation checks to every configuration, and refuse, naming the claim and the missing evidence, a safety-assurance scope that a route or process explicitly requires where its configuration's package does not establish it (xref:qos.adoc#req-ssr-qos-no-downgrade[QoS is honored or refused, never weakened]). An integrity level or the Enforce state is not such a request: a configuration without an assurance package is not refused for that alone, and its report says that its assurance is Draft and unassessed. The report lists the coherence duty on multi-core parts and the stack-sizing duty.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Select and record the supported configuration.**
  - Select the supported configuration and package version that the target uses, and record its exact compiler release, target triple, options, library scope, OS or RTOS, BSP and hardware revision.
  - Treat upstream target support, a compiler qualification for another scope and a successful build as distinct from that configuration's verification.
- [ ] **Cite the package's atomics evidence.**
  - Cite the package's evidence for 32-bit compare-and-swap, for how the toolchain lowers each atomic read-modify-write on the core (for RH850G3K, CAXI rather than an LDL.W/STC.W loop) and for coherence across the cores that share the element's memory.
  - Confirm that the deployed part, core count and memory placement are the ones the package covers.
  - Derive that evidence yourself only for a custom platform, a changed context or a part outside the package.
- [ ] **Keep cores that are not coherent apart.**
  - Run a separate instance per core on parts whose exclusive accesses are not shown coherent.
  - Between cores of different ABIs use a copying route through a transport that synchronises them, and confirm the native-ABI refusal at setup.
- [ ] **Account for what an interrupt mask leaves out.**
  - Where a private interrupt mask protects element state, take the contexts it covers from the package.
  - For each interrupt, non-maskable interrupt, fault handler or DMA engine that the deployment actually adds outside them and that can reach the element's memory, give an argument of its own or record a deviation.
  - No census of participants the deployment does not have is needed.
- [ ] **In strict mode, state which scopes are required.**
  - In strict mode, state which routes or processes explicitly require a safety-assurance scope.
  - Expect a named refusal where the package does not establish that scope.
  - Expect every other configuration, an ASIL-labelled development configuration without an assurance package included, to keep its target, dependency, ownership, known-failure and deviation checks and to proceed with a report that says Draft and unassessed.
- [ ] **Size stacks and analyse differences.**
  - Size the stacks of threads that call the element from its upper estimates.
  - Analyse where the verification environment differs from the target. An emulator run shows functional behavior only.

## Violation manifestations

- A lowering that is not atomic or not coherent can corrupt the element's state words.
- An unmasked participant can race state that a mask was meant to protect.
- A target outside its package's verified scope leaves that package's evidence without force.
- A stack overflow can corrupt memory.

## Detection limits

- The element cannot detect a lowering that is not atomic or not coherent, an unmasked participant, or a target outside its package's scope.
- A successful build or emulator run does not verify the physical target.
- The six current SDK profiles have functional support, no profile carries an ASIL of its own, and QNX remains future, source-only work.
- An integrity level or an ASIL label is a declaration. It neither requests nor confers assurance.
