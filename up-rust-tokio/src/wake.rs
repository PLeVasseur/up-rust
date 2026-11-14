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

//! The waker stored in a core [`RouteWake`].
//!
//! Core reserves two words and a state word for each cell and never looks inside
//! them, so this module owns the protocol. The state word arbitrates who may
//! touch the storage, the classic `AtomicWaker` protocol: a thread holds either
//! `REGISTERING` (writing a new waker) or `WAKING` (taking the stored one), and a
//! wake that lands during registration is handed to the registering thread.

use std::mem::{align_of, needs_drop, size_of};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Waker;

use up_rust::transport::wake::{RouteWake, WakeEvent};

const REGISTERING: usize = 0b001;
const WAKING: usize = 0b010;
/// `storage` holds a valid `Waker`.
const OCCUPIED: usize = 0b100;

const _: () = assert!(
    size_of::<Waker>() == size_of::<usize>() * 2
        && align_of::<Waker>() <= align_of::<usize>()
        && needs_drop::<Waker>(),
);

fn storage(cell: &RouteWake) -> *mut Waker {
    cell.storage_ptr().cast::<Waker>()
}

fn state_word(cell: &RouteWake) -> &AtomicUsize {
    // SAFETY: this module is the one waker protocol on a `RouteWake`: core only
    // calls its hook, and every use of this word below follows that protocol.
    unsafe { cell.state() }
}

/// Registers `waker` to be woken by the next completion on `cell`.
pub(crate) fn register(cell: &RouteWake, waker: &Waker) {
    cell.install_hook(hook);
    let word = state_word(cell);
    let mut state = word.load(Ordering::Acquire);
    loop {
        if state & (REGISTERING | WAKING) != 0 {
            // A wake is in flight, or another task owns this route: never lose it.
            waker.wake_by_ref();
            return;
        }
        match word.compare_exchange_weak(
            state,
            state | REGISTERING,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => break,
            Err(actual) => state = actual,
        }
    }
    // SAFETY: `REGISTERING` is exclusive access to the storage: no hook writes or
    // reads it while the bit is set, and only one registration can hold the bit.
    unsafe {
        if state & OCCUPIED != 0 {
            let existing = &mut *storage(cell);
            existing.clone_from(waker);
        } else {
            storage(cell).write(waker.clone());
        }
    }
    let owned = (state & !REGISTERING) | OCCUPIED;
    match word.compare_exchange(
        state | REGISTERING,
        owned,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => {}
        Err(_) => {
            // A wake arrived while registering; it left the job to this thread.
            // SAFETY: `REGISTERING` is still held; the waker was just written.
            let stored = unsafe { storage(cell).read() };
            word.store(0, Ordering::Release);
            stored.wake();
        }
    }
}

fn hook(cell: &RouteWake, event: WakeEvent) {
    let word = state_word(cell);
    let previous = word.fetch_or(WAKING, Ordering::AcqRel);
    if previous & (REGISTERING | WAKING) != 0 {
        // The registering thread sees `WAKING` and finishes, or another wake is
        // already taking the waker. A `Clear` here races a registration, which
        // only the owning task can run, so it cannot happen on a live future.
        return;
    }
    let taken = if previous & OCCUPIED != 0 {
        // SAFETY: `WAKING` was just taken and `REGISTERING` is clear, so this
        // thread alone may read the storage, and `OCCUPIED` says it holds a waker.
        Some(unsafe { storage(cell).read() })
    } else {
        None
    };
    word.store(0, Ordering::Release);
    if let (Some(waker), WakeEvent::Wake) = (taken.as_ref(), event) {
        waker.wake_by_ref();
    }
    drop(taken);
}
