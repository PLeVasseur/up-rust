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

//! Storage for one async waker, reserved by core and filled by `up-rust-tokio`.
//!
//! Core stays free of `core::task` and `core::future` (R34). A waker is two
//! machine words of opaque storage plus a state word, and `up-rust-tokio`
//! registers a hook that knows what those words mean. One [`RouteWake`] per call
//! slot and per receive route, so a call does not share a waker with another
//! (design 2.1). The owner of the storage asks the hook to clear it when the
//! `Call` or `Recv` that registered a waker drops, so a late completion can never
//! wake a stale task.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use crate::platform::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// What the async layer's hook is being asked to do.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WakeEvent {
    /// A frame or completion arrived: wake the stored waker, if any.
    Wake,
    /// The owner is finished with this cell: drop any stored waker without waking it.
    Clear,
}

/// A hook installed by the async layer. Called with the cell and the event.
#[doc(hidden)]
pub type WakeHook = fn(&RouteWake, WakeEvent);

/// Process-lifetime storage for one executor registration, reused only after clear.
/// The owning adapter registers before waiting and polls again after registration
/// and every wake: wakes are hints, never evidence that a value is ready. The
/// adapter clears on Call/Recv drop. Completion threads may wake concurrently;
/// the adapter owns one atomic arbitration protocol for all storage accesses.
/// Hooks must honor that protocol, never panic or block, and on a no-heap path
/// must neither acquire nor release heap memory. Core itself never allocates,
/// frees, blocks, or interprets the stored words. Executor behavior (including
/// Tokio's waker clone/drop) is outside core's heap guarantee.
/// Never replace a hook with an incompatible protocol while storage is occupied.
// [impl->req~ssr-async-seam~1]
#[doc(hidden)]
pub struct RouteWake {
    hook: AtomicPtr<()>,
    /// Arbitration word for `storage`, owned by the async layer.
    state: AtomicUsize,
    /// A stored waker's bytes, owned by the async layer. Never interpreted by core.
    storage: UnsafeCell<[MaybeUninit<usize>; 2]>,
}

impl core::fmt::Debug for RouteWake {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RouteWake").finish_non_exhaustive()
    }
}

// SAFETY: `storage` is accessed only by the async layer through its `state`
// arbitration; core never touches it, and every other field is atomic.
unsafe impl Sync for RouteWake {}
// SAFETY: as above.
unsafe impl Send for RouteWake {}

impl RouteWake {
    /// A cell with no hook.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            hook: AtomicPtr::new(core::ptr::null_mut()),
            state: AtomicUsize::new(0),
            storage: UnsafeCell::new([MaybeUninit::uninit(), MaybeUninit::uninit()]),
        }
    }

    /// The arbitration word for [`Self::storage_ptr`], for the async layer.
    ///
    /// # Safety
    /// Only the async layer's waker protocol may touch this word: a stored waker's
    /// validity is decided by it, so a stray write makes the hook read garbage as a
    /// waker. The caller must uphold the one protocol in effect on this cell.
    #[must_use]
    pub unsafe fn state(&self) -> &AtomicUsize {
        &self.state
    }

    /// Where the async layer keeps a waker. Writing through the pointer, or
    /// reading a value from it, is up to the async layer's protocol over
    /// [`Self::state`]. Taking the pointer is harmless, like `UnsafeCell::get`.
    #[must_use]
    pub fn storage_ptr(&self) -> *mut [MaybeUninit<usize>; 2] {
        self.storage.get()
    }

    /// Installs the hook the async layer wants called on completion.
    pub fn install_hook(&self, hook: WakeHook) {
        self.hook.store(hook as *mut (), Ordering::Release);
    }

    /// Forgets a registered waker. Runs when the owning call slot is released.
    pub fn clear(&self) {
        self.notify(WakeEvent::Clear);
    }

    /// Invokes the hook, if any. Heap-free on the synchronous path.
    pub fn wake(&self) {
        self.notify(WakeEvent::Wake);
    }

    fn notify(&self, event: WakeEvent) {
        let hook = self.hook.load(Ordering::Acquire);
        if hook.is_null() {
            return;
        }
        // SAFETY: only `install_hook` writes this pointer, from a `WakeHook`
        // function pointer, which is valid for the whole program.
        let hook: WakeHook = unsafe { core::mem::transmute(hook) };
        hook(self, event);
    }
}

impl Default for RouteWake {
    fn default() -> Self {
        Self::new()
    }
}

const _: () = assert!(!core::mem::needs_drop::<RouteWake>());
