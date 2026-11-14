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

//! A wake signal: the piece of a condition variable the SDK actually needs.
//!
//! One epoch word and one waiter count. `notify` bumps the epoch and makes the
//! wake call only when someone is waiting, so a delivery with no waiter costs no
//! system call. A waiter takes a [`WaitToken`] *before* checking its condition, so
//! a wake between "check the queue" and "go to sleep" is never lost. A token
//! remembers which signal issued it. `const`, no `Drop`, no heap, usable from a
//! `static`, and shareable across transports (R5).

use crate::platform::atomic::{AtomicU32, Ordering};
#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "nto",
    target_os = "qnx"
))]
use core::time::Duration;

use crate::platform::{Monotonic, Platform, WaitResult};

/// The outcome of a wait on a [`Signal`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalWait {
    /// The epoch moved. The caller should re-check its condition.
    Notified,
    /// The deadline passed with no notification.
    TimedOut,
    /// The token came from a different [`Signal`], so there is no epoch to compare
    /// and nothing was waited for. Take a token from this signal.
    WrongSignal,
}

/// A snapshot of a [`Signal`]'s epoch, taken before polling for work.
///
/// It is tied to the signal that issued it. Waiting on another signal returns
/// [`SignalWait::WrongSignal`] at once, instead of sleeping on the wrong word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitToken {
    epoch: u32,
    signal: usize,
}

/// A wake signal for one route, waiter or set of waiters.
#[derive(Debug)]
pub struct Signal {
    epoch: AtomicU32,
    waiters: AtomicU32,
}

impl Signal {
    /// A new signal. Usable in a `static`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            epoch: AtomicU32::new(0),
            waiters: AtomicU32::new(0),
        }
    }

    /// The identity a token is tied to.
    fn address(&self) -> usize {
        core::ptr::from_ref(self) as usize
    }

    /// Takes a token. Take this *before* checking the condition.
    #[must_use]
    pub fn token(&self) -> WaitToken {
        WaitToken {
            epoch: self.epoch.load(Ordering::Acquire),
            signal: self.address(),
        }
    }

    /// Nonblocking identity/epoch observation, including service-now and zero
    /// caller-budget paths. None means pending, not permission to sleep without
    /// wait_on's registration/compare handshake. No waiter counter is changed.
    pub(crate) fn inspect(&self, token: WaitToken) -> Option<SignalWait> {
        if token.signal != self.address() { return Some(SignalWait::WrongSignal); }
        if self.epoch.load(Ordering::Acquire) != token.epoch {
            Some(SignalWait::Notified)
        } else { None }
    }

    /// Blocks until the epoch moves past `token` or `deadline` passes.
    ///
    /// Returns immediately when a notification already happened after `token`,
    /// and with [`SignalWait::WrongSignal`] when `token` came from another signal.
    pub fn wait_on(
        &self,
        platform: &dyn Platform,
        token: WaitToken,
        deadline: Option<Monotonic>,
    ) -> SignalWait {
        if token.signal != self.address() {
            return SignalWait::WrongSignal;
        }
        loop {
            // This pair and the pair in `notify_on` are all `SeqCst`. Either the
            // notifier sees this waiter and wakes it, or this waiter sees the new
            // epoch. The platform wait compares the word again before sleeping.
            self.waiters.fetch_add(1, Ordering::SeqCst);
            let result = if self.epoch.load(Ordering::SeqCst) == token.epoch {
                Some(platform.wait(&self.epoch, token.epoch, deadline))
            } else {
                None
            };
            self.waiters.fetch_sub(1, Ordering::SeqCst);
            match result {
                None => return SignalWait::Notified,
                Some(WaitResult::TimedOut) if self.epoch.load(Ordering::Acquire) == token.epoch => {
                    return SignalWait::TimedOut;
                }
                Some(WaitResult::Returned | WaitResult::TimedOut) => {}
            }
        }
    }

    /// Bumps the epoch and wakes every waiter, making no wake call when there is
    /// none. No heap on any path.
    pub fn notify_on(&self, platform: &dyn Platform) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        if self.waiters.load(Ordering::SeqCst) != 0 {
            platform.wake_all(&self.epoch);
        }
    }
}

impl Default for Signal {
    fn default() -> Self {
        Self::new()
    }
}

const _: () = assert!(!core::mem::needs_drop::<Signal>());

/// The native-platform spelling used by ordinary code. Waits are always bounded
/// (R40), and the platform never appears in the signature (R29, R45).
#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "nto",
    target_os = "qnx"
))]
impl Signal {
    /// Blocks until the epoch moves past `token` or `timeout` elapses.
    pub fn wait(&self, token: WaitToken, timeout: Duration) -> SignalWait {
        let platform = crate::platform::Native;
        let nanos = u64::try_from(timeout.as_nanos()).unwrap_or(u64::MAX);
        let deadline = Platform::now(&platform).saturating_add(nanos);
        self.wait_on(&platform, token, Some(deadline))
    }

    /// Bumps the epoch and wakes every waiter.
    pub fn notify(&self) {
        self.notify_on(&crate::platform::Native);
    }
}
