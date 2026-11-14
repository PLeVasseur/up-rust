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

//! Route reports and counters: what each route got, and what it has carried.

use crate::platform::atomic::{AtomicU32, Ordering};

use super::envelope::Envelope;
use super::error::RouteName;
use super::qos::Qos;
use crate::wire::RuntimeHeap;

/// How a send route hands frames to its transport, reported at startup (R16).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SendMode {
    /// The payload is written once into a loan the transport lent.
    Lend,
    /// A value is written once into a startup slot and copied at handoff.
    SlotCopy,
    /// Already-encoded bytes go straight to the transport; no slot holds the payload.
    Direct,
}

/// Identifies one route for [`counters`](super::TransportRuntime::counters).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct RouteId(pub(crate) u32);

/// One route's outcome, reported by `TransportRuntime::routes`.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct RouteReport {
    /// Separate detection/quantization and qualified reporting bound. Missing
    /// evidence is explicit; a requested number cannot validate J.
    pub deadline_timing: super::deadline_timing::DeadlineTimingReport,
    /// Configured hottest-slot warning margin. Target timing still requires qualification.
    pub lifetime_policy: super::LifetimePolicy,
    /// The route.
    pub route: &'static RouteName,
    /// The route's id, for reading its counters.
    pub id: RouteId,
    /// How it sends, for send routes.
    pub send_mode: Option<SendMode>,
    /// Reserved startup slots.
    pub slots: usize,
    /// Bytes reserved per slot.
    pub slot_len: usize,
    /// The envelope.
    pub envelope: Envelope,
    /// The QoS this route was admitted with.
    pub qos: Qos,
    /// What the route's middleware does with the heap.
    pub heap: RuntimeHeap,
    /// Whether any admitted payload is native, including a native member of a
    /// mixed carrier route. This classifies heap policy independently of the
    /// physical loan mode; forwarding native bytes does not serialize them.
    pub native_payload: bool,
    /// Binding deviations that apply to this route.
    pub deviations: &'static [&'static super::Deviation],
}

/// One coherent observation of an armed receive route's deadline state.
///
/// Obtain this from `TransportRuntime::deadline_report`. Every field is copied
/// within one SDK metadata interval. It does not qualify clock/arrival evidence,
/// autonomous service, reporting delay J, restart custody or the target backend.
/// The owning runtime and RouteId identify a process-local, never-reset monitor;
/// this value is not a persistent restart identity or a mutation capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct DeadlineReport {
    /// Full activation sample or latest accepted forward arrival sample.
    /// Units and nonwrapping epoch are the owning runtime's monotonic clock.
    /// This need not identify the latest physical arrival when samples are missing.
    pub origin_ns: u64,
    /// Accumulated gap credits folded from accepted timestamp observations.
    /// Arithmetic is exact while `count_exhausted` is false, including at
    /// `u128::MAX`; a further credit saturates and latches that flag.
    ///
    /// `missing_sample` permanently invalidates a complete/exact physical-gap
    /// interpretation, even after later valid arrivals. With missing samples this
    /// value is neither a universal upper nor lower bound on physical gaps.
    /// Even both health flags being false does not authenticate the clock or
    /// ingress: physical-route interpretation requires qualified clock, queue
    /// arrival and recording contracts, which these flags do not establish.
    pub missed_gaps: u128,
    /// Sticky evidence that an arrival timestamp was unavailable. It does not
    /// invent an arrival, rearm the monitor or clear after a later valid sample.
    pub missing_sample: bool,
    /// Sticky evidence that an increment beyond u128::MAX was required.
    /// The count remains at its maximum and existing arrivals continue to rearm.
    pub count_exhausted: bool,
}

/// Fixed counters for one route, read without touching the heap.
///
/// Each field is an independent atomic observation, not a globally atomic
/// snapshot. `carried`, `busy` and `parked` wrap modulo 2^32; every fault counter
/// saturates at `u32::MAX`. Compare reads of the same route using [`Self::since`].
/// Flow deltas are exact only with fewer than 2^32 increments between reads
/// (about 429 seconds at ten million increments/second). At saturation a fault
/// delta is unknown, even when both samples equal the maximum.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct RouteCounters {
    /// Frames sent or received. On a receive route this includes frames evicted
    /// later under `DropOldest`, in the SDK's queue or the binding's own.
    /// Wrapping flow counter.
    pub carried: u32,
    /// Refused because every startup slot was in use.
    /// Saturating fault counter.
    pub slots_exhausted: u32,
    /// Refused because the transport was busy. On a receive route: a frame refused
    /// because its `Wait` queue was full, which its sender keeps and retries; it is
    /// not a drop.
    /// Wrapping flow counter.
    pub busy: u32,
    /// Refused or dropped for any other reason.
    /// Saturating fault counter.
    pub refused: u32,
    /// Frames refused under contention (the bounded eviction retry ran out).
    /// Saturating fault counter.
    pub refused_contention: u32,
    /// Inbound frames dropped as stale.
    /// Saturating fault counter.
    pub stale_peer: u32,
    /// Queued frames discarded to make room (`DropOldest`), in the SDK's queue or,
    /// as reported by the binding, in the binding's own.
    /// Saturating fault counter.
    pub dropped_oldest: u32,
    /// Inbound frames refused because the queue was full (`DropNewest`), in the
    /// SDK's queue or, as reported by the binding, in the binding's own.
    /// Saturating fault counter.
    pub dropped_newest: u32,
    /// Frames dropped because their time-to-live had passed.
    /// Saturating fault counter.
    pub expired: u32,
    /// Frames refused because the wall clock could not be read.
    /// Saturating fault counter.
    pub clock_unavailable: u32,
    /// Qualifying gaps in binding-queue arrivals, including later-refused frames.
    /// Legacy u32 diagnostic mirror; it can lag the joint full-width publication.
    /// Use `TransportRuntime::deadline_report` for the coherent deadline state.
    /// Reads here remain per-field, and timing/observation evidence is separate.
    /// Saturating fault counter.
    pub deadline_missed: u32,
    /// Requests from clients outside the deployment list.
    /// Saturating fault counter.
    pub unknown_client: u32,
    /// Frames a carrier kept to send later.
    /// Wrapping flow counter.
    pub parked: u32,
}

