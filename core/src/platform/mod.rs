/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

//! The platform layer: the only part of this crate that talks to an operating system.
//!
//! It sits below L1 (R45) and stays out of user code. Two traits, one per mode,
//! mirror the transport modes (R1, R2): a value of [`PlatformInit`] exists only
//! in initialization, and [`PlatformInit::runtime`] hands runtime code a value
//! that cannot reach back. Implementations must not allocate, panic or create
//! threads, and must not need any global state that requires first-use setup.
//!
//! Backends are chosen by `cfg` at build time. macOS is development only, with
//! no safety claim. QNX is source only until a toolchain exists; it has never
//! been compiled for or run on a real target (see `qnx.rs`).

pub(crate) mod atomic;
pub(crate) mod exclusion;
pub(crate) mod progress;
pub(crate) mod prepared;

use crate::platform::atomic::AtomicU32;

#[cfg(any(target_os = "linux", check_linux_backend))]
pub mod linux;
#[cfg(target_os = "linux")]
pub use linux::Native;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::Native;

#[cfg(any(target_os = "nto", target_os = "qnx", check_qnx_backend))]
pub mod qnx;
#[cfg(any(target_os = "nto", target_os = "qnx"))]
pub use qnx::Native;

/// A monotonic instant, in nanoseconds since an unspecified origin.
///
/// Only differences are meaningful. This clock is never the wall clock: a step
/// of the system time must not move a deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Monotonic(pub u64);

impl Monotonic {
    /// `self + nanos`, saturating. A saturated deadline means "effectively never".
    #[must_use]
    pub const fn saturating_add(self, nanos: u64) -> Self {
        Self(self.0.saturating_add(nanos))
    }

    /// Nanoseconds from `earlier` to `self`, or zero if `earlier` is later.
    #[must_use]
    pub const fn saturating_since(self, earlier: Self) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

/// How a [`Platform::wait`] ended.
///
/// A waiter must always re-check its condition afterwards: spurious returns are
/// allowed on every platform (interrupts, kernel memory pressure, racing wakes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitResult {
    /// Woken, or returned early for a reason that only requires a re-check.
    Returned,
    /// The deadline passed.
    TimedOut,
}

/// An initialization-time platform failure, as a plain value (no heap, no panic).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformError {
    /// The operation that failed.
    pub op: PlatformOp,
    /// The operating system error code, or zero if there is none.
    pub code: i32,
}

impl core::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let what = match self.op {
            PlatformOp::Entropy => "reading OS entropy failed",
            PlatformOp::Prefault => "making startup memory resident failed",
            PlatformOp::InvalidArgument => "invalid platform argument",
        };
        write!(f, "{what} (error code {})", self.code)
    }
}

impl core::error::Error for PlatformError {}

/// Which initialization operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlatformOp {
    /// Reading entropy for a seed.
    Entropy,
    /// Making a memory range resident and locked.
    Prefault,
    /// The requested range or size is invalid.
    InvalidArgument,
}

/// The native platform as a static, for constructors that need `&'static dyn`.
#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "nto",
    target_os = "qnx"
))]
pub static NATIVE: Native = Native;

/// Everything runtime code may ask of the platform.
///
/// Rules every implementation must keep: no heap, no panic, no thread creation,
/// no global state that needs first-use setup, and `wait` must not lose a wake
/// that happens after the caller observed `expected` (the classic futex contract).
pub trait Platform: Sync {
    /// The monotonic clock. Infallible by contract.
    fn now(&self) -> Monotonic;

    /// The wall clock, in milliseconds since the Unix epoch, or `None` if it cannot be read.
    ///
    /// UUIDv7 timestamps and message TTLs are defined in wall time, so this clock
    /// is read at runtime. A failed read must not panic: callers reuse the last
    /// good time for IDs and skip and count TTL checks.
    fn unix_time_ms(&self) -> Option<u64>;

    /// Blocks the caller while `word` still holds `expected`, until a wake or `deadline`.
    ///
    /// May return early; callers re-check their condition. `None` waits
    /// indefinitely, so anything built on this should pass a deadline (R40).
    fn wait(&self, word: &AtomicU32, expected: u32, deadline: Option<Monotonic>) -> WaitResult;

    /// Wakes at most one waiter blocked on `word`.
    fn wake_one(&self, word: &AtomicU32);

