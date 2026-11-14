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

//! The native platform backend and `Signal`, measured on the running host.
#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(missing_docs)]

mod support;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use support::{assert_no_heap, CountingAllocator};
use up_rust_core::platform::{Monotonic, Native, Platform, PlatformInit, PlatformOp, WaitResult};
use up_rust_core::signal::{Signal, SignalWait};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

static NATIVE: Native = Native;

fn deadline_in(milliseconds: u64) -> Monotonic {
    Native.now().saturating_add(milliseconds * 1_000_000)
}

#[test]
fn the_monotonic_clock_does_not_go_backward() {
    let mut previous = Native.now();
    for _ in 0..1000 {
        let next = Native.now();
        assert!(next >= previous);
        previous = next;
    }
}

#[test]
fn the_wall_clock_tracks_the_system_clock() {
    let wall = Native.unix_time_ms().expect("the wall clock is available");
    let system = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let delta = i128::from(wall) - system as i128;
    assert!(
        delta.abs() < 60_000,
        "wall clock differs from SystemTime by {delta} ms"
    );
}

#[test]
fn wait_times_out_when_the_word_does_not_change() {
    let word = AtomicU32::new(5);
    let start = Native.now();
    let result = Native.wait(&word, 5, Some(deadline_in(30)));
    assert_eq!(result, WaitResult::TimedOut);
    assert!(Native.now().saturating_since(start) >= 25 * 1_000_000);
}

#[test]
fn wait_returns_at_once_when_the_word_is_already_different() {
    let word = AtomicU32::new(6);
    let start = Native.now();
    let result = Native.wait(&word, 5, Some(deadline_in(5_000)));
    assert_eq!(result, WaitResult::Returned);
    assert!(Native.now().saturating_since(start) < 2_000_000_000);
}

#[test]
fn wake_one_wakes_a_waiter() {
    static WORD: AtomicU32 = AtomicU32::new(0);
    static WOKEN: AtomicBool = AtomicBool::new(false);
    let waiter = thread::spawn(|| {
        let started = Native.now();
        while WORD.load(Ordering::Acquire) == 0 {
            Native.wait(&WORD, 0, Some(deadline_in(30_000)));
        }
        WOKEN.store(true, Ordering::Release);
        Native.now().saturating_since(started)
    });
    thread::sleep(Duration::from_millis(20));
    WORD.store(1, Ordering::Release);
    Native.wake_one(&WORD);
    let waited = waiter.join().unwrap();
    assert!(WOKEN.load(Ordering::Acquire));
    // Far inside the 30 s deadline: a `wake_one` that does nothing fails here.
    assert!(
        waited < 10_000_000_000,
        "the waiter was not woken: it waited {waited} ns"
    );
}

#[test]
fn wake_all_wakes_every_waiter() {
    static WORD: AtomicU32 = AtomicU32::new(0);
    let waiters: Vec<_> = (0..4)
        .map(|_| {
            thread::spawn(|| {
                let started = Native.now();
                while WORD.load(Ordering::Acquire) == 0 {
                    Native.wait(&WORD, 0, Some(deadline_in(30_000)));
                }
                Native.now().saturating_since(started)
            })
        })
        .collect();
    thread::sleep(Duration::from_millis(20));
    WORD.store(1, Ordering::Release);
    Native.wake_all(&WORD);
    for waiter in waiters {
        let waited = waiter.join().unwrap();
        // Far inside the 30 s deadline: a `wake_all` that wakes only some waiters
        // or none fails here.
        assert!(
            waited < 10_000_000_000,
            "a waiter was not woken: it waited {waited} ns"
        );
    }
}

#[test]
fn entropy_fills_the_buffer() {
    let mut first = [0_u8; 512];
    let mut second = [0_u8; 512];
    Native.entropy(&mut first).unwrap();
    Native.entropy(&mut second).unwrap();
    assert_ne!(first, second);
    assert_ne!(first, [0_u8; 512]);
}

