// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Heap-use observation for test binaries.
//!
//! The rule this module verifies: in runtime mode, SDK and binding code neither
//! acquires nor releases heap memory, and anything lazy is forced during
//! initialization. Counting only allocations misses half of that rule, because
//! a free on a runtime path is heap use as well. So [`HeapGuard`] counts
//! allocations, deallocations and reallocations separately, and a window is
//! clean only when all three are zero.
//!
//! Install the guard as the global allocator of a test binary, then scope a
//! window around the code under test:
//!
//! ```
//! use std::alloc::System;
//! use up_rust::heap_guard::{assert_no_heap, HeapGuard, Scope};
//!
//! #[global_allocator]
//! static HEAP: HeapGuard = HeapGuard::new(System);
//!
//! fn main() {
//!     assert_no_heap("arithmetic", Scope::Enrolled, || 1 + 1);
//! }
//! ```
//!
//! A binding proves its runtime paths with the same few lines: wrap each
//! send, receive and refusal in [`assert_no_heap`].
//!
//! # What a window counts
//!
//! * [`Scope::Enrolled`] charges the opening thread and any thread that joins
//!   through [`WindowToken::enrol_this_thread`]. Such windows are independent of
//!   each other, so they work inside a libtest binary whose tests run concurrently.
//! * [`Scope::AllThreads`] charges every thread in the process. Only one can be
//!   open at a time, and anything else the process is doing is charged too, so it
//!   belongs in a harness-free binary or under `--test-threads=1`.
//!
//! A window covers heap operations causally after [`HeapWindow::open`] returned and
//! before [`HeapWindow::finish`] begins. An operation on another thread that races
//! the opening or the closing of the window may fall on either side, but closing
//! waits for every operation already charging the window, so a report never loses
//! one that the window saw. The first [`EVENT_LIMIT`](HeapReport::EVENT_LIMIT)
//! operations of a window are recorded individually, each with a backtrace unless
//! the window was opened with [`HeapWindow::open_counts_only`], so a failing window
//! says which of the three happened and where. Allocations made by that capture
//! are the guard's own and are never charged.
//!
//! # Limits
//!
//! The guard sees Rust allocation through `GlobalAlloc` only. C and C++ allocation
//! in middleware, and `std`'s direct use of `System`, are outside it. The `c_level`
//! submodule joins a window with the C-level allocation observer for those, on Linux glibc and
//! on macOS, and requires the observer to be loaded. The guard
//! uses const-initialized thread-locals from the allocator. On targets without
//! native thread-locals, such as QNX, `thread_local!` allocates on first access, so
//! a thread must have called [`prime_this_thread`] before it allocates inside a
//! window, or the allocator hook would recurse. An unprimed cold thread is therefore
//! not measurable on those targets. Fresh threads on targets with native
//! thread-locals, such as Linux and macOS, need no priming.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    backtrace::{Backtrace, BacktraceStatus},
    cell::{Cell, UnsafeCell},
    fmt,
    future::Future,
    marker::PhantomData,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};

#[cfg(any(all(target_os = "linux", target_env = "gnu"), target_os = "macos"))]
pub mod c_level;

/// Windows that may be open at the same time in one process.
const WINDOW_SLOTS: usize = 32;
/// Heap operations a single window records individually.
const EVENT_SLOTS: usize = 8;

/// Set once any heap operation has reached the guard.
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// Open windows of either scope. Zero keeps the allocator hook to one relaxed load.
static OPEN_WINDOWS: AtomicUsize = AtomicUsize::new(0);
/// `slot + 1` of the open all-thread window, zero when there is none.
static ALL_THREADS_SLOT: AtomicUsize = AtomicUsize::new(0);
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
static NEXT_THREAD_TAG: AtomicU64 = AtomicU64::new(1);
static WINDOWS: [WindowSlot; WINDOW_SLOTS] = [const { WindowSlot::new() }; WINDOW_SLOTS];

thread_local! {
    /// Set while this thread captures diagnostics; such allocation is not charged.
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
    /// A small number identifying this thread in reports, assigned on first use.
    static THREAD_TAG: Cell<u64> = const { Cell::new(0) };
    /// `(slot + 1, generation)` of the window this thread is enrolled in, if any.
    static ENROLLED: Cell<(usize, u64)> = const { Cell::new((0, 0)) };
}

