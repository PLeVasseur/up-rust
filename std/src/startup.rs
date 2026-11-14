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

//! Automatic, heap-backed startup memory for applications using `std`.
//!
//! The source supplies another region when the SDK needs one during setup.
//! Applications need not guess an arena byte count. Core enforces the existing
//! initialization/runtime boundary; this source does not bypass sealing.

use std::mem::MaybeUninit;

pub use up_rust_core::memory::{
    FixedVec, FixedVecIter, MemoryReport, RegionSource, StartupError, StartupMemory, StartupRegion,
};

/// Minimum region size: small startup placements share one allocation.
const DEFAULT_REGION_BYTES: usize = 256 * 1024;

/// A startup [`RegionSource`] retaining each supplied allocation for process life.
///
/// Allocation failure reported by the global allocator, or capacity overflow,
/// returns `None`. No backing allocation is reclaimed after a region is issued.
/// Actual allocator capacity can exceed the returned slice length; those excess
/// bytes remain retained too. `MemoryReport` counts only regions linked into
/// the startup-memory chain. It excludes allocations abandoned when concurrent
/// growth loses the linking race, unused Vec capacity, and allocator overhead;
/// it is not a report of the allocator's complete physical footprint.
#[derive(Clone, Copy, Debug, Default)]
pub struct LeakedHeapRegions;

impl RegionSource for LeakedHeapRegions {
    /// # Safety-usable invariant
    /// Success returns one fresh exclusive static mutable slice of exactly
    /// `max(min_len, 256 KiB)` valid `MaybeUninit<u8>` elements in one allocation.
    /// It does not overlap any previously returned live region. Ownership of
    /// that allocation is permanently relinquished; dropping the reference
    /// cannot deallocate or reissue it. Failure returns no storage capability.
    fn region(&'static self, min_len: usize) -> Option<&'static mut [MaybeUninit<u8>]> {
        let len = min_len.max(DEFAULT_REGION_BYTES);
        let mut bytes = Vec::<MaybeUninit<u8>>::new();
        bytes.try_reserve_exact(len).ok()?;
        // Vec's documented guarantees: successful try_reserve_exact gives
        // capacity >= len, and push cannot reallocate with available capacity.
        // Each iteration pushes a valid MaybeUninit value; no caller callback,
        // destructor or unchecked length/initialization operation is involved.
        for _ in 0..len {
            bytes.push(MaybeUninit::uninit());
        }
        // Vec::leak consumes the sole owner and does not shrink or reallocate
        // (documented since Rust 1.57; this crate's MSRV is 1.88). The element
        // type has no borrowed data, so the returned borrow may be static.
        Some(bytes.leak())
    }
}

/// Process-wide source for automatic startup regions.
pub static LEAKED_HEAP_REGIONS: LeakedHeapRegions = LeakedHeapRegions;

/// Creates growable startup memory without an application-selected byte count.
///
/// Requests and growth occur only while the core memory handle admits setup.
/// This is the `std` startup-allocation path, not a static-memory safety profile.
///
/// # Errors
/// Returns [`StartupError::MemoryExhausted`] if the first region cannot be
/// supplied. Later placement failures are returned by the requesting setup API.
pub fn heap_startup_memory() -> Result<StartupMemory, StartupError> {
    StartupMemory::from_source(&LEAKED_HEAP_REGIONS, DEFAULT_REGION_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_request_returns_the_documented_minimum() {
        let region = LEAKED_HEAP_REGIONS.region(0).unwrap();
        assert_eq!(region.len(), DEFAULT_REGION_BYTES);
    }

    #[test]
    fn capacity_overflow_returns_no_region() {
        assert!(LEAKED_HEAP_REGIONS.region(usize::MAX).is_none());
    }

    #[test]
    fn concurrent_sources_keep_disjoint_initialized_values_alive() {
        let (first, second) = std::thread::scope(|scope| {
            let first = scope.spawn(|| heap_startup_memory().unwrap().place([17_u8; 64]).unwrap());
            let second = scope.spawn(|| heap_startup_memory().unwrap().place([29_u8; 64]).unwrap());
            (first.join().unwrap(), second.join().unwrap())
        });
        // Both references survive the setup handles that supplied their storage.
        assert_eq!(first, &[17; 64]);
        assert_eq!(second, &[29; 64]);
        let first_start = first.as_ptr() as usize;
        let second_start = second.as_ptr() as usize;
        assert!(first_start + first.len() <= second_start || second_start + second.len() <= first_start);
        first[0] = 41;
        assert_eq!(second, &[29; 64]);
    }

    #[test]
    fn the_heap_source_needs_no_size_up_front() {
        let memory = heap_startup_memory().unwrap();
        // A 48 MB slot just works: the source supplies a region for it.
        const LARGE: usize = 48 * 1024 * 1024;
        let slot = memory.bytes(LARGE).unwrap();
        assert_eq!(slot.len(), LARGE);
        let value = memory.place([9_u8; 32]).unwrap();
        assert_eq!(value[0], 9);
        let table = memory.fixed_vec::<u32>(16).unwrap();
        assert!(table.is_empty());
        let report = memory.seal();
        assert!(report.regions >= 2);
        assert!(report.capacity >= LARGE);
        assert!(report.used <= report.capacity);
    }

    #[test]
    fn each_request_for_more_than_the_default_gets_its_own_region() {
        let memory = heap_startup_memory().unwrap();
        let first = memory.bytes(DEFAULT_REGION_BYTES * 2).unwrap();
        let second = memory.bytes(DEFAULT_REGION_BYTES * 2).unwrap();
        let first_end = first.as_ptr() as usize + first.len();
        let second_start = second.as_ptr() as usize;
        assert!(
            second_start >= first_end || second_start + second.len() <= first.as_ptr() as usize
        );
        assert!(memory.seal().regions >= 3);
    }
}
