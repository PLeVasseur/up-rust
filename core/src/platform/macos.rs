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

//! The macOS development backend.
//!
//! Wait and wake use the public `os_sync_wait_on_address` family (macOS 14.4 or
//! later), the platform's documented futex-style call. Its header notes it gives
//! no priority-inversion avoidance, which matters only for the safety path;
//! macOS is development only, with no safety claim.
//!
//! No `pthread_mutex_t`, no lazily boxed object, no static needing first-use setup.

use core::ffi::{c_int, c_void};
use crate::platform::atomic::AtomicU32;

use super::{Monotonic, Platform, PlatformError, PlatformInit, PlatformOp, WaitResult};

/// `OS_SYNC_WAIT_ON_ADDRESS_NONE` / `OS_SYNC_WAKE_BY_ADDRESS_NONE`: process-private.
const SYNC_FLAGS_NONE: u32 = 0;
/// `OS_CLOCK_MACH_ABSOLUTE_TIME` from `<os/clock.h>`, the only clock id these calls accept.
const OS_CLOCK_MACH_ABSOLUTE_TIME: u32 = 32;
/// `CLOCK_UPTIME_RAW` from `<time.h>`: what Rust `Instant` uses on macOS.
const CLOCK_UPTIME_RAW: u32 = 8;
/// `CLOCK_REALTIME` from `<time.h>`.
const CLOCK_REALTIME: u32 = 0;
/// `ETIMEDOUT` on Darwin.
const ETIMEDOUT: c_int = 60;
/// `getentropy` refuses requests larger than this.
const GETENTROPY_MAX: usize = 256;

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

extern "C" {
    fn os_sync_wait_on_address(addr: *mut c_void, value: u64, size: usize, flags: u32) -> c_int;
    fn os_sync_wait_on_address_with_timeout(
        addr: *mut c_void,
        value: u64,
        size: usize,
        flags: u32,
        clockid: u32,
        timeout_ns: u64,
    ) -> c_int;
    fn os_sync_wake_by_address_any(addr: *mut c_void, size: usize, flags: u32) -> c_int;
    fn os_sync_wake_by_address_all(addr: *mut c_void, size: usize, flags: u32) -> c_int;
    fn clock_gettime_nsec_np(clock_id: u32) -> u64;
    fn clock_gettime(clock_id: u32, tp: *mut Timespec) -> c_int;
    fn __error() -> *mut c_int;
    fn getentropy(buf: *mut c_void, buflen: usize) -> c_int;
    fn mlock(addr: *const c_void, len: usize) -> c_int;
    fn getpagesize() -> c_int;
}

/// The native platform. Stateless: nothing to initialize, nothing to tear down.
#[derive(Clone, Copy, Debug, Default)]
pub struct Native;

// One stored trait-object value, reused by setup, preparation and worker control.
// Copying this reference preserves its data and metadata; independently coercing
// Native at each call site would not guarantee equal vtable addresses. Native is
// stateless, so every Native value uses this canonical runtime value. Equality is
// request correlation only, never type/artifact/deployment authentication.
static RUNTIME: &dyn Platform = &Native;

const _: () = assert!(!core::mem::needs_drop::<Native>());

fn word_ptr(word: &AtomicU32) -> *mut c_void {
    word.as_ptr().cast()
}

fn errno() -> c_int {
    // SAFETY: `__error` returns this thread's errno location, always valid.
    unsafe { *__error() }
}

impl Native {
    /// Exercise the named worker-thread entrypoints during initialization.
    /// This calls both wait symbols and this thread's errno accessor; it does
    /// not establish that every OS error, loader or TLS path is allocation-free.
    pub(crate) fn prepare_worker_thread(&self) {
        let word = AtomicU32::new(0);
        let _ = self.now();
        let _ = self.unix_time_ms();
        let _ = self.wait(&word, 1, None);
        // SAFETY: word owns one initialized, four-byte-aligned AtomicU32 for
        // the entire synchronous call. Its as_ptr permits the documented atomic
        // comparison, and no reference to its interior is created. The private
        // word remains zero, so expected=1 cannot match or enter a blocking wait.
        // The Apple os_sync_wait_on_address.h contract accepts size=4, private
        // flags, OS_CLOCK_MACH_ABSOLUTE_TIME and a nonzero relative timeout.
        // No pointer is retained after return. Call directly so an elapsed Rust
        // deadline cannot skip the timed entrypoint during initialization.
        let _ = unsafe {
            os_sync_wait_on_address_with_timeout(
                word.as_ptr().cast(), 1, 4, SYNC_FLAGS_NONE,
                OS_CLOCK_MACH_ABSOLUTE_TIME, 1,
            )
        };
        // A mismatch can succeed without reading errno in wait(), so touch the
        // actual worker's errno explicitly regardless of the native return code.
        let _ = errno();
        self.wake_one(&word);
        self.wake_all(&word);
    }
}

