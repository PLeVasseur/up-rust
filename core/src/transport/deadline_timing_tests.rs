//! NEW fixtures, UNCOMPILED/UNRUN. Synthetic arithmetic inputs never qualify J.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;

const IDENTITY: TimingIdentity = TimingIdentity {
    sdk_source: [0; 32], compiled_artifact: [0; 32], binding_source: [0; 32],
    platform_source: [0; 32], hardware_and_atomic_abi: [0; 32],
    clock_timer_scheduler: [0; 32], complete_configuration: [0; 32],
};
const PINS: TimingEvidencePins = TimingEvidencePins {
    source_and_binary_correspondence: [0; 32], all_winner_wcet_and_progress: [0; 32],
    activation_and_no_waiter_service: [0; 32], r56_arrival_and_recording: [0; 32],
    r61_pending_and_freshness: [0; 32], counter_visibility_and_read_contract: [0; 32],
    runtime_memory_and_panic: [0; 32], independent_acceptance: [0; 32],
};

// Test-private construction exercises arithmetic only. No catalog entry, backend,
// physical qualification or production constructor is supplied by this helper.
fn synthetic_j(ns: u128) -> ValidatedJ {
    ValidatedJ { ns, pins: PINS, identity: IDENTITY, service_owner: "UNQUALIFIED-TEST",
        read_contract: CounterReadContract::OrderedAfterPublication }
}

// [test->req~ssr-qos-deadline~3] J amendment contribution: a number is not proof.
#[test]
fn proposed_j_and_matching_identifiers_cannot_self_qualify() {
    let proposal = DeadlineConfiguration {
        identity: IDENTITY, arrivals: &[], requested_j_ns: Some(0),
    };
    assert_eq!(qualify(proposal, IDENTITY, "UNQUALIFIED-TEST", &[]),
        Err(TimingGap::QualificationMissing));
}

// [test->req~ssr-qos-deadline~3] Exact reporting arithmetic; no physical bound.
#[test]
fn j_never_changes_gap_threshold_or_quantization() {
    let config = MonitorConfig::new(10_001, 17).unwrap();
    let before = config.report();
    let after = DeadlineTimingReport::qualified(config, synthetic_j(31)).unwrap();
    assert_eq!(after.threshold_ns, Some(before.threshold_ns));
    assert_eq!(after.resolution_ns, Some(before.quantization_tolerance_ns));
    assert_eq!(after.guaranteed_gap_ns, Some(before.detection_bound_ns));
    assert_eq!(after.reporting_bound_ns, before.detection_bound_ns.checked_add(31));
}

// [test->req~ssr-qos-deadline~3] No saturated or wrapped advertised bound.
#[test]
fn unrepresentable_j_sum_is_explicitly_incomplete() {
    let config = MonitorConfig::new(u64::MAX, 0).unwrap();
    assert_eq!(DeadlineTimingReport::qualified(config, synthetic_j(u128::MAX)),
        Err(TimingGap::ArithmeticOverflow));
}

// [test->req~ssr-qos-deadline~3] L retains its complete declared meaning.
#[test]
fn a_small_claimed_l_does_not_hide_recording_delay() {
    let route = ServiceRoute {
        route: RouteId(0), deadline_ns: Some(1024),
        arrival: Some(ArrivalTiming { route: RouteId(0), arrival_latency_ns: 2,
            declared_binding_latency_ns: 2, recording_delay_ns: 1 }),
        rpc_slots: 0, slots: 0, slot_len: 0, depth: 1,
    };
    assert_eq!(monitor_config(&route), Err(TimingGap::ArrivalBudgetMismatch));
}

// [test->req~ssr-qos-deadline~3] [test->req~ssr-atomic-windows~1]
// Exercises real publication storage and counter wrapper; does not certify timing.
#[test]
fn one_time_publication_never_resets_an_already_shared_monitor() {
    static REGION: crate::memory::StartupRegion<65536> = crate::memory::StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let config = MonitorConfig::new(1024, 0).unwrap();
    let report = DeadlineTimingReport::qualified(config, synthetic_j(7)).unwrap();
    let first = memory.place(ArmedDeadline { monitor: DeadlineMonitor::new(config, 0), report }).unwrap();
    first.monitor.arm(10);
    let slot = DeadlineSlot::new();
    slot.publish(first).unwrap();
    let counter = Counter::<true>::default();
    assert!(slot.check(1035, &counter));
    let replacement = memory.place(ArmedDeadline { monitor: DeadlineMonitor::new(config, 9000), report }).unwrap();
    assert_eq!(slot.publish(replacement), Err(TimingGap::AlreadyArmed));
    assert!(!slot.check(1036, &counter));
    assert_eq!(counter.get(), 1);
}
