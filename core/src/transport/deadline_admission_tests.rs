//! New refusal fixtures, UNCOMPILED / UNRUN. No accepted or mock timing record.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::memory::StartupRegion;
use crate::platform::prepared::{ClockImplementation, WorkerImplementation, WorkerObservation};
use crate::transport::deadline_service::DeadlineServiceRequest;
use crate::transport::mode::TransportSetup;
use crate::transport::tests::{CLOCK, NullHandoff};

const ID: TimingIdentity = TimingIdentity { sdk_source: [0;32], compiled_artifact: [0;32],
    binding_source: [0;32], platform_source: [0;32], hardware_and_atomic_abi: [0;32],
    clock_timer_scheduler: [0;32], complete_configuration: [0;32] };
const PINS: TimingEvidencePins = TimingEvidencePins { source_and_binary_correspondence: [0;32],
    all_winner_wcet_and_progress: [0;32], activation_and_no_waiter_service: [0;32],
    r56_arrival_and_recording: [0;32], r61_pending_and_freshness: [0;32],
    counter_visibility_and_read_contract: [0;32], runtime_memory_and_panic: [0;32],
    independent_acceptance: [0;32] };
const WORKER: WorkerObservation = WorkerObservation { implementation: WorkerImplementation::PthreadV1,
    clock: ClockImplementation::LinuxMonotonic, stack_bytes: 0,
    scheduler_policy: 0, scheduler_priority: 0 };

fn request(memory: &crate::memory::StartupMemory) -> DeadlineServiceRequest {
    let setup = TransportSetup::handoff_on(memory, &CLOCK, NullHandoff).unwrap();
    DeadlineServiceRequest { configuration: DeadlineConfiguration { identity: ID,
        arrivals: &[], requested_j_ns: None }, routes: &[], binding_name: "null-handoff",
        binding_version: None, shared: setup.shared(), tx: &[], rx: &[], reports: &[] }
}

#[test]
fn matching_application_fingerprints_do_not_hide_a_different_binding_or_version() {
    assert_eq!(check_selected_binding("actual", Some("2"), "accepted", Some("2")),
        Err(TimingGap::BindingMismatch));
    assert_eq!(check_selected_binding("same", Some("2"), "same", Some("1")),
        Err(TimingGap::BindingMismatch));
}

#[test]
fn same_metadata_from_another_setup_does_not_bind_the_observation() {
    static REGION: StartupRegion<65536> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let actual = request(&memory);
    let substituted = request(&memory);
    let observed = ObservedDeployment { request: substituted, worker: WORKER, identity: ID, evidence: PINS };
    assert_eq!(validate_observation(&actual, WORKER, ID, PINS, &observed),
        Err(TimingGap::WorkerRequestMismatch));
}

#[test]
fn changed_worker_scheduler_or_stack_invalidates_an_observation() {
    static REGION: StartupRegion<65536> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let actual = request(&memory);
    for changed in [WorkerObservation { scheduler_policy: 99, ..WORKER },
        WorkerObservation { stack_bytes: 123, ..WORKER }] {
        let observed = ObservedDeployment { request: actual, worker: changed, identity: ID, evidence: PINS };
        assert_eq!(validate_observation(&actual, WORKER, ID, PINS, &observed),
            Err(TimingGap::WorkerConfigurationMismatch));
    }
}

#[test]
fn rpc_roster_change_invalidates_the_bound_request_even_with_matching_identifiers() {
    static REGION: StartupRegion<65536> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let actual = request(&memory);
    let changed_routes = memory.place([ServiceRoute { route: RouteId(0), deadline_ns: None,
        arrival: None, rpc_slots: 1, slots: 1, slot_len: 64, depth: 1 }]).unwrap();
    let observed = ObservedDeployment { request: DeadlineServiceRequest { routes: changed_routes, ..actual },
        worker: WORKER, identity: ID, evidence: PINS };
    assert_eq!(validate_observation(&actual, WORKER, ID, PINS, &observed),
        Err(TimingGap::WorkerRequestMismatch));
}

#[test]
fn acceptance_pins_cannot_be_substituted_after_observation() {
    static REGION: StartupRegion<65536> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let actual = request(&memory);
    let changed = TimingEvidencePins { counter_visibility_and_read_contract: [1;32], ..PINS };
    let observed = ObservedDeployment { request: actual, worker: WORKER, identity: ID, evidence: changed };
    assert_eq!(validate_observation(&actual, WORKER, ID, PINS, &observed), Err(TimingGap::EvidenceMismatch));
}

#[cfg(any(all(target_os = "macos", target_pointer_width = "64"),
    all(target_os = "linux", target_env = "gnu", target_pointer_width = "64",
        any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64"))))]
#[test]
#[ignore = "requires a separate root grant for actual pthread preparation and cleanup"]
fn native_prepared_request_substitution_is_refused_before_arm() {
    use crate::platform::PlatformInit;
    static REGION: StartupRegion<{ 16 * 1024 * 1024 }> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let first_setup = TransportSetup::handoff_on(&memory, &crate::platform::NATIVE, NullHandoff).unwrap();
    let other_setup = TransportSetup::handoff_on(&memory, &crate::platform::NATIVE, NullHandoff).unwrap();
    let first = DeadlineServiceRequest { shared: first_setup.shared(), ..request(&memory) };
    let other = DeadlineServiceRequest { shared: other_setup.shared(), ..first };
    let mut prepared = crate::platform::NATIVE.prepare_deadline_service(&memory, first).unwrap();
    assert_eq!(qualify_prepared(&other, &prepared), Err(TimingGap::WorkerRequestMismatch));
    assert_eq!(qualify_prepared(&first, &prepared), Err(TimingGap::QualificationMissing));
    prepared.worker.cancel().unwrap();
}

#[test]
fn absent_trusted_observer_cannot_be_replaced_by_matching_metadata() {
    static REGION: StartupRegion<65536> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let actual = request(&memory);
    assert!(matches!(observe_deployment(None, &actual, WORKER), Err(TimingGap::DeploymentUnobserved)));
}
