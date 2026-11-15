//! Joint route-deadline publication under the SDK-owned metadata boundary.
//!
//! This process-local record is retained by one permanently published
//! ArmedDeadline. No caller holds a mutation ticket, mutable state reference,
//! execution guard or candidate to commit later. A sampled clock value is only
//! input: every decision uses the complete current record inside one interval.
//! This removes the split monitor/count publication, not the separate timing,
//! underlying-transport arrival, restart or target qualification obligations.

#[cfg(not(any(all(target_os = "macos", target_arch = "aarch64"), all(target_os = "linux", target_env = "gnu", any(target_arch = "x86_64", target_arch = "aarch64")), all(target_os = "windows", target_env = "msvc", target_arch = "x86_64"))))]
use super::Leaf;
use crate::transport::deadline_monitor::{CountSchedule, MonitorConfig};
use crate::transport::report::{add, Counter, DeadlineReport};

#[cfg(not(any(all(target_os = "macos", target_arch = "aarch64"), all(target_os = "linux", target_env = "gnu", any(target_arch = "x86_64", target_arch = "aarch64")), all(target_os = "windows", target_env = "msvc", target_arch = "x86_64"))))]
#[derive(Clone, Copy)]
struct Record {
    origin_ns: u64,
    counted: bool,
    missed_gaps: u128,
    missing_sample: bool,
    count_exhausted: bool,
}

#[cfg(not(any(all(target_os = "macos", target_arch = "aarch64"), all(target_os = "linux", target_env = "gnu", any(target_arch = "x86_64", target_arch = "aarch64")), all(target_os = "windows", target_env = "msvc", target_arch = "x86_64"))))]
pub(crate) struct CoherentDeadline {
    config: MonitorConfig,
    state: Leaf<Record>,
}

#[cfg(not(any(all(target_os = "macos", target_arch = "aarch64"), all(target_os = "linux", target_env = "gnu", any(target_arch = "x86_64", target_arch = "aarch64")), all(target_os = "windows", target_env = "msvc", target_arch = "x86_64"))))]
impl CoherentDeadline {
    #[cfg(test)]
    fn seed_for_test(&mut self, count: u128) {
        self.state.data.get_mut().missed_gaps = count;
    }
    pub(crate) fn try_new(config: MonitorConfig, origin_ns: u64) -> Option<Self> {
        Some(Self::new(config, origin_ns))
    }
    /// Construct unpublished storage. Qualification and permanent publication
    /// remain the owning DeadlineSlot's responsibility.
    pub(crate) fn new(config: MonitorConfig, origin_ns: u64) -> Self {
        Self {
            config,
            state: Leaf::new(Record {
                origin_ns,
                counted: false,
                missed_gaps: 0,
                missing_sample: false,
                count_exhausted: false,
            }),
        }
    }

    /// Exclusive setup-only reset. DeadlineSlot::publish consumes the permanent
    /// shared borrow, so safe runtime callers cannot obtain this &mut self.
    pub(crate) fn arm(&mut self, origin_ns: u64) {
        *self = Self::new(self.config, origin_ns);
    }

    /// Update the full report before mirroring its winning credit to the legacy
    /// u32 diagnostic. The legacy field remains an independent, possibly delayed
    /// observation; it is never part of DeadlineReport's coherence promise.
    // [impl->req~ssr-qos-deadline~3]
    pub(crate) fn arrival_and_publish(&self, sampled_ns: u64, counter: &Counter<true>) -> bool {
        let won = self.observe(Some(sampled_ns), true);
        if won {
            add(counter);
        }
        won
    }

    /// Complete the gap decision and full-width credit in the same interval.
    // [impl->req~ssr-qos-deadline~3]
    pub(crate) fn check_and_publish(&self, sampled_ns: u64, counter: &Counter<true>) -> bool {
        let won = self.observe(Some(sampled_ns), false);
        if won {
            add(counter);
        }
        won
    }

    /// Latch unavailable arrival evidence without inventing a timestamp or
    /// rearming the existing gap. This is a health observation, not an arrival.
    pub(crate) fn sample_missing(&self) {
        let _ = self.observe(None, true);
    }