/// One heap operation, as seen by [`HeapGuard`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeapOp {
    /// An allocation through `alloc`.
    Allocate,
    /// An allocation through `alloc_zeroed`.
    AllocateZeroed,
    /// A release through `dealloc`.
    Deallocate,
    /// A resize through `realloc`, from the given previous size.
    Reallocate {
        /// Size of the allocation before the call.
        old_size: usize,
    },
}

impl HeapOp {
    const fn encode(self) -> (usize, usize) {
        match self {
            HeapOp::Allocate => (1, 0),
            HeapOp::AllocateZeroed => (2, 0),
            HeapOp::Deallocate => (3, 0),
            HeapOp::Reallocate { old_size } => (4, old_size),
        }
    }

    const fn decode(kind: usize, old_size: usize) -> Self {
        match kind {
            1 => HeapOp::Allocate,
            2 => HeapOp::AllocateZeroed,
            3 => HeapOp::Deallocate,
            _ => HeapOp::Reallocate { old_size },
        }
    }

    const fn name(self) -> &'static str {
        match self {
            HeapOp::Allocate => "allocation",
            HeapOp::AllocateZeroed => "zeroed allocation",
            HeapOp::Deallocate => "deallocation",
            HeapOp::Reallocate { .. } => "reallocation",
        }
    }
}

/// Totals of the heap operations charged to one window.
///
/// `allocations` includes zeroed allocations. A reallocation is counted once, as a
/// reallocation only, whether or not the allocator moved the block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeapCounts {
    /// Calls to `alloc` and `alloc_zeroed`.
    pub allocations: usize,
    /// Bytes requested by those calls.
    pub allocated_bytes: usize,
    /// Calls to `dealloc`.
    pub deallocations: usize,
    /// Bytes released by those calls.
    pub deallocated_bytes: usize,
    /// Calls to `realloc`.
    pub reallocations: usize,
    /// New sizes requested by those calls.
    pub reallocated_bytes: usize,
}

impl HeapCounts {
    /// Whether no heap operation of any kind was charged.
    pub const fn is_clean(self) -> bool {
        self.operations() == 0
    }

    /// Total heap operations of all three kinds.
    pub const fn operations(self) -> usize {
        self.allocations + self.deallocations + self.reallocations
    }
}

/// The threads a window charges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Every thread in the process. Only one such window may be open at a time.
    AllThreads,
    /// The opening thread, plus threads that join through a [`WindowToken`].
    Enrolled,
}

struct EventSlot {
    written: AtomicBool,
    kind: AtomicUsize,
    old_size: AtomicUsize,
    size: AtomicUsize,
    align: AtomicUsize,
    thread: AtomicU64,
    backtrace: UnsafeCell<Option<Backtrace>>,
}

// SAFETY: `backtrace` is written only by the single hook that claimed this slot's
// index from `events_next`, and read only after the window closed and its in-flight
// counter drained to zero, which happens-after that write through the SeqCst
// counter and the Release store of `written`. Readers and writers therefore never
// overlap. Everything else is atomic.
unsafe impl Sync for EventSlot {}

impl EventSlot {
    const fn new() -> Self {
        Self {
            written: AtomicBool::new(false),
            kind: AtomicUsize::new(0),
            old_size: AtomicUsize::new(0),
            size: AtomicUsize::new(0),
            align: AtomicUsize::new(0),
            thread: AtomicU64::new(0),
            backtrace: UnsafeCell::new(None),
        }
    }

    fn write(&self, op: HeapOp, size: usize, align: usize, capture_backtrace: bool) {
        let (kind, old_size) = op.encode();
        self.kind.store(kind, Ordering::Relaxed);
        self.old_size.store(old_size, Ordering::Relaxed);
        self.size.store(size, Ordering::Relaxed);
        self.align.store(align, Ordering::Relaxed);
        self.thread.store(this_thread_tag(), Ordering::Relaxed);
        if capture_backtrace {
            // SAFETY: this hook claimed the slot exclusively (see the `Sync` impl).
            unsafe { *self.backtrace.get() = capture_culprit() };
        }
        self.written.store(true, Ordering::Release);
    }
}