/// Per-field changes from [`RouteCounters::since`].
/// Saturated or backwards fault observations are unknown (`None`); flow fields
/// use modular subtraction. This value does not make a snapshot globally atomic.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct RouteCounterDelta {
    /// Change in [`RouteCounters::carried`].
    pub carried: u32,
    /// Change in [`RouteCounters::slots_exhausted`]. Unknown at saturation.
    pub slots_exhausted: Option<u32>,
    /// Change in [`RouteCounters::busy`].
    pub busy: u32,
    /// Change in [`RouteCounters::refused`]. Unknown at saturation.
    pub refused: Option<u32>,
    /// Change in [`RouteCounters::refused_contention`]. Unknown at saturation.
    pub refused_contention: Option<u32>,
    /// Change in [`RouteCounters::stale_peer`]. Unknown at saturation.
    pub stale_peer: Option<u32>,
    /// Change in [`RouteCounters::dropped_oldest`]. Unknown at saturation.
    pub dropped_oldest: Option<u32>,
    /// Change in [`RouteCounters::dropped_newest`]. Unknown at saturation.
    pub dropped_newest: Option<u32>,
    /// Change in [`RouteCounters::expired`]. Unknown at saturation.
    pub expired: Option<u32>,
    /// Change in [`RouteCounters::clock_unavailable`]. Unknown at saturation.
    pub clock_unavailable: Option<u32>,
    /// Change in [`RouteCounters::deadline_missed`]. Unknown at saturation.
    pub deadline_missed: Option<u32>,
    /// Change in [`RouteCounters::unknown_client`]. Unknown at saturation.
    pub unknown_client: Option<u32>,
    /// Change in [`RouteCounters::parked`].
    pub parked: u32,
}

impl RouteCounters {
    /// Compares two observations of the same route, earlier then later.
    /// See the wrapping window and saturation limits on [`RouteCounters`].
    #[must_use]
    pub fn since(self, earlier: Self) -> RouteCounterDelta {
        let fault = |later: u32, before: u32| {
            if later == u32::MAX {
                None
            } else {
                later.checked_sub(before)
            }
        };
        RouteCounterDelta {
            carried: self.carried.wrapping_sub(earlier.carried),
            slots_exhausted: fault(self.slots_exhausted, earlier.slots_exhausted),
            busy: self.busy.wrapping_sub(earlier.busy),
            refused: fault(self.refused, earlier.refused),
            refused_contention: fault(self.refused_contention, earlier.refused_contention),
            stale_peer: fault(self.stale_peer, earlier.stale_peer),
            dropped_oldest: fault(self.dropped_oldest, earlier.dropped_oldest),
            dropped_newest: fault(self.dropped_newest, earlier.dropped_newest),
            expired: fault(self.expired, earlier.expired),
            clock_unavailable: fault(self.clock_unavailable, earlier.clock_unavailable),
            deadline_missed: fault(self.deadline_missed, earlier.deadline_missed),
            unknown_client: fault(self.unknown_client, earlier.unknown_client),
            parked: self.parked.wrapping_sub(earlier.parked),
        }
    }
}

/// Internal choice is fixed by the field type; all callers retain `add(&field)`.
#[derive(Debug, Default)]
pub(crate) struct Counter<const SATURATING: bool>(AtomicU32);

impl<const SATURATING: bool> Counter<SATURATING> {
    pub(crate) fn get(&self) -> u32 {
        // Stage 4 plan 11.3, counters: each independent figure is relaxed.
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Default)]
pub(crate) struct Counters {
    pub(crate) carried: Counter<false>,
    pub(crate) slots_exhausted: Counter<true>,
    pub(crate) busy: Counter<false>,
    pub(crate) refused: Counter<true>,
    pub(crate) refused_contention: Counter<true>,
    pub(crate) stale_peer: Counter<true>,
    pub(crate) dropped_oldest: Counter<true>,
    pub(crate) dropped_newest: Counter<true>,
    pub(crate) expired: Counter<true>,
    pub(crate) clock_unavailable: Counter<true>,
    pub(crate) deadline_missed: Counter<true>,
    pub(crate) unknown_client: Counter<true>,
    pub(crate) parked: Counter<false>,
}

