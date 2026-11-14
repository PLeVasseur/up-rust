// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! C-level heap counters, joined with a [`HeapGuard`](super::HeapGuard) window.
//!
//! The guard sees Rust `GlobalAlloc` traffic only. C and C++ allocation, and `std`'s
//! direct use of `System`, reach the C allocator without passing through it. The
//! allocation observer `libup_flow_allocation_observer_v2` interposes that allocator
//! on Linux glibc and on macOS and counts both acquisitions and releases. A window
//! from this module takes its Rust half from the guard and its C half from that
//! observer, so one assertion covers both.
//!
//! ```no_run
//! use std::alloc::System;
//! use up_rust::heap_guard::{c_level, HeapGuard};
//!
//! #[global_allocator]
//! static HEAP: HeapGuard = HeapGuard::new(System);
//!
//! fn main() {
//!     c_level::assert_no_heap("send", || 1 + 1);
//! }
//! ```
//!
//! # Loading the observer
//!
//! A window requires the observer: it is refused if the library is missing, if its
//! header version is not 2, or if it has no counter file mapped, because each of those
//! would count nothing and pass. Load the library into the test process and name a
//! fresh counter file:
//!
//! * Linux glibc: `ld.so --preload libup_flow_allocation_observer_v2.so <test>` or
//!   `LD_PRELOAD`.
//! * macOS: `DYLD_INSERT_LIBRARIES=libup_flow_allocation_observer_v2.dylib <test>`.
//! * Both: `UPROTOCOL_ALLOCATION_COUNTER_FILE=<fresh path>`.
//!
//! # What a window counts
//!
//! The C half is process-wide, so a window is an all-thread window: it needs a quiet
//! process, and only one can be open. Allocations are counted when requested. Releases
//! are counted by the observer as `free` of a non-null pointer, `realloc(p, 0)`, and
//! the old block of a moved `realloc`. A released block's bytes are its usable size,
//! which can exceed the size requested for it, so the two byte totals do not cancel.
//!
//! A `free` of a null pointer releases nothing, but it is still a call into the
//! allocator, so it fails a clean window and is reported on its own.
//!
//! [`Window::open`] captures a backtrace for the first Rust-level operations, and that
//! capture allocates below the guard. The C counters then include it; the window has
//! already failed at that point. Use [`Window::open_counts_only`] or [`observe`] when
//! the exact C-level numbers of a window with Rust-level heap use matter.

use std::{
    ffi::{c_char, c_uint, c_void, CStr},
    fmt,
    sync::OnceLock,
};

use super::{HeapReport, HeapWindow, Scope};

const OBSERVER_VERSION: c_uint = 2;
const OBSERVER_MAGIC: u64 = 0x3130_434f_4c41_5055;
const HEADER_WORDS: usize = 28;

// Word positions in the version 2 observer header.
const W_MAGIC: usize = 0;
const W_VERSION: usize = 1;
const W_BYTE_SIZE: usize = 2;
const W_ENABLED: usize = 5;
const W_PROCESS_CALLS: usize = 6;
const W_PROCESS_BYTES: usize = 7;
const W_MALLOC: usize = 10;
const W_CALLOC: usize = 11;
const W_REALLOC: usize = 12;
const W_ALIGNED: usize = 13;
const W_OVERFLOW: usize = 16;
const W_RELEASE_CALLS: usize = 19;
const W_RELEASED_BYTES: usize = 20;
const W_FREE: usize = 23;
const W_FREE_NULL: usize = 24;
const W_REALLOC_RELEASE: usize = 25;
const W_REALLOC_MOVE: usize = 26;

const LOAD_HINT: &str = "preload libup_flow_allocation_observer_v2 (Linux glibc: ld.so --preload or \
     LD_PRELOAD; macOS: DYLD_INSERT_LIBRARIES) and set UPROTOCOL_ALLOCATION_COUNTER_FILE to a fresh path";

#[cfg(target_os = "linux")]
#[link(name = "dl")]
extern "C" {}

extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

/// `dlsym` search handle for "every loaded image".
#[cfg(target_os = "macos")]
const RTLD_DEFAULT: *mut c_void = -2_isize as *mut c_void;
#[cfg(not(target_os = "macos"))]
const RTLD_DEFAULT: *mut c_void = core::ptr::null_mut();

type ObserverVersion = unsafe extern "C" fn() -> c_uint;
type ObserverMapped = unsafe extern "C" fn() -> c_uint;
type ObserverSnapshot = unsafe extern "C" fn(*mut u64, usize) -> usize;

fn symbol(name: &CStr) -> *mut c_void {
    // SAFETY: `name` is a valid NUL-terminated string and the handle is a dlsym constant.
    unsafe { dlsym(RTLD_DEFAULT, name.as_ptr()) }
}

