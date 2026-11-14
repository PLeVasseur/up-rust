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

//! The QNX backend. **Source only: it has never been compiled for a QNX target
//! or run on one**, because no QNX toolchain exists in this project's
//! environment. It is type-checked on the development host with
//! `--cfg check_qnx_backend`. Treat it as a design sketch until an SDP build and
//! a run on a real target exist.
//!
//! QNX has no futex. `wait` and `wake` use a static table of pinned
//! `pthread_mutex_t` and `pthread_cond_t` buckets keyed by word address,
//! `const`-initialized, so there is no heap and no first-use state. Mutexes and
//! condition variables use the platform's default priority-inheritance protocol.
//! A condition variable is initialized with `CLOCK_MONOTONIC`, so a deadline is
//! a monotonic time and a wall-clock step cannot move it.
//!
//! Each bucket lock is held only while reading the word or waking, never while a
//! waiter sleeps (the condition variable releases it), so waiters on one address
//! cannot block wakes on another.

use core::cell::UnsafeCell;
use core::ffi::{c_int, c_long, c_void};
use crate::platform::atomic::{AtomicU32, Ordering};

use super::{Monotonic, Platform, PlatformError, PlatformInit, PlatformOp, WaitResult};

const CLOCK_REALTIME: c_int = 0;
const CLOCK_MONOTONIC: c_int = 2;
const ETIMEDOUT: c_int = 260;
const NANOS_PER_SECOND: u64 = 1_000_000_000;
const SC_PAGESIZE: c_int = 11;
const O_RDONLY: c_int = 0;

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

/// QNX `sync_t`, the storage of both `pthread_mutex_t` and `pthread_cond_t`.
#[repr(C)]
struct SyncT {
    count: u32,
    owner: u32,
}

/// `_NTO_SYNC_NONRECURSIVE`, `_NTO_SYNC_INITIALIZER` on SDP 7.1 and later `nto`.
#[cfg(not(any(target_env = "nto71", target_env = "nto71_iosock", target_os = "qnx")))]
const MUTEX_INIT: SyncT = SyncT {
    count: 0x8000_0000,
    owner: 0xffff_ffff,
};
#[cfg(any(target_env = "nto71", target_env = "nto71_iosock"))]
const MUTEX_INIT: SyncT = SyncT {
    count: 0x8200_0000,
    owner: 0,
};
#[cfg(target_os = "qnx")]
const MUTEX_INIT: SyncT = SyncT {
    count: 0x8000_0000,
    owner: 0,
};

/// A condition variable initialized with `CLOCK_MONOTONIC` in its clock field.
const COND_INIT: SyncT = SyncT {
    count: CLOCK_MONOTONIC as u32,
    owner: 0xffff_fffb,
};

extern "C" {
    fn clock_gettime(clock_id: c_int, tp: *mut Timespec) -> c_int;
    fn pthread_mutex_lock(mutex: *mut SyncT) -> c_int;
    fn pthread_mutex_unlock(mutex: *mut SyncT) -> c_int;
    fn pthread_cond_timedwait(
        cond: *mut SyncT,
        mutex: *mut SyncT,
        abstime: *const Timespec,
    ) -> c_int;
    fn pthread_cond_signal(cond: *mut SyncT) -> c_int;
    fn pthread_cond_broadcast(cond: *mut SyncT) -> c_int;
    fn __get_errno_ptr() -> *mut c_int;
    fn open(path: *const u8, flags: c_int, ...) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
    fn close(fd: c_int) -> c_int;
    fn mlock(addr: *const c_void, len: usize) -> c_int;
    fn sysconf(name: c_int) -> c_long;
}

struct Bucket {
    mutex: UnsafeCell<SyncT>,
    cond: UnsafeCell<SyncT>,
}

// SAFETY: the pthread objects are only touched by pthread functions, which are
// thread-safe; the cells give them stable addresses.
unsafe impl Sync for Bucket {}

const BUCKET: Bucket = Bucket {
    mutex: UnsafeCell::new(MUTEX_INIT),
    cond: UnsafeCell::new(COND_INIT),
};
static BUCKETS: [Bucket; 64] = [BUCKET; 64];

fn bucket_for(word: &AtomicU32) -> &'static Bucket {
    // Words are at least 4-aligned; drop the low bits, then fold the address.
    let index = (word.as_ptr() as usize >> 2).wrapping_mul(2654435761) % BUCKETS.len();
    // `index` is reduced modulo the table length, so `get` always succeeds.
    match BUCKETS.get(index) {
        Some(bucket) => bucket,
        None => &BUCKET_FALLBACK,
    }
}

// Reached only if the modulo above were ever wrong; keeps this path panic-free.
static BUCKET_FALLBACK: Bucket = BUCKET;

/// The native platform. Stateless beyond the `const`-initialized bucket table.
#[derive(Clone, Copy, Debug, Default)]
pub struct Native;

const _: () = assert!(!core::mem::needs_drop::<Native>());

fn errno() -> c_int {
    // SAFETY: QNX's errno location for this thread, always valid.
    unsafe { *__get_errno_ptr() }
}

