// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Permanent decoded storage with explicit ownership of each initialized value.

use super::StartupError;
use crate::{
    memory::StartupMemory,
    wire::{StartupSlot, Wire, WireError},
};
use core::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    ptr::NonNull,
};

/// Only the owning route/table's unique slot obligation permits access.
/// There is no safe value accessor and no automatic destructor for these handles.
pub(crate) struct DecodedSlot<T: 'static> {
    // Safety invariant: nonnull, aligned storage for one T in a permanent typed
    // allocation. Its extent is distinct from other nonzero-size slot extents.
    // ZST elements may have equal addresses but remain distinct logical slots.
    value: NonNull<T>,
    // Safety invariant: true implies a valid owned T at value. False forbids
    // typed reads and destruction; it may cover vacant, leaked, or in-progress
    // initialization/destruction storage. Mutation requires the slot's exclusive
    // unpublished/releasing obligation; published leases only read true slots.
    initialized: UnsafeCell<bool>,
}

impl<T: 'static> DecodedSlot<T> {
    /// Initializes vacant storage through the existing exact-slot capability.
    ///
    /// # Safety
    /// Caller must own this slot exclusively, with no existing value borrow,
    /// throughout the call and the entire lifetime of the returned mutable
    /// reference. Exclude every other read, decode, initialization, destruction,
    /// publication and slot return for that interval, including callback reentry.
    ///
    /// Success returns this slot's valid T and sets initialized=true. An
    /// initializer error/unwind does not publish a T or set the flag; a safe
    /// initializer that writes then unwinds may leak that value. A previously
    /// initialized T remains valid when no initializer is called.
    pub(crate) unsafe fn get_or_init(&self, wire: &dyn Wire<T>) -> Result<&mut T, WireError> {
        // SAFETY: exclusive slot ownership covers the flag and typed storage
        // through the returned borrow. NonNull/typed allocation establish
        // alignment and the full extent, including ZST. MaybeUninit<T> has T's
        // size/alignment and permits the vacant/in-progress byte state.
        unsafe {
            if !*self.initialized.get() {
                let slot = &mut *self.value.as_ptr().cast::<MaybeUninit<T>>();
                let ready = wire.init_slot(StartupSlot::new(slot));
                let value = ready.into_value();
                if !core::ptr::eq(value, self.value.as_ptr()) {
                    return Err(WireError::Malformed(
                        "initializer returned a different slot",
                    ));
                }
                // Ready::into_value's exact-location postcondition, checked
                // against this pointer, proves initialized T independently of
                // safe Wire semantics. The initialization borrow ends here.
                *self.initialized.get() = true;
            }
            // initialized=true and the caller's continuing exclusive obligation
            // permit this &mut T for precisely the caller-governed borrow.
            Ok(&mut *self.value.as_ptr())
        }
    }

    /// # Safety
    /// Caller must hold this slot's live published/reply read obligation for
    /// the entire returned borrow. Exclude flag/value mutation, clear, reuse,
    /// and slot return throughout. This does not extend the owner's lifetime.
    /// Returns Some only for initialized storage; None creates no T reference.
    pub(crate) unsafe fn get(&self) -> Option<&T> {
        // SAFETY: the read obligation excludes flag/value mutation and keeps
        // the typed extent live. True is the private validity invariant; false
        // is checked before creating any reference to possibly vacant bytes.
        unsafe {
            if *self.initialized.get() {
                Some(&*self.value.as_ptr())
            } else {
                None
            }
        }
    }

    /// Destroys owned resources while retaining the slot's permanent backing.
    ///
    /// # Safety
    /// Caller must exclusively own this index after all value borrows end.
    /// Withhold the index from acquisition/publication throughout destruction,
    /// including reentry. On unwind it must not return the index to the pool.
    /// On normal return a drop-bearing value is vacant; no-drop T remains valid
    /// and reusable without a runtime initializer. No backing storage is freed.
    pub(crate) unsafe fn clear(&self) {
        // SAFETY: the unique releasing obligation excludes all views and other
        // flag accesses. Clearing the flag precedes arbitrary T::drop, so even
        // an unwind cannot authorize a second drop. The caller withholds this
        // index until the destructor completes; reentry cannot acquire it.
        unsafe {
            if mem::needs_drop::<T>() && *self.initialized.get() {
                *self.initialized.get() = false;
                core::ptr::drop_in_place(self.value.as_ptr());
            }
        }
    }
}

