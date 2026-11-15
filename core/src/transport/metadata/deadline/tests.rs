//! New affected controls, UNRUN. Host model observations do not qualify a target.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn monitor(origin_ns: u64) -> CoherentDeadline {
    CoherentDeadline::new(MonitorConfig::new(1024, 0).unwrap(), origin_ns)
}

// [utest->req~ssr-qos-deadline~3]
#[test]
fn joint_due_arrival_and_report_are_exactly_once() {
    let monitor = monitor(0);
    let legacy = Counter::<true>::default();
    assert!(monitor.check_and_publish(1025, &legacy));
    assert!(!monitor.check_and_publish(2048, &legacy));
    assert_eq!(monitor.report().missed_gaps, 1);
    assert!(!monitor.arrival_and_publish(2048, &legacy));
    let rearmed = monitor.report();
    assert_eq!((rearmed.origin_ns, rearmed.missed_gaps), (2048, 1));
    assert!(monitor.check_and_publish(4096, &legacy));
    assert_eq!(monitor.report().missed_gaps, 2);
    assert_eq!(legacy.get(), 2);
}

// [utest->req~ssr-qos-deadline~3] [utest->req~ssr-atomic-width~1]
#[test]
fn full_origin_survives_low_word_alias_and_older_sample() {
    let origin = (1_u64 << 61) + 17;
    let monitor = monitor(origin);
    let legacy = Counter::<true>::default();
    let before = monitor.report();
    assert!(!monitor.check_and_publish(origin, &legacy));
    assert!(!monitor.arrival_and_publish(origin, &legacy));
    assert!(!monitor.check_and_publish(17, &legacy));
    assert!(!monitor.arrival_and_publish(17, &legacy));
    assert_eq!(monitor.report(), before);
    assert!(monitor.check_and_publish(origin + 1025, &legacy));
    let counted = monitor.report();
    for old in [17, origin] {
        assert!(!monitor.check_and_publish(old, &legacy));
        assert!(!monitor.arrival_and_publish(old, &legacy));
        assert_eq!(monitor.report(), counted);
    }
    assert!(!monitor.arrival_and_publish(origin + 2048, &legacy));
    assert_eq!(monitor.report().origin_ns, origin + 2048);
    assert_eq!(monitor.report().missed_gaps, 1);
}

// [utest->req~ssr-qos-deadline~3] [utest->req~ssr-error-no-silent-loss~3]
#[test]
fn exact_max_and_overflow_are_distinct_and_health_is_sticky() {
    let mut monitor = monitor(0);
    // Exclusive test setup only. No shared publication or runtime reset exists.
    monitor.seed_for_test(u128::MAX - 1);
    let legacy = Counter::<true>::default();
    assert!(monitor.arrival_and_publish(2048, &legacy));
    let exact_max = monitor.report();
    assert_eq!(exact_max.missed_gaps, u128::MAX);
    assert!(!exact_max.count_exhausted);
    assert!(!exact_max.missing_sample);
    monitor.sample_missing();
    assert!(monitor.check_and_publish(4096, &legacy));
    let exhausted = monitor.report();
    assert_eq!(exhausted.missed_gaps, u128::MAX);
    assert!(exhausted.count_exhausted && exhausted.missing_sample);
    assert!(!monitor.arrival_and_publish(8192, &legacy));
    let later = monitor.report();
    assert_eq!(later.origin_ns, 8192);
    assert!(later.count_exhausted && later.missing_sample);
}

// [utest->req~ssr-qos-deadline~3] [utest->req~ssr-atomic-windows~1]
#[test]
fn full_record_does_not_alias_after_the_predecessor_word_period() {
    let monitor = monitor(0);
    let legacy = Counter::<true>::default();
    assert!(monitor.check_and_publish(1_u64 << 32, &legacy));
    assert!(!monitor.check_and_publish((1_u64 << 32) + 2048, &legacy));
    assert_eq!(monitor.report().missed_gaps, 1);
    // No schedule consumer is allowed to infer a qualified service delay here.
    assert_eq!(monitor.count_schedule(0, 1_u64 << 32), CountSchedule::Events {
        first_eligible_ns: None,
        maintenance_ns: 1_u128 << 28,
    });
}

