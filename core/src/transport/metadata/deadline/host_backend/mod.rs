//! Shared deadline state transitions for the admitted host atomic backend. No dependency, allocation, foreign lock, caller registry,
//! descriptor slot, or shortened timestamp/count field is used.
use crate::platform::atomic::{AtomicBool, Ordering};
use crate::platform::deadline_atomic128::{Atomic128, AtomicProfile};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeadlineReport {
    pub origin_ns: u64,
    pub missed_gaps: u128,
    pub missing_sample: bool,
    pub count_exhausted: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CountSchedule {
    Recheck,
    Events {
        first_eligible_ns: Option<u128>,
        maintenance_ns: u128,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Stable {
    origin: u64,
    counted: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Marker {
    old: u64,
    target: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Stable(Stable),
    Due(Marker),
}

impl Stable {
    fn word(self) -> (u64, u64) {
        if !self.counted {
            (self.origin, self.origin)
        } else if self.origin == u64::MAX {
            (u64::MAX, 1)
        } else {
            (self.origin.wrapping_add(1), self.origin)
        }
    }
}
impl Marker {
    // Only due operations install a marker. A due decision requires sample >
    // old, hence old < MAX. Arrival target > old; a check has target == old.
    fn word(self) -> (u64, u64) {
        if self.target > self.old {
            (self.old, self.target)
        } else if self.old == u64::MAX - 1 {
            (u64::MAX, 0)
        } else {
            (self.old.wrapping_add(2), self.old)
        }
    }
    fn id(self) -> u64 {
        self.old.wrapping_add(1)
    }
    fn result(self) -> Stable {
        Stable {
            origin: self.target,
            counted: self.target == self.old,
        }
    }
}
// Total arithmetic below has the same result on the proved constructor/marker
// domain. Wrapping operators remove unreachable overflow panic branches; they
// are NOT permission for a wrapping generation or a truncated timestamp.
fn decode(word: (u64, u64)) -> State {
    let (a, b) = word;
    if a < b {
        State::Due(Marker { old: a, target: b })
    } else if a == b {
        State::Stable(Stable {
            origin: a,
            counted: false,
        })
    } else if a == b.wrapping_add(1) {
        State::Stable(Stable {
            origin: b,
            counted: true,
        })
    } else if word == (u64::MAX, 0) {
        State::Due(Marker {
            old: u64::MAX - 1,
            target: u64::MAX - 1,
        })
    } else if word == (u64::MAX, 1) {
        State::Stable(Stable {
            origin: u64::MAX,
            counted: true,
        })
    } else {
        // Reachable words in this remaining class are precisely (b+2,b),
        // b <= MAX-2. No wrapping tag or truncated field participates.
        State::Due(Marker { old: b, target: b })
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Point {
    AfterStableRead,
    AfterMarker,
    AfterHighRead,
    AfterLowRead,
    AfterLowCredit,
    BeforeFinalize,
    ReportAfterCount,
}
macro_rules! point {
    ($hook:ident, $point:ident) => {
        #[cfg(test)]
        if let Some(hook) = $hook.as_deref_mut() {
            hook(Point::$point);
        }
    };
}

/// One private host backend. Configuration is immutable; runtime exposes no
/// reset. Moving/resetting an unshared value requires exclusive Rust ownership.
/// The owning SDK DeadlineSlot must retain its permanent setup/runtime boundary.
pub struct HostDeadline {
    profile: AtomicProfile,
    shift: u32,
    cutoff: u64,
    maintenance: u64,
    // Protocol invariant: a stable full origin/counted pair, or one complete
    // immutable due marker. Stable pairs never recur. Marker old < MAX.
    origin: Atomic128,
    // Protocol invariant: (full 64-bit count limb, exact due id). ID 0 denotes
    // setup; subsequent IDs are old_origin+1, strictly increasing, never reset.
    // During a marker, low is updated before high; high's id commits both limbs
    // and any exhaustion latch. No high/low payload bits are used as tags.
    low: Atomic128,
    high: Atomic128,
    missing: AtomicBool,
    exhausted: AtomicBool,
}

impl HostDeadline {
    /// Construct from the SDK's checked configuration. MonitorConfig's private
    /// fields and checked constructor enforce shift<=53 and cutoff<=2048.
    pub(crate) fn new(
        config: crate::transport::deadline_monitor::MonitorConfig,
        origin: u64,
        profile: AtomicProfile,
    ) -> Self {
        Self {
            profile,
            shift: config.tick_shift(),
            cutoff: u64::from(config.cutoff_ticks()),
            maintenance: config.report().inner_wait_from_sample_ns,
            origin: Atomic128::new(
                Stable {
                    origin,
                    counted: false,
                }
                .word(),
                profile,
            ),
            low: Atomic128::new((0, 0), profile),
            high: Atomic128::new((0, 0), profile),
            missing: AtomicBool::new(false),
            exhausted: AtomicBool::new(false),
        }
    }
    pub(crate) fn rearm(&mut self, config: crate::transport::deadline_monitor::MonitorConfig, origin: u64) {
        *self = Self::new(config, origin, self.profile);
    }
    #[cfg(test)]
    pub(crate) fn seed_for_test(&mut self, count: u128) {
        self.low = Atomic128::new((count as u64, 0), self.profile);
        self.high = Atomic128::new(((count >> 64) as u64, 0), self.profile);
    }

    pub fn sample_missing(&self) {
        self.missing.store(true, Ordering::Release);
    }

    /// Same result as the base SDK observe: exactly the initiating winner
    /// returns true, even if its marker was completed by somebody else. Helpers
    /// never mirror to the legacy u32, which stays a delayed separate diagnostic.
    pub fn observe(&self, sample: u64, arrival: bool) -> bool {
        self.observe_inner(
            sample,
            arrival,
            #[cfg(test)]
            None,
        )
    }
    fn observe_inner(
        &self,
        sample: u64,
        arrival: bool,
        #[cfg(test)] mut hook: Option<&mut dyn FnMut(Point)>,
    ) -> bool {
        loop {
            let word = self.origin.load();
            let State::Stable(old) = decode(word) else {
                self.help_word(word);
                continue;
            };
            point!(hook, AfterStableRead);
            if sample <= old.origin {
                return false;
            }
            let next_tick = sample.wrapping_shr(self.shift);
            let old_tick = old.origin.wrapping_shr(self.shift);
            let due = !old.counted && next_tick > old_tick && next_tick - old_tick > self.cutoff;
            if !arrival && !due {
                return false;
            }
            if !due {
                if self.origin.cas(
                    word,
                    Stable {
                        origin: sample,
                        counted: false,
                    }
                    .word(),
                ) {
                    return false;
                }
                continue;
            }
            let marker = Marker {
                old: old.origin,
                target: if arrival { sample } else { old.origin },
            };
            // The ONLY publication of a due descriptor consumes its exact
            // stable uncounted old state, atomically including the full target.
            // No helper can reserve, discard or modify an unpublished proposal.
            if self.origin.cas(word, marker.word()) {
                point!(hook, AfterMarker);
                self.help(
                    marker,
                    #[cfg(test)]
                    None,
                );
                return true;
            }
        }
    }

    fn help_word(&self, word: (u64, u64)) {
        if let State::Due(marker) = decode(word) {
            self.help(
                marker,
                #[cfg(test)]
                None,
            );
        }
    }

    /// Completes this exact marker, or finds it already superseded. Constant
    /// algorithmic steps, conditional on admitted atomic primitive progress.
    /// No loop waits for an owner/helper; no destructor or callback is required.
    fn help(&self, marker: Marker, #[cfg(test)] mut hook: Option<&mut dyn FnMut(Point)>) {
        let id = marker.id();
        let high = self.high.load();
        point!(hook, AfterHighRead);
        if high.1 < id {
            let mut low = self.low.load();
            point!(hook, AfterLowRead);
            if low.1 < id {
                // The acquired marker follows all prior finalizations. If both
                // limbs still precede id, they are the exact pre-credit count.
                // A stale helper's CAS cannot overwrite any >=id tagged limb.
                let saturated = high.0 == u64::MAX && low.0 == u64::MAX;
                if saturated {
                    self.exhausted.store(true, Ordering::Release);
                }
                let next = if saturated {
                    u64::MAX
                } else {
                    low.0.wrapping_add(1)
                };
                let _ = self.low.cas(low, (next, id));
                // A strong CAS failure means another helper advanced low's id;
                // acquire/read-read coherence makes this reload at least that
                // advanced generation. The current marker is never reused.
                low = self.low.load();
            }
            point!(hook, AfterLowCredit);
            if low.1 == id {
                let carry = u64::from(low.0 == 0);
                // At carry=1 the old high is <MAX: MAX/MAX takes the saturation
                // branch without low wrap. saturating_add also makes the local
                // arithmetic total, including a helper resuming much later.
                let _ = self.high.cas(high, (high.0.saturating_add(carry), id));
            }
        }
        // If high was already >=id, or our high CAS lost, some helper has
        // committed this credit (and possibly many successors). Otherwise our
        // successful high CAS did. Exact marker equality prevents stale writes.
        point!(hook, BeforeFinalize);
        let _ = self.origin.cas(marker.word(), marker.result().word());
    }

    pub fn report(&self) -> DeadlineReport {
        loop {
            if let Some(report) = self.collect(
                #[cfg(test)]
                None,
            ) {
                return report;
            }
        }
    }
    fn collect(
        &self,
        #[cfg(test)] mut hook: Option<&mut dyn FnMut(Point)>,
    ) -> Option<DeadlineReport> {
        let first = self.origin.load();
        let State::Stable(state) = decode(first) else {
            self.help_word(first);
            return None;
        };
        let low = self.low.load().0;
        let high = self.high.load().0;
        point!(hook, ReportAfterCount);
        let exhausted = self.exhausted.load(Ordering::Acquire);
        let missing = self.missing.load(Ordering::Acquire);
        let second = self.origin.load();
        if first != second {
            self.help_word(second);
            return None;
        }
        // Same stable pair cannot recur. Acquiring any new limb/latch would
        // force the second origin observation past that credit's marker, so a
        // successful double collect contains one complete operation-boundary
        // state. Choose missing's load as the linearization point.
        Some(DeadlineReport {
            origin_ns: state.origin,
            missed_gaps: u128::from(low) | (u128::from(high) << 64),
            missing_sample: missing,
            count_exhausted: exhausted,
        })
    }

    pub fn count_schedule(&self, previous: u64, sample: u64) -> CountSchedule {
        loop {
            let word = self.origin.load();
            let State::Stable(state) = decode(word) else {
                self.help_word(word);
                continue;
            };
            if sample < previous || sample < state.origin {
                return CountSchedule::Recheck;
            }
            let first_eligible_ns = if state.counted {
                None
            } else {
                Some(
                    (u128::from(state.origin.wrapping_shr(self.shift))
                        + u128::from(self.cutoff)
                        + 1)
                    .wrapping_mul(1_u128.wrapping_shl(self.shift)),
                )
            };
            return CountSchedule::Events {
                first_eligible_ns,
                maintenance_ns: u128::from(previous) + u128::from(self.maintenance),
            };
        }
    }
}