#[derive(Clone, Copy, Debug)]
struct Observer {
    snapshot: ObserverSnapshot,
}

impl Observer {
    /// Finds the observer, or says why it cannot be used.
    fn resolve() -> Result<Self, String> {
        let version_symbol = symbol(c"up_flow_alloc_observer_version");
        if version_symbol.is_null() {
            return Err(format!(
                "the C-level allocation observer is not loaded: {LOAD_HINT}"
            ));
        }
        // SAFETY: the symbol is the observer's version function, with this signature.
        let version =
            unsafe { std::mem::transmute::<*mut c_void, ObserverVersion>(version_symbol) };
        // SAFETY: the function takes no arguments and only returns a constant.
        let found = unsafe { version() };
        if found != OBSERVER_VERSION {
            return Err(format!(
                "the C-level allocation observer is version {found}, and this window needs \
                 version {OBSERVER_VERSION}: {LOAD_HINT}"
            ));
        }
        let mapped_symbol = symbol(c"up_flow_alloc_observer_mapped");
        let snapshot_symbol = symbol(c"up_flow_alloc_snapshot_v2");
        if mapped_symbol.is_null() || snapshot_symbol.is_null() {
            return Err(format!(
                "the loaded C-level allocation observer does not export the version 2 API: {LOAD_HINT}"
            ));
        }
        // SAFETY: these are the observer's mapped and snapshot functions, with these
        // signatures, resolved after the version check above.
        let (mapped, snapshot) = unsafe {
            (
                std::mem::transmute::<*mut c_void, ObserverMapped>(mapped_symbol),
                std::mem::transmute::<*mut c_void, ObserverSnapshot>(snapshot_symbol),
            )
        };
        // SAFETY: the function takes no arguments and only reads a static.
        if unsafe { mapped() } != 1 {
            return Err(format!(
                "the C-level allocation observer is loaded but has no counter file mapped, so it \
                 would count nothing: {LOAD_HINT}"
            ));
        }
        Ok(Self { snapshot })
    }

    /// Takes one coherent image of the counters. Does not allocate.
    fn read(&self) -> Snapshot {
        let mut words = [0_u64; HEADER_WORDS];
        // SAFETY: `words` has room for `HEADER_WORDS` values and the snapshot function
        // writes at most that many.
        let copied = unsafe { (self.snapshot)(words.as_mut_ptr(), HEADER_WORDS) };
        assert_eq!(
            copied, HEADER_WORDS,
            "the C-level allocation observer returned no counters"
        );
        assert!(
            words[W_MAGIC] == OBSERVER_MAGIC
                && words[W_VERSION] == u64::from(OBSERVER_VERSION)
                && words[W_BYTE_SIZE] == (HEADER_WORDS * 8) as u64,
            "the C-level allocation observer returned an unexpected header"
        );
        assert_eq!(
            words[W_ENABLED], 1,
            "the C-level allocation observer is disabled and would count nothing"
        );
        assert_eq!(
            words[W_OVERFLOW], 0,
            "a C-level allocation counter saturated, so the counts are no longer exact"
        );
        Snapshot(words)
    }
}

fn observer() -> Observer {
    static OBSERVER: OnceLock<Result<Observer, String>> = OnceLock::new();
    match OBSERVER.get_or_init(Observer::resolve) {
        Ok(found) => *found,
        Err(why) => panic!("{why}"),
    }
}

#[derive(Clone, Copy, Debug)]
struct Snapshot([u64; HEADER_WORDS]);

impl Snapshot {
    fn since(&self, earlier: &Self) -> CHeapCounts {
        let delta = |index: usize| match (self.0.get(index), earlier.0.get(index)) {
            (Some(now), Some(then)) => now.wrapping_sub(*then),
            _ => 0,
        };
        CHeapCounts {
            allocation_calls: delta(W_PROCESS_CALLS),
            requested_bytes: delta(W_PROCESS_BYTES),
            malloc_calls: delta(W_MALLOC),
            calloc_calls: delta(W_CALLOC),
            realloc_calls: delta(W_REALLOC),
            aligned_calls: delta(W_ALIGNED),
            release_calls: delta(W_RELEASE_CALLS),
            released_usable_bytes: delta(W_RELEASED_BYTES),
            free_calls: delta(W_FREE),
            realloc_release_calls: delta(W_REALLOC_RELEASE),
            realloc_move_calls: delta(W_REALLOC_MOVE),
            free_null_calls: delta(W_FREE_NULL),
        }
    }
}

