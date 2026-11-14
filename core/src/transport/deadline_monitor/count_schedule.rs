//! Isolated, UNCOMPILED child of the pinned deadline-monitor module.
//!
//! Computes an absolute count-eligibility event AND a maintenance event.
//! It does not supply a timer, a WCET, an arrival wake protocol or admission.
//! The new report adds owner-approved validated J; physical qualification remains open.

use super::{DeadlineMonitor, COUNTED, HALF_RANGE, PAST_OFFSET, TICK_MASK};
#[cfg(test)]
use super::MonitorConfig;
// [impl->req~ssr-atomic-width~1]
// Facade-routing contribution only; target lowering and the whole SDK remain unqualified.
use crate::platform::atomic::Ordering;

/// A planning observation, not a reservation of the monitor's current state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CountSchedule {
    /// Clock reversal, a word newer than the supplied sample, or a history that
    /// cannot be lifted. Resample/service; this does not repair an AoU violation.
    Recheck,
    Events {
        /// Earliest sampled time at which the observed uncounted word permits
        /// a winning check. None means its bit was claimed, NOT counter-published.
        first_eligible_ns: Option<u128>,
        /// Absolute maintenance event, anchored to the preceding completed
        /// pass's OLDEST SAMPLE, never to the end of that pass.
        maintenance_ns: u128,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WaitRequest {
    ServiceNow,
    /// Pass this absolute value to Signal::wait_on/Platform::wait. Do not turn
    /// it into a relative timeout and start that timeout after more work.
    Until(u64),
}

impl CountSchedule {
    /// Selects min(caller deadline, count eligibility, maintenance).
    ///
    /// This is arithmetic only. Before sleeping, integration must establish a
    /// no-lost-wake handshake covering EVERY arrival that can rearm a previously
    /// counted word. A stale counted snapshot otherwise hides a new deadline.
    /// Use the caller's ORIGINAL signal token throughout the internal loop.
    /// A ServiceNow result never authorizes a fabricated public timeout/wake.
    // [impl->req~ssr-qos-deadline~3]
    // Partial arithmetic contribution; real notification and service bounds remain open.
    pub(crate) fn wait_request(self, now_ns: u64, caller_deadline_ns: u64) -> WaitRequest {
        let Self::Events { first_eligible_ns, maintenance_ns } = self else {
            return WaitRequest::ServiceNow;
        };
        let mut due = u128::from(caller_deadline_ns).min(maintenance_ns);
        if let Some(first) = first_eligible_ns {
            due = due.min(first);
        }
        if due <= u128::from(now_ns) {
            WaitRequest::ServiceNow
        } else {
            // due <= caller_deadline_ns <= u64::MAX. The conversion is exact.
            WaitRequest::Until(due as u64)
        }
    }
}

/// Additional setup report; does not redefine MonitorReport's existing fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct CountTimeBudget {
    /// Baseline term D + 2L + 2Q, before explicit validated J.
    /// This field alone is not a physical publication guarantee.
    pub(crate) latest_count_offset_ns: u128,
    /// Existing quantization tolerance 2Q, unchanged.
    pub(crate) resolution_ns: u128,
    /// Q - (ceil(T/Q)*Q - T), in 1..=Q. Sufficient aggregate time from
    /// eligibility to COUNTER publication, for every arrival/quantum phase.
    /// Includes wake, route position, clock read, CAS and counter contention.
    /// This is a REQUIRED budget, NOT an observed or accepted platform bound.
    pub(crate) eligibility_to_publication_budget_ns: u64,
}

#[cfg(test)]
impl MonitorConfig {
    // [impl->req~ssr-qos-deadline~3]
    // Reports the preserved baseline arithmetic, not the J-qualified reporting bound.
    pub(crate) fn count_time_budget(self) -> CountTimeBudget {
        let q = 1_u128.wrapping_shl(self.shift);
        let preceding = u128::from(self.cutoff_ticks.saturating_sub(1)) * q;
        // Constructor facts: C=ceil(T/Q), Q>=1, so 1<=T-(C-1)Q<=Q.
        let tail = u128::from(self.threshold_ns).saturating_sub(preceding);
        CountTimeBudget {
            latest_count_offset_ns: u128::from(self.deadline_ns)
                + u128::from(self.latency_ns) * 2 + q * 2,
            resolution_ns: q * 2,
            eligibility_to_publication_budget_ns: tail as u64,
        }
    }
}

impl DeadlineMonitor {
    /// Compute events for one observed word without changing the counting kernel.
    ///
    /// Semantic premises: nondecreasing nonwrapping u64 clock; accepted pending
    /// observation and completed-sample freshness invariants already hold.
    /// `previous_sample_ns` is retained until an ENTIRE pass, including all
    /// winning counter publications, completes. Use activation as its initial
    /// baseline. Merely providing arbitrary safe integers proves no timing.
    /// Invalid inputs can give a wrong schedule but never authorize unsafe code.
    ///
    /// Arrivals after this snapshot may rearm the word: the real signal protocol
    /// must wake/rescan. An already-counted bit does not discharge the publisher's
    /// outstanding counter obligation. This helper cannot help a stalled winner.
    // [impl->req~ssr-qos-deadline~3]
    // [impl->req~ssr-atomic-windows~1]
    // Arithmetic under the documented sample window; no scheduler qualification.
    pub(crate) fn count_schedule(
        &self,
        previous_sample_ns: u64,
        sampled_now_ns: u64,
    ) -> CountSchedule {
        if sampled_now_ns < previous_sample_ns {
            return CountSchedule::Recheck;
        }
        let word = self.word.load(Ordering::Relaxed);
        let full_now_tick = sampled_now_ns.wrapping_shr(self.config.shift);
        let now_tick = (full_now_tick as u32) & TICK_MASK;
        let age = now_tick.wrapping_sub(word & TICK_MASK) & TICK_MASK;
        if age >= HALF_RANGE || u64::from(age) > full_now_tick {
            // A concurrent arrival may be newer than the supplied sample.
            // The one word cannot distinguish that from a broken timing AoU.
            return CountSchedule::Recheck;
        }
        let q = 1_u128.wrapping_shl(self.config.shift);
        let first_eligible_ns = if word & COUNTED == 0 {
            // The check requires age > C. Reconstruct the stored tick's full
            // origin from this fresh sample; do not mistake low-bit wrap for a
            // new mission. The checked subtraction is exact, not saturating.
            let origin_tick = full_now_tick.wrapping_sub(u64::from(age));
            Some((u128::from(origin_tick) + u128::from(self.config.cutoff_ticks) + 1) * q)
        } else {
            None
        };
        CountSchedule::Events {
            first_eligible_ns,
            maintenance_ns: u128::from(previous_sample_ns) + u128::from(PAST_OFFSET) * q,
        }
    }
}
