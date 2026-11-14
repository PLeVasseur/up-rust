//! Private exclusion for the full ordinary metadata records.
//!
//! The two target models are complete single-core SDK-owned executable images,
//! not generic Cortex-M/RISC-V or RTOS support. build.rs checks the exact triple;
//! the final link must contain the supplied strong ownership anchor and vector /
//! trap script. The image source owns reset, privilege, exceptional entry and
//! trap dispatch. Unsupported interrupt/vector/context installation requires an
//! unsafe operation and violates that SDK platform's whole-image contract.
//!
//! No target gate/CAS/retry exists. Masking prevents a configurable interrupt
//! from introducing a second metadata borrower; private leaves do not call
//! application code. Nonmaskable/fault handlers terminate the image without
//! entering metadata. A whole-core failure is not successful completion.
//!
//! This proves an exclusion mechanism within that image. It does not prove a
//! WCET, autonomous service supply, arbitrary application interrupt-latency bound,
//! real-device fault recovery, production ASIL support or generic RTOS admission.
//! The host model intentionally uses a gate and has no bounded-wait claim.

use super::atomic::{compiler_fence, fence, Ordering};

#[cfg(up_rust_native32_model = "host")]
use super::atomic::AtomicBool;

#[cfg(not(any(
    up_rust_native32_model = "host",
    up_rust_native32_model = "cortex-m7-an500",
    up_rust_native32_model = "riscv32-sifive-e"
)))]
compile_error!("native32 experiment needs a supplied SDK image or explicit host behavior model");

#[cfg(any(
    up_rust_native32_model = "cortex-m7-an500",
    up_rust_native32_model = "riscv32-sifive-e"
))]
unsafe extern "C" {
    /// # Safety
    /// The defining SDK image must be the reviewed single-core model selected
    /// by this build. Its strong reset/vector/trap symbols initialize execution
    /// and keep metadata callers privileged on the sole participating core;
    /// nonmaskable/fault handlers never enter metadata, and configurable
    /// handlers cannot invoke metadata until reset has initialized ordinary
    /// Rust storage. No external definition may impersonate this symbol.
    /// The function itself is an empty, returning, callback-free link anchor.
    fn __up_sdk_native32_owned_image_v1();
}

/// One private metadata access domain. Target storage is protected by the
/// SDK image's global interrupt exclusion; host test domains use separate gates.
pub(crate) struct Exclusion {
    #[cfg(up_rust_native32_model = "host")]
    held: AtomicBool,
}

impl Exclusion {
    pub(crate) const fn new() -> Self {
        Self {
            #[cfg(up_rust_native32_model = "host")]
            held: AtomicBool::new(false),
        }
    }

    /// # Safety-usable invariant
    /// On the reviewed SDK-owned single-core image, blocks other admitted
    /// interrupt/task contexts from starting a metadata leaf until Guard drops.
    /// Does not protect against synchronous nested entry in the same context:
    /// private Leaf::apply's unsafe contract forbids that while its borrow lives.
    /// No protected reference is produced here. The guard cannot leave the crate
    /// or move to another context. Entry and exit order ordinary memory accesses
    /// across each exclusion interval; exit restores exactly the previous mask.
    /// Host behavior-model acquisition uses Acquire/Release and may wait forever.
    pub(crate) fn enter(&self) -> Guard<'_> {
        #[cfg(any(
            up_rust_native32_model = "cortex-m7-an500",
            up_rust_native32_model = "riscv32-sifive-e"
        ))]
        // SAFETY: the selected model's final executable uses the SDK-owned
        // reset/vector/trap implementation bound by the mandatory strong symbol.
        // Its linker script forbids competing vector/trap sections. No safe API
        // can install an alternative implementation of this unsafe ABI contract.
        // This call precedes the protected borrow and invokes no application code.
        unsafe {
            __up_sdk_native32_owned_image_v1()
        };

        #[cfg(up_rust_native32_model = "host")]
        while self
            .held
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }

        #[cfg(up_rust_native32_model = "cortex-m7-an500")]
        let saved = {
            let saved: u32;
            // SAFETY: the exact thumbv7em image executes on one privileged M7.
            // PRIMASK and CPSID are available; no pointer operand, stack memory
            // or flags are changed. No nomem/readonly option permits protected
            // accesses to move across the asm. Saved is initialized by MRS before
            // CPSID, and no reference to protected data exists at this point.
            unsafe {
                core::arch::asm!("mrs {prior}, PRIMASK", "cpsid i",
                    prior = out(reg) saved, options(nostack, preserves_flags));
            }
            saved
        };

        #[cfg(up_rust_native32_model = "riscv32-sifive-e")]
        let saved = {
            let saved: u32;
            // SAFETY: the exact sifive_e model is hart zero in M-mode; mstatus is
            // accessible. CSRRCI atomically returns the prior CSR while clearing
            // only MIE(bit3). No pointer or stack operand, and no nomem promise.
            // The instruction establishes masking before any protected access.
            unsafe {
                core::arch::asm!("csrrci {prior}, mstatus, 8",
                    prior = out(reg) saved, options(nostack));
            }
            saved & 8
        };

        compiler_fence(Ordering::SeqCst);
        fence(Ordering::SeqCst);
        Guard {
            #[cfg(up_rust_native32_model = "host")]
            domain: self,
            #[cfg(not(up_rust_native32_model = "host"))]
            _domain: self,
            _context: core::marker::PhantomData,
            #[cfg(not(up_rust_native32_model = "host"))]
            saved,
        }
    }
}

