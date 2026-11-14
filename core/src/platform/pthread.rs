//! POSIX worker for the named Darwin LP64 and glibc LP64 ABIs only.
//! ABI declarations: Apple SDK pthread.h/_pthread_types.h/sched.h and glibc
//! 2.40 pthread.h/bits/pthreadtypes*.h. FFI-INPUT-PINS records exact sources.
//! No numeric scheduler policy, stack minimum, page size or J is invented here.

use core::ffi::{c_int, c_long, c_void};
use core::mem::MaybeUninit;
use crate::memory::StartupMemory;
use crate::platform::{Native, Platform, PlatformInit};
use crate::transport::error::StartupError;
use crate::transport::{DeadlineServiceEntry, TimingGap};
use super::{ClockImplementation, Control, Join, Worker, WorkerImplementation, WorkerObservation,
    ACTIVE, CANCEL_INITIALIZATION, PARKED, RETAIN_FOREVER};
use crate::platform::atomic::Ordering;

#[cfg(target_os = "macos")]
type Thread = *mut c_void; // __darwin_pthread_t: pointer to opaque pthread object.
#[cfg(target_os = "linux")]
type Thread = core::ffi::c_ulong; // glibc public pthread_t, not an address cast.

#[cfg(target_os = "macos")]
#[repr(C)]
struct Attr { signature: c_long, opaque: [u8; 56] }
#[cfg(target_os = "linux")]
#[repr(C)]
union Attr {
    #[cfg(target_arch = "aarch64")]
    opaque: [u8; 64],
    #[cfg(not(target_arch = "aarch64"))]
    opaque: [u8; 56],
    alignment: c_long,
}

#[repr(C)]
struct SchedParam {
    priority: c_int,
    #[cfg(target_os = "macos")]
    opaque: [u8; 4], // public sched.h's __SCHED_PARAM_SIZE__, not a guessed ABI.
}

#[cfg(target_os = "macos")]
const JOINABLE: c_int = 1; // Apple pthread.h PTHREAD_CREATE_JOINABLE.
#[cfg(target_os = "linux")]
const JOINABLE: c_int = 0; // glibc pthread.h first enum member.

#[cfg_attr(target_os = "linux", link(name = "pthread"))]
extern "C" {
    fn pthread_attr_init(attr: *mut Attr) -> c_int;
    fn pthread_attr_destroy(attr: *mut Attr) -> c_int;
    fn pthread_attr_setdetachstate(attr: *mut Attr, state: c_int) -> c_int;
    fn pthread_attr_getstacksize(attr: *const Attr, size: *mut usize) -> c_int;
    fn pthread_attr_setstack(attr: *mut Attr, address: *mut c_void, size: usize) -> c_int;
    fn pthread_create(thread: *mut Thread, attr: *const Attr,
        entry: unsafe extern "C" fn(*mut c_void) -> *mut c_void, arg: *mut c_void) -> c_int;
    pub(super) fn pthread_join(thread: Thread, value: *mut *mut c_void) -> c_int;
    fn pthread_self() -> Thread;
    fn pthread_getschedparam(thread: Thread, policy: *mut c_int, parameter: *mut SchedParam) -> c_int;
    fn getpagesize() -> c_int;
}

struct Attributes {
    // Safety invariant: exclusive, permanent, aligned storage for one C Attr;
    // when initialized=true it holds one live pthread_attr_init object, with no
    // copied object or concurrent accessor. No native call retains its address.
    storage: &'static mut MaybeUninit<Attr>,
    initialized: bool,
}

impl Attributes {
    /// # Safety-usable invariant
    /// Success owns one initialized C attribute object at a stable address.
    /// Failure exposes no object eligible for native destruction.
    fn new(memory: &StartupMemory) -> Result<Self, StartupError> {
        let storage = memory.place(MaybeUninit::<Attr>::uninit())?;
        // SAFETY: pthread_attr_init accepts writable uninitialized Attr storage;
        // place's postcondition supplies exclusive alignment/extent/liveness.
        // The public target ABI above fixes size/alignment. This object has not
        // previously been initialized. On success only C interprets its bytes.
        let rc = unsafe { pthread_attr_init(storage.as_mut_ptr()) };
        if rc != 0 { return Err(failure("pthread_attr_init", rc)); }
        Ok(Self { storage, initialized: true })
    }

    fn close(&mut self) -> i32 {
        if !self.initialized { return 0; }
        self.initialized = false;
        // SAFETY: the field invariant supplies a live initialized Attr, no other
        // caller accesses it, and all pthread_create calls using it returned.
        // Destroy occurs exactly once. Its bytes are never read as a Rust Attr.
        unsafe { pthread_attr_destroy(self.storage.as_mut_ptr()) }
    }
}
impl Drop for Attributes { fn drop(&mut self) { let _ = self.close(); } }

fn failure(operation: &'static str, code: i32) -> StartupError {
    StartupError::DeadlineWorker { operation, code, attribute_cleanup: None, join_cleanup: None, memory: None }
}

