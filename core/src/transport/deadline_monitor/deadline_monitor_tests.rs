//! Prepared fixtures; NOT COMPILED OR RUN. Uses std only for test concurrency.
extern crate std;

use super::*;

fn monitor(deadline_ns: u64, now_ns: u64) -> DeadlineMonitor {
    DeadlineMonitor::new(MonitorConfig::new(deadline_ns, 0).unwrap(), now_ns)
}

#[test]
fn setup_rejects_unrepresentable_or_unapproved_enabled_deadlines() {
    assert_eq!(MonitorConfig::new(0, 0), Err(SetupError::DeadlineBelowMinimum));
    assert_eq!(MonitorConfig::new(1023, 0), Err(SetupError::DeadlineBelowMinimum));
    assert_eq!(MonitorConfig::new(1024, 1025), Err(SetupError::LatencyExceedsDeadline));
    assert_eq!(MonitorConfig::new(u64::MAX, 1), Err(SetupError::ThresholdOverflow));
    assert!(MonitorConfig::new(1024, 1024).is_ok());
    // Route-level None is disabled; not a fabricated enabled zero interval.
    let disabled: Option<DeadlineMonitor> = None;
    assert!(!disabled.as_ref().is_some_and(|m| m.check(u64::MAX)));
}

#[test]
fn quantized_threshold_never_counts_an_equal_actual_gap() {
    // Q=8, T=10001, ceil(T/Q)=1251. Floor(T/Q)=1250 would count this
    // exact-threshold gap early, because the starting phase is seven ns.
    let m = monitor(10001, 7);
    assert_eq!(m.config.report().quantum_ns, 8);
    assert_eq!(m.config.cutoff_ticks, 1251);
    assert!(!m.check(10008));
    assert!(!m.check(10015));
    assert!(m.check(10016));
}

#[test]
fn scale_transition_uses_ceil_without_early_count_or_wide_atomic() {
    let c = MonitorConfig::new(4095, 0).unwrap();
    assert_eq!(c.report().quantum_ns, 2);
    assert_eq!(c.cutoff_ticks, 2048);
    let m = DeadlineMonitor::new(c, 1);
    assert!(!m.check(4096));
    assert!(m.check(4098));
}

#[test]
fn closes_silent_gaps_even_without_an_intervening_check() {
    let m = monitor(1024, 0);
    assert!(m.arrival(1025));
    assert!(!m.check(1025));
    assert!(m.arrival(2050));
    assert!(!m.check(2050));
}

#[test]
fn duplicate_checks_count_once_and_next_arrival_rearms() {
    let m = monitor(1024, 0);
    assert!(!m.check(1024));
    assert!(m.check(1025));
    assert!(!m.check(1026));
    assert!(!m.arrival(2000));
    assert!(!m.check(3024));
    assert!(m.check(3025));
}

#[test]
fn equal_and_older_arrivals_cannot_reset_a_counted_gap() {
    let m = monitor(1024, 5000);
    assert!(m.check(6025));
    let before = m.word.load(Ordering::Relaxed);
    assert!(!m.arrival(5000));
    assert!(!m.arrival(4999));
    assert_eq!(m.word.load(Ordering::Relaxed), before);
    assert!(!m.arrival(6026));
    assert!(m.check(7051));
}

#[test]
fn arrival_and_check_follow_time_across_tick_wrap() {
    let start = u64::from(TICK_MASK) - 20;
    let m = monitor(1024, start);
    let arrival = start + 2025;
    assert!(m.arrival(arrival));
    assert_eq!(m.word.load(Ordering::Relaxed), (arrival as u32) & TICK_MASK);
    assert!(!m.check(arrival + 1024));
    assert!(m.check(arrival + 1025));
}