/// Captures a backtrace outside the accounting. Runs at most once per recorded
/// event, and only after a window has already seen a heap operation.
fn capture_culprit() -> Option<Backtrace> {
    // Capturing during a panic could deadlock on the backtrace lock the panic hook
    // holds while it allocates, and a panic must never escape an allocator hook.
    if std::thread::panicking() {
        return None;
    }
    let _ = CAPTURING.try_with(|flag| flag.set(true));
    let captured = catch_unwind(AssertUnwindSafe(Backtrace::force_capture)).ok();
    let _ = CAPTURING.try_with(|flag| flag.set(false));
    captured
}

fn this_thread_tag() -> u64 {
    THREAD_TAG
        .try_with(|tag| {
            if tag.get() == 0 {
                tag.set(NEXT_THREAD_TAG.fetch_add(1, Ordering::Relaxed));
            }
            tag.get()
        })
        .unwrap_or(0)
}

struct WindowSlot {
    /// Zero while free; otherwise the generation of the window holding the slot.
    generation: AtomicU64,
    open: AtomicBool,
    all_threads: AtomicBool,
    capture_backtraces: AtomicBool,
    opener: AtomicU64,
    in_flight: AtomicUsize,
    allocations: AtomicUsize,
    allocated_bytes: AtomicUsize,
    deallocations: AtomicUsize,
    deallocated_bytes: AtomicUsize,
    reallocations: AtomicUsize,
    reallocated_bytes: AtomicUsize,
    events_next: AtomicUsize,
    events: [EventSlot; EVENT_SLOTS],
}

impl WindowSlot {
    const fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            open: AtomicBool::new(false),
            all_threads: AtomicBool::new(false),
            capture_backtraces: AtomicBool::new(false),
            opener: AtomicU64::new(0),
            in_flight: AtomicUsize::new(0),
            allocations: AtomicUsize::new(0),
            allocated_bytes: AtomicUsize::new(0),
            deallocations: AtomicUsize::new(0),
            deallocated_bytes: AtomicUsize::new(0),
            reallocations: AtomicUsize::new(0),
            reallocated_bytes: AtomicUsize::new(0),
            events_next: AtomicUsize::new(0),
            events: [const { EventSlot::new() }; EVENT_SLOTS],
        }
    }

    fn reset(&self) {
        for counter in [
            &self.allocations,
            &self.allocated_bytes,
            &self.deallocations,
            &self.deallocated_bytes,
            &self.reallocations,
            &self.reallocated_bytes,
            &self.events_next,
        ] {
            counter.store(0, Ordering::Relaxed);
        }
    }

    fn charge(&self, op: HeapOp, size: usize, align: usize) {
        match op {
            HeapOp::Allocate | HeapOp::AllocateZeroed => {
                self.allocations.fetch_add(1, Ordering::Relaxed);
                self.allocated_bytes.fetch_add(size, Ordering::Relaxed);
            }
            HeapOp::Deallocate => {
                self.deallocations.fetch_add(1, Ordering::Relaxed);
                self.deallocated_bytes.fetch_add(size, Ordering::Relaxed);
            }
            HeapOp::Reallocate { .. } => {
                self.reallocations.fetch_add(1, Ordering::Relaxed);
                self.reallocated_bytes.fetch_add(size, Ordering::Relaxed);
            }
        }
        let index = self.events_next.fetch_add(1, Ordering::Relaxed);
        if let Some(event) = self.events.get(index) {
            event.write(
                op,
                size,
                align,
                self.capture_backtraces.load(Ordering::Relaxed),
            );
        }
    }

    fn counts(&self) -> HeapCounts {
        HeapCounts {
            allocations: self.allocations.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            deallocations: self.deallocations.load(Ordering::Relaxed),
            deallocated_bytes: self.deallocated_bytes.load(Ordering::Relaxed),
            reallocations: self.reallocations.load(Ordering::Relaxed),
            reallocated_bytes: self.reallocated_bytes.load(Ordering::Relaxed),
        }
    }

    /// Drains the recorded events. Must run after the window closed and drained.
    fn take_events(&self) -> (Vec<HeapEvent>, usize) {
        let opener = self.opener.load(Ordering::Relaxed);
        let observed = self.events_next.load(Ordering::Relaxed);
        let mut events = Vec::with_capacity(observed.min(EVENT_SLOTS));
        for event in &self.events {
            if !event.written.swap(false, Ordering::Acquire) {
                continue;
            }
            let thread = event.thread.load(Ordering::Relaxed);
            events.push(HeapEvent {
                op: HeapOp::decode(
                    event.kind.load(Ordering::Relaxed),
                    event.old_size.load(Ordering::Relaxed),
                ),
                size: event.size.load(Ordering::Relaxed),
                align: event.align.load(Ordering::Relaxed),
                thread,
                on_opening_thread: thread == opener,
                // SAFETY: the window has closed and drained, so no hook can still
                // hold this slot; see the `Sync` impl of `EventSlot`.
                backtrace: unsafe { (*event.backtrace.get()).take() },
            });
        }
        (events, observed.saturating_sub(EVENT_SLOTS))
    }
}

