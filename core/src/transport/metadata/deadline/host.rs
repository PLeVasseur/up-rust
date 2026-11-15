//! Admitted host adapter to the common SDK deadline call shape.
//! Other profiles retain their existing implementations and qualification gates.
#[path = "host_backend/mod.rs"]
mod deadline_host;
use super::{CountSchedule, Counter, DeadlineReport, MonitorConfig};
use deadline_host::HostDeadline;

pub(crate) struct CoherentDeadline {
    config: MonitorConfig,
    state: HostDeadline,
}
impl CoherentDeadline {
    pub(crate) fn try_new(config: MonitorConfig, origin_ns: u64) -> Option<Self> {
        let profile = crate::platform::deadline_atomic128::AtomicProfile::admit()?;
        Some(Self { config, state: HostDeadline::new(config, origin_ns, profile) })
    }
    #[cfg(test)]
    pub(crate) fn new(config: MonitorConfig, origin_ns: u64) -> Self {
        Self::try_new(config, origin_ns).expect("test requires admitted host CPU")
    }
    pub(crate) fn arm(&mut self, origin_ns: u64) {
        self.state.rearm(self.config, origin_ns);
    }
    // [impl->req~ssr-qos-deadline~3]
    pub(crate) fn arrival_and_publish(&self, sampled_ns: u64, counter: &Counter<true>) -> bool {
        let won = self.state.observe(sampled_ns, true);
        if won {
            super::add(counter);
        }
        won
    }
    // [impl->req~ssr-qos-deadline~3]
    pub(crate) fn check_and_publish(&self, sampled_ns: u64, counter: &Counter<true>) -> bool {
        let won = self.state.observe(sampled_ns, false);
        if won {
            super::add(counter);
        }
        won
    }
    pub(crate) fn sample_missing(&self) {
        self.state.sample_missing();
    }
    pub(crate) fn report(&self) -> DeadlineReport {
        let value = self.state.report();
        DeadlineReport {
            origin_ns: value.origin_ns,
            missed_gaps: value.missed_gaps,
            missing_sample: value.missing_sample,
            count_exhausted: value.count_exhausted,
        }
    }
    pub(crate) fn count_schedule(&self, previous: u64, sampled: u64) -> CountSchedule {
        match self.state.count_schedule(previous, sampled) {
            deadline_host::CountSchedule::Recheck => CountSchedule::Recheck,
            deadline_host::CountSchedule::Events {
                first_eligible_ns,
                maintenance_ns,
            } => CountSchedule::Events {
                first_eligible_ns,
                maintenance_ns,
            },
        }
    }
    #[cfg(test)]
    pub(super) fn seed_for_test(&mut self, count: u128) {
        self.state.seed_for_test(count);
    }
}

#[cfg(test)]
mod successor_controls {
    use super::*;
    // [utest->req~ssr-qos-deadline~3] [utest->req~ssr-atomic-width~1]
    #[test]
    fn sdk_host_layout_full_count_and_legacy_call_shape() {
        extern crate std;
        assert_eq!(
            crate::platform::deadline_atomic128::Atomic128::layout(),
            (16 << 32) | 16
        );
        assert!(!core::mem::needs_drop::<CoherentDeadline>());
        let mut monitor = CoherentDeadline::new(MonitorConfig::new(1024, 0).unwrap(), 0);
        monitor.seed_for_test(u128::MAX - 1);
        let legacy = Counter::<true>::default();
        assert!(monitor.arrival_and_publish(2048, &legacy));
        assert_eq!(monitor.report().missed_gaps, u128::MAX);
        assert!(!monitor.report().count_exhausted);
        monitor.sample_missing();
        assert!(monitor.check_and_publish(4096, &legacy));
        assert!(monitor.report().count_exhausted && monitor.report().missing_sample);
        assert!(!monitor.arrival_and_publish(8192, &legacy));
        assert_eq!(legacy.get(), 2);
        std::println!(
            "CoherentDeadline_bytes={} HostDeadline_bytes={} MonitorConfig_bytes={}",
            core::mem::size_of::<CoherentDeadline>(),
            core::mem::size_of::<HostDeadline>(),
            core::mem::size_of::<MonitorConfig>()
        );
    }
}