    fn observe(&self, sample: Option<u64>, arrival: bool) -> bool {
        // Compute immutable scalar policy outside the protected reference.
        let shift = self.config.tick_shift();
        let cutoff = u64::from(self.config.cutoff_ticks());
        // SAFETY: Leaf::apply requires a crate-controlled, non-reentrant closure
        // returning only owned copied metadata. This nested closure reads/writes
        // only scalar fields of Record, matches a copied Option<u64>, and calls
        // core integer checked_add. It performs no callback, nested leaf access,
        // destructor, handoff or reference escape. shift is constructor-bounded
        // to 0..=53; subtraction is dominated by next_tick > origin_tick. No
        // intermediate record can be observed before apply ends its borrow.
        unsafe {
            self.state.apply(|record| {
                let Some(sampled_ns) = sample else {
                    record.missing_sample = true;
                    return false;
                };
                if sampled_ns <= record.origin_ns {
                    return false;
                }
                let next_tick = sampled_ns >> shift;
                let origin_tick = record.origin_ns >> shift;
                let due = !record.counted
                    && next_tick > origin_tick
                    && next_tick - origin_tick > cutoff;
                if due {
                    match record.missed_gaps.checked_add(1) {
                        Some(next) => record.missed_gaps = next,
                        None => record.count_exhausted = true,
                    }
                    record.counted = true;
                }
                if arrival {
                    // Keep the full canonical timestamp. The quantized value is
                    // derived only when comparing; no truncating epoch selector.
                    record.origin_ns = sampled_ns;
                    record.counted = false;
                }
                due
            })
        }
    }

    /// Copies every report field at one operation boundary. This observation
    /// performs no service/check and promises no physical observation deadline.
    pub(crate) fn report(&self) -> DeadlineReport {
        // SAFETY: the nested closure copies initialized integer/bool fields into
        // an owned Copy report. It invokes no other code, creates no escaping
        // reference and cannot reenter any Leaf. Leaf::apply retains exclusion
        // through the last copy, so all fields describe the same record state.
        unsafe {
            self.state.apply(|record| DeadlineReport {
                origin_ns: record.origin_ns,
                missed_gaps: record.missed_gaps,
                missing_sample: record.missing_sample,
                count_exhausted: record.count_exhausted,
            })
        }
    }

    /// One coherent planning snapshot; no reference or delayed mutation ticket
    /// escapes. The existing signal handshake must cover a later arrival.
    pub(crate) fn count_schedule(&self, previous_ns: u64, sampled_ns: u64) -> CountSchedule {
        // SAFETY: this nested closure returns a Copy record by value and invokes
        // no callbacks/helpers/drop/handoff. It cannot leak a protected reference
        // or reenter a Leaf. Scheduling arithmetic runs after the borrow ends.
        let record = unsafe { self.state.apply(|record| *record) };
        if sampled_ns < previous_ns || sampled_ns < record.origin_ns {
            return CountSchedule::Recheck;
        }
        let shift = self.config.tick_shift();
        let quantum = 1_u128 << shift;
        let first_eligible_ns = if record.counted {
            None
        } else {
            Some((u128::from(record.origin_ns >> shift)
                + u128::from(self.config.cutoff_ticks()) + 1) * quantum)
        };
        CountSchedule::Events {
            first_eligible_ns,
            // Retain the existing service cadence. Full-width origin eliminates
            // modular-refresh aliasing but supplies no autonomous-service bound.
            maintenance_ns: u128::from(previous_ns)
                + u128::from(self.config.report().inner_wait_from_sample_ns),
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(any(all(target_os = "macos", target_arch = "aarch64"), all(target_os = "linux", target_env = "gnu", any(target_arch = "x86_64", target_arch = "aarch64")), all(target_os = "windows", target_env = "msvc", target_arch = "x86_64")))]
#[path = "deadline/host.rs"]
mod host;
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"), all(target_os = "linux", target_env = "gnu", any(target_arch = "x86_64", target_arch = "aarch64")), all(target_os = "windows", target_env = "msvc", target_arch = "x86_64")))]
pub(crate) use host::CoherentDeadline;