/// Charges a heap operation to the window in `slot`, if that window is still the
/// one the caller observed. `in_flight` is raised before re-checking the window so
/// that `close` either sees this operation or the operation sees the window closed.
fn charge_window(slot: usize, generation: Option<u64>, op: HeapOp, size: usize, align: usize) {
    let Some(window) = WINDOWS.get(slot) else {
        return;
    };
    window.in_flight.fetch_add(1, Ordering::SeqCst);
    let current = window.generation.load(Ordering::SeqCst);
    let matches_window = current != 0
        && generation.is_none_or(|expected| expected == current)
        && window.open.load(Ordering::SeqCst)
        // An enrolled thread on an all-thread window is charged once, not twice.
        && !(generation.is_some() && window.all_threads.load(Ordering::SeqCst));
    if matches_window {
        window.charge(op, size, align);
    }
    window.in_flight.fetch_sub(1, Ordering::SeqCst);
}

#[inline]
fn on_heap_operation(op: HeapOp, size: usize, align: usize) {
    if !INSTALLED.load(Ordering::Relaxed) {
        INSTALLED.store(true, Ordering::Relaxed);
    }
    if OPEN_WINDOWS.load(Ordering::Relaxed) == 0 {
        return;
    }
    if CAPTURING.try_with(Cell::get).unwrap_or(true) {
        return;
    }
    let all_threads = ALL_THREADS_SLOT.load(Ordering::SeqCst);
    if all_threads != 0 {
        charge_window(all_threads - 1, None, op, size, align);
    }
    let (enrolled, generation) = ENROLLED.try_with(Cell::get).unwrap_or((0, 0));
    if enrolled != 0 {
        charge_window(enrolled - 1, Some(generation), op, size, align);
    }
}

/// A `GlobalAlloc` that reports every heap operation to the open [`HeapWindow`]s
/// and forwards it unchanged to `A`.
///
/// Declare it once per test binary, as the module example shows. It adds no heap
/// use of its own to the operations it forwards.
#[derive(Debug)]
pub struct HeapGuard<A = System> {
    inner: A,
}

impl<A> HeapGuard<A> {
    /// Wraps `inner`, the allocator every operation is forwarded to.
    pub const fn new(inner: A) -> Self {
        Self { inner }
    }
}

