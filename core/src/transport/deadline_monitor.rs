//! Monitor configuration and retained predecessor counting-model controls.
//! Runtime publication is now in metadata::CoherentDeadline in this experiment.
//! The one-word algorithm below is compiled only for historical model tests.
//!
//! The following historical notes describe the test-only predecessor model.
//! This is counting state only: it publishes no message or payload memory.
//! All shared mutable state is a single AtomicU32. No unsafe code, allocation,
//! destructor, wide atomic, lock, division, or panicking index is used here.
//!
//! Correct deadline behavior requires a nondecreasing nanosecond clock, a
//! completed-service SAMPLE never more than 2^29 scaled ticks behind current
//! time, with activation as the initial baseline. Merely spacing completions by
//! 2^29 ticks does not establish that invariant. Each observation's completing
//! CAS must be less than 2^28 TICK COUNTS after its sample (including retries).
//! Different quantum phases matter; the physical limits below account for them.
//! These are timing assumptions, not Rust safety preconditions.
//! Violating them can silently miss deadlines. This kernel cannot diagnose it.
//! The transport wait loop and setup report must establish/disclose these bounds.
//!
//! An operation returning true won responsibility for ONE diagnostic increment.
//! Both arrival and check callers must perform that increment. The atomic word
//! does not make a separate diagnostic counter update atomic with the transition.

// [impl->req~ssr-atomic-width~1]
// Facade-routing contribution only; this import does not establish target support.
#[cfg(test)]
use crate::platform::atomic::{AtomicU32, Ordering};

#[cfg(test)]
const COUNTED: u32 = 1 << 31;
#[cfg(test)]
const TICK_MASK: u32 = COUNTED - 1;
#[cfg(test)]
const HALF_RANGE: u32 = 1 << 30;
#[cfg(test)]
const REFRESH_AGE: u32 = 1 << 29;
const PAST_OFFSET: u32 = 1 << 28;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SetupError {
    DeadlineBelowMinimum,
    LatencyExceedsDeadline,
    ThresholdOverflow,
}

/// Setup-only, immutable parameters. A disabled monitor is represented by None
/// at the owning route; zero is not accepted as an enabled deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MonitorConfig {
    deadline_ns: u64,
    latency_ns: u64,
    threshold_ns: u64,
    shift: u32,
    cutoff_ticks: u32,
}

impl MonitorConfig {
    pub(crate) fn new(deadline_ns: u64, latency_ns: u64) -> Result<Self, SetupError> {
        if deadline_ns < 1024 {
            return Err(SetupError::DeadlineBelowMinimum);
        }
        if latency_ns > deadline_ns {
            return Err(SetupError::LatencyExceedsDeadline);
        }
        let threshold_ns = deadline_ns
            .checked_add(latency_ns)
            .ok_or(SetupError::ThresholdOverflow)?;
        // threshold >= 1024, hence floor(log2(threshold)) is in 10..=63.
        let shift = 63_u32
            .saturating_sub(threshold_ns.leading_zeros())
            .saturating_sub(10);
        let quantum = 1_u64.wrapping_shl(shift);
        let floor_ticks = threshold_ns.wrapping_shr(shift);
        let remainder = threshold_ns & quantum.wrapping_sub(1);
        // ceil(T/Q), not floor(T/Q): floor-quantized timestamp differences
        // can overestimate the actual duration by up to Q-1 nanoseconds.
        // floor_ticks is 1024..=2047; cutoff can be 2048.
        let cutoff_ticks = (floor_ticks as u32).wrapping_add(u32::from(remainder != 0));
        Ok(Self { deadline_ns, latency_ns, threshold_ns, shift, cutoff_ticks })
    }

    pub(crate) fn deadline_ns(self) -> u64 { self.deadline_ns }
    pub(crate) fn latency_ns(self) -> u64 { self.latency_ns }