pub(crate) struct Guard<'a> {
    #[cfg(up_rust_native32_model = "host")]
    domain: &'a Exclusion,
    #[cfg(not(up_rust_native32_model = "host"))]
    _domain: &'a Exclusion,
    // Safety invariant: saved is the prior interrupt mask from this exact
    // lexical entry in the same execution context. No public guard escapes.
    #[cfg(not(up_rust_native32_model = "host"))]
    saved: u32,
    _context: core::marker::PhantomData<*mut ()>,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        compiler_fence(Ordering::SeqCst);
        fence(Ordering::SeqCst);

        #[cfg(up_rust_native32_model = "host")]
        self.domain.held.store(false, Ordering::Release);

        #[cfg(up_rust_native32_model = "cortex-m7-an500")]
        // SAFETY: Guard's invariant supplies the initialized prior mask from
        // this same context. No protected borrow remains when the private guard
        // drops. MSR restores that mask exactly; nested masking stays masked.
        // No stack/flag access or nomem claim is made.
        unsafe {
            core::arch::asm!("msr PRIMASK, {prior}", prior = in(reg) self.saved,
                options(nostack, preserves_flags));
        }

        #[cfg(up_rust_native32_model = "riscv32-sifive-e")]
        if self.saved != 0 {
            // SAFETY: the saved MIE was set on this context's entry. Leaf code
            // cannot change execution privilege or trap state. Restoring only
            // MIE avoids overwriting unrelated mstatus bits; the protected borrow
            // has ended and preceding writes are ordered before interrupts resume.
            unsafe { core::arch::asm!("csrsi mstatus, 8", options(nostack)) };
        }
    }
}

/// Test-only observation of both nested-mask and outer-mask restoration.
/// Does not create a metadata borrow or change the image's initial mask.
#[cfg(all(up_rust_test_hooks, not(up_rust_native32_model = "host")))]
pub(crate) fn nested_mask_control() -> bool {
    fn mask() -> u32 {
        let value: u32;
        #[cfg(up_rust_native32_model = "cortex-m7-an500")]
        // SAFETY: the selected SDK image is privileged on the exact M7 target.
        // MRS only reads PRIMASK into an initialized register; it cannot expose
        // a protected reference or change memory, flags, or interrupt state.
        unsafe {
            core::arch::asm!("mrs {value}, PRIMASK", value = out(reg) value,
                options(nostack, preserves_flags));
        }
        #[cfg(up_rust_native32_model = "riscv32-sifive-e")]
        // SAFETY: the selected SDK image runs in M-mode on hart zero. CSRRS
        // with zero input (CSRR alias) reads mstatus without modifying it. This
        // has no pointer/stack operands and initializes the output register.
        unsafe {
            core::arch::asm!("csrr {value}, mstatus", value = out(reg) value,
                options(nostack));
        }
        #[cfg(up_rust_native32_model = "cortex-m7-an500")]
        {
            value & 1
        }
        #[cfg(up_rust_native32_model = "riscv32-sifive-e")]
        {
            u32::from(value & 8 == 0)
        }
    }
    let domain = Exclusion::new();
    let prior = mask();
    let outer = domain.enter();
    let after_outer = mask();
    let inner = domain.enter();
    let after_inner = mask();
    drop(inner);
    let after_inner_drop = mask();
    drop(outer);
    after_outer == 1 && after_inner == 1 && after_inner_drop == 1 && mask() == prior
}