// SAFETY: every method forwards the caller's arguments and contract unchanged to
// `inner`. The bookkeeping runs before forwarding and touches only static atomics
// and const-initialized thread-locals; it neither allocates nor unwinds.
unsafe impl<A: GlobalAlloc> GlobalAlloc for HeapGuard<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        on_heap_operation(HeapOp::Allocate, layout.size(), layout.align());
        // SAFETY: the caller upholds `alloc`'s layout contract.
        unsafe { self.inner.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        on_heap_operation(HeapOp::AllocateZeroed, layout.size(), layout.align());
        // SAFETY: the caller upholds `alloc_zeroed`'s layout contract.
        unsafe { self.inner.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        on_heap_operation(HeapOp::Deallocate, layout.size(), layout.align());
        // SAFETY: the caller upholds `dealloc`'s pointer and layout contract.
        unsafe { self.inner.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        on_heap_operation(
            HeapOp::Reallocate {
                old_size: layout.size(),
            },
            new_size,
            layout.align(),
        );
        // SAFETY: the caller upholds `realloc`'s pointer, layout and size contract.
        unsafe { self.inner.realloc(pointer, layout, new_size) }
    }
}

/// Fails a window opened in a binary that did not install the guard, which would
/// otherwise count nothing and pass.
fn assert_guard_installed() {
    if INSTALLED.load(Ordering::Relaxed) {
        return;
    }
    drop(std::hint::black_box(Box::new(0_u8)));
    assert!(
        INSTALLED.load(Ordering::Relaxed),
        "no HeapGuard is installed: add `#[global_allocator] static HEAP: HeapGuard = HeapGuard::new(System);` to this test binary"
    );
}

/// Forces this thread's guard state into existence.
///
/// Needed only on targets without native thread-locals, where the first access
/// allocates. Call it on every thread before that thread allocates in a window.
pub fn prime_this_thread() {
    let _ = CAPTURING.try_with(Cell::get);
    let _ = ENROLLED.try_with(Cell::get);
    let _ = this_thread_tag();
}

/// An open measurement window. Closes when finished or dropped, including on panic.
///
/// Must be closed on the thread that opened it, which is why it is not `Send`.
#[derive(Debug)]
pub struct HeapWindow {
    slot: usize,
    generation: u64,
    scope: Scope,
    closed: bool,
    not_send: PhantomData<*const ()>,
}

impl HeapWindow {
    /// Opens a window that records up to [`HeapReport::EVENT_LIMIT`] operations
    /// with backtraces.
    ///
    /// # Panics
    ///
    /// Panics when the guard is not installed, when an all-thread window is already
    /// open, when this thread is already enrolled in a window, or when all window
    /// slots are in use.
    pub fn open(scope: Scope) -> Self {
        Self::open_with(scope, true)
    }

    /// Opens a window that counts and records operations without capturing
    /// backtraces, for measurements where the capture itself could perturb timing.
    ///
    /// # Panics
    ///
    /// Panics under the same conditions as [`HeapWindow::open`].
    pub fn open_counts_only(scope: Scope) -> Self {
        Self::open_with(scope, false)
    }

    fn open_with(scope: Scope, capture_backtraces: bool) -> Self {
        assert_guard_installed();
        prime_this_thread();
        if scope == Scope::Enrolled {
            assert_eq!(
                ENROLLED.with(Cell::get),
                (0, 0),
                "this thread is already enrolled in a window"
            );
        }
        let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
        let (slot, window) = WINDOWS
            .iter()
            .enumerate()
            .find(|(_, window)| {
                window
                    .generation
                    .compare_exchange(0, generation, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            })
            .expect("all heap window slots are in use");
        window.reset();
        window
            .capture_backtraces
            .store(capture_backtraces, Ordering::Relaxed);
        window.opener.store(this_thread_tag(), Ordering::Relaxed);
        window
            .all_threads
            .store(scope == Scope::AllThreads, Ordering::SeqCst);
        if scope == Scope::AllThreads
            && ALL_THREADS_SLOT
                .compare_exchange(0, slot + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        {
            window.generation.store(0, Ordering::SeqCst);
            panic!("only one all-thread heap window may be open at a time");
        }
        // Counted before the hook can reach this window through the publication below.
        OPEN_WINDOWS.fetch_add(1, Ordering::SeqCst);
        window.open.store(true, Ordering::SeqCst);
        if scope == Scope::Enrolled {
            ENROLLED.with(|enrolled| enrolled.set((slot + 1, generation)));
        }
        Self {
            slot,
            generation,
            scope,
            closed: false,
            not_send: PhantomData,
        }
    }

    /// The scope this window charges.
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// A handle another thread can use to join this window.
    pub fn token(&self) -> WindowToken {
        WindowToken {
            slot: self.slot,
            generation: self.generation,
        }
    }

    /// Totals so far. The window stays open.
    pub fn counts(&self) -> HeapCounts {
        WINDOWS
            .get(self.slot)
            .map(WindowSlot::counts)
            .unwrap_or_default()
    }

    /// Closes the window and returns everything it charged.
    pub fn finish(mut self) -> HeapReport {
        let (counts, events, unrecorded) = self.close();
        HeapReport {
            scope: self.scope,
            counts,
            events,
            unrecorded,
        }
    }

    fn close(&mut self) -> (HeapCounts, Vec<HeapEvent>, usize) {
        let Some(window) = WINDOWS.get(self.slot) else {
            return (HeapCounts::default(), Vec::new(), 0);
        };
        self.closed = true;
        match self.scope {
            Scope::AllThreads => {
                let _ = ALL_THREADS_SLOT.compare_exchange(
                    self.slot + 1,
                    0,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                );
            }
            Scope::Enrolled => {
                let _ = ENROLLED.try_with(|enrolled| enrolled.set((0, 0)));
            }
        }
        window.open.store(false, Ordering::SeqCst);
        // Every operation that saw the window open has finished charging once this
        // reads zero, and any later one sees it closed. See `charge_window`.
        let mut spins = 0_u32;
        while window.in_flight.load(Ordering::SeqCst) != 0 {
            spins = spins.saturating_add(1);
            if spins > 256 {
                std::thread::yield_now();
            } else {
                std::hint::spin_loop();
            }
        }
        let counts = window.counts();
        let (events, unrecorded) = window.take_events();
        window.generation.store(0, Ordering::SeqCst);
        OPEN_WINDOWS.fetch_sub(1, Ordering::SeqCst);
        (counts, events, unrecorded)
    }
}

impl Drop for HeapWindow {
    fn drop(&mut self) {
        if !self.closed {
            // Discarding the observations here is fine: the test already panicked.
            let _ = self.close();
        }
    }
}

/// A copyable handle that lets another thread join an open window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowToken {
    slot: usize,
    generation: u64,
}

impl WindowToken {
    /// Charges this thread's heap operations to the window until the returned
    /// [`Enrolment`] drops.
    ///
    /// # Panics
    ///
    /// Panics if the window has closed or this thread is already enrolled.
    #[must_use = "the thread is only enrolled while the returned guard is alive"]
    pub fn enrol_this_thread(self) -> Enrolment {
        let window = WINDOWS
            .get(self.slot)
            .expect("window token refers to an invalid slot");
        assert!(
            window.generation.load(Ordering::SeqCst) == self.generation
                && window.open.load(Ordering::SeqCst),
            "the window this token refers to has closed"
        );
        prime_this_thread();
        assert_eq!(
            ENROLLED.with(Cell::get),
            (0, 0),
            "this thread is already enrolled in a window"
        );
        ENROLLED.with(|enrolled| enrolled.set((self.slot + 1, self.generation)));
        Enrolment {
            not_send: PhantomData,
        }
    }
}

/// Keeps the current thread enrolled in a window. Dropping it leaves the window.
#[derive(Debug)]
pub struct Enrolment {
    not_send: PhantomData<*const ()>,
}

impl Drop for Enrolment {
    fn drop(&mut self) {
        let _ = ENROLLED.try_with(|enrolled| enrolled.set((0, 0)));
    }
}

/// One heap operation recorded by a window.
#[derive(Debug)]
pub struct HeapEvent {
    /// Which operation happened.
    pub op: HeapOp,
    /// Bytes requested, released, or requested as the new size of a reallocation.
    pub size: usize,
    /// Alignment of the layout.
    pub align: usize,
    /// A small number identifying the thread within this process.
    pub thread: u64,
    /// Whether the operation happened on the thread that opened the window.
    pub on_opening_thread: bool,
    /// Where it happened, unless the window was opened with
    /// [`HeapWindow::open_counts_only`] or the thread was unwinding.
    pub backtrace: Option<Backtrace>,
}

/// Frames that belong to the standard library or to this module, not to a culprit.
fn is_library_frame(symbol: &str) -> bool {
    let path = symbol.trim_start_matches('<');
    [
        "std::",
        "core::",
        "alloc::",
        "__rust",
        "___rust",
        "rust_",
        "_Unwind",
        "up_rust::heap_guard",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
}

impl HeapEvent {
    /// The call sites below the allocator that name who asked for the heap,
    /// nearest first, skipping standard-library frames and this module. Empty
    /// without a captured backtrace. Resolves symbols, so call it after the window.
    pub fn culprit(&self, limit: usize) -> Vec<String> {
        let Some(backtrace) = &self.backtrace else {
            return Vec::new();
        };
        if backtrace.status() != BacktraceStatus::Captured {
            return Vec::new();
        }
        backtrace
            .to_string()
            .lines()
            .filter_map(|line| {
                let (number, symbol) = line.trim_start().split_once(": ")?;
                number
                    .chars()
                    .all(|digit| digit.is_ascii_digit())
                    .then_some(symbol)
            })
            .filter(|symbol| !is_library_frame(symbol))
            .take(limit)
            .map(str::to_owned)
            .collect()
    }
}

impl fmt::Display for HeapEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} of {} bytes (align {}) on thread {}{}",
            self.op.name(),
            self.size,
            self.align,
            self.thread,
            if self.on_opening_thread {
                ", the opening thread"
            } else {
                ", another thread"
            }
        )?;
        if let HeapOp::Reallocate { old_size } = self.op {
            write!(formatter, ", resized from {old_size} bytes")?;
        }
        Ok(())
    }
}

