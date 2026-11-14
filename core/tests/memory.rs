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

//! `StartupMemory`, `StartupRegion`, `FixedVec` and the region source (R42, R15).
#![allow(missing_docs)]

mod support;

use std::mem::{align_of, size_of, MaybeUninit};
use std::sync::atomic::{AtomicUsize, Ordering};

use support::{assert_no_heap, CountingAllocator};
use up_rust_core::memory::{RegionSource, StartupError, StartupMemory, StartupRegion};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Leaked bytes that are never initialized here: `Box::new_uninit_slice` writes
/// nothing. `vec![MaybeUninit::uninit(); len]` stores every element one at a time,
/// which under Miri is over ten million interpreted stores for the 10 MiB region
/// below, and more than an hour.
fn leaked_bytes(len: usize) -> &'static mut [MaybeUninit<u8>] {
    Box::leak(Box::new_uninit_slice(len))
}

#[test]
fn placement_is_aligned_in_bounds_and_disjoint() {
    let memory = StartupMemory::new(leaked_bytes(4096)).unwrap();
    let first = memory.place(1_u64).unwrap() as *mut u64 as usize;
    let second = memory.place(2_u8).unwrap() as *mut u8 as usize;
    let third = memory.place(3_u128).unwrap() as *mut u128 as usize;
    assert_eq!(first % align_of::<u64>(), 0);
    assert_eq!(third % align_of::<u128>(), 0);
    assert!(second >= first + size_of::<u64>());
    assert!(third > second);
}

#[test]
fn placement_keeps_the_written_value() {
    let memory = StartupMemory::new(leaked_bytes(4096)).unwrap();
    let value = memory.place([7_u32; 4]).unwrap();
    assert_eq!(*value, [7; 4]);
    value[0] = 9;
    assert_eq!(value[0], 9);
}

#[test]
fn exhaustion_is_an_error_and_never_a_panic() {
    let memory = StartupMemory::new(leaked_bytes(512)).unwrap();
    let error = memory.bytes(1 << 30).unwrap_err();
    assert!(matches!(error, StartupError::MemoryExhausted { .. }));
    // A later request that does fit still succeeds.
    assert!(memory.bytes(16).is_ok());
    assert!(matches!(
        memory.fixed_vec::<u64>(usize::MAX),
        Err(StartupError::MemoryExhausted { .. })
    ));
}

#[test]
fn seal_reports_consumption() {
    let memory = StartupMemory::new(leaked_bytes(4096)).unwrap();
    let _ = memory.bytes(100).unwrap();
    let report = memory.seal();
    assert!(report.used > 100 && report.used <= 4096);
    assert_eq!(report.capacity, 4096);
    assert_eq!(report.regions, 1);
}

#[test]
fn a_static_region_can_be_claimed_only_once() {
    static REGION: StartupRegion<4096> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    assert!(memory.bytes(16).is_ok());
    assert_eq!(
        REGION.claim().unwrap_err(),
        StartupError::RegionAlreadyClaimed
    );
}

#[test]
fn a_static_region_too_small_for_its_header_is_refused() {
    static REGION: StartupRegion<8> = StartupRegion::new();
    assert!(matches!(
        REGION.claim(),
        Err(StartupError::MemoryExhausted { .. })
    ));
}

#[test]
fn fixed_vec_refuses_at_capacity() {
    let memory = StartupMemory::new(leaked_bytes(4096)).unwrap();
    let mut vec = memory.fixed_vec::<u32>(2).unwrap();
    vec.push(1).unwrap();
    vec.push(2).unwrap();
    assert_eq!(vec.push(3).unwrap_err(), StartupError::TableFull);
    assert_eq!(vec.as_slice(), [1, 2]);
    assert_eq!(vec.len(), 2);
}

#[test]
fn fixed_vec_is_consumed_by_value_exactly_once_each() {
    let memory = StartupMemory::new(leaked_bytes(4096)).unwrap();
    let mut vec = memory.fixed_vec::<String>(3).unwrap();
    vec.push("a".to_string()).unwrap();
    vec.push("b".to_string()).unwrap();
    let values: Vec<String> = vec.into_iter().collect();
    assert_eq!(values, ["a", "b"]);
}

#[test]
fn fixed_vec_becomes_a_static_slice() {
    let memory = StartupMemory::new(leaked_bytes(4096)).unwrap();
    let mut vec = memory.fixed_vec::<u16>(2).unwrap();
    vec.push(7).unwrap();
    let slice: &'static [u16] = vec.into_static_slice();
    assert_eq!(slice, [7]);
}

