//! NEW fixtures only: UNCOMPILED / UNRUN. No hardware timing evidence.
use super::*;
use super::count_schedule::{CountSchedule, WaitRequest};
use crate::transport::report::Counters;

fn make(deadline: u64, latency: u64, now: u64) -> DeadlineMonitor {
    DeadlineMonitor::new(MonitorConfig::new(deadline, latency).unwrap(), now)
}

// [utest->req~ssr-qos-deadline~3]
// Scope: Earliest eligible count event precedes the long maintenance event; arithmetic only.
#[test]
fn earliest_count_preempts_long_maintenance_wait() {
    let m = make(1024, 0, 0);
    assert_eq!(m.count_schedule(0, 1), CountSchedule::Events {
        first_eligible_ns: Some(1025), maintenance_ns: u128::from(PAST_OFFSET),
    });
    assert_eq!(m.count_schedule(0, 1).wait_request(1, u64::MAX), WaitRequest::Until(1025));
}

// [utest->req~ssr-qos-deadline~3]
// Scope: Ceiling cutoff and unchanged 2Q resolution/report offset; no publication deadline proof.
#[test]
fn eligibility_uses_ceil_cutoff_and_keeps_resolution() {
    let m = make(10001, 0, 0);
    assert_eq!(m.count_schedule(0, 0).wait_request(0, u64::MAX), WaitRequest::Until(10016));
    let budget = m.config.count_time_budget();
    assert_eq!(budget.resolution_ns, 16);
    assert_eq!(budget.latest_count_offset_ns, 10017);
    assert_eq!(budget.eligibility_to_publication_budget_ns, 1);
}

// [utest->req~ssr-qos-deadline~3]
// Scope: Quantum phase and exact-multiple residual arithmetic; no platform delay measurement.
#[test]
fn phase_and_exact_multiple_have_distinct_slack() {
    let at_phase_seven = make(10001, 0, 7);
    assert_eq!(at_phase_seven.count_schedule(7, 7).wait_request(7, u64::MAX), WaitRequest::Until(10016));
    let exact = make(10000, 0, 0);
    assert_eq!(exact.config.count_time_budget().eligibility_to_publication_budget_ns, 8);
    assert_eq!(exact.count_schedule(0, 0).wait_request(0, u64::MAX), WaitRequest::Until(10008));
}

// [utest->req~ssr-qos-deadline~3]
// Scope: Absolute deadline selection accounts for elapsed work and caller deadline; no wait implementation proof.
#[test]
fn elapsed_prior_pass_never_restarts_relative_timeout() {
    let m = make(1024, 0, 0);
    let events = m.count_schedule(0, 300);
    assert_eq!(events.wait_request(300, u64::MAX), WaitRequest::Until(1025));
    assert_eq!(events.wait_request(1027, u64::MAX), WaitRequest::ServiceNow);
    assert_eq!(events.wait_request(300, 400), WaitRequest::Until(400));
}

// [utest->req~ssr-qos-deadline~3]
// [utest->req~ssr-atomic-windows~1]
// Scope: Counted-word maintenance stays anchored to the completed sample; one sequential counter update.
#[test]
fn counted_route_still_schedules_anchored_maintenance() {
    let m = make(1024, 0, 0);
    let counters = Counters::default();
    assert!(m.check_and_publish(1025, &counters.deadline_missed));
    assert_eq!(m.count_schedule(1025, 1050), CountSchedule::Events {
        first_eligible_ns: None, maintenance_ns: 1025 + u128::from(PAST_OFFSET),
    });
    assert_eq!(counters.snapshot().deadline_missed, 1);
}

// [utest->req~ssr-qos-deadline~3]
// Scope: Stale snapshot differs after an arrival rearms the gap; exposes, but does not establish, the real notification obligation.
#[test]
fn rearm_after_counted_snapshot_requires_arrival_notification() {
    let m = make(1024, 0, 0);
    let counters = Counters::default();
    assert!(m.check_and_publish(1025, &counters.deadline_missed));
    let stale = m.count_schedule(1025, 1026);
    assert!(!m.arrival_and_publish(2000, &counters.deadline_missed));
    assert_eq!(stale.wait_request(2000, u64::MAX), WaitRequest::Until(1025 + u64::from(PAST_OFFSET)));
    assert_eq!(m.count_schedule(1025, 2000).wait_request(2000, u64::MAX), WaitRequest::Until(3025));
    // This difference is the obligation for the REAL signal path, not a proof
    // that the old callsites already perform that handshake.
}