#[test]
fn refresh_into_the_past_preserves_a_previously_sampled_arrival() {
    let m = monitor(1024, 0);
    assert!(m.check(1025));
    let sampled_arrival = u64::from(REFRESH_AGE) - 1;
    assert!(!m.check(u64::from(REFRESH_AGE)));
    assert_eq!(m.word.load(Ordering::Relaxed), PAST_OFFSET | COUNTED);
    assert!(!m.arrival(sampled_arrival));
    assert_eq!(m.word.load(Ordering::Relaxed), sampled_arrival as u32);
    assert!(m.check(sampled_arrival + 1025));
}

#[test]
fn refresh_keeps_arrival_delayed_to_just_inside_the_timing_bound() {
    let m = monitor(1024, 0);
    let refreshed_now = u64::from(REFRESH_AGE);
    let sampled = refreshed_now - u64::from(PAST_OFFSET) + 1;
    assert!(m.check(refreshed_now));
    assert!(!m.arrival(sampled));
    assert_eq!(m.word.load(Ordering::Relaxed), sampled as u32);
    assert!(m.check(refreshed_now + 1));
}

#[test]
fn first_count_and_refresh_are_one_cas_before_next_service_interval() {
    let m = monitor(1024, 0);
    assert!(m.check(u64::from(REFRESH_AGE)));
    assert_eq!(m.word.load(Ordering::Relaxed), PAST_OFFSET | COUNTED);
    assert!(!m.check(u64::from(REFRESH_AGE) * 2));
    assert_eq!(m.word.load(Ordering::Relaxed), (PAST_OFFSET * 3) | COUNTED);
    let arrival = u64::from(REFRESH_AGE) * 2 + 1;
    assert!(!m.arrival(arrival));
    assert!(m.check(arrival + 1025));
}

#[test]
fn counted_silence_survives_multiple_full_tick_wraps_with_servicing() {
    let m = monitor(1024, 0);
    let mut counts = 0;
    for step in 1..=12_u64 {
        counts += u32::from(m.check(step * u64::from(REFRESH_AGE)));
    }
    assert_eq!(counts, 1);
    let arrival = 12 * u64::from(REFRESH_AGE) + 1;
    assert!(!m.arrival(arrival));
    assert!(m.check(arrival + 1025));
}

#[test]
fn stale_cas_recomputes_against_winning_arrival_instead_of_double_counting() {
    let m = monitor(1024, 0);
    let old = m.word.load(Ordering::Relaxed);
    let (proposed, count) = m.transition(old, 1025, Observation::Check).unwrap();
    assert!(count);
    assert!(m.arrival(1025));
    assert!(m.word.compare_exchange(old, proposed, Ordering::Relaxed, Ordering::Relaxed).is_err());
    assert!(!m.check(1025));
}

#[test]
fn stale_check_cas_cannot_overwrite_a_newer_sample() {
    let m = monitor(1024, 0);
    let old = m.word.load(Ordering::Relaxed);
    let (proposed, _) = m.transition(old, 1025, Observation::Check).unwrap();
    assert!(m.arrival(2000));
    assert!(m.word.compare_exchange(old, proposed, Ordering::Relaxed, Ordering::Relaxed).is_err());
    assert!(!m.check(1025));
    assert_eq!(m.word.load(Ordering::Relaxed), 2000);
}

#[test]
fn competing_arrival_and_check_assign_one_increment_for_the_same_gap() {
    let m = monitor(1024, 0);
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let left = scope.spawn(|| { barrier.wait(); m.arrival(1025) });
        let right = scope.spawn(|| { barrier.wait(); m.check(1025) });
        assert_eq!(u32::from(left.join().unwrap()) + u32::from(right.join().unwrap()), 1);
    });
}

#[test]
fn startup_exclusive_arm_and_large_report_preserve_full_values() {
    let mut m = monitor(1024, 0);
    m.arm(10000);
    assert!(!m.check(11024));
    assert!(m.check(11025));
    let report = MonitorConfig::new(u64::MAX, 0).unwrap().report();
    assert_eq!(report.inner_wait_from_sample_ns, u64::MAX);
    assert_eq!(report.quantum_ns, 1_u64 << 53);
    assert_eq!(report.completed_sample_max_age_ns, 1_u128 << 82);
    assert_eq!(report.pending_observation_limit_ns, ((1_u128 << 28) - 1) * (1_u128 << 53) + 1);
    assert!(report.detection_bound_ns > u128::from(u64::MAX));
}

