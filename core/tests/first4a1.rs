// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Targeted 4A1 behavior. No native or whole-delivery claim.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]
pub use up_rust_core::{
    communication, frame, memory, transport, uattributes, uri, uuid, wire,
};
// Host integration fixture for the shared test doubles' private atomic import.
// Keep the production module private; this is the same public AtomicU32 type.
pub mod platform {
    pub use up_rust_core::platform::*;
    pub(crate) mod atomic {
        pub(crate) use core::sync::atomic::AtomicU32;
    }
}
#[path = "../src/transport/tests.rs"]
mod doubles;
mod support;
use communication::two_mode::{Publisher, Subscriber};
use communication::LocalUriProvider;
use doubles::{NullHandoff, TestUri, TestWire, CLOCK};
use memory::StartupMemory;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use support::assert_no_heap;
use transport::wake::{RouteWake, WakeEvent};
use transport::{
    Deadline, Qos, RouteView, SetupCheck, StartupError, TransportSetup, TransportView,
};

#[global_allocator]
static ALLOCATOR: support::CountingAllocator = support::CountingAllocator;
fn make_setup() -> TransportSetup {
    let memory = StartupMemory::new(Box::leak(Box::new_uninit_slice(256 * 1024))).unwrap();
    TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap()
}
struct Policy {
    routes: AtomicU32,
    transitions: AtomicU32,
    refuse: u16,
    enforce: bool,
}
impl Policy {
    fn new(refuse: u16, enforce: bool) -> &'static Self {
        Box::leak(Box::new(Self {
            routes: AtomicU32::new(0),
            transitions: AtomicU32::new(0),
            refuse,
            enforce,
        }))
    }
}
impl SetupCheck for Policy {
    fn is_enforcing(&self) -> bool {
        self.enforce
    }
    fn route(&self, transport: &TransportView<'_>, route: &RouteView) -> Result<(), StartupError> {
        self.routes.fetch_add(1, Ordering::Relaxed);
        assert_eq!(transport.binding, "null-handoff");
        assert_eq!(route.wires.len(), 1);
        if route.report.route.source.resource_id() == self.refuse {
            return Err(StartupError::Strict {
                route: Some(route.report.route.clone()),
                rule: "TEST-REFUSAL",
                requirement: "req~ssr-strict-init-only~1",
                fix: "choose another route",
            });
        }
        Ok(())
    }
    fn transport(&self, transport: &TransportView<'_>) -> Result<(), StartupError> {
        self.transitions.fetch_add(1, Ordering::Relaxed);
        assert!(!transport.routes.is_empty());
        Ok(())
    }
}

// [utest->req~ssr-strict-init-only~1] method: requirements-based test
#[test]
fn first4a1_strict_checks_build_and_transition_never_runtime() {
    let policy = Policy::new(0xffff, true);
    let setup = make_setup().strict(policy);
    let publisher = Publisher::<u32>::on(&setup, &TestUri(1), 0x8001)
        .wire(TestWire { fail_encode: false })
        .build()
        .unwrap();
    assert_eq!(policy.routes.load(Ordering::Relaxed), 1);
    let runtime = assert_no_heap("strict transition", || setup.enter_runtime().unwrap());
    assert_eq!(policy.routes.load(Ordering::Relaxed), 2);
    assert_eq!(policy.transitions.load(Ordering::Relaxed), 1);
    assert_no_heap("strict is absent from runtime", || {
        let publisher = publisher.activate(&runtime).unwrap();
        for value in 0..4 {
            publisher.publish(&value).unwrap();
        }
        runtime.pump();
        runtime.service();
    });
    assert_eq!(policy.routes.load(Ordering::Relaxed), 2);
    assert_eq!(policy.transitions.load(Ordering::Relaxed), 1);
}

// [utest->req~ssr-strict-init-only~1] method: fault injection
#[test]
fn first4a1_refusal_names_route_and_late_policy_rechecks() {
    let policy = Policy::new(0x8002, true);
    let setup = make_setup().strict(policy);
    let err = Publisher::<u32>::on(&setup, &TestUri(1), 0x8002)
        .wire(TestWire { fail_encode: false })
        .build()
        .unwrap_err();
    assert!(
        matches!(err, StartupError::Strict { route: Some(ref name), rule:"TEST-REFUSAL", .. }
        if name.source.resource_id()==0x8002)
    );
    assert_eq!(policy.transitions.load(Ordering::Relaxed), 0);
    let later = make_setup();
    Publisher::<u32>::on(&later, &TestUri(1), 0x8002)
        .wire(TestWire { fail_encode: false })
        .build()
        .unwrap();
    assert!(matches!(
        later.strict(policy).enter_runtime(),
        Err(StartupError::Strict { route: Some(_), .. })
    ));
}

