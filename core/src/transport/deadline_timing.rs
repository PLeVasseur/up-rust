//! Explicit J admission. Source candidate, not a qualified platform implementation.
//! [impl->req~ssr-qos-deadline~3] Partial contribution; accepted J amendment
//! is recorded separately in OWNER-ACCEPTANCE.md, not an invented spec revision.

use super::deadline_monitor::{DeadlineMonitor, MonitorConfig, CountSchedule};
use super::error::StartupError;
use super::report::{Counter, RouteId};
use crate::platform::atomic::{AtomicPtr, Ordering};

/// Exact build and deployment identity. These are identifiers, not validation.
/// The qualification catalog binds their contents to reviewed proof artifacts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimingIdentity {
    /// Digest identifying the SDK source in the qualification record.
    pub sdk_source: [u8; 32],
    /// Digest identifying the compiled artifact in the qualification record.
    pub compiled_artifact: [u8; 32],
    /// Digest identifying the selected binding source.
    pub binding_source: [u8; 32],
    /// Digest identifying the selected platform source.
    pub platform_source: [u8; 32],
    /// Digest identifying the hardware and atomic ABI configuration.
    pub hardware_and_atomic_abi: [u8; 32],
    /// Digest identifying the clock, timer and scheduler configuration.
    pub clock_timer_scheduler: [u8; 32],
    /// Digest identifying the complete deployment configuration.
    pub complete_configuration: [u8; 32],
}

/// Route-specific arrival contract; no implicit L=0. L retains its established
/// meaning and must cover binding plus recording delay under the exact proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArrivalTiming {
    /// Route whose configuration or arrival contract is described.
    pub route: RouteId,
    /// Complete admitted arrival latency L, in nanoseconds.
    pub arrival_latency_ns: u64,
    /// Binding portion of the arrival latency, in nanoseconds.
    pub declared_binding_latency_ns: u64,
    /// Delay to record an arrival, in nanoseconds.
    pub recording_delay_ns: u64,
}

/// Direct construction and config loading produce this same non-generic value.
/// A requested J is only a proposal; it cannot construct a qualification.
#[derive(Clone, Copy, Debug)]
pub struct DeadlineConfiguration {
    /// Exact source, artifact and deployment identity to match.
    pub identity: TimingIdentity,
    /// Explicit arrival contracts for the configured routes.
    pub arrivals: &'static [ArrivalTiming],
    /// Proposed service/publication delay J, in nanoseconds; not acceptance.
    pub requested_j_ns: Option<u128>,
}

/// The actual complete service order, including routes without a QoS monitor.
/// RPC and notification costs cannot disappear by omitting a monitored route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceRoute {
    /// Route whose configuration or arrival contract is described.
    pub route: RouteId,
    /// Configured deadline D, in nanoseconds, when enabled.
    pub deadline_ns: Option<u64>,
    /// Arrival contract supplied for this route, if any.
    pub arrival: Option<ArrivalTiming>,
    /// RPC call slots included in the service workload.
    pub rpc_slots: usize,
    /// Reserved slots included in this route workload.
    pub slots: usize,
    /// Bytes per reserved slot.
    pub slot_len: usize,
    /// Configured route queue depth.
    pub depth: u16,
}

/// A missing or mismatched prerequisite that prevents timing admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimingGap {
    /// No deadline configuration was supplied.
    ConfigurationMissing,
    /// An arrival contract needed by a route is missing.
    ArrivalEvidenceMissing,
    /// No supported prepared service backend is available.
    BackendMissing,
    /// No accepted timing qualification matches this deployment.
    QualificationMissing,
    /// Source, artifact or deployment identities do not match.
    IdentityMismatch,
    /// The complete route configuration does not match its proof.
    RouteConfigurationMismatch,
    /// A checked timing calculation overflowed.
    ArithmeticOverflow,
    /// Binding and recording delays do not match the arrival budget.
    ArrivalBudgetMismatch,
    /// The supplied deadline cannot form a valid monitor configuration.
    InvalidDeadline,
    /// The monitor was already armed.
    AlreadyArmed,
    /// Prepared worker activation failed.
    WorkerActivationFailed,
    /// The prepared worker does not correlate with this activation request.
    WorkerRequestMismatch,
    /// Initialization cleanup failed with the retained backend error code.
    WorkerCleanupFailed(i32),
    /// No accepted observer established the actual deployment.
    DeploymentUnobserved,
    /// The binding identity does not match the qualification.
    BindingMismatch,
    /// The observed worker configuration does not match the qualification.
    WorkerConfigurationMismatch,
    /// Evidence identifiers do not match the accepted proof record.
    EvidenceMismatch,
}