    /// # Safety-usable invariant
    /// Returns the immutable constructor-checked shift in 0..=53. Performs only
    /// a scalar copy; invokes no callback, destructor or metadata operation.
    pub(crate) fn tick_shift(self) -> u32 { self.shift }

    /// # Safety-usable invariant
    /// Returns the immutable cutoff ceil((D + L) / quantum), in 1024..=2048.
    /// Performs only a scalar copy; invokes no callback or metadata operation.
    pub(crate) fn cutoff_ticks(self) -> u32 { self.cutoff_ticks }

    pub(crate) fn report(self) -> MonitorReport {
        let quantum_ns = 1_u64.wrapping_shl(self.shift);
        let resolution_ns = u128::from(quantum_ns) * 2;
        MonitorReport {
            threshold_ns: self.threshold_ns,
            quantum_ns,
            quantization_tolerance_ns: resolution_ns,
            // Exact setup reporting, not silently saturating the advertised bound.
            detection_bound_ns: u128::from(self.deadline_ns)
                + u128::from(self.latency_ns) * 2 + resolution_ns,
            // Age of the latest completed service's SAMPLE, not completion
            // spacing. Physical age <= R*Q is sufficient for tick age <= R.
            completed_sample_max_age_ns: 1_u128.wrapping_shl(self.shift.saturating_add(29)),
            // Strict tick distance < P: a phase-independent physical delay
            // <= (P-1)*Q suffices. +1 encodes that inclusive integer-ns bound
            // as a strict limit. The old P*Q advertisement was too generous.
            pending_observation_limit_ns: (u128::from(PAST_OFFSET) - 1)
                * u128::from(quantum_ns) + 1,
            // Upper span measured FROM THE PREVIOUS SERVICE SAMPLE. Subtract
            // time already spent completing that service before waiting.
            inner_wait_from_sample_ns: self.sample_wait_span_ns(),
        }
    }

    #[cfg(test)]
    fn tick(self, now_ns: u64) -> u32 {
        (now_ns.wrapping_shr(self.shift) as u32) & TICK_MASK
    }

    fn sample_wait_span_ns(self) -> u64 {
        1_u64.checked_shl(self.shift.saturating_add(28)).unwrap_or(u64::MAX)
    }

    /// Internal wait integration helper, NOT an independently sufficient
    /// scheduler. previous_sample_ns is the oldest sample in this caller's
    /// completed service pass. Before calculating this budget, elapsed time from
    /// that sample must be <= P*Q. Dispatch/overshoot/wakeup plus the NEXT whole
    /// pass (including all routes, counters and contention) must fit <= (P-1)*Q
    /// ns. The completed-sample freshness invariant must already hold on entry.
    ///
    /// The span from that sample to the end of the next pass is then < 2P*Q,
    /// even including residual time spent completing the previous pass. A zero
    /// budget requests internal service now, never a fabricated public timeout
    /// or notification. Zero does not diagnose or repair a missed timing bound.
    /// Use the absolute deadline below for the platform wait; do not restart a
    /// relative wait with a stale computed duration. Runtime uses scalar math.
    #[cfg(test)]
    pub(crate) fn inner_wait_budget_ns(self, previous_sample_ns: u64, now_ns: u64) -> u64 {
        if now_ns < previous_sample_ns {
            return 0;
        }
        self.inner_service_deadline_ns(previous_sample_ns).saturating_sub(now_ns)
    }