// [utest->req~ssr-strict-lock~1] method: negative control
#[cfg(up_rust_strict)]
#[test]
fn first4a1_locked_transition_refuses_missing_and_report_only_policy() {
    for setup in [
        make_setup(),
        make_setup().strict(Policy::new(0xffff, false)),
    ] {
        assert!(matches!(
            setup.enter_runtime(),
            Err(StartupError::Strict {
                route: None,
                rule: "BUILD-LOCK",
                ..
            })
        ));
    }
}

// [utest->req~ssr-strict-init-only~1] method: requirements-based test
#[test]
fn first4a1_event_only_is_explicit_and_unmonitored() {
    assert_eq!(Qos::DEFAULT.deadline, Deadline::Unspecified);
    let qos = Qos::DEFAULT
        .with_deadline(Duration::from_secs(1))
        .event_only();
    assert_eq!(qos.deadline, Deadline::EventOnly);
    assert_eq!(qos.deadline.duration(), None);
    assert!(transport::check_qos(
        &qos,
        &transport::QosSupport::new()
            .with_drop_oldest(true)
            .with_max_depth(8)
    )
    .is_ok());
    assert!(transport::check_qos(
        &qos.with_deadline(Duration::ZERO),
        &transport::QosSupport::new()
            .with_drop_oldest(true)
            .with_max_depth(8)
    )
    .is_err());
}

