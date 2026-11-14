// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Unrun source fixtures: policy/ownership integration, not native acceptance.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
extern crate std;

use super::*;
use crate::communication::two_mode::Publisher;
use crate::memory::StartupRegion;
use crate::transport::pool::{IndexPoolSetup, IndexSlot, LifetimePolicy, LifetimeState, PoolError};
use crate::transport::tests::{NullHandoff, TestUri, TestWire, CLOCK};
use std::sync::Barrier;
use std::thread;
use std::vec::Vec;

const A: u64 = 0x0123_4567_89ab_cdef;
const B: u64 = 0x0765_4321_fedc_ba98;
static POLICY_A: LifetimePolicy = match LifetimePolicy::new(A) {
    Ok(p) => p, Err(_) => LifetimePolicy::DEFAULT,
};
static POLICY_B: LifetimePolicy = match LifetimePolicy::new(B) {
    Ok(p) => p, Err(_) => LifetimePolicy::DEFAULT,
};

#[test]
fn native32_policy_publication_never_observes_mixed_halves() {
    let publication = PublishedLifetimePolicy::new();
    publication.publish(&POLICY_A);
    let start = Barrier::new(5);
    thread::scope(|scope| {
        scope.spawn(|| {
            start.wait();
            for _ in 0..2048 {
                publication.publish(&POLICY_B);
                publication.publish(&POLICY_A);
            }
        });
        for _ in 0..4 {
            scope.spawn(|| {
                start.wait();
                for _ in 0..4096 {
                    assert!(matches!(publication.get().warning_remaining(), A | B));
                }
            });
        }
    });
    assert_eq!(publication.get(), POLICY_A);
}

#[test]
fn native32_old_policy_target_remains_valid_after_republication() {
    let publication = PublishedLifetimePolicy::new();
    publication.publish(&POLICY_A);
    let old = publication.current.load(Ordering::Acquire);
    publication.publish(&POLICY_B);
    // SAFETY: ptr::read requires an aligned pointer valid to read one initialized
    // LifetimePolicy. publish's Safety-usable invariant and the Acquire capture
    // supply these properties for static POLICY_A, including provenance and the
    // full typed extent. Replacement neither mutates nor reclaims the target.
    // LifetimePolicy is Copy; reading it creates no extra drop/ownership duty.
    assert_eq!(unsafe { old.read() }, POLICY_A);
    assert_eq!(publication.get(), POLICY_B);
}

#[test]
fn native32_policy_is_the_same_in_setup_route_report_and_runtime() {
    static REGION: StartupRegion<262144> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap()
        .with_lifetime_policy(POLICY_A).unwrap();
    let shared = setup.shared;
    let identity = setup.id();
    let role = Publisher::<u32>::on(&setup, &TestUri(7), 0x8001)
        .wire(TestWire { fail_encode: false }).build().unwrap();
    let runtime = setup.enter_runtime().unwrap();
    assert_eq!(identity, runtime.core.id);
    assert_eq!(shared.lifetime_policy(), POLICY_A);
    assert_eq!(runtime.routes().len(), 1);
    for route in runtime.routes() {
        assert_eq!(route.lifetime_policy, POLICY_A);
        assert_eq!(runtime.pool_lifetime(route.id), Some(LifetimeState::Healthy));
    }
    let _active = role.activate(&runtime).unwrap();
    assert!(!core::mem::needs_drop::<TransportRuntime>());
    let copy = runtime;
    let _ = runtime;
    assert_eq!(copy.core.id, identity);
    assert_eq!(copy.core.shared.lifetime_policy(), POLICY_A);
}

#[test]
fn native32_policy_change_after_route_reservation_keeps_existing_refusal() {
    static REGION: StartupRegion<16384> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    let shared = setup.shared;
    setup.next_route_id().unwrap();
    assert!(matches!(setup.with_lifetime_policy(POLICY_A),
        Err(StartupError::InvalidConfig("set lifetime policy before declaring routes"))));
    assert_eq!(shared.lifetime_policy(), LifetimePolicy::DEFAULT);
}

fn exhaust(memory: &StartupMemory) {
    let crate::memory::StartupError::MemoryExhausted { remaining, .. } =
        memory.bytes(usize::MAX).unwrap_err()
    else { panic!("expected bounded memory exhaustion"); };
    memory.bytes(remaining).unwrap();
}

#[test]
fn native32_same_policy_needs_no_space_changed_policy_failure_is_atomic() {
    static REGION: StartupRegion<16384> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    let shared = setup.shared;
    exhaust(&memory);
    let setup = setup.with_lifetime_policy(LifetimePolicy::DEFAULT).unwrap();
    assert!(matches!(setup.with_lifetime_policy(POLICY_A),
        Err(StartupError::Memory(crate::memory::StartupError::MemoryExhausted { .. }))));
    assert_eq!(shared.lifetime_policy(), LifetimePolicy::DEFAULT);
}