/// Owns completed setup seeds until the containing route is fully installed.
/// This guard is kept inside a prepared-owner wrapper that exposes no usable
/// route/table until it consumes the guard. Forgetting the wrapper only leaks.
pub(crate) struct InitialValues<T: 'static> {
    // Safety invariant: this guard is the sole cleanup owner for these slot
    // values, and no runtime/read/producer obligation is reachable for them.
    // Their containing unpublished wrapper may inspect reports, never values.
    slots: &'static [DecodedSlot<T>],
}

impl<T: 'static> InitialValues<T> {
    /// # Safety-usable invariant
    /// Returns the exact permanent handles owned by this guard, without granting
    /// value access or transferring cleanup. All value methods remain unsafe.
    /// The containing prepared owner retains this guard until installation ends.
    pub(crate) fn slots(&self) -> &'static [DecodedSlot<T>] {
        self.slots
    }

    /// # Safety-usable invariant
    /// Consumes the only setup cleanup obligation without destroying any value.
    /// After this call, the installed route/table's slot protocol is the only
    /// owner that may destroy values. Calling this early can leak, not duplicate
    /// cleanup or expose a safe value reference.
    pub(crate) fn commit(self) {
        mem::forget(self);
    }
}

impl<T: 'static> Drop for InitialValues<T> {
    fn drop(&mut self) {
        for slot in self.slots {
            // SAFETY: this guard's unpublished-owner invariant excludes every
            // runtime/view obligation. A completed initializer sets the flag;
            // clear ignores vacant/in-progress storage and clears before Drop.
            // An unwinding destructor can leak remaining seeds, never drop any
            // twice or publish their backing. No pool index is returned here.
            unsafe { slot.clear() };
        }
    }
}

/// # Safety-usable invariant
/// Success returns sole cleanup ownership of exactly count initialized values
/// in disjoint permanent typed slots. The guard must stay inside a prepared
/// owner until that owner is installed. Completed seeds are cleaned on normal
/// error; an unwinding initializer/destructor may leak but cannot double-drop.
/// Count zero is empty. Typed storage preserves nonnull alignment for ZSTs.
pub(crate) fn initialize<T: Send + Sync + 'static>(
    memory: &StartupMemory,
    wire: &'static dyn Wire<T>,
    count: usize,
) -> Result<InitialValues<T>, StartupError> {
    let storage = memory.slice::<T>(count)?;
    let mut handles = memory.fixed_vec::<DecodedSlot<T>>(count)?;
    for cell in storage.iter_mut() {
        handles.push(DecodedSlot {
            value: NonNull::from(cell).cast(),
            initialized: UnsafeCell::new(false),
        })?;
    }
    // No element reference is used again after its raw handle is recorded.
    // StartupMemory owns permanent backing; this guard owns only the values.
    let guard = InitialValues {
        slots: handles.into_static_slice(),
    };
    for slot in guard.slots {
        // SAFETY: initialize owns every typed element and no owner/view has
        // escaped. iter_mut supplied disjoint elements (including logical ZST
        // slots); this temporary T borrow ends before any next initializer.
        unsafe { slot.get_or_init(wire) }
            .map_err(|_| StartupError::InvalidConfig("wire slot initialization mismatch"))?;
    }
    Ok(guard)
}

#[cfg(test)]
#[path = "decoded_tests.rs"]
mod tests;
