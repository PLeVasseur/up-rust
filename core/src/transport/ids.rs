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
//! an atomic counter mixed with SplitMix64. Time comes from the runtime wall
//! clock. IDs are unique, not secret (design 2.2.2).

use crate::platform::atomic::{AtomicU64, Ordering};

use crate::platform::Platform;
use crate::uuid::UUID;

/// Generates UUIDv7 message IDs for one route.
#[derive(Debug)]
pub(crate) struct IdGenerator {
    state: AtomicU64,
    last_millis: AtomicU64,
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
            state: AtomicU64::new(lsb ^ msb.rotate_left(17)),
            last_millis: AtomicU64::new(first_millis),
        }
    }

    pub(crate) fn next_id(&self, platform: &dyn Platform) -> Option<UUID> {
        const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;
        let millis = match platform.unix_time_ms() {
            Some(millis) => {
                self.last_millis.store(millis, Ordering::Relaxed);
                millis
            }
            // IDs keep working on a failed read: reuse the last good time.
            None => self.last_millis.load(Ordering::Relaxed),
        };
        let counter = self.state.fetch_add(GOLDEN, Ordering::Relaxed);
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
