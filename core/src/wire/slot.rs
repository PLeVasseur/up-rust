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

//! The first valid value of a startup slot.
//!
//! Decode writes into a slot through `&mut T`, so every slot must hold a valid
//! value before any frame arrives. `Wire::init_slot` provides it, and a slot is
//! built through a pointer so that a large value never crosses the stack.

use core::marker::PhantomData;
use core::mem::{self, MaybeUninit};
use core::ptr;

/// One slot's storage during initialization. The only way to make a [`Ready`].
// [impl->req~ssr-unsafe-wire-untrusted~1]
pub struct StartupSlot<'s, T> {
    // Safety invariant: exclusive storage for one T, not yet promised initialized;
    // only consuming write/zeroed may turn this capability into Ready.
    slot: &'s mut MaybeUninit<T>,
    // Invariant in `'s`: without this a stored `Ready<'static, T>` could be
    // returned for a different, shorter-lived slot.
    _lifetime: PhantomData<fn(&'s ()) -> &'s ()>,
}

impl<T> core::fmt::Debug for StartupSlot<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StartupSlot").finish_non_exhaustive()
    }
}

/// Proof that a [`StartupSlot`] now holds a valid `T`. Safe code cannot forge one.
// [impl->req~ssr-unsafe-wire-untrusted~1]
pub struct Ready<'s, T> {
    // Safety invariant: exclusive initialized T in the exact originating
    // StartupSlot's storage; no safe constructor or mutation can replace location.
    value: &'s mut T,
    _lifetime: PhantomData<fn(&'s ()) -> &'s ()>,
}

impl<T> core::fmt::Debug for Ready<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Ready").finish_non_exhaustive()
    }
}

impl<'s, T> StartupSlot<'s, T> {
    pub(crate) fn new(slot: &'s mut MaybeUninit<T>) -> Self {
        Self {
            slot,
            _lifetime: PhantomData,
        }
    }

    /// Writes `value` into the slot.
    ///
    /// `T` crosses the stack here, which is fine for any value that fits on one.
    /// For the very large case use [`Self::zeroed`].
    /// # Safety-usable invariant
    /// Consumes the exact storage capability, initializes it with value and
    /// returns Ready for that same location/lifetime. No duplicate owner is made.
    pub fn write(self, value: T) -> Ready<'s, T> {
        Ready {
            value: self.slot.write(value),
            _lifetime: PhantomData,
        }
    }

    /// Initializes the slot to all zero bytes, in place. Nothing crosses the stack.
    /// # Safety-usable invariant
    /// Consumes the exact storage capability and returns Ready pointing there;
    /// all-zero bytes form a valid T under the unsafe ZeroSlot implementer contract.
    pub fn zeroed(self) -> Ready<'s, T>
    where
        T: ZeroSlot,
    {
        // SAFETY: the pointer covers exactly one `T`, the `&mut` makes it
        // exclusive, and `T: ZeroSlot` promises all-zero bytes are a valid `T`.
        let value = unsafe {
            ptr::write_bytes(self.slot.as_mut_ptr(), 0, 1);
            self.slot.assume_init_mut()
        };
        Ready {
            value,
            _lifetime: PhantomData,
        }
    }
}

impl<'s, T> Ready<'s, T> {
    /// The slot's initialized value, for the SDK only.
    /// # Safety-usable invariant
    /// Returns the exact initialized location/lifetime carried by this Ready,
    /// consuming its capability. Caller-provided Wire code cannot substitute a
    /// shorter-lived Ready because the private lifetime marker is invariant.
    pub(crate) fn into_value(self) -> &'s mut T {
        self.value
    }
}

/// All-zero bytes are a valid value of this type.
///
/// Written by `#[derive(ZeroSlot)]`, never by hand: the derive checks every field.
///
/// # Safety
///
/// Implementations must guarantee that a value whose every byte is zero is a
/// valid instance of `Self`. No padding, pointers or uninhabited states may
/// make all-zero invalid.
pub unsafe trait ZeroSlot: Sized {}

macro_rules! impl_zero_slot {
    ($($ty:ty),* $(,)?) => {
        $(
            // SAFETY: integer, float and `bool` types accept all-zero bytes.
            unsafe impl ZeroSlot for $ty {}
        )*
    };
}

impl_zero_slot!(
    (),
    u8,
    i8,
    u16,
    i16,
    u32,
    i32,
    u64,
    i64,
    u128,
    i128,
    usize,
    isize,
    f32,
    f64,
    bool,
);

// SAFETY: an array of all-zero `T`s is valid when all-zero `T` is valid.
unsafe impl<T: ZeroSlot, const N: usize> ZeroSlot for [T; N] {}

// SAFETY: `MaybeUninit` accepts any bytes.
unsafe impl<T> ZeroSlot for MaybeUninit<T> {}

/// A `StartupSlot` for contract tests in `up-rust`; not a stable API.
#[doc(hidden)]
#[cfg(up_rust_test_hooks)]
pub fn __contract_slot<T>(slot: &mut MaybeUninit<T>) -> StartupSlot<'_, T> {
    StartupSlot::new(slot)
}

/// The value behind a `Ready`, for contract tests in `up-rust`; not a stable API.
#[doc(hidden)]
#[cfg(up_rust_test_hooks)]
pub fn __contract_value<T>(ready: Ready<'_, T>) -> &mut T {
    ready.into_value()
}

/// Compile-time check that `T` is zeroable, used by the generated `ZeroSlot` derive.
#[doc(hidden)]
pub const fn assert_zero_slot<T: ZeroSlot>() {}

/// Compile-time assertion that a `ZeroSlot` type has no drop glue, so a slot
/// that is never dropped loses nothing. Used by the derive.
#[doc(hidden)]
pub const fn assert_no_drop<T>() {
    assert!(
        !mem::needs_drop::<T>(),
        "a startup-slot payload cannot have drop glue"
    );
}
