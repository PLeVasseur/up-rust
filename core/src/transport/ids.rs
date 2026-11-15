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

//! Message IDs without a thread-local RNG, the heap or a panic path.
//!
//! One generator per route, seeded once from init-only entropy, then advanced by
//! a counter through private metadata operations, mixed with SplitMix64. Time
//! comes from the runtime wall clock. IDs are unique, not secret (design 2.2.2).

#[cfg(test)]
use crate::test_support::FixtureResult as _;
use super::metadata::Generator;

use crate::platform::Platform;
use crate::uuid::UUID;

/// Generates UUIDv7 message IDs for one route.
#[derive(Debug)]
pub(crate) struct IdGenerator {
    metadata: Generator,
}

impl IdGenerator {
    /// A generator seeded from entropy read once during initialization.
    pub(crate) fn seeded(seed: [u8; 16], first_millis: u64) -> Self {
        let mut a = [0_u8; 8];
        let mut b = [0_u8; 8];
        let (left, right) = seed.split_at(8);
        a.copy_from_slice(left);
        b.copy_from_slice(right);
        let msb = u64::from_ne_bytes(a);
        let lsb = u64::from_ne_bytes(b);
        Self {
            metadata: Generator::new(lsb ^ msb.rotate_left(17), first_millis),
        }
    }

    pub(crate) fn next_id(&self, platform: &dyn Platform) -> Option<UUID> {
        const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;
        // The callback precedes metadata access and can reenter the same route.
        let sampled = platform.unix_time_ms();
        let millis = self.metadata.clock(sampled);
        // Intentionally a separate operation: preserve the existing interleaving.
        let counter = self.metadata.advance(GOLDEN);
        let mix = |mut z: u64| {
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        let rand_a = mix(counter.wrapping_add(GOLDEN));
        let rand_b = mix(counter.wrapping_add(GOLDEN.rotate_left(11)));
        let msb = ((millis & 0xFFFF_FFFF_FFFF) << 16) | 0x7000 | (rand_a & 0x0FFF);
        let lsb = 0x8000_0000_0000_0000 | (rand_b & 0x3FFF_FFFF_FFFF_FFFF);
        UUID::try_from_wire_pair(msb, lsb).ok()
    }
}

#[cfg(test)]
mod scoped_exclusion_tests {
    use super::*;
    use crate::platform::{Monotonic, WaitResult};
    use crate::platform::atomic::{AtomicBool, AtomicU32, Ordering};

    struct Clock<'a> { generator: &'a IdGenerator, entered: AtomicBool }
    impl Platform for Clock<'_> {
        fn now(&self) -> Monotonic { Monotonic(0) }
        fn unix_time_ms(&self) -> Option<u64> {
            if self.entered.swap(true, Ordering::Relaxed) { Some((1_u64 << 45) + 7) }
            else { assert!(self.generator.next_id(self).is_some()); None }
        }
        fn wait(&self, _: &AtomicU32, _: u32, _: Option<Monotonic>) -> WaitResult { WaitResult::Returned }
        fn wake_one(&self, _: &AtomicU32) {}
        fn wake_all(&self, _: &AtomicU32) {}
    }
    #[test]
    fn reentrant_platform_clock_updates_shared_fallback_before_outer_id() {
        let generator = IdGenerator::seeded([7; 16], 1);
        let clock = Clock { generator: &generator, entered: AtomicBool::new(false) };
        let id = generator.next_id(&clock).require_fixture();
        assert_eq!(id.as_u64_pair().0 >> 16, (1_u64 << 45) + 7);
    }
}