#[test]
fn prefault_touches_and_locks_a_small_region() {
    let mut pages = vec![0xAA_u8; 3 * 4096].into_boxed_slice();
    // SAFETY: the buffer is writable, owned here, and holds nothing we keep.
    unsafe { Native.prefault(pages.as_mut_ptr(), pages.len()) }.unwrap();
    assert!(pages.iter().all(|byte| *byte == 0 || *byte == 0xAA));
}

#[test]
fn prefault_refuses_an_impossible_range() {
    // SAFETY: the range overflows the address space, so it is refused before use.
    let refused = unsafe { Native.prefault(usize::MAX as *mut u8, 64) }.unwrap_err();
    assert_eq!(refused.op, PlatformOp::InvalidArgument);
}

#[test]
fn the_runtime_face_is_usable_as_a_platform() {
    let runtime: &'static dyn Platform = NATIVE.runtime();
    assert!(runtime.now() >= Monotonic(0));
}

#[test]
fn a_signal_wakes_a_waiter() {
    static SIGNAL: Signal = Signal::new();
    let token = SIGNAL.token();
    let waiter = thread::spawn(move || SIGNAL.wait(token, Duration::from_secs(5)));
    thread::sleep(Duration::from_millis(20));
    SIGNAL.notify();
    assert_eq!(waiter.join().unwrap(), SignalWait::Notified);
}

#[test]
fn a_notification_before_the_wait_is_not_lost() {
    static SIGNAL: Signal = Signal::new();
    let token = SIGNAL.token();
    SIGNAL.notify();
    // The epoch moved after the token was taken, so this returns at once.
    let start = Native.now();
    assert_eq!(
        SIGNAL.wait(token, Duration::from_secs(5)),
        SignalWait::Notified
    );
    assert!(Native.now().saturating_since(start) < 2_000_000_000);
}

#[test]
fn a_signal_wait_times_out() {
    static SIGNAL: Signal = Signal::new();
    let token = SIGNAL.token();
    assert_eq!(
        SIGNAL.wait(token, Duration::from_millis(30)),
        SignalWait::TimedOut
    );
}

/// Delegates to the native platform and counts what a `Signal` asks of it.
#[derive(Debug)]
struct CountingPlatform {
    waiting: AtomicBool,
    wakes: AtomicU32,
}

impl CountingPlatform {
    const fn new() -> Self {
        Self {
            waiting: AtomicBool::new(false),
            wakes: AtomicU32::new(0),
        }
    }
}

impl Platform for CountingPlatform {
    fn now(&self) -> Monotonic {
        Native.now()
    }

    fn unix_time_ms(&self) -> Option<u64> {
        Native.unix_time_ms()
    }

    fn wait(&self, word: &AtomicU32, expected: u32, deadline: Option<Monotonic>) -> WaitResult {
        self.waiting.store(true, Ordering::SeqCst);
        // Absorb spurious returns, so the waiter stays counted until it is woken.
        loop {
            let result = Native.wait(word, expected, deadline);
            if result == WaitResult::TimedOut || word.load(Ordering::SeqCst) != expected {
                return result;
            }
        }
    }

    fn wake_one(&self, word: &AtomicU32) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
        Native.wake_one(word);
    }

    fn wake_all(&self, word: &AtomicU32) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
        Native.wake_all(word);
    }
}

#[test]
fn a_notify_with_no_waiter_makes_no_wake_call() {
    let platform = CountingPlatform::new();
    let signal = Signal::new();
    let token = signal.token();
    signal.notify_on(&platform);
    signal.notify_on(&platform);
    assert_eq!(platform.wakes.load(Ordering::SeqCst), 0);
    // The notification still counts: a later wait returns at once.
    assert_eq!(
        signal.wait_on(&platform, token, Some(deadline_in(5_000))),
        SignalWait::Notified
    );
}

#[test]
fn a_notify_with_a_waiter_makes_one_wake_call() {
    static SIGNAL: Signal = Signal::new();
    static PLATFORM: CountingPlatform = CountingPlatform::new();
    let token = SIGNAL.token();
    let waiter = thread::spawn(move || SIGNAL.wait_on(&PLATFORM, token, Some(deadline_in(5_000))));
    while !PLATFORM.waiting.load(Ordering::SeqCst) {
        thread::yield_now();
    }
    SIGNAL.notify_on(&PLATFORM);
    assert_eq!(waiter.join().unwrap(), SignalWait::Notified);
    assert_eq!(PLATFORM.wakes.load(Ordering::SeqCst), 1);
}

