//! New source-only fixtures. UNCOMPILED / UNRUN; no finite J or native credit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::memory::StartupRegion;
use crate::transport::mode::{RuntimeCore, TransportSetup};
use crate::transport::tests::{CLOCK, NullHandoff};

fn core(memory: &crate::memory::StartupMemory) -> &'static RuntimeCore {
    let setup = TransportSetup::handoff_on(memory, &CLOCK, NullHandoff).unwrap();
    memory.place(RuntimeCore { activation_sample_ns: 0, id: 1, binding: "fixture",
        shared: setup.shared(), tx: &[], rx: &[], waivers: &[], reports: &[] }).unwrap()
}

fn worker(memory: &crate::memory::StartupMemory) -> Worker {
    Worker { control: memory.place(Control::new(&CLOCK)).unwrap(), join: None,
        observation: WorkerObservation { implementation: WorkerImplementation::PthreadV1,
            clock: ClockImplementation::LinuxMonotonic, stack_bytes: 0,
            scheduler_policy: 0, scheduler_priority: 0 }, activated: false, committed: false }
}

// Tests irreversible entry ownership and cancellation, without creating a native
// thread or admitting any timing record. No mock execution proves physical J.
#[test]
fn second_activation_cannot_replace_published_core() {
    static REGION: StartupRegion<65536> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let first = core(&memory);
    let other = core(&memory);
    let mut worker = worker(&memory);
    worker.activate(DeadlineServiceEntry::new(first)).unwrap();
    assert_eq!(worker.activate(DeadlineServiceEntry::new(other)), Err(TimingGap::WorkerActivationFailed));
    assert!(core::ptr::eq(worker.control.core().unwrap(), first));
    worker.cancel().unwrap();
    assert_eq!(worker.control.command.load(Ordering::Acquire), RETAIN_FOREVER);
    assert!(first.shared.deadline_cancelled.load(Ordering::Acquire));
    assert!(!other.shared.deadline_cancelled.load(Ordering::Acquire));
}

// The storage survives forgotten ownership and the allocating StartupMemory
// handle's scope. This is a targeted lifetime fixture, not a native timing test.
#[test]
fn forgotten_worker_never_revokes_published_storage() {
    static REGION: StartupRegion<65536> = StartupRegion::new();
    let (control, expected) = {
        let memory = REGION.claim().unwrap();
        let expected = core(&memory);
        let mut worker = worker(&memory);
        worker.activate(DeadlineServiceEntry::new(expected)).unwrap();
        let control = worker.control;
        core::mem::forget(worker);
        (control, expected)
    };
    assert!(core::ptr::eq(control.core().unwrap(), expected));
    assert!(!expected.shared.deadline_cancelled.load(Ordering::Acquire));
}

#[test]
fn committed_initialization_owner_drops_without_stopping_service() {
    static REGION: StartupRegion<65536> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let core = core(&memory);
    let mut worker = worker(&memory);
    worker.activate(DeadlineServiceEntry::new(core)).unwrap();
    let control = worker.control;
    worker.commit();
    drop(worker);
    assert_eq!(control.command.load(Ordering::Acquire), ACTIVE);
    assert!(!core.shared.deadline_cancelled.load(Ordering::Acquire));
}

#[cfg(any(all(target_os = "macos", target_pointer_width = "64"),
    all(target_os = "linux", target_env = "gnu", target_pointer_width = "64",
        any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64"))))]
#[test]
#[ignore = "requires a separate root grant for real pthread creation, stack locking and join"]
fn native_worker_prepares_before_activation_and_joins_on_initialization_cancel() {
    // Explicit fixture resource budget; not a production route, worker or stack
    // cap. Production stack size is queried from native attributes.
    static REGION: StartupRegion<{ 16 * 1024 * 1024 }> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let mut worker = prepare(&memory, &crate::platform::NATIVE).unwrap();
    assert!(worker.control.ready.load(Ordering::Acquire));
    assert_eq!(worker.control.command.load(Ordering::Acquire), PARKED);
    assert!(worker.control.core().is_none());
    assert!(worker.observation.stack_bytes > 0);
    assert!(worker.join.is_some());
    worker.cancel().unwrap();
    assert!(worker.join.is_none());
    assert_eq!(worker.control.command.load(Ordering::Acquire), CANCEL_INITIALIZATION);
}

// Separate non-inlined access sites exercise the canonical copy under the
// selected multiple-codegen-unit build. This cannot prove vtable uniqueness;
// the source contract avoids requiring it. No native worker is prepared.
#[inline(never)]
fn canonical_runtime_first() -> &'static dyn Platform {
    crate::platform::PlatformInit::runtime(&crate::platform::NATIVE)
}
mod canonical_runtime_other_unit {
    #[inline(never)]
    pub(super) fn get() -> &'static dyn crate::platform::Platform {
        crate::platform::PlatformInit::runtime(&crate::platform::NATIVE)
    }
}
#[test]
fn canonical_native_runtime_reference_is_copied_into_control() {
    let first = canonical_runtime_first();
    let other = canonical_runtime_other_unit::get();
    assert!(core::ptr::eq(first, other));
    let control = Control::new(first);
    assert!(core::ptr::eq(control.platform, other));
    assert!(control.core().is_none());
    assert_eq!(control.command.load(Ordering::Acquire), PARKED);
}
