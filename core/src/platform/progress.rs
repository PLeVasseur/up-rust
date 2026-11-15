//! Private platform-progress boundary for metadata records.
//!
//! The state machine remains in `transport::metadata`; this module names the
//! primitive that supplies its exclusion interval. The first candidate keeps
//! the existing SDK-owned primitives unchanged: hosts still use the historical
//! spin gate, while Cortex-M and RV32 images still use their interrupt
//! exclusion. A later reviewed backend may replace only the host implementation
//! after capability and progress evidence exists.

use super::exclusion::{Exclusion, Guard};

/// The guarantee supplied by the selected primitive. This is deliberately
/// descriptive rather than a public API: callers must not infer host progress
/// from the presence of this seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProgressGuarantee {
    /// The current desktop behavior may wait for another caller indefinitely.
    #[cfg(up_rust_native32_model = "host")]
    HostSpinGate,
    /// The selected SDK-owned image masks configurable interrupts for a bounded
    /// metadata interval. Target WCET qualification remains separate.
    #[cfg(any(
        up_rust_native32_model = "cortex-m7-an500",
        up_rust_native32_model = "riscv32-sifive-e"
    ))]
    InterruptExclusion,
}

/// Internal platform contract used by the shared metadata state machine.
///
/// The guard type is crate-private and cannot escape the metadata operation.
/// Implementations must not call application code, allocate, free, panic or
/// re-enter a `Leaf` while the guard is live. These are source-level obligations
/// for each backend; this first seam does not weaken or close them.
pub(crate) trait ProgressBackend {
    /// Enters the platform's metadata exclusion interval.
    fn enter(&self) -> Guard<'_>;

    /// Reports the selected primitive for diagnostics and source controls.
    fn guarantee() -> ProgressGuarantee;
}

impl ProgressBackend for Exclusion {
    fn enter(&self) -> Guard<'_> {
        // Keep the selected profile explicit at the one boundary every leaf
        // uses. This is diagnostic metadata only; it does not upgrade the
        // host spin gate into a bounded-progress primitive.
        let _ = Self::guarantee();
        self.enter()
    }

    #[cfg(up_rust_native32_model = "host")]
    fn guarantee() -> ProgressGuarantee {
        ProgressGuarantee::HostSpinGate
    }

    #[cfg(any(
        up_rust_native32_model = "cortex-m7-an500",
        up_rust_native32_model = "riscv32-sifive-e"
    ))]
    fn guarantee() -> ProgressGuarantee {
        ProgressGuarantee::InterruptExclusion
    }
}

#[cfg(test)]
mod tests {
    use super::{Exclusion, ProgressBackend, ProgressGuarantee};

    #[test]
    fn selected_backend_is_explicit() {
        let _ = Exclusion::new();
        #[cfg(up_rust_native32_model = "host")]
        assert_eq!(
            <Exclusion as ProgressBackend>::guarantee(),
            ProgressGuarantee::HostSpinGate
        );
        #[cfg(any(
            up_rust_native32_model = "cortex-m7-an500",
            up_rust_native32_model = "riscv32-sifive-e"
        ))]
        assert_eq!(
            <Exclusion as ProgressBackend>::guarantee(),
            ProgressGuarantee::InterruptExclusion
        );
    }
}