#[test]
fn placement_after_seal_refuses_through_a_surviving_handle() {
    // `seal` consumes the caller's handle. Copy the bytes of the handle to show
    // the backstop for a setup that kept internal access: it is still refused.
    let memory = StartupMemory::new(leaked_bytes(4096)).unwrap();
    // SAFETY: the handle has no `Drop`, and it only refers to shared `'static`
    // state, so a bitwise copy creates no second owner of anything.
    let survivor: StartupMemory = unsafe { std::ptr::read(&memory) };
    let _ = memory.seal();
    assert_eq!(survivor.bytes(16).unwrap_err(), StartupError::RegionSealed);
}

#[test]
fn placement_and_refusal_touch_no_heap() {
    let bytes = leaked_bytes(4096);
    assert_no_heap("placement", || {
        let memory = StartupMemory::new(bytes).unwrap();
        let _ = memory.place(1_u64).unwrap();
        let mut vec = memory.fixed_vec::<u64>(1).unwrap();
        vec.push(1).unwrap();
        assert!(vec.push(2).is_err());
        assert!(memory.bytes(1 << 30).is_err());
        let report = memory.seal();
        assert!(report.used <= report.capacity);
    });
}

// The region source (R4): a `std` path needs no size up front.

struct LeakingSource(AtomicUsize);

impl RegionSource for LeakingSource {
    fn region(&'static self, min_len: usize) -> Option<&'static mut [MaybeUninit<u8>]> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Some(leaked_bytes(min_len.max(1024)))
    }
}

static LEAKING: LeakingSource = LeakingSource(AtomicUsize::new(0));
static COUNTED: LeakingSource = LeakingSource(AtomicUsize::new(0));

#[test]
fn a_source_supplies_more_regions_as_requests_arrive() {
    let memory = StartupMemory::from_source(&COUNTED, 256).unwrap();
    assert_eq!(COUNTED.0.load(Ordering::SeqCst), 1);
    // Far larger than the first region: a fresh one is requested.
    let huge = memory.bytes(10 * 1024 * 1024).unwrap();
    assert_eq!(huge.len(), 10 * 1024 * 1024);
    assert_eq!(COUNTED.0.load(Ordering::SeqCst), 2);
    let small = memory.bytes(64).unwrap();
    assert_eq!(small.len(), 64);
    let report = memory.seal();
    assert!(report.regions >= 2);
    assert!(report.capacity >= 10 * 1024 * 1024);
    assert!(report.used <= report.capacity);
}

#[test]
fn memory_stays_writable_across_regions() {
    let memory = StartupMemory::from_source(&LEAKING, 64).unwrap();
    let mut held: Vec<(*mut u8, usize)> = Vec::new();
    for len in [8_usize, 1, 4096, 3, 123, 8192, 7] {
        let bytes = memory.bytes(len).unwrap();
        for (index, slot) in bytes.iter_mut().enumerate() {
            slot.write(index as u8);
        }
        held.push((bytes.as_mut_ptr().cast::<u8>(), len));
    }
    for (ptr, len) in held {
        // SAFETY: each region stays allocated forever; the bytes were written above.
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        for (index, byte) in bytes.iter().enumerate() {
            assert_eq!(*byte, index as u8);
        }
    }
}

#[test]
fn a_static_region_is_a_one_shot_source() {
    static REGION: StartupRegion<4096> = StartupRegion::new();
    let memory = StartupMemory::from_source(&REGION, 128).unwrap();
    assert!(memory.bytes(64).is_ok());
    // The source is spent, so a request that does not fit is refused.
    assert!(matches!(
        memory.bytes(1 << 20),
        Err(StartupError::MemoryExhausted { .. })
    ));
}

#[test]
fn a_source_that_returns_none_is_refused_cleanly() {
    struct Refusing;
    impl RegionSource for Refusing {
        fn region(&'static self, _min_len: usize) -> Option<&'static mut [MaybeUninit<u8>]> {
            None
        }
    }
    static REFUSING: Refusing = Refusing;
    assert!(matches!(
        StartupMemory::from_source(&REFUSING, 64),
        Err(StartupError::MemoryExhausted { .. })
    ));
}

#[test]
fn concurrent_placement_never_shares_a_byte() {
    use std::collections::BTreeSet;
    let memory = StartupMemory::from_source(&LEAKING, 64).unwrap();
    let memory: &'static StartupMemory = Box::leak(Box::new(memory));
    let ranges: Vec<(usize, usize)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(move || {
                    let mut out = Vec::new();
                    for len in [8_usize, 16, 32, 64].iter().cycle().take(200) {
                        let bytes = memory.bytes(*len).unwrap();
                        out.push((bytes.as_ptr() as usize, *len));
                    }
                    out
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect()
    });
    let mut seen = BTreeSet::new();
    for (start, len) in ranges {
        for byte in start..start + len {
            assert!(seen.insert(byte), "byte {byte:#x} was handed out twice");
        }
    }
}
