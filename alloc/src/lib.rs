//! Experimental optional no_std + alloc helpers using Rust 1.100.
//!
//! This candidate is not the integrated production SDK.
//!
//! Core still owns checked reservation and initialization sealing. This package
//! neither chooses a backing allocator nor promises that arbitrary standard
//! collection operations satisfy native-runtime heap/panic/resource contracts.
//! Every constructor takes an explicit standard Allocator; no Global default is
//! selected here. Allocation can mean fixed arena reservation, not global heap.
#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::TryReserveError;
use alloc::vec::Vec;
use core::alloc::{AllocError, Allocator, Layout};

/// The common core's checked startup storage contract, unchanged except for the
/// isolated public aligned-reservation candidate. External providers use this
/// public surface rather than private modules or their own arena algorithm.
pub use up_rust_core::memory;

/// Fixed startup-region implementation shared by independent providers.
pub mod fixed;

/// Constructs a vector with fallible initial capacity using the explicit allocator.
///
/// Rust 1.100 stabilizes `Vec::new_in`, while `try_with_capacity_in` is still
/// unstable. This equivalent sequence uses stable `try_reserve_exact`.
///
/// # Errors
/// Returns the standard reservation error on capacity overflow or allocator
/// refusal. No element exists yet, so refusal drops no caller-supplied value.
pub fn try_vec_in<T, A: Allocator>(allocator: A, capacity: usize)
    -> Result<Vec<T, A>, TryReserveError>
{
    let mut values = Vec::new_in(allocator);
    values.try_reserve_exact(capacity)?;
    Ok(values)
}

/// Fallible Box construction using only stable Rust 1.100 primitives.
///
/// This owns one value in the explicit allocator. The value's ordinary Drop
/// still runs when the Box is dropped and can have its own effects; choosing a
/// fixed backing allocator does not qualify arbitrary T for native runtime.
///
/// # Errors
/// On allocation refusal, returns the error, untouched value and allocator.
/// The caller chooses what to do with them; this error path does not drop T.
pub fn try_box_in<T, A: Allocator>(value: T, allocator: A)
    -> Result<Box<T, A>, (AllocError, T, A)>
{
    let memory = match allocator.allocate(Layout::new::<T>()) {
        Ok(memory) => memory,
        Err(error) => return Err((error, value, allocator)),
    };
    let pointer = memory.cast::<T>();
    // SAFETY: Allocator::allocate(Layout::new::<T>()) supplies nonnull storage
    // aligned for T, with >=size_of::<T>() bytes in one valid allocation and
    // exclusive ownership until invalidation. No references/other owners of
    // those bytes were exposed. Even T's zero-size case has nonnull alignment.
    // ptr::write initializes one valid T by moving value, without reading or
    // dropping the old uninitialized bytes. It invokes no user callback.
    unsafe { pointer.as_ptr().write(value) };
    // SAFETY: The preceding write established one initialized T in the block
    // allocated by `allocator` with its exact T layout. Moving the allocator
    // preserves equivalence per the standard Allocator contract, including when
    // A is a reference to an external allocator. No callback, old destructor or
    // fallible operation intervenes. Box::from_raw_in takes sole ownership of
    // both T and its allocation; no raw owner remains in use. For a ZST the
    // pointer remains nonnull/aligned and T still receives exactly one Drop.
    Ok(unsafe { Box::from_raw_in(pointer.as_ptr(), allocator) })
}