fn with_attribute_cleanup(error: StartupError, code: i32) -> StartupError {
    if code == 0 { return error; }
    if let StartupError::DeadlineWorker { operation, code: primary, memory, join_cleanup, .. } = error {
        StartupError::DeadlineWorker { operation, code: primary, attribute_cleanup: Some(code), join_cleanup, memory }
    } else if let StartupError::Memory(memory) = error {
        StartupError::DeadlineWorker {
            operation: "startup memory", code: 0, attribute_cleanup: Some(code), join_cleanup: None, memory: Some(memory) }
    } else {
        error
    }
}

fn with_join_cleanup(error: StartupError, code: i32) -> StartupError {
    if code == 0 { return error; }
    if let StartupError::DeadlineWorker { operation, code: primary, memory, attribute_cleanup, .. } = error {
        StartupError::DeadlineWorker { operation, code: primary, attribute_cleanup, join_cleanup: Some(code), memory }
    } else {
        error
    }
}

/// # Safety-usable invariant
/// Success returns the unique join capability for an already-created, parked
/// thread. Its stack/control are permanent disjoint StartupMemory reservations;
/// only this SDK's Native platform and worker_entry execute before activation.
/// Failure never exposes an uninitialized handle or a shorter-lived pointer.
pub(crate) fn prepare(memory: &StartupMemory, native: &'static Native) -> Result<Worker, StartupError> {
    let mut attributes = Attributes::new(memory)?;
    let result = prepare_with_attributes(memory, native, &mut attributes);
    let cleanup = attributes.close();
    match result {
        Ok(mut worker) if cleanup != 0 => {
            let join = worker.cancel().err();
            let mut error = failure("pthread_attr_destroy", cleanup);
            if let Some(TimingGap::WorkerCleanupFailed(code)) = join { error = with_join_cleanup(error, code); }
            Err(error)
        }
        Ok(worker) => Ok(worker),
        Err(error) => Err(with_attribute_cleanup(error, cleanup)),
    }
}