/// Heap operations the observer saw at the C level during a window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CHeapCounts {
    /// Entries to the malloc family: `malloc`, `calloc`, `realloc` and the aligned family.
    pub allocation_calls: u64,
    /// Bytes those entries requested.
    pub requested_bytes: u64,
    /// `malloc` entries.
    pub malloc_calls: u64,
    /// `calloc` entries.
    pub calloc_calls: u64,
    /// `realloc` entries, including in-place ones.
    pub realloc_calls: u64,
    /// Aligned-family entries.
    pub aligned_calls: u64,
    /// Blocks handed back: non-null `free`, `realloc(p, 0)` and moved `realloc` old blocks.
    pub release_calls: u64,
    /// Usable bytes of those blocks.
    pub released_usable_bytes: u64,
    /// `free` entries with a non-null pointer.
    pub free_calls: u64,
    /// `realloc(p, 0)` releases.
    pub realloc_release_calls: u64,
    /// Old blocks released because `realloc` moved them.
    pub realloc_move_calls: u64,
    /// `free` entries with a null pointer. They release nothing, and are counted alone.
    pub free_null_calls: u64,
}

impl CHeapCounts {
    /// Whether the window made no call into the C allocator that touches the heap.
    pub const fn is_clean(self) -> bool {
        self.allocation_calls == 0
            && self.requested_bytes == 0
            && self.release_calls == 0
            && self.released_usable_bytes == 0
            && self.free_null_calls == 0
    }
}

impl fmt::Display for CHeapCounts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} allocation entries ({} bytes requested), {} releases ({} usable bytes), {} free(NULL)",
            self.allocation_calls,
            self.requested_bytes,
            self.release_calls,
            self.released_usable_bytes,
            self.free_null_calls
        )
    }
}

/// A window over both the Rust guard and the C-level observer.
///
/// Closes when finished or dropped, including on panic, on the thread that opened it.
#[derive(Debug)]
pub struct Window {
    rust: HeapWindow,
    observer: Observer,
    before: Snapshot,
}

impl Window {
    /// Opens a window that records up to
    /// [`HeapReport::EVENT_LIMIT`] Rust-level operations with backtraces.
    ///
    /// # Panics
    ///
    /// Panics when the Rust guard is not installed, when the C-level observer is not
    /// loaded, is the wrong version or has no counter file mapped, and when another
    /// all-thread window is open.
    pub fn open() -> Self {
        let observer = observer();
        let rust = HeapWindow::open(Scope::AllThreads);
        let before = observer.read();
        Self {
            rust,
            observer,
            before,
        }
    }

    /// Opens a window without Rust-level backtrace capture, so the C-level numbers are
    /// exactly what the code did even when the Rust guard sees an operation.
    ///
    /// # Panics
    ///
    /// Panics under the same conditions as [`Window::open`].
    pub fn open_counts_only() -> Self {
        let observer = observer();
        let rust = HeapWindow::open_counts_only(Scope::AllThreads);
        let before = observer.read();
        Self {
            rust,
            observer,
            before,
        }
    }

    /// Closes the window and returns what both levels saw.
    ///
    /// # Panics
    ///
    /// Panics when the observer's counters were disabled or saturated in the window.
    pub fn finish(self) -> Report {
        let after = self.observer.read();
        let rust = self.rust.finish();
        Report {
            rust,
            c: after.since(&self.before),
        }
    }
}

/// Everything a finished [`Window`] observed.
#[derive(Debug)]
pub struct Report {
    rust: HeapReport,
    c: CHeapCounts,
}

impl Report {
    /// What the Rust guard saw.
    pub fn rust(&self) -> &HeapReport {
        &self.rust
    }

    /// What the C-level observer saw.
    pub fn c(&self) -> CHeapCounts {
        self.c
    }

    /// Whether neither level saw any heap operation.
    pub fn is_clean(&self) -> bool {
        self.rust.is_clean() && self.c.is_clean()
    }

    /// Asserts neither level saw any heap operation.
    ///
    /// # Panics
    ///
    /// Panics with the counts of both levels when either saw any heap operation.
    pub fn assert_no_heap(&self, subject: &str) {
        if !self.is_clean() {
            // Formatted before panicking: resolving symbols allocates.
            let message = format!("{subject}: {self}");
            panic!("{message}");
        }
    }
}

impl fmt::Display for Report {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "C level: {}", self.c)?;
        write!(formatter, "\nRust level: {}", self.rust)
    }
}

/// Runs `operation` inside a counts-only window and returns both levels' report.
///
/// # Panics
///
/// Panics under the same conditions as [`Window::open`].
pub fn observe<T>(operation: impl FnOnce() -> T) -> (T, Report) {
    let window = Window::open_counts_only();
    let value = operation();
    (value, window.finish())
}

/// Runs `operation` and asserts it touched the heap at neither level.
///
/// # Panics
///
/// Panics when the window saw any heap operation at either level, and under the same
/// conditions as [`Window::open`].
pub fn assert_no_heap<T>(subject: &str, operation: impl FnOnce() -> T) -> T {
    let window = Window::open();
    let value = operation();
    window.finish().assert_no_heap(subject);
    value
}
