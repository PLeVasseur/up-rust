# Experimental core platform selection

On this experimental branch, ordinary Linux and macOS Cargo builds select the
existing host metadata implementation automatically. No custom Rust flags are
required. This remains a no_std core: selecting host does not add a dependency
on std, alloc, up-rust-std or up-rust-alloc.

The host implementation uses an AtomicBool spinlock for private metadata access.
It can wait indefinitely if a holder is delayed. Automatic selection changes
build configuration only; it does not satisfy the production no-wait/progress
requirement, establish a waiting-time bound, or qualify the host implementation.
Application callbacks cannot run inside the private protected operations.

Bare-metal builds retain explicit selection of one supplied SDK-owned image:

| Rust target | Required cfg value |
| --- | --- |
| thumbv7em-none-eabihf | cortex-m7-an500 |
| riscv32imac-unknown-none-elf | riscv32-sifive-e |

The cfg name is up_rust_native32_model. These profiles require the matching
SDK-owned startup, vectors/traps and link ownership checks. They do not provide
generic MCU, multicore or RTOS support. A bare-metal target with no selection,
an unknown selection, or a mismatched profile/target is rejected.

Existing explicit host selection remains compatible on Linux/macOS. An explicit
selection is validated as supplied; an invalid selection is never silently
replaced. Unsupported operating systems are still rejected. The automatic cfg
is emitted only for this crate; consumers should not branch on this internal
setting. Platform selection is independent of custom allocator selection.