fn prepare_with_attributes(memory: &StartupMemory, native: &'static Native, attributes: &mut Attributes)
    -> Result<Worker, StartupError>
{
    if !attributes.initialized { return Err(failure("closed pthread attributes", 0)); }
    let attr = attributes.storage.as_mut_ptr();
    // SAFETY: Attributes' invariant and exclusive borrow give one live Attr;
    // JOINABLE is the exact public ABI constant. No thread has consumed it yet.
    let rc = unsafe { pthread_attr_setdetachstate(attr, JOINABLE) };
    if rc != 0 { return Err(failure("pthread_attr_setdetachstate", rc)); }
    let mut stack_bytes = 0_usize;
    // SAFETY: initialized attr remains exclusively owned; size is a writable,
    // aligned local size_t. C writes the size synchronously and retains nothing.
    let rc = unsafe { pthread_attr_getstacksize(attr, &mut stack_bytes) };
    if rc != 0 { return Err(failure("pthread_attr_getstacksize", rc)); }
    // SAFETY: getpagesize has no pointer preconditions and returns c_int.
    let page = usize::try_from(unsafe { getpagesize() }).unwrap_or(0);
    if !page.is_power_of_two() || stack_bytes == 0 {
        return Err(failure("pthread stack/page geometry", 0));
    }
    let stack = memory.aligned_bytes(stack_bytes, page)?;
    let stack_ptr = stack.as_mut_ptr().cast::<u8>();
    // The mutable slice is not used after this point: pthread owns all access
    // once created. No Rust reference aliases any worker stack frame.
    // SAFETY: aligned_bytes supplies unique, live, writable permanent bytes for
    // stack_bytes, with no objects/references yet placed there. The last use of
    // stack precedes prefault. Native prefault overwrites/locks this reservation;
    // it is initialization-only and cannot shorten its lifetime on failure.
    if let Err(error) = unsafe { native.prefault(stack_ptr, stack_bytes) } {
        return Err(failure("worker stack prefault", error.code));
    }
    // SAFETY: POSIX attr_setstack requires a writable stack allocation with the
    // platform's alignment/size. Page alignment is checked and the size is the
    // platform's own attr default. The OS validates additional minimum/granule
    // requirements and returns an error. Permanent unique backing outlives both
    // successful creation and join; this code never accesses it after creation.
    let rc = unsafe { pthread_attr_setstack(attr, stack_ptr.cast(), stack_bytes) };
    if rc != 0 { return Err(failure("pthread_attr_setstack", rc)); }
    let control: &'static Control = memory.place(Control::new(native.runtime()))?;
    let arg = core::ptr::from_ref(control).cast_mut().cast::<c_void>();
    let mut handle = MaybeUninit::<Thread>::uninit();
    // SAFETY: POSIX pthread_create receives a live initialized Attr, an aligned
    // Thread out-slot and worker_entry with its exact C ABI. arg is derived from
    // a fully initialized static Sync Control. worker_entry's contract permits
    // that shared access and returns null only for initialization cancellation;
    // the unique static stack is untouched by Rust thereafter. No foreign
    // exception/async cancellation is allowed by the opted-in platform contract.
    let rc = unsafe { pthread_create(handle.as_mut_ptr(), attr, worker_entry, arg) };
    if rc != 0 { return Err(failure("pthread_create", rc)); }
    // SAFETY: success (rc=0) is POSIX's postcondition that handle contains the
    // initialized thread id. It is read once, then owned only by this Worker.
    let handle = unsafe { handle.assume_init() };
    #[cfg(target_os = "macos")]
    let join = Join::Darwin(handle);
    #[cfg(target_os = "linux")]
    let join = Join::Glibc(handle);
    #[cfg(target_os = "macos")]
    let clock = ClockImplementation::DarwinUptimeRaw;
    #[cfg(target_os = "linux")]
    let clock = ClockImplementation::LinuxMonotonic;
    let mut worker = Worker { control, join: Some(join), activated: false, committed: false,
        observation: WorkerObservation { implementation: WorkerImplementation::PthreadV1,
            clock, stack_bytes, scheduler_policy: 0, scheduler_priority: 0 } };
    // Native-only callbacks; a token exists before any wait or subsequent work.
    // The command word changes to ACTIVE only after this initializer returns.
    while !control.ready.load(Ordering::Acquire) {
        native.wait(&control.command, PARKED, Some(native.now().saturating_add(1_000_000)));
    }
    let code = control.init_error.load(Ordering::Relaxed);
    if code != 0 {
        let cleanup = match worker.cancel() {
            Err(TimingGap::WorkerCleanupFailed(code)) => code,
            _ => 0,
        };
        return Err(with_join_cleanup(failure("pthread_getschedparam", code), cleanup));
    }
    worker.observation.scheduler_policy = control.policy.load(Ordering::Relaxed);
    worker.observation.scheduler_priority = control.priority.load(Ordering::Relaxed);
    Ok(worker)
}

/// # Safety
/// pthread_create must call this once with arg pointing to a fully initialized,
/// aligned, shared static Control, on the separately reserved worker stack.
/// The platform must not inject foreign exceptions or asynchronous cancellation.
/// Control must remain live even when initialization is forgotten or unwinds.
/// Returns null only before runtime activation; once activated it never returns,
/// so pthread/TLS teardown is not a runtime operation. The service receives only
/// permanently shared RuntimeCore storage; no borrowed lifetime is extended.
unsafe extern "C" fn worker_entry(arg: *mut c_void) -> *mut c_void {
    // SAFETY: the entry precondition establishes exact Control extent, alignment,
    // initialization, static liveness and shared access; all mutable fields are
    // atomic. arg preserves the reference-derived provenance from prepare. No
    // reference to stack frames or mutable Control is created here.
    let control = unsafe { &*arg.cast::<Control>() };
    let mut policy = 0;
    let mut parameter = SchedParam { priority: 0, #[cfg(target_os = "macos")] opaque: [0; 4] };
    // SAFETY: pthread_self has no preconditions and names this live worker.
    let thread = unsafe { pthread_self() };
    // SAFETY: thread is this live pthread; policy and parameter are separate,
    // aligned writable out-values with the pinned public ABI layout. POSIX
    // writes synchronously and does not retain either stack pointer.
    let code = unsafe { pthread_getschedparam(thread, &mut policy, &mut parameter) };
    control.policy.store(policy, Ordering::Relaxed);
    control.priority.store(parameter.priority, Ordering::Relaxed);
    control.init_error.store(code, Ordering::Relaxed);
    // Prime the concrete clock, wait, wake-one, wake-all and error/TLS paths
    // before publishing readiness. ACTIVE cannot be issued until prepare returns;
    // waiting for ACTIVE while the actual word is PARKED must return immediately
    // under the existing Native futex/address-wait contract. No application hook
    // or blanket timeout is used. Qualification still audits loader/OS internals.
    let _ = control.platform.now();
    let _ = control.platform.wait(&control.command, ACTIVE, None);
    control.platform.wake_one(&control.command);
    control.platform.wake_all(&control.command);
    control.ready.store(true, Ordering::Release);
    control.platform.wake_all(&control.command);
    loop {
        let command = control.command.load(Ordering::Acquire);
        if command == CANCEL_INITIALIZATION { return core::ptr::null_mut(); }
        if command == ACTIVE {
            if let Some(core) = control.core() { DeadlineServiceEntry::new(core).run(); }
            // Entry cancellation after activation must NOT return from pthread.
            control.command(RETAIN_FOREVER);
        }
        let command = control.command.load(Ordering::Acquire);
        if command == ACTIVE || command == CANCEL_INITIALIZATION { continue; }
        control.platform.wait(&control.command, command, None);
    }
}
