//! SDK-owned deadline execution. No application polling duty is introduced.
//! [impl->req~ssr-qos-deadline~3] [impl->req~ssr-atomic-windows~1]
//! Source only: PlatformInit must supply and qualify the prepared executor.

use super::deadline_monitor::WaitRequest;
use super::metadata::CoherentDeadline as DeadlineMonitor;
use super::deadline_timing::{self as timing, ArmedDeadline, DeadlineConfiguration,
    DeadlineTimingReport, ServiceRoute, TimingGap};
use super::error::StartupError;
use super::inbound::InboundDispatch;
use super::mode::RuntimeCore;
use crate::memory::{FixedVec, StartupMemory};
use crate::platform::{Monotonic, Platform, PlatformInit};
use crate::platform::atomic::Ordering;
use crate::signal::{SignalWait, WaitToken};

/// SDK-created view of the exact setup. No public constructor: safe platform
/// implementations may forward a request but cannot forge its binding or roster.
/// The pointers correlate one process-local instance; they do NOT authenticate
/// executable artifacts, middleware internals or scheduler configuration.
#[derive(Clone, Copy)]
pub struct DeadlineServiceRequest {
    pub(crate) configuration: DeadlineConfiguration,
    pub(crate) routes: &'static [ServiceRoute],
    pub(crate) binding_name: &'static str,
    pub(crate) binding_version: Option<&'static str>,
    pub(crate) shared: &'static super::mode::TransportShared,
    pub(crate) tx: &'static [&'static super::mode::TxRoute],
    pub(crate) rx: &'static [&'static dyn InboundDispatch],
    pub(crate) reports: &'static [super::report::RouteReport],
}

impl core::fmt::Debug for DeadlineServiceRequest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeadlineServiceRequest").field("binding", &self.binding_name)
            .field("version", &self.binding_version).field("routes", &self.routes).finish_non_exhaustive()
    }
}
impl DeadlineServiceRequest {
    pub(crate) fn same_request(&self, other: &Self) -> bool {
        core::ptr::eq(self.shared, other.shared) && core::ptr::eq(self.tx, other.tx)
            && core::ptr::eq(self.rx, other.rx) && core::ptr::eq(self.reports, other.reports)
            && self.binding_name == other.binding_name && self.binding_version == other.binding_version
            && self.routes == other.routes && self.configuration.identity == other.configuration.identity
            && self.configuration.arrivals == other.configuration.arrivals
            && self.configuration.requested_j_ns == other.configuration.requested_j_ns
    }
}

/// Opaque SDK preparation, replacing the open safe worker-identity trait.
/// A caller cannot mint this value from strings, safe callbacks or a numeric J.
/// Public Drop only parks and retains resources; it never joins/exits/frees.
/// The SDK owns explicit initialization rollback through a private transaction.
/// Forget is memory-safe because control, stack and entry data are permanent.
/// No preparation value appears in TransportRuntime.
pub struct PreparedDeadlineService {
    pub(crate) request: DeadlineServiceRequest,
    pub(crate) worker: crate::platform::prepared::Worker,
}

impl core::fmt::Debug for PreparedDeadlineService {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PreparedDeadlineService").finish_non_exhaustive()
    }
}

