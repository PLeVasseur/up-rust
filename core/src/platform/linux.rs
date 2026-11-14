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

//! The Linux backend.
//!
//! Wait uses `FUTEX_WAIT_BITSET` with `FUTEX_BITSET_MATCH_ANY`, which measures
//! an absolute deadline against `CLOCK_MONOTONIC` unless `FUTEX_CLOCK_REALTIME`
//! is set. A wall-clock step therefore cannot move a deadline.
//!
//! Everything is process-private (`FUTEX_PRIVATE_FLAG`). There is deliberately
//! no shared-memory futex and no robust list: R40 forbids cross-process locks.

use core::ffi::{c_int, c_long, c_uint, c_void};
use crate::platform::atomic::AtomicU32;

use super::{Monotonic, Platform, PlatformError, PlatformInit, PlatformOp, WaitResult};

#[cfg(target_arch = "x86_64")]
const SYS_FUTEX: c_long = 202;
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
const SYS_FUTEX: c_long = 98;

const FUTEX_WAIT_BITSET: c_int = 9;
const FUTEX_WAKE: c_int = 1;
const FUTEX_PRIVATE_FLAG: c_int = 128;
const FUTEX_BITSET_MATCH_ANY: u32 = 0xffff_ffff;

const CLOCK_REALTIME: c_int = 0;
const CLOCK_MONOTONIC: c_int = 1;
const ETIMEDOUT: c_int = 110;
const NANOS_PER_SECOND: u64 = 1_000_000_000;
const MAX_GETRANDOM: usize = 256;
const SC_PAGESIZE: c_int = 30;

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

extern "C" {
    fn syscall(num: c_long, ...) -> c_long;
    fn clock_gettime(clock_id: c_int, tp: *mut Timespec) -> c_int;
    fn __errno_location() -> *mut c_int;
    fn getrandom(buf: *mut c_void, buflen: usize, flags: c_uint) -> isize;
    fn mlock(addr: *const c_void, len: usize) -> c_int;
    fn sysconf(name: c_int) -> c_long;
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

fn word_ptr(word: &AtomicU32) -> *mut u32 {
    word.as_ptr()
}

fn read_clock(clock: c_int) -> Option<Timespec> {
    let mut ts = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is valid writable storage for one timespec.
    if unsafe { clock_gettime(clock, &mut ts) } != 0 {
        return None;
    }
    Some(ts)
}

fn errno() -> c_int {
    // SAFETY: `__errno_location` returns this thread's errno location, always valid.
    unsafe { *__errno_location() }
}

impl Platform for Native {
    fn now(&self) -> Monotonic {
        // CLOCK_MONOTONIC cannot fail with a valid pointer on Linux. If it ever
        // did, zero ("arbitrary origin") only shortens a wait.
        let ts = read_clock(CLOCK_MONOTONIC).unwrap_or(Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        });
        let seconds = u64::try_from(ts.tv_sec).unwrap_or(0);
        let nanos = u64::try_from(ts.tv_nsec).unwrap_or(0);
        Monotonic(seconds.wrapping_mul(NANOS_PER_SECOND).wrapping_add(nanos))
    }

    fn unix_time_ms(&self) -> Option<u64> {
        let ts = read_clock(CLOCK_REALTIME)?;
        let seconds = u64::try_from(ts.tv_sec).ok()?;
        let millis = u64::try_from(ts.tv_nsec).ok()? / 1_000_000;
        seconds.checked_mul(1000)?.checked_add(millis)
    }

    fn wait(&self, word: &AtomicU32, expected: u32, deadline: Option<Monotonic>) -> WaitResult {
        let timeout = deadline.map(|deadline| Timespec {
            tv_sec: (deadline.0 / NANOS_PER_SECOND) as i64,
            tv_nsec: (deadline.0 % NANOS_PER_SECOND) as i64,
        });
        let timeout_ptr = timeout
            .as_ref()
            .map_or(core::ptr::null(), |t| t as *const Timespec);
        // SAFETY: `word` is a live, aligned u32; the timespec outlives the syscall;
        // with WAIT_BITSET the timeout is an absolute CLOCK_MONOTONIC value.
        let rc = unsafe {
            syscall(
                SYS_FUTEX,
                word_ptr(word),
                FUTEX_WAIT_BITSET | FUTEX_PRIVATE_FLAG,
                expected,
                timeout_ptr,
                core::ptr::null::<u32>(),
                FUTEX_BITSET_MATCH_ANY,
            )
        };
        if rc == -1 && errno() == ETIMEDOUT {
            return WaitResult::TimedOut;
        }
        // EAGAIN (the value changed) and EINTR need only a re-check by the caller.
        WaitResult::Returned
    }

    fn wake_one(&self, word: &AtomicU32) {
        // SAFETY: `word` is a live u32.
        unsafe {
            syscall(
                SYS_FUTEX,
                word_ptr(word),
                FUTEX_WAKE | FUTEX_PRIVATE_FLAG,
                1_i32,
            )
        };
    }

    fn wake_all(&self, word: &AtomicU32) {
        // SAFETY: `word` is a live u32.
        unsafe {
            syscall(
                SYS_FUTEX,
                word_ptr(word),
                FUTEX_WAKE | FUTEX_PRIVATE_FLAG,
                i32::MAX,
            )
        };
    }
}

impl PlatformInit for Native {
    #[cfg(all(target_env = "gnu", target_pointer_width = "64", any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64")))]
    fn prepare_deadline_service(
        &'static self, memory: &crate::memory::StartupMemory,
        request: crate::transport::DeadlineServiceRequest,
    ) -> Result<crate::transport::PreparedDeadlineService, crate::transport::error::StartupError> {
        crate::transport::PreparedDeadlineService::prepare_native(memory, self, request)
    }

    fn entropy(&self, out: &mut [u8]) -> Result<(), PlatformError> {
        for chunk in out.chunks_mut(MAX_GETRANDOM) {
            // SAFETY: `chunk` is writable. Flag 0: block only until the pool is ready.
            let got = unsafe { getrandom(chunk.as_mut_ptr().cast(), chunk.len(), 0) };
            if got < 0 || got.unsigned_abs() != chunk.len() {
                return Err(PlatformError {
                    op: PlatformOp::Entropy,
                    code: errno(),
                });
            }
        }
        Ok(())
    }

    unsafe fn prefault(&self, ptr: *mut u8, len: usize) -> Result<(), PlatformError> {
        // SAFETY: plain libc call.
        let page = unsafe { sysconf(SC_PAGESIZE) };
        let mask = super::page_mask(usize::try_from(page).unwrap_or(0))?;
        let (start, end, aligned_end) = super::aligned_bounds(ptr, len, mask)?;
        // SAFETY: the caller promised the range is writable and owned.
        unsafe { super::touch_pages(ptr as usize, end, mask.wrapping_add(1))? };
        // SAFETY: the range covers pages the caller owns or shares with its static storage.
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