impl Platform for Native {
    fn now(&self) -> Monotonic {
        // SAFETY: plain libSystem call with a valid clock id; returns a value.
        Monotonic(unsafe { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) })
    }

    fn unix_time_ms(&self) -> Option<u64> {
        let mut ts = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `ts` is valid writable storage for one timespec.
        if unsafe { clock_gettime(CLOCK_REALTIME, &mut ts) } != 0 {
            return None;
        }
        let seconds = u64::try_from(ts.tv_sec).ok()?;
        let millis = u64::try_from(ts.tv_nsec).ok()? / 1_000_000;
        seconds.checked_mul(1000)?.checked_add(millis)
    }

    fn wait(&self, word: &AtomicU32, expected: u32, deadline: Option<Monotonic>) -> WaitResult {
        let addr = word_ptr(word);
        let rc = match deadline {
            None => {
                // SAFETY: `addr` is a live, 4-aligned u32; size matches.
                unsafe { os_sync_wait_on_address(addr, u64::from(expected), 4, SYNC_FLAGS_NONE) }
            }
            Some(deadline) => {
                let remaining = deadline.saturating_since(self.now());
                if remaining == 0 {
                    return WaitResult::TimedOut;
                }
                // SAFETY: as above; the clock id and non-zero timeout satisfy the contract.
                unsafe {
                    os_sync_wait_on_address_with_timeout(
                        addr,
                        u64::from(expected),
                        4,
                        SYNC_FLAGS_NONE,
                        OS_CLOCK_MACH_ABSOLUTE_TIME,
                        remaining,
                    )
                }
            }
        };
        if rc < 0 && errno() == ETIMEDOUT {
            return WaitResult::TimedOut;
        }
        // EINTR, EFAULT, ENOMEM and EINVAL fall through: re-check and retry. The
        // kernel may run short of its own memory here and return early.
        WaitResult::Returned
    }

    fn wake_one(&self, word: &AtomicU32) {
        // SAFETY: `addr` is a live u32; ENOENT (no waiters) is fine to ignore.
        unsafe { os_sync_wake_by_address_any(word_ptr(word), 4, SYNC_FLAGS_NONE) };
    }

    fn wake_all(&self, word: &AtomicU32) {
        // SAFETY: as in `wake_one`.
        unsafe { os_sync_wake_by_address_all(word_ptr(word), 4, SYNC_FLAGS_NONE) };
    }
}

impl PlatformInit for Native {
    #[cfg(target_pointer_width = "64")]
    fn prepare_deadline_service(
        &'static self, memory: &crate::memory::StartupMemory,
        request: crate::transport::DeadlineServiceRequest,
    ) -> Result<crate::transport::PreparedDeadlineService, crate::transport::error::StartupError> {
        crate::transport::PreparedDeadlineService::prepare_native(memory, self, request)
    }

    fn entropy(&self, out: &mut [u8]) -> Result<(), PlatformError> {
        for chunk in out.chunks_mut(GETENTROPY_MAX) {
            // SAFETY: `chunk` is writable and at most GETENTROPY_MAX bytes.
            if unsafe { getentropy(chunk.as_mut_ptr().cast(), chunk.len()) } != 0 {
                return Err(PlatformError {
                    op: PlatformOp::Entropy,
                    code: errno(),
                });
            }
        }
        Ok(())
    }

    unsafe fn prefault(&self, ptr: *mut u8, len: usize) -> Result<(), PlatformError> {
        // SAFETY: plain libSystem call.
        let page = unsafe { getpagesize() };
        let mask = super::page_mask(usize::try_from(page).unwrap_or(0))?;
        let (start, end, aligned_end) = super::aligned_bounds(ptr, len, mask)?;
        // SAFETY: the caller promised the range is writable and owned.
        unsafe { super::touch_pages(ptr as usize, end, mask.wrapping_add(1))? };
        // SAFETY: `start..aligned_end` covers only pages the caller owns or shares
        // with its own static storage; `mlock` does not write to it.
        if unsafe { mlock(start as *const c_void, aligned_end.wrapping_sub(start)) } != 0 {
            return Err(PlatformError {
                op: PlatformOp::Prefault,
                code: errno(),
            });
        }
        Ok(())
    }

    fn runtime(&'static self) -> &'static dyn Platform {
        RUNTIME
    }
}