impl PreparedDeadlineService {
    #[cfg(any(all(target_os = "macos", target_pointer_width = "64"),
        all(target_os = "linux", target_env = "gnu", target_pointer_width = "64",
            any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64"))))]
    pub(crate) fn prepare_native(memory: &StartupMemory, native: &'static crate::platform::Native,
        request: DeadlineServiceRequest) -> Result<Self, StartupError>
    {
        // Correlation only: the independent deployment observer must authenticate
        // the actual platform/artifacts. No safe trait string establishes that.
        // Native::runtime copies its one SDK-owned canonical trait-object value;
        // setup and worker control use that same value, not fresh coercions.
        if !core::ptr::eq(request.shared.platform, native.runtime()) {
            return Err(timing::startup_error(TimingGap::WorkerRequestMismatch));
        }
        let worker = crate::platform::prepared::prepare(memory, native)?;
        Ok(Self { request, worker })
    }

    fn refuse(&mut self, gap: TimingGap) -> StartupError {
        let cleanup_code = match self.worker.cancel() {
            Err(TimingGap::WorkerCleanupFailed(code)) => Some(code), _ => None,
        };
        StartupError::DeadlineTiming { gap, cleanup_code }
    }

    pub(crate) fn activate(&mut self, entry: DeadlineServiceEntry) -> Result<(), TimingGap> {
        if !core::ptr::eq(self.request.shared, entry.core.shared)
            || !core::ptr::eq(self.request.rx, entry.core.rx)
            || !core::ptr::eq(self.request.tx, entry.core.tx) {
            return Err(TimingGap::WorkerRequestMismatch);
        }
        self.worker.activate(entry)
    }
    pub(crate) fn commit(&mut self) { self.worker.commit(); }
}

/// This guard never leaves SDK initialization; unlike the public opaque value,
/// it is authorized to join an unactivated worker on error/unwind. Its Drop is
/// resource cleanup only: all referenced memory is independently permanent.
pub(crate) struct StartupWorker { service: PreparedDeadlineService }
impl StartupWorker {
    pub(crate) fn activate(&mut self, entry: DeadlineServiceEntry) -> Result<(), TimingGap> {
        self.service.activate(entry)
    }
    pub(crate) fn commit(&mut self) { self.service.commit(); }
}
impl Drop for StartupWorker {
    fn drop(&mut self) { let _ = self.service.worker.cancel(); }
}

/// A concrete SDK entrypoint retained by a setup-created backend worker.
/// Non-Clone/non-Copy; it retains only permanent shared data, never a borrowed
/// guard whose destructor must stop a thread. Safe forget loses service, not memory.
pub struct DeadlineServiceEntry {
    core: &'static RuntimeCore,
}

impl core::fmt::Debug for DeadlineServiceEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeadlineServiceEntry").finish_non_exhaustive()
    }
}