    /// Wakes all waiters blocked on `word`.
    fn wake_all(&self, word: &AtomicU32);
}

/// What only initialization may ask of the platform.
///
/// Nothing in runtime mode holds one of these.
pub trait PlatformInit: Sync {
    /// Initialization-only SDK-owned worker preparation, using the setup's own
    /// permanent storage. The opaque result cannot be implemented or constructed
    /// by a safe caller. It is correlated again with this exact setup before arm.
    /// Platform implementations without a worker remain explicitly incomplete.
    fn prepare_deadline_service(
        &'static self, _memory: &crate::memory::StartupMemory,
        _request: crate::transport::DeadlineServiceRequest,
    ) -> Result<crate::transport::PreparedDeadlineService, crate::transport::error::StartupError> {
        Err(crate::transport::error::StartupError::DeadlineTiming {
            gap: crate::transport::TimingGap::BackendMissing, cleanup_code: None })
    }

    /// Fills `out` with OS entropy. Used once, to seed message-ID generators.
    ///
    /// # Errors
    /// Returns a [`PlatformError`] with the operating system's error code.
    fn entropy(&self, out: &mut [u8]) -> Result<(), PlatformError>;

    /// Makes `[ptr, ptr + len)` resident: touches every page, then locks it where the
    /// platform can. After this, the first write in runtime mode takes no page fault.
    ///
    /// # Errors
    /// Returns a [`PlatformError`] when the range is invalid or cannot be locked.
    ///
    /// # Safety
    /// `ptr..ptr + len` must be writable memory owned by the caller, with no live
    /// references to it and nothing yet placed in it (every byte is overwritten).
    unsafe fn prefault(&self, ptr: *mut u8, len: usize) -> Result<(), PlatformError>;

    /// The runtime face of this platform, handed to runtime values at the mode change.
    fn runtime(&'static self) -> &'static dyn Platform;
}

/// Page size is checked in one place so backends stay small.
#[cfg(any(target_os = "linux", target_os = "macos", check_linux_backend))]
pub(crate) fn page_mask(page: usize) -> Result<usize, PlatformError> {
    if page == 0 || !page.is_power_of_two() {
        return Err(PlatformError {
            op: PlatformOp::InvalidArgument,
            code: 0,
        });
    }
    Ok(page.wrapping_sub(1))
}

/// Touches every page of `[start, end)`. Shared by the backends.
///
/// # Safety
/// The range must be writable memory owned by the caller.
#[cfg(any(target_os = "linux", target_os = "macos", check_linux_backend))]
pub(crate) unsafe fn touch_pages(
    start: usize,
    end: usize,
    page: usize,
) -> Result<(), PlatformError> {
    let mut cursor = start;
    while cursor < end {
        // SAFETY: `cursor` stays in the caller's writable range.
        unsafe { core::ptr::write_volatile(cursor as *mut u8, 0) };
        cursor = cursor.checked_add(page).ok_or(PlatformError {
            op: PlatformOp::InvalidArgument,
            code: 0,
        })?;
    }
    if start < end {
        // SAFETY: `end - 1` is inside the caller's range because the range is non-empty.
        // Touching the last byte covers a range that ends just past a page start.
        unsafe { core::ptr::write_volatile(end.wrapping_sub(1) as *mut u8, 0) };
    }
    Ok(())
}

/// Page-aligned bounds of `[ptr, ptr + len)`, for `mlock`.
#[cfg(any(target_os = "linux", target_os = "macos", check_linux_backend))]
pub(crate) fn aligned_bounds(
    ptr: *mut u8,
    len: usize,
    mask: usize,
) -> Result<(usize, usize, usize), PlatformError> {
    let invalid = PlatformError {
        op: PlatformOp::InvalidArgument,
        code: 0,
    };
    let start = ptr as usize;
    let end = start.checked_add(len).ok_or(invalid)?;
    let aligned_end = end.checked_add(mask).ok_or(invalid)? & !mask;
    Ok((start & !mask, end, aligned_end))
}

#[cfg(any(all(target_os = "macos", target_arch = "aarch64"), all(target_os = "linux", target_env = "gnu", any(target_arch = "x86_64", target_arch = "aarch64"))))]
pub(crate) mod deadline_atomic128;