/// Provenance for the complete theorem, not a percentile or a single run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimingEvidencePins {
    /// Digest of the source-to-binary correspondence evidence.
    pub source_and_binary_correspondence: [u8; 32],
    /// Digest of worst-case work and progress evidence for every winner.
    pub all_winner_wcet_and_progress: [u8; 32],
    /// Digest of activation and autonomous service evidence.
    pub activation_and_no_waiter_service: [u8; 32],
    /// Digest of R56 arrival and recording evidence.
    pub r56_arrival_and_recording: [u8; 32],
    /// Digest of R61 pending-observation and freshness evidence.
    pub r61_pending_and_freshness: [u8; 32],
    /// Digest of counter visibility and qualifying-read evidence.
    pub counter_visibility_and_read_contract: [u8; 32],
    /// Digest of runtime memory and panic-freedom evidence.
    pub runtime_memory_and_panic: [u8; 32],
    /// Digest of independent acceptance of the complete proof.
    pub independent_acceptance: [u8; 32],
}

/// Conservative upper bounds; each covers every permitted arrival/check winner.
/// Overlapping terms may be conservatively charged twice, never silently omitted.
#[derive(Clone, Copy, Debug)]
struct DelayTerms {
    installation_and_first_arm: u128,
    dispatch_lateness: u128,
    planning_and_preceding_work: u128,
    route_rpc_and_notifications: u128,
    clock_monitor_retries: u128,
    winner_preemption: u128,
    counter_retries: u128,
    qualifying_visibility: u128,
}

impl DelayTerms {
    fn total(self) -> Option<u128> {
        self.installation_and_first_arm
            .checked_add(self.dispatch_lateness)?
            .checked_add(self.planning_and_preceding_work)?
            .checked_add(self.route_rpc_and_notifications)?
            .checked_add(self.clock_monitor_retries)?
            .checked_add(self.winner_preemption)?
            .checked_add(self.counter_retries)?
            .checked_add(self.qualifying_visibility)
    }
}

/// Crate-owned reviewed records only. No public constructor, serde, macro,
/// callback, safe trait assertion, or supplied number can add an accepted record.
struct AcceptedTiming {
    identity: TimingIdentity,
    service_owner: &'static str,
    routes: &'static [ServiceRoute],
    terms: DelayTerms,
    j_ns: u128,
    read_contract: CounterReadContract,
    pins: TimingEvidencePins,
    binding_name: &'static str,
    binding_version: Option<&'static str>,
    worker: crate::platform::prepared::WorkerObservation,
    // Only crate-owned, independently reviewed observers may occupy this slot.
    // No Platform/BindingSetup trait result or application callback installs one.
    observe: Option<DeploymentObserver>,
}

// Deliberately empty: a pthread service is implemented; NO finite physical J is qualified.
// Adding an entry requires the independent, source/platform/configuration-bound
// proof described in TIMING-EVIDENCE.md. This is an evidence gate, not a new
// supported-target/deployment exclusion or permission to select a J value.
static ACCEPTED_TIMING: &[AcceptedTiming] = &[];

/// Read-only proof reference. Constructible only after matching a catalog entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedJ {
    ns: u128,
    pins: TimingEvidencePins,
    identity: TimingIdentity,
    service_owner: &'static str,
    read_contract: CounterReadContract,
}

impl ValidatedJ {
    /// Returns the validated service/publication delay J in nanoseconds.
    pub fn nanoseconds(self) -> u128 { self.ns }
    /// Returns the identifiers of the accepted proof artifacts.
    pub fn evidence(self) -> TimingEvidencePins { self.pins }
    /// Returns the source, artifact and deployment identity matched by admission.
    pub fn identity(self) -> TimingIdentity { self.identity }
    /// Returns the service owner named by the accepted record.
    pub fn service_owner(self) -> &'static str { self.service_owner }
    /// Returns the required qualifying counter-read observation boundary.
    pub fn read_contract(self) -> CounterReadContract { self.read_contract }
}

/// The actual load must meet the proof's named observation boundary. Neither
/// enum variant nor an Acquire load alone establishes the required ordering.
/// No rule is selected for a deployment by this source-only candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CounterReadContract {
    /// The counter publication happens before this load under the Rust model.
    /// This does not assert physical visibility for arbitrary other-core reads.
    OrderedAfterPublication,
    /// The exact hardware/backend/load instruction and scheduling configuration
    /// prove the visibility and qualifying-next-read rule in the pinned evidence.
    TargetCoherentLoad,
}