impl DeadlineServiceEntry {
    pub(crate) fn new(core: &'static RuntimeCore) -> Self { Self { core } }
    pub(crate) fn core(&self) -> &'static RuntimeCore { self.core }

    /// Runs without a user waiter, receiver, counter reader or application tick.
    /// Only initialization failure cancellation ends this entry; no runtime
    /// teardown, thread creation, allocation or owning destructor is added here.
    pub fn run(self) {
        let core = self.core;
        let platform = core.shared.platform;
        let signal = &core.shared.deadline_signal;
        let mut previous = core.activation_sample_ns;
        loop {
            if core.shared.deadline_cancelled.load(Ordering::Acquire) { return; }
            // Snapshot BEFORE service/planning so a rearm/new RPC during either
            // scan prevents sleep. This signal is separate from public wait tokens.
            let token = signal.token();
            let receipt = service_pass(core, previous);
            previous = receipt.oldest_sample_ns;
            if core.shared.deadline_cancelled.load(Ordering::Acquire) { return; }
            let sampled = platform.now().0;
            match plan(core, previous, sampled, u64::MAX) {
                WaitRequest::ServiceNow => {},
                WaitRequest::Until(due) => {
                    let _ = signal.wait_on(platform, token, Some(Monotonic(due)));
                }
            }
        }
    }
}

/// Only service_pass constructs this receipt after every route, RPC expiry,
/// counter wrapper and requested wake has returned. It is not a physical WCET
/// certificate and does not assert completion of a concurrently stalled winner.
pub(crate) struct CompletedPass {
    // Retained complete-pass audit data, not another scheduler baseline.
    #[allow(dead_code)]
    pub(crate) previous_oldest_sample_ns: u64,
    pub(crate) oldest_sample_ns: u64,
    #[allow(dead_code)]
    pub(crate) completed_ns: u64,
    pub(crate) completed_items: usize,
}

pub(crate) fn service_pass(core: &'static RuntimeCore, previous: u64) -> CompletedPass {
    let platform = core.shared.platform;
    let started = platform.now().0;
    let mut oldest = None::<u64>;
    let mut total = 0_usize;
    for route in core.rx {
        // Each check receives its ACTUAL sample, not the start of an earlier scan.
        let sampled = platform.now();
        oldest = Some(oldest.map_or(sampled.0, |old| old.min(sampled.0)));
        let completed = route.service(sampled);
        total = total.saturating_add(completed);
        if completed != 0 { route.wake(); }
    }
    // No early return, partial pass, callback or caller can synthesize this record.
    CompletedPass {
        previous_oldest_sample_ns: previous,
        oldest_sample_ns: oldest.unwrap_or(started),
        completed_ns: platform.now().0,
        completed_items: total,
    }
}

pub(crate) fn plan(
    core: &'static RuntimeCore, previous: u64, sampled: u64, caller_deadline: u64,
) -> WaitRequest {
    let mut due = caller_deadline;
    for route in core.rx {
        // Scan time, RPC inspection and concurrent changes are charged to J;
        // this fresh sample does not replace the completed-pass baseline.
        let now = core.shared.platform.now().0;
        if now < sampled || now < previous { return WaitRequest::ServiceNow; }
        if let Some(schedule) = route.deadline_schedule(previous, now) {
            match schedule.wait_request(now, due) {
                WaitRequest::ServiceNow => return WaitRequest::ServiceNow,
                WaitRequest::Until(next) => due = due.min(next),
            }
        }
        if let Some(rpc) = route.rpc_deadline() { due = due.min(rpc.0); }
    }
    if due <= core.shared.platform.now().0 {
        WaitRequest::ServiceNow
    } else { WaitRequest::Until(due) }
}

/// Absolute caller deadline is computed exactly once. The ORIGINAL token is
/// preserved even after an internal timeout; a racing notification remains real.
pub(crate) fn wait(
    core: &'static RuntimeCore, token: WaitToken, timeout: core::time::Duration,
) -> SignalWait {
    let platform = core.shared.platform;
    let signal = core.shared.signal();
    if signal.inspect(token) == Some(SignalWait::WrongSignal) {
        return SignalWait::WrongSignal;
    }
    let nanos = u64::try_from(timeout.as_nanos()).unwrap_or(u64::MAX);
    let caller_deadline = platform.now().saturating_add(nanos).0;
    let mut previous = core.activation_sample_ns;
    loop {
        let receipt = service_pass(core, previous);
        previous = receipt.oldest_sample_ns;
        // Notification precedence also covers zero budget and timeout/service races.
        if let Some(outcome) = signal.inspect(token) { return outcome; }
        let now = platform.now().0;
        if now >= caller_deadline {
            // Recheck at the return boundary, matching Signal's timeout semantics.
            return signal.inspect(token).unwrap_or(SignalWait::TimedOut);
        }
        match plan(core, previous, now, caller_deadline) {
            WaitRequest::ServiceNow => continue,
            WaitRequest::Until(due) => {
                match signal.wait_on(platform, token, Some(Monotonic(due))) {
                    SignalWait::WrongSignal => return SignalWait::WrongSignal,
                    SignalWait::Notified => {
                        let _ = service_pass(core, previous);
                        return SignalWait::Notified;
                    }
                    SignalWait::TimedOut => {}, // internal expiry is not a public timeout
                }
            }
        }
    }
}

struct PendingMonitor {
    route: &'static dyn InboundDispatch,
    // Exclusive reference exists only during initialization; never copied/shared.
    value: &'static mut ArmedDeadline,
}

pub(crate) struct PreparedTiming {
    worker: Option<StartupWorker>,
    pending: FixedVec<PendingMonitor>,
}

impl PreparedTiming {
    /// Reserves ALL storage and obtains the actual worker before the first arm.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare(
        memory: &StartupMemory,
        init: &'static dyn PlatformInit,
        rx: &'static [&'static dyn InboundDispatch],
        configuration: Option<DeadlineConfiguration>,
        binding_name: &'static str,
        binding_version: Option<&'static str>,
        shared: &'static super::mode::TransportShared,
        tx: &'static [&'static super::mode::TxRoute],
        reports: &'static [super::report::RouteReport],
    ) -> Result<Self, StartupError> {
        let monitored = rx.iter().any(|r| r.common().qos.deadline.duration().is_some());
        let mut pending = memory.fixed_vec::<PendingMonitor>(rx.len())?;
        if !monitored { return Ok(Self { worker: None, pending }); }
        let config = configuration.ok_or_else(|| timing::startup_error(TimingGap::ConfigurationMissing))?;
        let mut actual = memory.fixed_vec::<ServiceRoute>(rx.len())?;
        for route in rx {
            let report = route.report();
            let d = report.qos.deadline.duration().map(|v| u64::try_from(v.as_nanos()))
                .transpose().map_err(|_| timing::startup_error(TimingGap::InvalidDeadline))?;
            let mut matches = config.arrivals.iter().filter(|a| a.route == route.id());
            let arrival = matches.next().copied();
            if matches.next().is_some() {
                return Err(timing::startup_error(TimingGap::RouteConfigurationMismatch));
            }
            actual.push(ServiceRoute { route: route.id(), deadline_ns: d, arrival,
                rpc_slots: route.rpc_capacity(), slots: report.slots,
                slot_len: report.slot_len, depth: report.qos.depth })?;
        }
        let actual = actual.into_static_slice();
        // Validate every exact monitor domain before invoking external setup code.
        for route in actual { let _ = timing::monitor_config(route).map_err(timing::startup_error)?; }
        let request = DeadlineServiceRequest { configuration: config, routes: actual,
            binding_name, binding_version, shared, tx, rx, reports };
        let mut worker = StartupWorker { service: init.prepare_deadline_service(memory, request)? };
        let j = match timing::qualify_prepared(&request, &worker.service) {
            Ok(j) => j,
            Err(gap) => return Err(worker.service.refuse(gap)),
        };
        for (route, input) in rx.iter().zip(actual) {
            if let Some(monitor_config) = timing::monitor_config(input).map_err(timing::startup_error)? {
                let mut report = DeadlineTimingReport::qualified(monitor_config, j)
                    .map_err(timing::startup_error)?;
                report.requested_j_ns = config.requested_j_ns;
                let value = memory.place(ArmedDeadline {
                    monitor: DeadlineMonitor::new(monitor_config, 0), report,
                })?;
                pending.push(PendingMonitor { route: *route, value })?;
            }
        }
        Ok(Self { worker: Some(worker), pending })
    }

    /// Consumes setup-only exclusive owners; no shared-to-mutable cast or live
    /// reset is used. Each `Platform::now` is an open safe callback, including
    /// between publications: it may reenter, panic or stall. The current and
    /// remaining monitors are still exclusively owned, already-published monitors
    /// have permanent shared backing, and reentry cannot acquire this consumed
    /// setup's unpublished CoreCell. Unwind may leak storage or withhold progress
    /// but does not invalidate published references. Clock callback work is part
    /// of activation/J and R61 qualification; memory safety alone does not prove
    /// no allocation, no panic, timely activation or service progress.
    pub(crate) fn arm(self, platform: &dyn Platform)
        -> Result<(Option<StartupWorker>, u64), StartupError>
    {
        let mut oldest = None::<u64>;
        for pending in self.pending {
            let sample = platform.now().0;
            pending.value.monitor.arm(sample);
            oldest = Some(oldest.map_or(sample, |old| old.min(sample)));
            pending.route.common().monitor.publish(pending.value)
                .map_err(timing::startup_error)?;
        }
        Ok((self.worker, oldest.unwrap_or_else(|| platform.now().0)))
    }
}