#[test]
fn native32_concurrent_setup_identities_survive_abandoned_setups() {
    static REGION: StartupRegion<131072> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let start = Barrier::new(8);
    let ids = thread::scope(|scope| {
        let mut workers = Vec::new();
        for _ in 0..8 {
            let memory = &memory;
            let start = &start;
            workers.push(scope.spawn(move || {
                start.wait();
                let mut ids = Vec::new();
                for _ in 0..8 {
                    let setup = TransportSetup::handoff_on(memory, &CLOCK, NullHandoff).unwrap();
                    let id = setup.id();
                    assert_eq!(id, core::ptr::from_ref(setup.core).addr() as u64);
                    assert_ne!(id, 0);
                    ids.push(id);
                    drop(setup); // abandoned setup never returns its placed CoreCell
                }
                ids
            }));
        }
        let mut ids = Vec::new();
        for worker in workers { ids.extend(worker.join().unwrap()); }
        ids
    });
    let mut unique = ids;
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 64);
    let next = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    assert!(!unique.contains(&next.id()));
}

struct FailStart;
impl BindingSetup for FailStart {
    type Running = NullHandoff;
    fn name(&self) -> &'static str { "publication-fail-start" }
    fn capabilities(&self) -> Capabilities { NullHandoff.capabilities() }
    fn qos_support(&self, kind: RouteKind) -> QosSupport { NullHandoff.qos_support(kind) }
    fn prepare_route(&mut self, route: &RouteDeclaration<'_>) -> Result<PreparedRoute, BindingRefusal> {
        NullHandoff.prepare_route(route)
    }
    fn into_running(self, _: RuntimePort) -> Result<NullHandoff, BindingRefusal> {
        Err(BindingRefusal("injected start refusal"))
    }
}

#[test]
fn native32_failed_start_cannot_recycle_identity_or_policy() {
    static REGION: StartupRegion<32768> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let failed = TransportSetup::handoff_on(&memory, &CLOCK, FailStart).unwrap()
        .with_lifetime_policy(POLICY_A).unwrap();
    let failed_id = failed.id();
    let retained_shared = failed.shared;
    assert!(matches!(failed.enter_runtime(), Err(StartupError::Binding { .. })));
    let next = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    assert_ne!(next.id(), failed_id);
    assert_eq!(retained_shared.lifetime_policy(), POLICY_A);
    assert_eq!(next.shared.lifetime_policy(), LifetimePolicy::DEFAULT);
}

#[test]
fn native32_published_policy_warning_and_retirement_keep_held_owners() {
    // Reuse the actual accepted pool. This tests its connection to publication,
    // not a substitute algorithm or a fresh qualification of the existing pool.
    let publication = PublishedLifetimePolicy::new();
    publication.publish(&POLICY_A);
    let mut cells = [IndexSlot::new(), IndexSlot::new()];
    let last_free = u64::MAX - 1;
    let mut setup = IndexPoolSetup::new(&mut cells).unwrap()
        .with_lifetime_policy(publication.get());
    setup.__seed_free_for_test(0, last_free - 2 * A - 2).unwrap();
    setup.__seed_free_for_test(1, last_free - 2).unwrap();
    let pool = setup.enter_runtime();
    assert_eq!(pool.lifetime(), LifetimeState::Healthy);
    let held = pool.try_acquire().unwrap();
    assert_eq!(pool.lifetime(), LifetimeState::LifetimeMarginLow);
    let closing = pool.try_acquire().unwrap();
    let before_close = Barrier::new(2);
    let after_close = Barrier::new(2);
    thread::scope(|scope| {
        scope.spawn(|| {
            assert_eq!(pool.lifetime(), LifetimeState::LifetimeMarginLow);
            before_close.wait();
            after_close.wait();
            assert_eq!(pool.lifetime(), LifetimeState::Retired);
        });
        before_close.wait();
        drop(closing);
        after_close.wait();
    });
    assert_eq!(held.index(), 0);
    assert!(matches!(pool.try_acquire(), Err(PoolError::PoolRetired)));
    drop(held);
    assert_eq!(pool.lifetime(), LifetimeState::Retired);
}

#[test]
fn native32_wrong_transport_activation_is_still_refused() {
    static REGION: StartupRegion<524288> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    // Both setups must exist before either enters runtime, under R2/R3.
    let first = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    let second = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    let role = Publisher::<u32>::on(&first, &TestUri(9), 0x8001)
        .wire(TestWire { fail_encode: false }).build().unwrap();
    let first_runtime = first.enter_runtime().unwrap();
    let second_runtime = second.enter_runtime().unwrap();
    assert_ne!(first_runtime.core.id, second_runtime.core.id);
    assert!(matches!(role.activate(&second_runtime), Err(StartupError::WrongTransport)));
}