// [utest->req~ssr-qos-deadline~3] [utest->req~ssr-atomic-width~1]
#[test]
#[cfg(not(target_os = "none"))]
fn concurrent_reports_never_pair_a_new_origin_with_an_old_count() {
    extern crate std;
    let mut monitor = monitor(0);
    let seed = (1_u128 << 32) - 1;
    monitor.seed_for_test(seed);
    let legacy = Counter::<true>::default();
    let start = std::sync::Barrier::new(3);
    let complete = std::sync::Barrier::new(3);
    let mut observations_valid = true;
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for sequence in 1_u64..=16 {
                start.wait();
                monitor.arrival_and_publish(sequence * 2048, &legacy);
                monitor.sample_missing();
                complete.wait();
            }
        });
        scope.spawn(|| {
            for sequence in 1_u64..=16 {
                start.wait();
                monitor.check_and_publish(sequence * 2048 - 1, &legacy);
                complete.wait();
            }
        });
        for sequence in 1_u64..=16 {
            start.wait();
            for _ in 0..8 {
                let report = monitor.report();
                let base = seed + u128::from(sequence - 1);
                // The check can credit the old origin before the arrival, or
                // the arrival can credit/rearm first. Either order credits once.
                observations_valid &= report.missed_gaps == base || report.missed_gaps == base + 1;
                if report.origin_ns == sequence * 2048 {
                    observations_valid &= report.missed_gaps == base + 1;
                } else {
                    observations_valid &= report.origin_ns == (sequence - 1) * 2048;
                }
                if report.missing_sample { observations_valid &= report.missed_gaps > seed; }
                if sequence > 1 { observations_valid &= report.missing_sample; }
                observations_valid &= !report.count_exhausted;
            }
            complete.wait();
            let done = monitor.report();
            observations_valid &= (done.origin_ns, done.missed_gaps)
                == (sequence * 2048, seed + u128::from(sequence));
            observations_valid &= done.missing_sample && !done.count_exhausted;
            observations_valid &= u64::from(legacy.get()) == sequence;
        }
    });
    // Assert only after every finite rendezvous and scoped join. An oracle
    // failure cannot strand the workers. The external test timeout still bounds
    // an unrelated product panic/no-progress defect inside a worker.
    assert!(observations_valid, "incoherent report or duplicate/missing credit");
    assert_eq!(monitor.report().missed_gaps, seed + 16);
}

// [utest->req~ssr-qos-deadline~3] [utest->req~ssr-error-no-silent-loss~3]
#[test]
fn missing_sample_invalidates_physical_exactness_and_stays_latched() {
    let monitor = monitor(0);
    let legacy = Counter::<true>::default();
    // Physical arrival at 800: the timestamp is unavailable to the SDK.
    // Neither physical interval (800, then 225) exceeds D=1024.
    monitor.sample_missing();
    assert_eq!(monitor.report().origin_ns, 0);
    assert!(monitor.check_and_publish(1025, &legacy));
    let incomplete = monitor.report();
    assert_eq!(incomplete.missed_gaps, 1);
    assert!(incomplete.missing_sample && !incomplete.count_exhausted);
    // A valid subsequent arrival cannot reconstruct the missing history.
    assert!(!monitor.arrival_and_publish(1500, &legacy));
    assert!(monitor.check_and_publish(2525, &legacy));
    let later = monitor.report();
    assert_eq!((later.origin_ns, later.missed_gaps), (1500, 2));
    assert!(later.missing_sample && !later.count_exhausted);
}

// [utest->req~ssr-qos-deadline~3] [utest->req~ssr-atomic-width~1]
#[test]
fn carries_preserve_full_count_and_independent_health() {
    for bits in [32, 64, 96] {
        let mut monitor = monitor(0);
        let seed = (1_u128 << bits) - 1;
        monitor.seed_for_test(seed);
        monitor.sample_missing();
        let legacy = Counter::<true>::default();
        assert!(monitor.arrival_and_publish(1025, &legacy));
        let report = monitor.report();
        assert_eq!((report.origin_ns, report.missed_gaps), (1025, seed + 1));
        assert!(report.missing_sample && !report.count_exhausted);
        assert!(!monitor.check_and_publish(1025, &legacy));
        assert_eq!(legacy.get(), 1);
    }
}