/// Qualification state reported for a configured deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimingStatus {
    /// This route has no configured deadline monitor.
    Disabled,
    /// Timing lacks the named prerequisite.
    Incomplete(TimingGap),
    /// A matching accepted proof supplies the validated J and read contract.
    Qualified(ValidatedJ),
}

/// Threshold and quantization remain distinct from a qualified reporting bound.
/// None never means zero, infinity, "best effort", or an accepted restriction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeadlineTimingReport {
    /// Configured deadline D, in nanoseconds, when enabled.
    pub deadline_ns: Option<u128>,
    /// Complete admitted arrival latency L, in nanoseconds.
    pub arrival_latency_ns: Option<u64>,
    /// Derived detection threshold, in nanoseconds, when available.
    pub threshold_ns: Option<u64>,
    /// Monitor quantization interval, in nanoseconds, when available.
    pub quantum_ns: Option<u64>,
    /// Derived reporting resolution, in nanoseconds, when available.
    pub resolution_ns: Option<u128>,
    /// R61 all-times completed-sample age requirement; not a measured result.
    pub completed_sample_max_age_ns: Option<u128>,
    /// Strict sample-to-CAS limit; includes every pending arrival/check winner.
    pub pending_observation_limit_ns: Option<u128>,
    /// Internal wait span anchored to the preceding completed pass's sample.
    pub service_wait_span_ns: Option<u64>,
    /// Derived guaranteed detection gap, in nanoseconds, when available.
    pub guaranteed_gap_ns: Option<u128>,
    /// Proposed service/publication delay J, in nanoseconds; not acceptance.
    pub requested_j_ns: Option<u128>,
    /// Catalog-validated J, in nanoseconds, only after qualification.
    pub validated_j_ns: Option<u128>,
    /// Qualified D + 2L + resolution + J bound, in nanoseconds.
    pub reporting_bound_ns: Option<u128>,
    /// Observation boundary required by the qualified counter-read proof.
    pub qualifying_read: Option<CounterReadContract>,
    /// Whether timing is disabled, incomplete or qualified.
    pub status: TimingStatus,
}

impl DeadlineTimingReport {
    pub(crate) fn pending(deadline: Option<core::time::Duration>) -> Self {
        Self {
            deadline_ns: deadline.map(|d| d.as_nanos()),
            arrival_latency_ns: None, threshold_ns: None, quantum_ns: None,
            resolution_ns: None, completed_sample_max_age_ns: None,
            pending_observation_limit_ns: None, service_wait_span_ns: None,
            guaranteed_gap_ns: None, requested_j_ns: None,
            validated_j_ns: None, reporting_bound_ns: None,
            qualifying_read: None,
            status: if deadline.is_some() {
                TimingStatus::Incomplete(TimingGap::QualificationMissing)
            } else { TimingStatus::Disabled },
        }
    }

    pub(crate) fn qualified(config: MonitorConfig, j: ValidatedJ) -> Result<Self, TimingGap> {
        let facts = config.report();
        let bound = facts.detection_bound_ns.checked_add(j.ns)
            .ok_or(TimingGap::ArithmeticOverflow)?;
        Ok(Self {
            deadline_ns: Some(u128::from(config.deadline_ns())),
            arrival_latency_ns: Some(config.latency_ns()),
            threshold_ns: Some(facts.threshold_ns), quantum_ns: Some(facts.quantum_ns),
            resolution_ns: Some(facts.quantization_tolerance_ns),
            completed_sample_max_age_ns: Some(facts.completed_sample_max_age_ns),
            pending_observation_limit_ns: Some(facts.pending_observation_limit_ns),
            service_wait_span_ns: Some(facts.inner_wait_from_sample_ns),
            guaranteed_gap_ns: Some(facts.detection_bound_ns),
            requested_j_ns: None, validated_j_ns: Some(j.ns),
            reporting_bound_ns: Some(bound), status: TimingStatus::Qualified(j),
            qualifying_read: Some(j.read_contract),
        })
    }
}