// Protocol for this test only: 0 empty, 1 owner writing/reading, 2 occupied.
// No concurrent registration in this test. The callback synchronizes access
// with acquire/release. Clear consumes the registration without a wake.
static WAKES: AtomicU32 = AtomicU32::new(0);
fn register(cell: &RouteWake, value: usize) {
    cell.install_hook(hook);
    // SAFETY: this test owns the only protocol and registers with an empty cell.
    let state = unsafe { cell.state() };
    assert_eq!(
        state.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed),
        Ok(0)
    );
    // SAFETY: exclusive state 1; both words are initialized before publication.
    unsafe {
        (*cell.storage_ptr())[0].write(value);
        (*cell.storage_ptr())[1].write(!value);
    }
    state.store(2, Ordering::Release);
}
fn hook(cell: &RouteWake, event: WakeEvent) {
    // SAFETY: the one protocol installed by register, using the same state word.
    let state = unsafe { cell.state() };
    if state
        .compare_exchange(2, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_ok()
    {
        // SAFETY: acquired occupied storage; no registration can write until 0.
        let (a, b) = unsafe {
            (
                (*cell.storage_ptr())[0].assume_init(),
                (*cell.storage_ptr())[1].assume_init(),
            )
        };
        assert_eq!(a, !b);
        if event == WakeEvent::Wake {
            WAKES.fetch_add(1, Ordering::Relaxed);
        }
        state.store(0, Ordering::Release);
    }
}

// [utest->req~ssr-async-seam~1] method: fault injection, heap observation, Miri
#[test]
fn first4a1_seam_storage_register_poll_clear_reuse() {
    let cell = RouteWake::new();
    WAKES.store(0, Ordering::Relaxed);
    assert_no_heap("register, poll, wake, clear, reuse", || {
        cell.wake(); // completion before registration: the adapter must poll again
        register(&cell, 10);
        assert_eq!(WAKES.load(Ordering::Relaxed), 0); // not evidence of readiness
        cell.wake();
        assert_eq!(WAKES.load(Ordering::Relaxed), 1);
        register(&cell, 11);
        cell.clear(); // cancellation/drop
        cell.wake(); // late completion cannot wake the removed registration
        assert_eq!(WAKES.load(Ordering::Relaxed), 1);
        register(&cell, 12);
        cell.wake();
        assert_eq!(WAKES.load(Ordering::Relaxed), 2);
        cell.clear();
    });
    register(&cell, 13);
    std::thread::scope(|scope| {
        scope.spawn(|| assert_no_heap("completion thread", || cell.wake()));
    });
    assert_eq!(WAKES.load(Ordering::Relaxed), 3);
    cell.clear();
}

// [utest->req~ssr-async-seam~1] method: requirements-based test, heap observation
#[cfg(not(up_rust_strict))]
#[test]
fn first4a1_receive_seam_poll_and_cancel() {
    let setup = make_setup();
    let receiver = Subscriber::<u32>::on(&setup, TestUri(1).get_resource_uri(0x8001))
        .wire(TestWire { fail_encode: false })
        .qos(Qos::DEFAULT.event_only())
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let receiver = receiver.activate(&runtime).unwrap();
    assert_no_heap("receive seam", || {
        let cell = receiver.__route_wake();
        register(cell, 20);
        assert!(receiver.try_recv().unwrap().is_none());
        cell.wake(); // hint may be spurious; polling still yields no value
        assert!(receiver.try_recv().unwrap().is_none());
        register(cell, 21);
        cell.clear();
        cell.wake();
    });
}

// [utest->req~ssr-strict-init-only~1] method: requirements-based test
#[test]
fn first4a1_deviation_declaration_survives_route_admission() {
    use transport::{BindingSetup, BindingRefusal, BindingRoute, PreparedRoute, RouteDeclaration, RouteKind, RuntimePort, Capabilities, QosSupport, Deviation, DeviationKind};
    static RECORD: Deviation = Deviation { id: "TEST-ONLY-1", revision: 2,
        kind: DeviationKind::Accepted, title: "test descriptor", can_interfere: false };
    static RECORDS: &[&Deviation] = &[&RECORD];
    struct Declares;
    impl BindingSetup for Declares {
        type Running = NullHandoff;
        fn name(&self) -> &'static str { "null-handoff" }
        fn capabilities(&self) -> Capabilities { NullHandoff.capabilities().with_version("test-1") }
        fn qos_support(&self, kind: RouteKind) -> QosSupport { NullHandoff.qos_support(kind) }
        fn prepare_route(&mut self, _: &RouteDeclaration<'_>) -> Result<PreparedRoute, BindingRefusal> {
            const PREPARED: PreparedRoute = PreparedRoute::new(BindingRoute(0), wire::RuntimeHeap::None)
                .with_deviations(RECORDS);
            Ok(PREPARED)
        }
        fn into_running(self, _: RuntimePort) -> Result<NullHandoff, BindingRefusal> { Ok(NullHandoff) }
    }
    struct Inspect;
    impl SetupCheck for Inspect {
        fn is_enforcing(&self) -> bool { true }
        fn route(&self, transport: &TransportView<'_>, route: &RouteView) -> Result<(), StartupError> {
            assert_eq!(transport.binding_version, Some("test-1"));
            assert_eq!(route.report.deviations, RECORDS);
            Ok(())
        }
        fn transport(&self, _: &TransportView<'_>) -> Result<(), StartupError> { Ok(()) }
    }
    let memory = StartupMemory::new(Box::leak(Box::new_uninit_slice(256 * 1024))).unwrap();
    let setup = TransportSetup::handoff_on(&memory, &CLOCK, Declares).unwrap().strict(&Inspect);
    Publisher::<u32>::on(&setup, &TestUri(1), 0x8001).wire(TestWire { fail_encode:false }).build().unwrap();
    let runtime=assert_no_heap("deviation transition",||setup.enter_runtime().unwrap());
    assert_eq!(runtime.routes().first().map(|route| route.deviations), Some(RECORDS));
}

// V7 normative naming; protocol naming is not a gated safety requirement.
#[test]
#[allow(deprecated)]
fn first4a1_uri_names_and_aliases_preserve_full_entity_id() {
    assert!(uri::UUri::try_from_wire_parts("local", 0, 1, 0x8001).is_err());
    for (entity, kind, instance) in [
        (0x0001_1234, 0x1234, 1),
        (0xffff_ffff, 0xffff, 0xffff),
        (0x1234_5678, 0x5678, 0x1234),
    ] {
        let uri = uri::UUri::try_from_wire_parts("local", entity, 1, 0x8001).unwrap();
        assert_eq!(
            (
                uri.authority_name(),
                uri.ue_version_major(),
                uri.resource_id()
            ),
            ("local", 1, 0x8001)
        );
        assert_eq!((uri.ue_type_id(), uri.ue_instance_id()), (kind, instance));
        assert_eq!(
            (uri.uentity_type_id(), uri.uentity_instance_id()),
            (kind, instance)
        );
    }
}