// [utest->req~ssr-qos-deadline~3]
#[test]
fn quantization_preserves_forward_samples_and_strict_cutoff() {
    for (deadline, latency, quantum, cutoff) in
        [(1024, 0, 1, 1024), (3000, 1000, 2, 2000), (4094, 1, 2, 2048)]
    {
        let config = MonitorConfig::new(deadline, latency).unwrap();
        assert_eq!(config.report().quantum_ns, quantum);
        assert_eq!(u64::from(config.cutoff_ticks()), cutoff);
        let origin = 2 * quantum;
        let monitor = CoherentDeadline::new(config, origin);
        let legacy = Counter::<true>::default();
        if quantum > 1 {
            assert!(!monitor.arrival_and_publish(origin + 1, &legacy));
            assert_eq!(monitor.report().origin_ns, origin + 1);
        }
        let at_cutoff = origin + cutoff * quantum;
        assert!(!monitor.check_and_publish(at_cutoff, &legacy));
        let due = at_cutoff + quantum;
        assert!(monitor.check_and_publish(due, &legacy));
        assert!(!monitor.check_and_publish(due + quantum, &legacy));
        assert_eq!(monitor.report().missed_gaps, 1);
        assert!(!monitor.arrival_and_publish(due + quantum, &legacy));
        assert_eq!(monitor.report().origin_ns, due + quantum);
    }
}

// [utest->req~ssr-qos-deadline~3] [utest->req~ssr-atomic-width~1]
#[test]
fn scheduling_widens_endpoints_and_caps_before_narrowing() {
    use crate::transport::deadline_monitor::WaitRequest;
    let near_end = monitor(u64::MAX);
    let plan = near_end.count_schedule(u64::MAX, u64::MAX);
    assert!(matches!(plan, CountSchedule::Events {
        first_eligible_ns: Some(first), maintenance_ns
    } if first > u128::from(u64::MAX) && maintenance_ns > u128::from(u64::MAX)));
    assert_eq!(plan.wait_request(u64::MAX - 1, u64::MAX), WaitRequest::Until(u64::MAX));
    assert_eq!(plan.wait_request(u64::MAX, u64::MAX), WaitRequest::ServiceNow);
    assert_eq!(near_end.count_schedule(u64::MAX, u64::MAX - 1), CountSchedule::Recheck);
    assert_eq!(near_end.count_schedule(0, u64::MAX - 1), CountSchedule::Recheck);
    let start = monitor(0);
    let plan = start.count_schedule(0, 0);
    assert_eq!(plan.wait_request(0, 5000), WaitRequest::Until(1025));
    assert_eq!(plan.wait_request(0, 500), WaitRequest::Until(500));
    assert_eq!(plan.wait_request(1025, 5000), WaitRequest::ServiceNow);
}

#[cfg(not(target_os = "none"))]
mod route_controls;

// [utest->req~ssr-qos-deadline~3] [utest->req~ssr-mode-init-failure~2]
#[test]
fn actual_deadline_slot_keeps_one_publication_and_exposes_joint_report() {
    use crate::memory::StartupRegion;
    use crate::transport::deadline_timing::{ArmedDeadline, DeadlineSlot, DeadlineTimingReport, TimingGap};
    static REGION: StartupRegion<4096> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let slot = DeadlineSlot::new();
    assert_eq!(slot.report(), None);
    let first = memory.place(ArmedDeadline {
        monitor: monitor(0),
        report: DeadlineTimingReport::pending(Some(core::time::Duration::from_nanos(1024))),
    }).unwrap();
    slot.publish(first).unwrap();
    let legacy = Counter::<true>::default();
    slot.arrival(2048, &legacy);
    slot.sample_missing();
    let report = slot.report().unwrap();
    assert_eq!((report.origin_ns, report.missed_gaps), (2048, 1));
    assert!(report.missing_sample && !report.count_exhausted);
    let replacement = memory.place(ArmedDeadline {
        monitor: monitor(9000),
        report: DeadlineTimingReport::pending(None),
    }).unwrap();
    assert_eq!(slot.publish(replacement), Err(TimingGap::AlreadyArmed));
    assert_eq!(slot.report(), Some(report));
}
