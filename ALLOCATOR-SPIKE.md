# Explicit allocator and MCU integration experiment

This feature branch extends the shared SDK at204eaa24. It is an isolated experiment;
the production feature branch and its Rust1.88 minimum remain unchanged.

- `core/`: dependency-free, no_std, no alloc. The existing checked aligned-memory
  reservation is public so independent providers can use it safely.
- `std/`: optional host startup-memory convenience. Reviewed extraction preserves
  the existing facade API and delegates checked placement to core.
- `alloc/`: separate optional Rust1.100 package/workspace. Standard Allocator,
  fallible Vec/Box helpers and a reusable fixed startup-region provider.

The fixed provider never frees backing storage or resets its arena. Sealing
refuses new nonempty reservations; element destructors retain their own behavior.
Native payload storage remains in underlying-transport loans/samples.

External Cortex-M and RISC-V provider crates use public interfaces and pin this
same experimental SDK commit. The intended emulator profiles are Cortex-M7 on
QEMU mps2-an500 and RV32IMAC on QEMU sifive_e,revb=on. No physical board is required.

Current source still contains wide atomics outside the memory allocator. Complete
32bit SDK target support is under active implementation; these provider experiments
are not a claim of a linked MCU image, RTOS integration or qualified deployment.
Ferrocene lists M7 targets; RV32IMAC is an upstream-Rust experiment. Evolving FLS
or alloc qualification does not gate the current MCU engineering work.

This composition passed core/std host checks on stable, Rust 1.88 and Rust 1.100,
and the optional alloc package passed its Rust 1.100 host check. The external
provider fixtures also passed their host package/example and common consumer
checks. The seven accepted allocator controls were reused for the unchanged
implementation. Checks of consumers resolving the exact published Git revision
remain a separate integration step; publication alone does not close that step.

Host observations and source review do not establish runtime resource guarantees
for arbitrary element code, transport integration, or target timing/recovery.
