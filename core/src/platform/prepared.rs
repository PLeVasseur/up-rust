//! Permanent SDK worker storage; creation and join are initialization operations.
//! No worker-owned borrow is shorter than `'static`. Forgetting a preparation
//! leaks a parked thread and its resources; it cannot invalidate its arguments.

use crate::platform::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicU32, Ordering};
use crate::transport::{DeadlineServiceEntry, TimingGap};
use crate::transport::mode::RuntimeCore;
use super::Platform;

// These are SDK protocol tags, not operating-system ABI constants.
const PARKED: u32 = 0;
const ACTIVE: u32 = 1;
const CANCEL_INITIALIZATION: u32 = 2;
const RETAIN_FOREVER: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkerImplementation { PthreadV1 }

// The catalog vocabulary includes other targets so a cross-target record can
// be refused; each concrete build constructs only its own clock variant.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClockImplementation { DarwinUptimeRaw, LinuxMonotonic }

/// Actual preparation observations, not a certificate for artifacts or timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkerObservation {
    pub(crate) implementation: WorkerImplementation,
    pub(crate) clock: ClockImplementation,
    pub(crate) stack_bytes: usize,
    pub(crate) scheduler_policy: i32,
    pub(crate) scheduler_priority: i32,
}

struct Control {
    // Safety invariant: published pointers derive from initialized static shared
    // RuntimeCore references; they are never replaced or reclaimed.
    core: AtomicPtr<RuntimeCore>,
    command: AtomicU32,
    ready: AtomicBool,
    init_error: AtomicI32,
    cleanup_error: AtomicI32,
    policy: AtomicI32,
    priority: AtomicI32,
    // Native constructor supplies the concrete SDK platform, never an open
    // callback supplied by an application. No lifetime is extended here.
    platform: &'static dyn Platform,
    // Test-only observation of the real worker, never a runtime admission token.
    #[cfg(test)]
    heap: WorkerHeapObservation,
}

#[cfg(test)]
struct WorkerHeapObservation {
    calibrated: AtomicBool,
    finished: AtomicU32,
    valid: AtomicBool,
    allocs: crate::platform::atomic::AtomicUsize,
    frees: crate::platform::atomic::AtomicUsize,
}

#[cfg(test)]
impl WorkerHeapObservation {
    fn new() -> Self {
        Self { calibrated: AtomicBool::new(false), finished: AtomicU32::new(0),
            valid: AtomicBool::new(false), allocs: crate::platform::atomic::AtomicUsize::new(0),
            frees: crate::platform::atomic::AtomicUsize::new(0) }
    }
}

impl Control {
    fn new(platform: &'static dyn Platform) -> Self {
        Self { core: AtomicPtr::new(core::ptr::null_mut()), command: AtomicU32::new(PARKED),
            ready: AtomicBool::new(false), init_error: AtomicI32::new(0), cleanup_error: AtomicI32::new(0),
            policy: AtomicI32::new(0), priority: AtomicI32::new(0), platform,
            #[cfg(test)] heap: WorkerHeapObservation::new() }
    }

    /// # Safety-usable invariant
    /// Some is an initialized, permanently shared RuntimeCore. No reference is
    /// produced for a null/unpublished cell; no reset or mutable alias exists.
    fn core(&self) -> Option<&'static RuntimeCore> {
        let ptr = self.core.load(Ordering::Acquire);
        if ptr.is_null() { return None; }
        // SAFETY: Reference validity/aliasing and AtomicPtr Acquire require a
        // live aligned initialized pointee and shared access for the returned
        // lifetime. The field invariant and activate's static-reference input
        // supply these. Acquire observes its Release publication; no writer can
        // replace it or deallocate it and no callback intervenes. There is no
        // pointer arithmetic, metadata conversion or shorter borrow to extend.
        Some(unsafe { &*ptr })
    }

    fn command(&self, command: u32) {
        self.command.store(command, Ordering::Release);
        self.platform.wake_all(&self.command);
    }
}

/// Sole initialization owner of a pthread handle. No public constructor, Clone,
/// handle accessor, detach, cancellation syscall, or runtime destructor exists.
/// The resources referenced by the worker remain permanent even without Drop.
pub(crate) struct Worker {
    control: &'static Control,
    // Safety invariant: Some is the sole join capability from a successful
    // explicit-JOINABLE pthread_create. No detach/cancel/handle export or other
    // join capability exists; before activation that thread runs only SDK code.
    join: Option<Join>,
    observation: WorkerObservation,
    activated: bool,
    committed: bool,
}