/// Everything a finished window observed.
#[derive(Debug)]
pub struct HeapReport {
    scope: Scope,
    counts: HeapCounts,
    events: Vec<HeapEvent>,
    unrecorded: usize,
}

impl HeapReport {
    /// Operations a window records individually; later ones are only counted.
    pub const EVENT_LIMIT: usize = EVENT_SLOTS;

    /// The scope this window charged.
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// The totals of the window.
    pub fn counts(&self) -> HeapCounts {
        self.counts
    }

    /// The operations recorded individually, in order of observation.
    pub fn events(&self) -> &[HeapEvent] {
        &self.events
    }

    /// Operations beyond [`Self::EVENT_LIMIT`], counted but not recorded.
    pub fn unrecorded_events(&self) -> usize {
        self.unrecorded
    }

    /// Whether the window saw no heap operation of any kind.
    pub fn is_clean(&self) -> bool {
        self.counts.is_clean()
    }

    /// Asserts the window saw no heap operation of any kind.
    ///
    /// # Panics
    ///
    /// Panics with the counts, the recorded operations and the first backtrace
    /// when the window saw any allocation, deallocation or reallocation.
    pub fn assert_no_heap(&self, subject: &str) {
        if !self.is_clean() {
            // Formatted before panicking: resolving symbols allocates.
            let message = format!("{subject}: {self}");
            panic!("{message}");
        }
    }