    #[cfg(test)]
    pub(crate) fn inner_service_deadline_ns(self, previous_sample_ns: u64) -> u64 {
        previous_sample_ns.saturating_add(self.sample_wait_span_ns())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MonitorReport {
    pub threshold_ns: u64,
    pub quantum_ns: u64,
    /// Gaps above threshold by less than this may go uncounted; never count early.
    pub quantization_tolerance_ns: u128,
    /// True-gap threshold for guaranteed counting, not diagnostic visibility time.
    pub detection_bound_ns: u128,
    /// Maximum age of the latest completed service's SAMPLE, inclusive, at ALL
    /// times; activation supplies the initial baseline. Not completion spacing.
    pub completed_sample_max_age_ns: u128,
    /// Clock-sample-to-completing-CAS physical delay must be STRICTLY below this
    /// phase-independent integer-nanosecond limit to guarantee < P tick counts.
    pub pending_observation_limit_ns: u128,
    /// Maximum sample-anchored span. Use inner_wait_budget_ns, not a fresh full
    /// sleep of this duration after a service pass has already completed.
    pub inner_wait_from_sample_ns: u64,
}

// Retained predecessor model controls only. Actual runtime uses CoherentDeadline.
#[cfg(test)]
pub(crate) struct DeadlineMonitor {
    config: MonitorConfig,
    word: AtomicU32,
}

#[cfg(test)]
#[derive(Clone, Copy)]
enum Observation { Arrival, Check }

#[cfg(test)]
impl DeadlineMonitor {
    /// Construct/arm at activation before sharing the runtime route.
    pub(crate) fn new(config: MonitorConfig, now_ns: u64) -> Self {
        Self { config, word: AtomicU32::new(config.tick(now_ns)) }
    }

    /// Startup-only exclusive reset: shared runtime handles cannot re-arm it.
    pub(crate) fn arm(&mut self, now_ns: u64) {
        *self.word.get_mut() = self.config.tick(now_ns);
    }

    pub(crate) fn arrival(&self, sampled_now_ns: u64) -> bool {
        self.observe(sampled_now_ns, Observation::Arrival)
    }

    pub(crate) fn check(&self, sampled_now_ns: u64) -> bool {
        self.observe(sampled_now_ns, Observation::Check)
    }

    fn observe(&self, now_ns: u64, observation: Observation) -> bool {
        let now_tick = self.config.tick(now_ns);
        let mut word = self.word.load(Ordering::Relaxed);
        loop {
            let Some((next, counted_now)) = self.transition(word, now_tick, observation) else {
                return false;
            };
            // The successful CAS linearizes this counting-state operation.
            // No payload or other memory is synchronized by this word, so relaxed
            // ordering suffices. Strong CAS avoids gratuitous spurious retries;
            // contention still has progress-dependent retries, not a fixed bound.
            match self.word.compare_exchange(word, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return counted_now,
                Err(actual) => word = actual,
            }
        }
    }

    fn transition(&self, word: u32, now_tick: u32, observation: Observation) -> Option<(u32, bool)> {
        let age = now_tick.wrapping_sub(word & TICK_MASK) & TICK_MASK;
        if age == 0 || age >= HALF_RANGE {
            // Equal/older sampled arrival or check cannot move the state back.
            // This also illustrates why missed servicing is not diagnosable here.
            return None;
        }
        let already_counted = word & COUNTED != 0;
        match observation {
            Observation::Arrival => Some((now_tick, !already_counted && age > self.config.cutoff_ticks)),
            Observation::Check => {
                let counted_now = !already_counted && age > self.config.cutoff_ticks;
                if !already_counted && !counted_now {
                    return None;
                }
                if age >= REFRESH_AGE {
                    // N2: keep a counted origin in the past, so a bounded-delay
                    // arrival sampled before this refresh can still advance it.
                    // N6: first count AND refresh are one indivisible transition.
                    let refreshed = now_tick.wrapping_sub(PAST_OFFSET) & TICK_MASK;
                    Some((refreshed | COUNTED, counted_now))
                } else if counted_now {
                    Some((word | COUNTED, true))
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "deadline_monitor/deadline_monitor_tests.rs"]
mod tests;

// Original counting logic retained; imports/trace comments updated. UNCOMPILED.
#[path = "deadline_monitor/count_schedule.rs"]
mod count_schedule;
pub(crate) use count_schedule::{CountSchedule, WaitRequest};
#[cfg(test)]
#[path = "deadline_monitor/counter_publication.rs"]
mod counter_publication;
#[cfg(test)]
#[path = "deadline_monitor/count_schedule_tests.rs"]
mod count_schedule_tests;