#[test]
fn a_fast_producer_never_loses_a_wake() {
    static SIGNAL: Signal = Signal::new();
    static WORK: AtomicU32 = AtomicU32::new(0);
    const ROUNDS: u32 = 2_000;
    let consumer = thread::spawn(|| {
        let mut seen = 0;
        while seen < ROUNDS {
            let token = SIGNAL.token();
            let available = WORK.load(Ordering::SeqCst);
            if available > seen {
                seen = available;
                continue;
            }
            // A lost wake shows up as this bounded wait timing out with work ready.
            if SIGNAL.wait(token, Duration::from_secs(5)) == SignalWait::TimedOut {
                return Err(WORK.load(Ordering::SeqCst));
            }
        }
        Ok(seen)
    });
    for _ in 0..ROUNDS {
        WORK.fetch_add(1, Ordering::SeqCst);
        SIGNAL.notify();
    }
    assert_eq!(consumer.join().unwrap(), Ok(ROUNDS));
}

#[test]
fn a_token_from_another_signal_is_refused_without_waiting() {
    let first = Signal::new();
    let second = Signal::new();
    let start = Native.now();
    assert_eq!(
        first.wait(second.token(), Duration::from_secs(5)),
        SignalWait::WrongSignal
    );
    assert!(Native.now().saturating_since(start) < 2_000_000_000);
}

#[test]
fn platform_and_signal_touch_no_heap_including_timeouts_and_first_use() {
    static SIGNAL: Signal = Signal::new();
    static WORD: AtomicU32 = AtomicU32::new(0);
    assert_no_heap("clocks", || {
        for _ in 0..1000 {
            let _ = Native.now();
            let _ = Native.unix_time_ms();
        }
    });
    assert_no_heap("signal first use, notify and timeouts", || {
        let token = SIGNAL.token();
        SIGNAL.notify();
        assert_eq!(
            SIGNAL.wait(token, Duration::from_millis(1)),
            SignalWait::Notified
        );
        let token = SIGNAL.token();
        assert_eq!(
            SIGNAL.wait(token, Duration::from_millis(5)),
            SignalWait::TimedOut
        );
    });
    assert_no_heap("wait and wake", || {
        assert_eq!(
            Native.wait(&WORD, 1, Some(deadline_in(1))),
            WaitResult::Returned
        );
        assert_eq!(
            Native.wait(&WORD, 0, Some(deadline_in(1))),
            WaitResult::TimedOut
        );
        Native.wake_one(&WORD);
        Native.wake_all(&WORD);
    });
}

/// Positive control: the window must be able to see a real allocation.
#[test]
fn the_heap_window_sees_a_box() {
    let (boxed, counts) = support::count_heap(|| Box::new(7_u8));
    assert!(
        counts[0] >= 1,
        "the window cannot see an allocation: {counts:?}"
    );
    drop(boxed);
}

/// Positive control for the macOS gate: the first lock of a `std` mutex boxes a
/// pthread object, so a window over it must report the allocation.
#[cfg(target_os = "macos")]
#[test]
fn the_heap_window_sees_a_lazy_pthread_mutex() {
    let mutex = Box::leak(Box::new(std::sync::Mutex::new(0_u32)));
    let (_, counts) = support::count_heap(|| {
        *mutex.lock().unwrap() += 1;
    });
    assert!(counts[0] >= 1, "the first lock was not seen: {counts:?}");
}

#[test]
fn signal_wake_across_threads_touches_no_heap_on_either_side() {
    static SIGNAL: Signal = Signal::new();
    let token = SIGNAL.token();
    let waiter = thread::spawn(move || {
        assert_no_heap("waiter", || {
            assert_eq!(
                SIGNAL.wait(token, Duration::from_secs(5)),
                SignalWait::Notified
            );
        });
    });
    thread::sleep(Duration::from_millis(20));
    assert_no_heap("notifier", || SIGNAL.notify());
    waiter.join().unwrap();
}