/// Only a reviewed, crate-owned deployment observer can create this witness.
/// The observer must authenticate loaded artifacts and binding implementation,
/// clock/scheduler/hardware, complete immutable configuration AND every permitted
/// runtime route/RPC/wake callback/configuration transition, for this exact
/// request and prepared worker. A name/hash returned by a safe trait is not that
/// observation. The fields are module-private; there is no public token factory.
/// This is a product-admission boundary, never a Rust memory-safety premise.
struct ObservedDeployment {
    request: super::deadline_service::DeadlineServiceRequest,
    worker: crate::platform::prepared::WorkerObservation,
    identity: TimingIdentity,
    evidence: TimingEvidencePins,
}

type DeploymentObserver = fn(&super::deadline_service::DeadlineServiceRequest,
    crate::platform::prepared::WorkerObservation) -> Result<ObservedDeployment, TimingGap>;

/// Actual request correlation is independent of any timing catalog. An open safe
/// PlatformInit can swap two opaque preparations, but that substitution is refused
/// before the first monitor arm. Native setup and worker control copy the same
/// stored canonical trait-object value, avoiding fresh-coercion vtable identity.
/// Equality still is not type/deployment authentication: the trusted observer
/// below must authenticate actual artifacts and configuration independently.
pub(crate) fn qualify_prepared(
    request: &super::deadline_service::DeadlineServiceRequest,
    prepared: &super::deadline_service::PreparedDeadlineService,
) -> Result<ValidatedJ, TimingGap> {
    if !request.same_request(&prepared.request)
        || !core::ptr::eq(request.shared.platform, prepared.worker.platform()) {
        return Err(TimingGap::WorkerRequestMismatch);
    }
    let observed_worker = prepared.worker.observation();
    let record = ACCEPTED_TIMING.iter().find(|record|
        record.identity == request.configuration.identity)
        .ok_or(TimingGap::QualificationMissing)?;
    check_selected_binding(request.binding_name, request.binding_version,
        record.binding_name, record.binding_version)?;
    if record.routes != request.routes { return Err(TimingGap::RouteConfigurationMismatch); }
    if record.worker != observed_worker { return Err(TimingGap::WorkerConfigurationMismatch); }
    // Merely populating fingerprints/names/numbers cannot enable runtime: the
    // compiled, trusted observer must independently authenticate this selection.
    // No observer or proof has yet been qualified; the catalog remains empty.
    let observed = observe_deployment(record.observe, request, observed_worker)?;
    validate_observation(request, observed_worker, record.identity, record.pins, &observed)?;
    finish_qualification(request.configuration, record)
}

fn observe_deployment(observer: Option<DeploymentObserver>,
    request: &super::deadline_service::DeadlineServiceRequest,
    worker: crate::platform::prepared::WorkerObservation) -> Result<ObservedDeployment, TimingGap>
{
    let observe = observer.ok_or(TimingGap::DeploymentUnobserved)?;
    observe(request, worker)
}

fn check_selected_binding(actual_name: &str, actual_version: Option<&str>,
    accepted_name: &str, accepted_version: Option<&str>) -> Result<(), TimingGap>
{
    if actual_name != accepted_name || actual_version != accepted_version {
        return Err(TimingGap::BindingMismatch);
    }
    Ok(())
}

fn validate_observation(request: &super::deadline_service::DeadlineServiceRequest,
    worker: crate::platform::prepared::WorkerObservation, identity: TimingIdentity,
    evidence: TimingEvidencePins, observed: &ObservedDeployment) -> Result<(), TimingGap>
{
    if !request.same_request(&observed.request) { return Err(TimingGap::WorkerRequestMismatch); }
    if worker != observed.worker { return Err(TimingGap::WorkerConfigurationMismatch); }
    if identity != observed.identity || request.configuration.identity != observed.identity {
        return Err(TimingGap::IdentityMismatch);
    }
    if evidence != observed.evidence { return Err(TimingGap::EvidenceMismatch); }
    Ok(())
}

fn finish_qualification(config: DeadlineConfiguration, record: &AcceptedTiming) -> Result<ValidatedJ, TimingGap> {
    let complete = record.terms.total().ok_or(TimingGap::ArithmeticOverflow)?;
    if complete > record.j_ns || config.requested_j_ns.is_some_and(|j| j != record.j_ns) {
        return Err(TimingGap::QualificationMissing);
    }
    Ok(ValidatedJ { ns: record.j_ns, pins: record.pins, identity: record.identity,
        service_owner: record.service_owner, read_contract: record.read_contract })
}

