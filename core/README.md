# Experimental core platform selection

On this experimental branch, Linux, macOS and Windows x86-64 MSVC Cargo builds
select the host execution model automatically. The x86-64 deadline backends require the explicit compiler target profile below. This remains a no_std core:
selecting host does not add a dependency
on std, alloc, up-rust-std or up-rust-alloc.

The host deadline record uses a lock-free wide-atomic protocol on the profiles
below; Windows remains a candidate pending review and execution. Reports may retry under interference; lock-free does not
promise a bounded completion time. Host peer/RPC metadata still uses a spin gate
and can wait indefinitely if a holder is delayed. Application callbacks cannot
run inside the private protected operations. Neither automatic model selection
nor the selected tests establishes a complete platform or timing qualification.

| Deadline target | Compiler and capability boundary |
| --- | --- |
| `aarch64-apple-darwin` | Native Apple Clang build; Apple ARM64 baseline |
| `aarch64-unknown-linux-gnu` | Native Clang build with LSE shim; setup checks Linux `HWCAP_ATOMICS` |
| `x86_64-unknown-linux-gnu` | Native Clang build; Rust target must include `cmpxchg16b` |
| `x86_64-pc-windows-msvc` | Clang with an explicit MSVC target and static unwind tables; `llvm-lib`; Rust target must include `cmpxchg16b` |

For either x86-64 target, select a CX16 minimum-ISA binary through normal Cargo target
configuration, for example:

```toml
# .cargo/config.toml in the application workspace
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "target-feature=+cmpxchg16b"]

[target.x86_64-pc-windows-msvc]
rustflags = ["-C", "target-feature=+cmpxchg16b"]
```

Every CPU eligible to run that binary must support CX16 for its lifetime,
including migration and hotplug. The local CPUID check is a sanity check; it
does not certify the other CPUs. A generic x86-64 build without the selected
feature is refused. There is no hidden lock fallback. The checked compiler
profiles are Apple Clang 17, Linux x86-64 Clang 19 and Linux ARM64 Clang 18.
Mac/Linux builds also require `ar` and compile the C shim natively. The Windows
MSVC shim supports cross-compilation with Clang and `llvm-lib`; `CC` and `AR`
may select their executable paths. The Windows GNU target is not supported.
See [the source proof](docs/pal-deadline-safety.md) for exact ABI and progress
scope. Other target combinations have no qualification from these results.

Windows support here is limited to the private deadline primitive. The SDK does
not export a Windows `Native` platform or provide a Windows service worker in this
candidate. Windows wait safety, final linking/execution, runtime resources and
finite deadline timing remain separate open obligations.

Bare-metal builds retain explicit selection of one supplied SDK-owned image:

| Rust target | Required cfg value |
| --- | --- |
| thumbv7em-none-eabihf | cortex-m7-an500 |
| riscv32imac-unknown-none-elf | riscv32-sifive-e |

The cfg name is up_rust_native32_model. These profiles require the matching
SDK-owned startup, vectors/traps and link ownership checks. They do not provide
generic MCU, multicore or RTOS support. A bare-metal target with no selection,
an unknown selection, or a mismatched profile/target is rejected.

Existing explicit host selection remains compatible on Linux/macOS/Windows MSVC. An explicit
selection is validated as supplied; an invalid selection is never silently
replaced. Unsupported operating systems are still rejected. The automatic cfg
is emitted only for this crate; consumers should not branch on this internal
setting. Platform selection is independent of custom allocator selection.
