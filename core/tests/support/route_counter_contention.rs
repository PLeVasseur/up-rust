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

//! Threaded regressions for the private production route counters.
//! Loaded only as a cfg(test) child of transport::report.

extern crate std;

use super::{add, Counter, Counters};
use crate::platform::atomic::Ordering;

// Test-only threads/barrier create simultaneous writers; no lock serializes
// add(). Join precedes observation, so the expected totals include every
// completed call. This exercises actual atomics, not an allocator model or
// payload-publication proof. Scheduler coverage and retry bounds stay open.
const WRITERS: usize = 4;
const INCREMENTS_PER_WRITER: u32 = 4096;
const CONTENDED_TOTAL: u32 = WRITERS as u32 * INCREMENTS_PER_WRITER;

fn add_under_contention<const SATURATING: bool>(counter: &Counter<SATURATING>) {
    let start = std::sync::Barrier::new(WRITERS);
    std::thread::scope(|scope| {
        for _ in 0..WRITERS {
            let start = &start;
            scope.spawn(move || {
                start.wait();
                for _ in 0..INCREMENTS_PER_WRITER {
                    add(counter);
                }
            });
        }
    });
}

// Requirement mapping: req~ssr-error-no-silent-loss~3.
// Scoped regression only; G3/OFT closure pending.
#[test]
fn route_fault_counter_contention_loses_no_unsaturated_increments() {
    let counters = Counters::default();
    counters.refused.0.store(7, Ordering::Relaxed);
    let before = counters.snapshot();
    add_under_contention(&counters.refused);
    let later = counters.snapshot();
    assert_eq!(later.refused, 7 + CONTENDED_TOTAL);
    assert_eq!(later.since(before).refused, Some(CONTENDED_TOTAL));
}

// Requirement mapping: req~ssr-error-no-silent-loss~3.
// Scoped regression only; G3/OFT closure pending.
#[test]
fn route_fault_counter_contention_sticks_at_saturation() {
    let counters = Counters::default();
    counters
        .deadline_missed
        .0
        .store(u32::MAX - CONTENDED_TOTAL / 2, Ordering::Relaxed);
    let before = counters.snapshot();
    add_under_contention(&counters.deadline_missed);
    let saturated = counters.snapshot();
    assert_eq!(saturated.deadline_missed, u32::MAX);
    assert_eq!(saturated.since(before).deadline_missed, None);
    // Exercise concurrent calls after saturation as well as crossing it.
    add_under_contention(&counters.deadline_missed);
    assert_eq!(counters.deadline_missed.get(), u32::MAX);
    assert_eq!(counters.snapshot().since(saturated).deadline_missed, None);
}

// Requirement mapping: req~ssr-error-no-silent-loss~3.
// Scoped regression only; G3/OFT closure pending.
#[test]
fn route_flow_counter_contention_preserves_modular_totals() {
    let counters = Counters::default();
    let initial = u32::MAX - CONTENDED_TOTAL / 2;
    for counter in [&counters.carried, &counters.busy, &counters.parked] {
        counter.0.store(initial, Ordering::Relaxed);
    }
    let before = counters.snapshot();
    for counter in [&counters.carried, &counters.busy, &counters.parked] {
        add_under_contention(counter);
        assert_eq!(counter.get(), initial.wrapping_add(CONTENDED_TOTAL));
    }
    let delta = counters.snapshot().since(before);
    assert_eq!(delta.carried, CONTENDED_TOTAL);
    assert_eq!(delta.busy, CONTENDED_TOTAL);
    assert_eq!(delta.parked, CONTENDED_TOTAL);
}
