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

//! Heap-backed startup memory for the `std` path (R15).
//!
//! [`StartupMemory`] takes its memory from a [`RegionSource`]. The `std` path
//! needs no size up front: [`heap_startup_memory`] obtains a fresh boxed slice
//! per request and leaks it for the rest of the process, so a 48 MB route
//! just works. The ASIL path uses a static region or caller bytes instead.

use std::mem::MaybeUninit;

pub use up_rust_core::memory::{
    FixedVec, FixedVecIter, MemoryReport, RegionSource, StartupError, StartupMemory, StartupRegion,
};

/// The smallest region requested from the heap, so small placements share one.
const DEFAULT_REGION_BYTES: usize = 256 * 1024;

/// A [`RegionSource`] that leaks one fresh boxed slice per request.
///
/// Nothing it supplies is ever freed, by design (R38). A failed allocation is
/// refused, it does not abort.
#[derive(Clone, Copy, Debug, Default)]
pub struct LeakedHeapRegions;

impl RegionSource for LeakedHeapRegions {
    fn region(&'static self, min_len: usize) -> Option<&'static mut [MaybeUninit<u8>]> {
        let len = min_len.max(DEFAULT_REGION_BYTES);
        let mut bytes = Vec::<MaybeUninit<u8>>::new();
        bytes.try_reserve_exact(len).ok()?;
        // SAFETY: the reservation above covers `len` elements, and
        // `MaybeUninit<u8>` needs no initialization.
        unsafe { bytes.set_len(len) };
        Some(Box::leak(bytes.into_boxed_slice()))
    }
}

/// The process-wide heap source.
pub static LEAKED_HEAP_REGIONS: LeakedHeapRegions = LeakedHeapRegions;

/// Creates startup memory backed by leaked heap regions (R15).
///
/// Regions are requested as placements need them; no size is chosen up front.
///
/// # Errors
/// Returns [`StartupError::MemoryExhausted`] if the first region cannot be
/// allocated.
pub fn heap_startup_memory() -> Result<StartupMemory, StartupError> {
    StartupMemory::from_source(&LEAKED_HEAP_REGIONS, DEFAULT_REGION_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

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