// [utest->req~ssr-qos-deadline~3]
// [utest->req~ssr-atomic-windows~1]
// Scope: Newer word/reversed sample returns Recheck; no proof that a real scheduler supplies fresh samples.
#[test]
fn word_newer_than_sample_requests_resample() {
    let m = make(1024, 0, 0);
    assert!(!m.arrival(10));
    assert_eq!(m.count_schedule(0, 5), CountSchedule::Recheck);
    assert_eq!(m.count_schedule(10, 9), CountSchedule::Recheck);
}

// [utest->req~ssr-qos-deadline~3]
// [utest->req~ssr-atomic-windows~1]
// Scope: Low-tick wrap preserves the full-clock absolute deadline in the stated sample window.
#[test]
fn absolute_deadline_crosses_tick_wrap_without_shortening_clock() {
    let start = u64::from(TICK_MASK) - 10;
    let m = make(1024, 0, start);
    assert_eq!(m.count_schedule(start, start + 1000).wait_request(start + 1000, u64::MAX),
        WaitRequest::Until(start + 1025));
}

// [utest->req~ssr-qos-deadline~3]
// Scope: A u128 count event beyond u64 is retained while the wait is clamped to caller deadline; no wrapped-clock support.
#[test]
fn full_clock_edge_retains_exact_unrepresentable_count_event() {
    let start = u64::MAX - 500;
    let m = make(1024, 0, start);
    assert_eq!(m.count_schedule(start, start), CountSchedule::Events {
        first_eligible_ns: Some(u128::from(start) + 1025),
        maintenance_ns: u128::from(start) + u128::from(PAST_OFFSET),
    });
    assert_eq!(m.count_schedule(start, start).wait_request(start, u64::MAX), WaitRequest::Until(u64::MAX));
}

// [utest->req~ssr-qos-deadline~3]
// Scope: Largest accepted threshold's scalar arithmetic and exact report; does not test atomic lowering.
#[test]
fn largest_threshold_preserves_wide_report_without_wide_atomic() {
    let m = make(u64::MAX, 0, 0);
    let q = 1_u128 << 53;
    assert_eq!(m.count_schedule(0, 0), CountSchedule::Events {
        first_eligible_ns: Some(2049 * q), maintenance_ns: 1_u128 << 81,
    });
    assert_eq!(m.config.count_time_budget().eligibility_to_publication_budget_ns,
        ((1_u128 << 53) - 1) as u64);
}

// [utest->req~ssr-qos-deadline~3]
// Scope: Declared latency contributes to the original D+2L+2Q report and exact tail budget.
#[test]
fn positive_latency_preserves_original_bound_and_tail_budget() {
    let m = make(1024, 1, 1);
    assert_eq!(m.config.count_time_budget().latest_count_offset_ns, 1028);
    assert_eq!(m.config.count_time_budget().eligibility_to_publication_budget_ns, 1);
    assert_eq!(m.count_schedule(1, 1).wait_request(1, u64::MAX), WaitRequest::Until(1027));
}

// [utest->req~ssr-qos-deadline~3]
// Scope: Sequential arrival/check winners publish one count per observed gap; not a concurrency or bounded-publication proof.
#[test]
fn both_winner_paths_publish_into_the_real_route_counter() {
    let m = make(1024, 0, 0);
    let counters = Counters::default();
    assert!(m.arrival_and_publish(1025, &counters.deadline_missed));
    assert_eq!(counters.snapshot().deadline_missed, 1);
    assert!(!m.check_and_publish(1025, &counters.deadline_missed));
    assert!(m.check_and_publish(2050, &counters.deadline_missed));
    assert_eq!(counters.snapshot().deadline_missed, 2);
}