/// # Invariants
/// Every value is the unique unjoined handle of one successfully created,
/// explicitly joinable pthread. No native handle is exported to safe callers;
/// it is consumed at most once, before its worker can execute an entrypoint.
enum Join {
    #[cfg(all(target_os = "macos", target_pointer_width = "64"))]
    Darwin(*mut core::ffi::c_void),
    #[cfg(all(target_os = "linux", target_env = "gnu", target_pointer_width = "64",
        any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64")))]
    Glibc(core::ffi::c_ulong),
}

impl Worker {
    pub(crate) fn observation(&self) -> WorkerObservation { self.observation }

    pub(crate) fn platform(&self) -> &'static dyn Platform { self.control.platform }

    /// Publishes only an initialized permanent entry. No OS thread is created.
    pub(crate) fn activate(&mut self, entry: DeadlineServiceEntry) -> Result<(), TimingGap> {
        if self.activated || self.control.command.load(Ordering::Acquire) != PARKED {
            return Err(TimingGap::WorkerActivationFailed);
        }
        let ptr = core::ptr::from_ref(entry.core()).cast_mut();
        self.control.core.compare_exchange(core::ptr::null_mut(), ptr,
            Ordering::Release, Ordering::Relaxed).map_err(|_| TimingGap::WorkerActivationFailed)?;
        // Set the irreversible phase before wake. Unwind/reentry cannot select
        // initialization join after a worker may have begun accessing the core.
        self.activated = true;
        self.control.command(ACTIVE);
        Ok(())
    }

    /// Called only after binding.start succeeds. The running thread, libc TCB,
    /// caller-owned stack and static control remain reserved until process exit.
    pub(crate) fn commit(&mut self) { self.committed = true; }

    /// Explicit initialization rollback. After activation, park permanently:
    /// returning from a pthread would run TLS teardown and potentially free.
    /// No memory lifetime or Rust soundness depends on successful cancellation.
    pub(crate) fn cancel(&mut self) -> Result<(), TimingGap> {
        if self.committed { return Ok(()); }
        if self.activated {
            if let Some(core) = self.control.core() { core.shared.cancel_deadline_service(); }
            self.control.command(RETAIN_FOREVER);
            // Retain the joinable OS resource. No pthread_join/detach/exit/free.
            self.join = None;
            return Ok(());
        }
        if let Some(join) = self.join.take() {
            self.control.command(CANCEL_INITIALIZATION);
            let code = join.finish();
            self.control.cleanup_error.store(code, Ordering::Release);
            if code != 0 { return Err(TimingGap::WorkerCleanupFailed(code)); }
        }
        Ok(())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // An open PlatformInit can retain or drop an opaque preparation at any
        // time. Drop therefore NEVER joins/exits/frees, even before activation.
        // The SDK's private StartupWorker guard performs explicit initialization
        // rollback. An externally dropped/forgotten preparation retains resources.
        if !self.committed && (self.join.is_some() || self.activated) {
            if let Some(core) = self.control.core() { core.shared.cancel_deadline_service(); }
            self.control.command(RETAIN_FOREVER);
        }
    }
}

impl Join {
    fn finish(self) -> i32 {
        match self {
            #[cfg(all(target_os = "macos", target_pointer_width = "64"))]
            Self::Darwin(thread) => {
                // SAFETY: the private, consumed Join is the only join capability
                // for a successful pthread_create with explicit JOINABLE attrs.
                // Worker calls this only before activation; the thread received
                // only Control and cannot acquire this handle or join itself.
                // No detach, other join or pthread_cancel is exposed. Null output
                // is permitted by POSIX pthread_join. Success ends OS ownership;
                // caller-owned static stack/control are deliberately not reclaimed.
                unsafe { native::pthread_join(thread, core::ptr::null_mut()) }
            }
            #[cfg(all(target_os = "linux", target_env = "gnu", target_pointer_width = "64",
                any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64")))]
            Self::Glibc(thread) => {
                // SAFETY: identical unique, joinable, non-self handle invariant
                // as Darwin above; glibc's pthread_t is c_ulong, not a pointer.
                unsafe { native::pthread_join(thread, core::ptr::null_mut()) }
            }
        }
    }
}

#[cfg(any(all(target_os = "macos", target_pointer_width = "64"),
    all(target_os = "linux", target_env = "gnu", target_pointer_width = "64",
        any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64"))))]
#[path = "pthread.rs"]
mod native;

#[cfg(any(all(target_os = "macos", target_pointer_width = "64"),
    all(target_os = "linux", target_env = "gnu", target_pointer_width = "64",
        any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64"))))]
pub(crate) use native::prepare;

#[cfg(test)]
#[path = "prepared_tests.rs"]
mod tests;