#[test]
fn pending_physical_limit_preserves_strict_ticks_for_every_selected_quantum_phase() {
    for deadline in [1024, 2048, 10001, 1_000_000] {
        let config = MonitorConfig::new(deadline, 0).unwrap();
        let report = config.report();
        let q = report.quantum_ns;
        let max_delay = (u64::from(PAST_OFFSET) - 1) * q;
        assert_eq!(report.pending_observation_limit_ns, u128::from(max_delay) + 1);
        for phase in 0..q {
            let m = DeadlineMonitor::new(config, 0);
            let refresh = u64::from(REFRESH_AGE) * q;
            let sampled_arrival = (u64::from(PAST_OFFSET) + 1) * q + phase;
            assert!(u128::from(refresh - sampled_arrival) < report.pending_observation_limit_ns);
            assert!(m.check(refresh));
            assert!(!m.arrival(sampled_arrival));
            assert_eq!(m.word.load(Ordering::Relaxed) & COUNTED, 0);
            // The next recorded arrival closes a second, very long gap.
            assert!(m.arrival(refresh + deadline + 2 * q));
        }
    }
    let corrected = MonitorConfig::new(10001, 0).unwrap().report();
    let old_counterexample_delay = u128::from(PAST_OFFSET) * 8 - 5;
    // The original report's physical promise was false. This corrected bound
    // rejects that exact advertised delay; it does not relabel the trace a pass.
    assert!(old_counterexample_delay >= corrected.pending_observation_limit_ns);
}

#[test]
fn wait_is_anchored_to_service_sample_including_prior_and_next_pass_time() {
    let config = MonitorConfig::new(1024, 0).unwrap();
    let report = config.report();
    let m = DeadlineMonitor::new(config, 0);
    let mut sample = 0_u64;
    let mut completed = 10_u64;
    let mut counts = 0;
    for _ in 0..8 {
        let deadline = config.inner_service_deadline_ns(sample);
        assert_eq!(config.inner_wait_budget_ns(sample, completed), deadline - completed);
        // Bounded dispatch/wake/pass latency. Old sample remains fresh through
        // completion; completion itself is NOT the next timing origin.
        let next_sample = deadline + 20;
        let next_completed = next_sample + 30;
        assert!(u128::from(next_completed - sample) <= report.completed_sample_max_age_ns);
        counts += u32::from(m.check(next_sample));
        sample = next_sample;
        completed = next_completed;
    }
    assert_eq!(counts, 1);
    assert!(!m.arrival(completed + 1));
    assert!(m.check(completed + 1026));
    assert_eq!(config.inner_wait_budget_ns(0, u64::from(PAST_OFFSET)), 0);
    assert_eq!(config.inner_wait_budget_ns(10, 9), 0);
    assert_eq!(config.inner_service_deadline_ns(u64::MAX - 10), u64::MAX);
    assert_eq!(config.inner_wait_budget_ns(u64::MAX - 10, u64::MAX - 5), 5);
}

#[test]
fn completion_spacing_is_not_completed_sample_freshness() {
    let r = u64::from(REFRESH_AGE);
    let report = MonitorConfig::new(1024, 0).unwrap().report();
    let first_sample = r - 20;
    let first_completed = r + 10;
    let next_sample = u64::from(HALF_RANGE);
    let next_completed = next_sample + 10;
    assert_eq!(next_completed - first_completed, r);
    assert!(u128::from(next_sample - first_sample) > report.completed_sample_max_age_ns);
    // The original two-completion promise admits this counterexample. The
    // source model's current-time versus last-completed-SAMPLE invariant does not.
    // No old model, failed trace or receipt is converted to current evidence.
}