    fn write_summary(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let counts = self.counts;
        write!(
            formatter,
            "heap used in a no-heap window ({} scope): {} allocations ({} bytes), {} deallocations ({} bytes), {} reallocations ({} bytes)",
            match self.scope {
                Scope::AllThreads => "all-thread",
                Scope::Enrolled => "enrolled-thread",
            },
            counts.allocations,
            counts.allocated_bytes,
            counts.deallocations,
            counts.deallocated_bytes,
            counts.reallocations,
            counts.reallocated_bytes,
        )
    }
}

impl fmt::Display for HeapReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_clean() {
            return write!(formatter, "no heap operations");
        }
        self.write_summary(formatter)?;
        for (index, event) in self.events.iter().enumerate() {
            write!(formatter, "\n  #{} {event}", index + 1)?;
            let culprit = event.culprit(3);
            if !culprit.is_empty() {
                write!(formatter, "\n     at {}", culprit.join(" <- "))?;
            }
        }
        if self.unrecorded != 0 {
            write!(
                formatter,
                "\n  ... and {} further operations counted but not recorded",
                self.unrecorded
            )?;
        }
        if let Some(backtrace) = self.events.first().and_then(|e| e.backtrace.as_ref()) {
            let status = backtrace.status();
            if status == BacktraceStatus::Captured {
                write!(formatter, "\nfirst operation:\n{backtrace}")?;
            } else {
                write!(formatter, "\nfirst operation backtrace: {status:?}")?;
            }
        }
        Ok(())
    }
}

/// Runs `operation` inside a window of the given scope and returns the report.
///
/// # Panics
///
/// Panics under the same conditions as [`HeapWindow::open`].
pub fn observe<T>(scope: Scope, operation: impl FnOnce() -> T) -> (T, HeapReport) {
    let window = HeapWindow::open(scope);
    let value = operation();
    (value, window.finish())
}

/// Awaits `operation` inside a window of the given scope and returns the report.
///
/// The window is held across awaits, so the returned future is not `Send`. Use
/// [`Scope::AllThreads`] for futures whose work crosses threads.
///
/// # Panics
///
/// Panics under the same conditions as [`HeapWindow::open`].
pub async fn observe_async<F: Future>(scope: Scope, operation: F) -> (F::Output, HeapReport) {
    let window = HeapWindow::open(scope);
    let value = operation.await;
    (value, window.finish())
}

/// Runs `operation` and asserts it neither allocated, freed nor reallocated.
///
/// # Panics
///
/// Panics when the window saw any heap operation, with a report naming which
/// operations happened and where the first one came from, and under the same
/// conditions as [`HeapWindow::open`].
pub fn assert_no_heap<T>(subject: &str, scope: Scope, operation: impl FnOnce() -> T) -> T {
    let (value, report) = observe(scope, operation);
    report.assert_no_heap(subject);
    value
}
