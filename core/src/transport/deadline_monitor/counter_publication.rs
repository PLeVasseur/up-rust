//! Isolated, UNCOMPILED child of the pinned deadline-monitor module.
//! Calls the actual SDK counter API; no mock counter and no new counter format.
//!
//! This closes an integration opportunity to discard a winning bool. It DOES
//! NOT make monitor CAS and counter CAS one transaction, nor bound preemption.
//! Both methods must finish by the count deadline for timing acceptance.

use super::DeadlineMonitor;
use crate::transport::report::{add, Counter};

impl DeadlineMonitor {
    /// Record the report's sampled time and publish any winning closed-gap count.
    /// Every real arrival must also participate in the transport's notification
    /// handshake after the word update, including when this returns false.
    /// `counter` must be this route's deadline_missed counter for correct metrics;
    /// a wrong counter is a logic error, never an unchecked memory obligation.
    // [impl->req~ssr-qos-deadline~3]
    // Winning-count publication; completing it by the physical bound remains unproved.
    pub(crate) fn arrival_and_publish(&self, sampled_ns: u64, counter: &Counter<true>) -> bool {
        let won = self.arrival(sampled_ns);
        if won {
            add(counter);
        }
        won
    }

    /// Service one route and complete its saturating counter update before
    /// returning. The surrounding pass may advance its completed-sample baseline
    /// only after ALL routes' calls and publications have completed.
    // [impl->req~ssr-qos-deadline~3]
    // Winning-count publication; no atomicity with the monitor word is claimed.
    pub(crate) fn check_and_publish(&self, sampled_ns: u64, counter: &Counter<true>) -> bool {
        let won = self.check(sampled_ns);
        if won {
            add(counter);
        }
        won
    }
}