// Compatibility entry for the retained five original J fixtures, whose bytes and
// unrun provenance are unchanged. Claim-only admission is now ALWAYS refused,
// even if a future trusted catalog exists. Production calls qualify_prepared.
#[cfg(test)]
fn qualify(config: DeadlineConfiguration, observed: TimingIdentity,
    _owner: &'static str, _routes: &[ServiceRoute]) -> Result<ValidatedJ, TimingGap>
{
    if config.identity != observed { return Err(TimingGap::IdentityMismatch); }
    Err(TimingGap::QualificationMissing)
}

pub(crate) fn monitor_config(route: &ServiceRoute) -> Result<Option<MonitorConfig>, TimingGap> {
    let Some(d) = route.deadline_ns else { return Ok(None); };
    let a = route.arrival.ok_or(TimingGap::ArrivalEvidenceMissing)?;
    let complete_l = a.declared_binding_latency_ns.checked_add(a.recording_delay_ns)
        .ok_or(TimingGap::ArithmeticOverflow)?;
    if complete_l > a.arrival_latency_ns { return Err(TimingGap::ArrivalBudgetMismatch); }
    MonitorConfig::new(d, a.arrival_latency_ns).map(Some).map_err(|_| TimingGap::InvalidDeadline)
}

pub(crate) fn startup_error(gap: TimingGap) -> StartupError {
    StartupError::DeadlineTiming { gap, cleanup_code: None }
}

pub(crate) struct ArmedDeadline {
    pub(crate) monitor: DeadlineMonitor,
    pub(crate) report: DeadlineTimingReport,
}

/// One-time publication of separately allocated, exclusively armed startup data.
/// No runtime initializer or mutable monitor reference is exposed.
pub(crate) struct DeadlineSlot {
    // Safety invariant: null, or a pointer derived from an initialized, aligned,
    // permanently live shared ArmedDeadline; non-null contents are never reset,
    // moved or destroyed, and only their atomic fields may change.
    armed: AtomicPtr<ArmedDeadline>,
}

impl DeadlineSlot {
    pub(crate) const fn new() -> Self { Self { armed: AtomicPtr::new(core::ptr::null_mut()) } }

    /// # Safety-usable invariant
    /// On success publishes a permanently shared initialized monitor exactly once.
    /// Failure preserves the old publication. No mutable access is manufactured;
    /// `value`'s shared static borrow already excludes subsequent exclusive use.
    pub(crate) fn publish(&self, value: &'static ArmedDeadline) -> Result<(), TimingGap> {
        let ptr = core::ptr::from_ref(value).cast_mut();
        self.armed.compare_exchange(core::ptr::null_mut(), ptr, Ordering::Release, Ordering::Relaxed)
            .map(|_| ()).map_err(|_| TimingGap::AlreadyArmed)
    }

    /// # Safety-usable invariant
    /// Returns no reference before publication; a returned shared reference is
    /// initialized and permanently live. This performs no reset or mutable access.
    pub(crate) fn get(&self) -> Option<&ArmedDeadline> {
        let ptr = self.armed.load(Ordering::Acquire);
        if ptr.is_null() { return None; }
        // SAFETY: dereferencing ptr to create a shared ArmedDeadline reference
        // requires alignment, initialization, liveness and shared-access validity
        // (Reference behavior-considered-undefined; std AtomicPtr::load).
        // The private field invariant and publish's safety-usable postcondition
        // establish all four. This Acquire observes the sole Release publication;
        // null was excluded. There is no deletion/reset/other pointer writer, no
        // callback between load and reference creation, and no mutable reference
        // survives publish. Shared runtime changes affect atomic fields only.
        // The returned lifetime is bounded by &self; storage lives for 'static.
        Some(unsafe { &*ptr })
    }

    pub(crate) fn arrival(&self, now_ns: u64, counter: &Counter<true>) {
        if let Some(armed) = self.get() {
            // The wrapper owns and publishes any credit. No second add here.
            let _ = armed.monitor.arrival_and_publish(now_ns, counter);
        }
    }
    pub(crate) fn check(&self, now_ns: u64, counter: &Counter<true>) -> bool {
        self.get().is_some_and(|armed| armed.monitor.check_and_publish(now_ns, counter))
    }
    pub(crate) fn schedule(&self, previous_ns: u64, sampled_ns: u64) -> Option<CountSchedule> {
        self.get().map(|armed| armed.monitor.count_schedule(previous_ns, sampled_ns))
    }
}

#[cfg(test)]
#[path = "deadline_timing_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "deadline_admission_tests.rs"]
mod admission_tests;
