//! Fixed-region allocator shared by external platform providers.
//!
//! This is a local allocator: it imposes no Send/Sync requirement on consumers.
//! Backing is static/caller-selected StartupRegion memory; there is no global
//! allocator fallback, private core access, OS allocation or second arena.

use core::alloc::{AllocError, Allocator, Layout};
use core::cell::Cell;
use core::fmt;
use core::ptr::NonNull;
use crate::memory::{MemoryReport, StartupError, StartupMemory, StartupRegion};

/// A claimed fixed startup region exposed through Rust's standard Allocator.
///
/// Shared references are equivalent allocators, as specified by Allocator.
/// The allocator is not Clone/Copy and does not implement unstable allocator
/// extension traits. Its Cell makes shared cross-thread use unavailable.
/// Dropping or forgetting this handle never returns the permanent region.
pub struct FixedStartupAllocator {
    // Safety invariant: Some is the unique owned StartupMemory handle from a
    // successful fixed StartupRegion::claim (never from a custom RegionSource).
    // None carries no placement authority. All handed-out regions are permanent,
    // so neither moving this field nor sealing it invalidates issued blocks.
    memory: Cell<Option<StartupMemory>>,
}

impl fmt::Debug for FixedStartupAllocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FixedStartupAllocator").finish_non_exhaustive()
    }
}

impl FixedStartupAllocator {
    /// Claims a fixed region during initialization. Each region can be claimed once.
    ///
    /// # Errors
    /// Returns the core's already-claimed or insufficient-header-storage refusal.
    pub fn claim<const N: usize>(region: &'static StartupRegion<N>) -> Result<Self, StartupError> {
        let memory = region.claim()?;
        // Establish the field invariant with the fixed-source constructor only.
        Ok(Self { memory: Cell::new(Some(memory)) })
    }

    /// Permanently ends allocation from this handle and reports linked core bytes.
    ///
    /// Existing allocations and their initialized values remain valid. Future
    /// allocation/growth attempts fail; pre-reserved Vec capacity remains usable.
    /// Returns None on a repeated call. This is allocator sealing, not admission
    /// of an SDK transport runtime or a proof of arbitrary collection behavior.
    pub fn seal(&self) -> Option<MemoryReport> {
        // Removing the handle prevents any future reservation. Core performs its
        // actual consuming seal; permanent regions outlive every outstanding
        // allocator reference. No borrowed owner or live allocation is touched.
        self.memory.take().map(StartupMemory::seal)
    }

    /// # Safety-usable invariant
    /// Success issues exactly layout.size() exclusive, nonnull, properly aligned
    /// bytes in one permanent core reservation; no already-issued block is
    /// touched. Failure exposes no block. The fixed-source core path cannot
    /// call user code or unwind; this method restores its sole StartupMemory
    /// handle before returning. No global allocation or backing free occurs.
    fn reserve(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let memory = self.memory.take().ok_or(AllocError)?;
        let result = memory.aligned_bytes(layout.size(), layout.align());
        // The core method's fixed-source no-unwind/no-callback contract means
        // no safe reentrant seal/reservation can run while this handle is out.
        // Restore the same fixed handle on both success and refusal.
        self.memory.set(Some(memory));
        let bytes = result.map_err(|_| AllocError)?;
        let length = bytes.len();
        // Raw conversion relinquishes this exclusive MaybeUninit slice. It
        // neither reads bytes nor claims initialized u8 values; the allocated
        // extent and provenance come entirely from core's checked reservation.
        let pointer = NonNull::from(bytes).cast::<u8>();
        Ok(NonNull::slice_from_raw_parts(pointer, length))
    }
}

// SAFETY: `reserve` documents exact allocation extent/alignment, permanent
// validity, nonoverlap, no access to live issued blocks and no-unwind behavior.
// All constructors enforce a fixed core source; no caller callback participates.
// The handle owns no backing and has no Drop glue that can unwind. Moving,
// reference reborrowing and sealing preserve storage; no clone/equivalence API
// other than standard references is added. Cell prevents concurrent shared
// access and raw fixed-source calls cannot reenter. `deallocate` is deliberately
// a no-op: logical ownership ends by the trait contract, but bytes are never
// reissued. Every returned length equals requested size, so fitting layouts
// have that exact size/alignment. The standard default allocate_zeroed/grow/
// grow_zeroed/shrink implementations use these operations: fresh reservations
// are disjoint, successful reallocation copies only the documented extent and
// invalidates the old owner, while failure leaves the old allocation untouched.
// The trait's unsafe caller size/fitting preconditions discharge their internal
// assertions. Zero-sized blocks remain nonnull/aligned; zero-byte copies need
// no initialized payload. No operation reaches a process-global allocator.
unsafe impl Allocator for FixedStartupAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.reserve(layout)
    }

    /// # Safety
    /// The caller must provide a currently allocated block of this allocator or
    /// an equivalent reference allocator, and a fitting layout, as required by
    /// Allocator::deallocate. This logically invalidates caller ownership but
    /// permanently retains backing; no bytes are accessed or made reusable.
    unsafe fn deallocate(&self, _pointer: NonNull<u8>, _layout: Layout) {}
}