impl Counters {
    pub(crate) fn snapshot(&self) -> RouteCounters {
        RouteCounters {
            carried: self.carried.get(),
            slots_exhausted: self.slots_exhausted.get(),
            busy: self.busy.get(),
            refused: self.refused.get(),
            refused_contention: self.refused_contention.get(),
            stale_peer: self.stale_peer.get(),
            dropped_oldest: self.dropped_oldest.get(),
            dropped_newest: self.dropped_newest.get(),
            expired: self.expired.get(),
            clock_unavailable: self.clock_unavailable.get(),
            deadline_missed: self.deadline_missed.get(),
            unknown_client: self.unknown_client.get(),
            parked: self.parked.get(),
        }
    }
}

pub(crate) fn add<const SATURATING: bool>(counter: &Counter<SATURATING>) {
    // Stage 4 plan 11.3, counters: modification order of this word is enough;
    // none of these relaxed accesses publishes payload or ownership.
    if !SATURATING {
        counter.0.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let mut value = counter.0.load(Ordering::Relaxed);
    loop {
        let Some(next) = value.checked_add(1) else {
            return;
        };
        match counter
            .0
            .compare_exchange_weak(value, next, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => return,
            Err(actual) => value = actual,
        }
    }
}

#[cfg(test)]
mod counter_tests {
    use super::*;

    // Neutral spec req~ssr-error-no-silent-loss~3; actual u32 CAS path, not a model.
    // Requirement mapping: req~ssr-error-no-silent-loss~3.
    // Scoped regression only; G3/OFT closure pending.
    #[test]
    fn route_fault_counter_increments_and_sticks_at_maximum() {
        let counter = Counter::<true>::default();
        add(&counter);
        assert_eq!(counter.get(), 1);
        counter.0.store(u32::MAX - 1, Ordering::Relaxed);
        add(&counter);
        assert_eq!(counter.get(), u32::MAX);
        add(&counter);
        assert_eq!(counter.get(), u32::MAX);
    }

    // Requirement mapping: req~ssr-error-no-silent-loss~3.
    // Scoped regression only; G3/OFT closure pending.
    #[test]
    fn route_flow_counters_wrap_and_deltas_cross_zero() {
        let counters = Counters::default();
        for counter in [&counters.carried, &counters.busy, &counters.parked] {
            counter.0.store(u32::MAX - 1, Ordering::Relaxed);
        }
        let before = counters.snapshot();
        for counter in [&counters.carried, &counters.busy, &counters.parked] {
            for _ in 0..3 {
                add(counter);
            }
            assert_eq!(counter.get(), 1);
        }
        let delta = counters.snapshot().since(before);
        assert_eq!((delta.carried, delta.busy, delta.parked), (3, 3, 3));
    }

    // Requirement mapping: req~ssr-error-no-silent-loss~3.
    // Scoped regression only; G3/OFT closure pending.
    #[test]
    fn route_all_fault_fields_saturate_and_saturated_deltas_are_unknown() {
        let counters = Counters::default();
        let before = counters.snapshot();
        for counter in [
            &counters.slots_exhausted,
            &counters.refused,
            &counters.refused_contention,
            &counters.stale_peer,
            &counters.dropped_oldest,
            &counters.dropped_newest,
            &counters.expired,
            &counters.clock_unavailable,
            &counters.deadline_missed,
            &counters.unknown_client,
        ] {
            counter.0.store(u32::MAX - 1, Ordering::Relaxed);
            add(counter);
            add(counter);
            assert_eq!(counter.get(), u32::MAX);
        }
        let later = counters.snapshot();
        for delta in [later.since(before), later.since(later)] {
            assert_eq!(delta.slots_exhausted, None);
            assert_eq!(delta.refused, None);
            assert_eq!(delta.refused_contention, None);
            assert_eq!(delta.stale_peer, None);
            assert_eq!(delta.dropped_oldest, None);
            assert_eq!(delta.dropped_newest, None);
            assert_eq!(delta.expired, None);
            assert_eq!(delta.clock_unavailable, None);
            assert_eq!(delta.deadline_missed, None);
            assert_eq!(delta.unknown_client, None);
        }
    }

    // Requirement mapping: req~ssr-error-no-silent-loss~3.
    // Scoped regression only; G3/OFT closure pending.
    #[test]
    fn route_unsaturated_fault_deltas_are_exact_and_backwards_are_unknown() {
        let earlier = RouteCounters {
            refused: 4,
            ..RouteCounters::default()
        };
        let later = RouteCounters {
            refused: 9,
            ..RouteCounters::default()
        };
        assert_eq!(later.since(earlier).refused, Some(5));
        assert_eq!(earlier.since(later).refused, None);
        assert_eq!(earlier.since(earlier).slots_exhausted, Some(0));
    }
}

#[cfg(test)]
#[path = "../../tests/support/route_counter_contention.rs"]
mod counter_contention_tests;