impl Platform for Native {
    fn now(&self) -> Monotonic {
        let mut ts = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `ts` is valid writable storage; a failure leaves zero, which
        // only shortens a wait.
        unsafe { clock_gettime(CLOCK_MONOTONIC, &mut ts) };
        let seconds = u64::try_from(ts.tv_sec).unwrap_or(0);
        let nanos = u64::try_from(ts.tv_nsec).unwrap_or(0);
        Monotonic(seconds.wrapping_mul(NANOS_PER_SECOND).wrapping_add(nanos))
    }

    fn unix_time_ms(&self) -> Option<u64> {
        let mut ts = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `ts` is valid writable storage.
        if unsafe { clock_gettime(CLOCK_REALTIME, &mut ts) } != 0 {
            return None;
        }
        let seconds = u64::try_from(ts.tv_sec).ok()?;
        let millis = u64::try_from(ts.tv_nsec).ok()? / 1_000_000;
        seconds.checked_mul(1000)?.checked_add(millis)
    }

    fn wait(&self, word: &AtomicU32, expected: u32, deadline: Option<Monotonic>) -> WaitResult {
        let bucket = bucket_for(word);
        let deadline_spec = deadline.map(|deadline| Timespec {
            tv_sec: (deadline.0 / NANOS_PER_SECOND) as i64,
            tv_nsec: (deadline.0 % NANOS_PER_SECOND) as i64,
        });
        // The lock pairs with the wake path: a wake that stores the new value
        // before locking the bucket cannot slip between this check and the wait.
        // SAFETY: the bucket's objects live forever and are pthread-initialized statically.
        unsafe {
            pthread_mutex_lock(bucket.mutex.get());
        }
        let result = if word.load(Ordering::Acquire) != expected {
            WaitResult::Returned
        } else {
            let abstime = deadline_spec
                .as_ref()
                .map_or(core::ptr::null(), |t| t as *const Timespec);
            // SAFETY: as above; the timespec outlives the call.
            let rc =
                unsafe { pthread_cond_timedwait(bucket.cond.get(), bucket.mutex.get(), abstime) };
            if rc == ETIMEDOUT {
                WaitResult::TimedOut
            } else {
                WaitResult::Returned
            }
        };
        // SAFETY: the bucket mutex is held by this thread here.
        unsafe {
            pthread_mutex_unlock(bucket.mutex.get());
        }
        result
    }

    fn wake_one(&self, word: &AtomicU32) {
        let bucket = bucket_for(word);
        // SAFETY: statically initialized pthread objects; lock held only around the signal.
        unsafe {
            pthread_mutex_lock(bucket.mutex.get());
            pthread_cond_signal(bucket.cond.get());
            pthread_mutex_unlock(bucket.mutex.get());
        }
    }

    fn wake_all(&self, word: &AtomicU32) {
        let bucket = bucket_for(word);
        // SAFETY: statically initialized pthread objects; lock held only around the broadcast.
        unsafe {
            pthread_mutex_lock(bucket.mutex.get());
            pthread_cond_broadcast(bucket.cond.get());
            pthread_mutex_unlock(bucket.mutex.get());
        }
    }
}

impl PlatformInit for Native {
    fn entropy(&self, out: &mut [u8]) -> Result<(), PlatformError> {
        let failed = |code| PlatformError {
            op: PlatformOp::Entropy,
            code,
        };
        // SAFETY: the path is a valid NUL-terminated string.
        let fd = unsafe { open(c"/dev/urandom".as_ptr().cast(), O_RDONLY) };
        if fd < 0 {
            return Err(failed(errno()));
        }
        let mut filled = 0_usize;
        while filled < out.len() {
            let rest = out.get_mut(filled..).ok_or(failed(0))?;
            // SAFETY: `rest` is writable for `rest.len()` bytes.
            let got = unsafe { read(fd, rest.as_mut_ptr().cast(), rest.len()) };
            if got <= 0 {
                let code = errno();
                // SAFETY: `fd` is an open descriptor owned here.
                unsafe { close(fd) };
                return Err(failed(code));
            }
            filled = filled.wrapping_add(got.unsigned_abs());
        }
        // SAFETY: `fd` is an open descriptor owned here.
        unsafe { close(fd) };
        Ok(())
    }

    unsafe fn prefault(&self, ptr: *mut u8, len: usize) -> Result<(), PlatformError> {
        // SAFETY: plain libc call.
        let page = unsafe { sysconf(SC_PAGESIZE) };
        let page = usize::try_from(page).unwrap_or(0);
        let invalid = PlatformError {
            op: PlatformOp::InvalidArgument,
            code: 0,
        };
        if page == 0 || !page.is_power_of_two() {
            return Err(invalid);
        }
        let mask = page.wrapping_sub(1);
        let start = (ptr as usize) & !mask;
        let end = (ptr as usize).checked_add(len).ok_or(invalid)?;
        let aligned_end = end.checked_add(mask).ok_or(invalid)? & !mask;
        let mut cursor = ptr as usize;
        while cursor < end {
            // SAFETY: `cursor` stays in the caller's writable range.
            unsafe { core::ptr::write_volatile(cursor as *mut u8, 0) };
            cursor = cursor.checked_add(page).ok_or(invalid)?;
        }
        if len != 0 {
            // SAFETY: `end - 1` is inside the range because `len != 0`.
            unsafe { core::ptr::write_volatile(end.wrapping_sub(1) as *mut u8, 0) };
        }
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
        self
    }
}
