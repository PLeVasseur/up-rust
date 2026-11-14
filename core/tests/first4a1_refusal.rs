// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! A failed route admission must not start binding resources it left behind.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]
pub use up_rust_core::{
    communication, frame, memory, platform, transport, uattributes, uri, uuid, wire,
};
#[path = "../src/transport/tests.rs"]
mod doubles;
use communication::two_mode::{Publisher, RpcClient, RpcServer, Subscriber};
mod support;
#[global_allocator]
static ALLOCATOR: support::CountingAllocator = support::CountingAllocator;
use communication::LocalUriProvider;
use doubles::{NullHandoff, TestUri, TestWire, CLOCK};
use std::sync::atomic::{AtomicUsize, Ordering};
use transport::{
    BindingRefusal, BindingSetup, Capabilities, PreparedRoute, QosSupport, RouteDeclaration,
    RouteKind, RouteView, RuntimePort, SetupCheck, StartupError, TransportSetup, TransportView,
};

#[derive(Default)]
struct Counts {
    preparations: AtomicUsize,
    starts: AtomicUsize,
    policy_routes: AtomicUsize,
    policy_transitions: AtomicUsize,
}
struct Probe {
    counts: &'static Counts,
    refuse: bool,
}
impl BindingSetup for Probe {
    type Running = NullHandoff;
    fn name(&self) -> &'static str {
        "side-effectful-probe"
    }
    fn capabilities(&self) -> Capabilities {
        NullHandoff.capabilities()
    }
    fn qos_support(&self, kind: RouteKind) -> QosSupport {
        NullHandoff.qos_support(kind)
    }
    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        self.counts.preparations.fetch_add(1, Ordering::Relaxed);
        if self.refuse {
            return Err(BindingRefusal("resources reserved before refusal"));
        }
        NullHandoff.prepare_route(route)
    }
    fn into_running(self, _: RuntimePort) -> Result<NullHandoff, BindingRefusal> {
        self.counts.starts.fetch_add(1, Ordering::Relaxed);
        Ok(NullHandoff)
    }
}
struct Policy {
    reject: bool,
    response_only: bool,
    counts: &'static Counts,
}
impl SetupCheck for Policy {
    fn is_enforcing(&self) -> bool {
        true
    }
    fn route(&self, _: &TransportView<'_>, route: &RouteView) -> Result<(), StartupError> {
        self.counts.policy_routes.fetch_add(1, Ordering::Relaxed);
        if self.reject
            && (!self.response_only
                || route.report.route.kind == Some(uattributes::UMessageType::Response))
        {
            Err(StartupError::InvalidConfig("policy rejection"))
        } else {
            Ok(())
        }
    }
    fn transport(&self, _: &TransportView<'_>) -> Result<(), StartupError> {
        self.counts
            .policy_transitions
            .fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}
fn setup(refuse: bool, reject: bool) -> (TransportSetup, &'static Counts) {
    setup_response(refuse, reject, false)
}
fn setup_response(
    refuse: bool,
    reject: bool,
    response_only: bool,
) -> (TransportSetup, &'static Counts) {
    let memory = memory::StartupMemory::new(Box::leak(Box::new_uninit_slice(256 * 1024))).unwrap();
    setup_response_in(&memory, refuse, reject, response_only)
}
fn setup_response_in(
    memory: &memory::StartupMemory,
    refuse: bool,
    reject: bool,
    response_only: bool,
) -> (TransportSetup, &'static Counts) {
    let counts = Box::leak(Box::new(Counts::default()));
    let policy = Box::leak(Box::new(Policy {
        reject,
        response_only,
        counts,
    }));
    (
        TransportSetup::handoff_on(memory, &CLOCK, Probe { counts, refuse })
            .unwrap()
            .strict(policy),
        counts,
    )
}
fn incomplete(error: &StartupError) -> bool {
    matches!(
        error,
        StartupError::InvalidConfig(
            "incomplete route admission: discard and recreate this transport setup"
        )
    )
}
fn refuses_retry_and_start(setup: TransportSetup, counts: &Counts) {
    let before = counts.preparations.load(Ordering::Relaxed);
    assert!(before > 0);
    let error = Publisher::<u32>::on(&setup, &TestUri(1), 0x8002)
        .wire(TestWire { fail_encode: false })
        .build()
        .unwrap_err();
    assert!(incomplete(&error));
    assert_eq!(counts.preparations.load(Ordering::Relaxed), before);
    refuses_start(setup, counts);
}
fn refuses_start(setup: TransportSetup, counts: &Counts) {
    let policy_before = counts.policy_routes.load(Ordering::Relaxed);
    let error = support::assert_no_heap("refused transition includes frees", || {
        setup.enter_runtime().unwrap_err()
    });
    assert!(incomplete(&error));
    assert_eq!(counts.policy_routes.load(Ordering::Relaxed), policy_before);
    assert_eq!(counts.policy_transitions.load(Ordering::Relaxed), 0);
    assert_eq!(counts.starts.load(Ordering::Relaxed), 0);
}

// [utest->req~ssr-strict-init-only~1] method: focused fixture regression; partial evidence, no G2 credit
// [utest->req~ssr-mode-init-failure~2] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn policy_refused_send_and_receive_cannot_start_or_prepare_again() {
    for receive in [false, true] {
        let (setup, counts) = setup(false, true);
        let result = if receive {
            Subscriber::<u32>::on(&setup, TestUri(1).get_resource_uri(0x8001))
                .wire(TestWire { fail_encode: false })
                .build()
                .map(|_| ())
        } else {
            Publisher::<u32>::on(&setup, &TestUri(1), 0x8001)
                .wire(TestWire { fail_encode: false })
                .build()
                .map(|_| ())
        };
        assert!(matches!(
            result,
            Err(StartupError::InvalidConfig("policy rejection"))
        ));
        refuses_retry_and_start(setup, counts);
    }
}

// [utest->req~ssr-strict-init-only~1] method: focused fixture regression; partial evidence, no G2 credit
// [utest->req~ssr-mode-init-failure~2] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn side_effectful_binding_refusal_cannot_start() {
    let (setup, counts) = setup(true, false);
    assert!(matches!(
        Publisher::<u32>::on(&setup, &TestUri(1), 0x8001)
            .wire(TestWire { fail_encode: false })
            .build(),
        Err(StartupError::Binding { .. })
    ));
    refuses_retry_and_start(setup, counts);
}

// [utest->req~ssr-strict-init-only~1] method: focused fixture regression; partial evidence, no G2 credit
// [utest->req~ssr-mode-init-failure~2] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn successful_route_commits_allow_further_preparation_and_start() {
    let (setup, counts) = setup(false, false);
    for resource in [0x8001, 0x8002] {
        Publisher::<u32>::on(&setup, &TestUri(1), resource)
            .wire(TestWire { fail_encode: false })
            .build()
            .unwrap();
    }
    support::assert_no_heap("successful transition includes frees", || {
        setup.enter_runtime().unwrap()
    });
    assert_eq!(counts.preparations.load(Ordering::Relaxed), 2);
    assert_eq!(counts.starts.load(Ordering::Relaxed), 1);
}

// [utest->req~ssr-strict-init-only~1] method: focused fixture regression; partial evidence, no G2 credit
// [utest->req~ssr-mode-init-failure~2] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn policy_refused_carrier_ingress_and_egress_cannot_start() {
    use transport::{CarriedWire, CarrierRoute};
    use wire::Wire;
    for receive in [false, true] {
        let (setup, counts) = setup(false, true);
        let source = TestUri(1).get_resource_uri(0x8001);
        let wires = [CarriedWire::of(&TestWire { fail_encode: false }.describe())];
        let route = CarrierRoute::new(&source, &wires, 4);
        let error = if receive {
            setup.carrier_ingress(&route).map(|_| ())
        } else {
            setup.carrier_egress(&route).map(|_| ())
        }
        .unwrap_err();
        assert!(matches!(
            error,
            StartupError::InvalidConfig("policy rejection")
        ));
        assert_eq!(counts.preparations.load(Ordering::Relaxed), 1);
        refuses_retry_and_start(setup, counts);
    }
}
// [utest->req~ssr-strict-init-only~1] method: focused fixture regression; partial evidence, no G2 credit
// [utest->req~ssr-mode-init-failure~2] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn policy_refused_rpc_response_after_committed_request_cannot_start() {
    let (setup, counts) = setup_response(false, true, true);
    let error = RpcServer::<u32, u32>::on(&setup, &TestUri(2), 1)
        .wire(TestWire { fail_encode: false })
        .build()
        .unwrap_err();
    assert!(matches!(
        error,
        StartupError::InvalidConfig("policy rejection")
    ));
    assert_eq!(counts.preparations.load(Ordering::Relaxed), 2);
    assert_eq!(counts.policy_routes.load(Ordering::Relaxed), 2);
    refuses_retry_and_start(setup, counts);
}

fn build_rpc(
    setup: &TransportSetup,
    server: bool,
    response_max: usize,
) -> Result<(), StartupError> {
    if server {
        RpcServer::<u32, u32>::on(setup, &TestUri(2), 1)
            .wire(TestWire { fail_encode: false })
            .max_response(response_max)
            .build()
            .map(|_| ())
    } else {
        RpcClient::<u32, u32>::on(setup, &TestUri(1), TestUri(2).get_resource_uri(1))
            .wire(TestWire { fail_encode: false })
            .max_response(response_max)
            .build()
            .map(|_| ())
    }
}

// [utest->req~ssr-mode-init-failure~2] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn rpc_response_pre_admission_refusal_contains_the_committed_request() {
    for server in [false, true] {
        let (setup, counts) = setup(false, false);
        assert!(matches!(
            build_rpc(&setup, server, usize::MAX),
            Err(StartupError::FrameTooLarge { .. })
        ));
        // The first route committed, but the second never reached the binding.
        assert_eq!(counts.preparations.load(Ordering::Relaxed), 1);
        assert_eq!(counts.policy_routes.load(Ordering::Relaxed), 1);
        refuses_retry_and_start(setup, counts);
    }
}

fn bounded_setup(capacity: usize) -> (TransportSetup, &'static Counts, memory::StartupMemory) {
    // Both calibration and exhaustion use the same alignment, independent of
    // the allocator's size classes. Only the exposed slice is available to SDK.
    #[repr(align(64))]
    struct Region([core::mem::MaybeUninit<u8>; 256 * 1024]);
    let region = Box::leak(Box::new(Region(
        [core::mem::MaybeUninit::uninit(); 256 * 1024],
    )));
    let memory = memory::StartupMemory::new(&mut region.0[..capacity]).unwrap();
    let (setup, counts) = setup_response_in(&memory, false, false, false);
    (setup, counts, memory)
}

// [utest->req~ssr-mode-init-failure~2] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn rpc_real_startup_exhaustion_between_legs_contains_the_committed_request() {
    for server in [false, true] {
        // Measure the real placements through response pre-admission on this
        // architecture. No injected error or hard-coded private layout size.
        let (calibration, counts, memory) = bounded_setup(256 * 1024);
        assert!(matches!(
            build_rpc(&calibration, server, usize::MAX),
            Err(StartupError::FrameTooLarge { .. })
        ));
        assert_eq!(counts.preparations.load(Ordering::Relaxed), 1);
        let capacity = memory.seal().used - 1;
        drop(calibration);

        // One byte less than that boundary permits the request commit, then
        // refuses a genuine response-side allocation before binding preparation.
        let (setup, counts, _) = bounded_setup(capacity);
        assert!(matches!(
            build_rpc(&setup, server, 4),
            Err(StartupError::Memory(
                memory::StartupError::MemoryExhausted { .. }
            ))
        ));
        assert_eq!(counts.preparations.load(Ordering::Relaxed), 1);
        assert_eq!(counts.policy_routes.load(Ordering::Relaxed), 1);
        // An exhausted region may refuse the retry's wire placement before it
        // reaches the latch. It must not prepare any additional binding route.
        assert!(build_rpc(&setup, server, 4).is_err());
        assert_eq!(counts.preparations.load(Ordering::Relaxed), 1);
        refuses_start(setup, counts);
    }
}

// [utest->req~ssr-mode-init-failure~2] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn complete_rpc_roles_allow_more_preparation_and_heap_free_start() {
    for declared_clients in [false, true] {
        let (setup, counts) = setup(false, false);
        let clients = [TestUri(1).get_source_uri(), TestUri(3).get_source_uri()];
        let server = RpcServer::<u32, u32>::on(&setup, &TestUri(2), 1)
            .wire(TestWire { fail_encode: false })
            .clients(if declared_clients { &clients } else { &[] })
            .build()
            .unwrap();
        let client = RpcClient::<u32, u32>::on(&setup, &TestUri(1), TestUri(2).get_resource_uri(1))
            .wire(TestWire { fail_encode: false })
            .build()
            .unwrap();
        let publisher = Publisher::<u32>::on(&setup, &TestUri(1), 0x8001)
            .wire(TestWire { fail_encode: false })
            .build()
            .unwrap();
        support::assert_no_heap("complete RPC activation includes frees", || {
            let runtime = setup.enter_runtime().unwrap();
            let _server = server.activate(&runtime).unwrap();
            let _client = client.activate(&runtime).unwrap();
            let _publisher = publisher.activate(&runtime).unwrap();
        });
        assert_eq!(
            counts.preparations.load(Ordering::Relaxed),
            if declared_clients { 6 } else { 5 }
        );
        assert_eq!(counts.starts.load(Ordering::Relaxed), 1);
    }
}

// [utest->req~ssr-mode-init-failure~2] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn rpc_refusal_before_first_preparation_remains_retryable() {
    for server in [false, true] {
        let (setup, counts) = setup(false, false);
        let result = if server {
            RpcServer::<u32, u32>::on(&setup, &TestUri(2), 1)
                .wire(TestWire { fail_encode: false })
                .max_payload(usize::MAX)
                .build()
                .map(|_| ())
        } else {
            RpcClient::<u32, u32>::on(&setup, &TestUri(1), TestUri(2).get_resource_uri(1))
                .wire(TestWire { fail_encode: false })
                .max_payload(usize::MAX)
                .build()
                .map(|_| ())
        };
        assert!(matches!(result, Err(StartupError::FrameTooLarge { .. })));
        assert_eq!(counts.preparations.load(Ordering::Relaxed), 0);
        build_rpc(&setup, server, 4).unwrap();
        support::assert_no_heap("retry after harmless RPC refusal", || {
            setup.enter_runtime().unwrap()
        });
        assert_eq!(counts.preparations.load(Ordering::Relaxed), 2);
        assert_eq!(counts.starts.load(Ordering::Relaxed), 1);
    }
}

// [utest->req~ssr-mode-init-failure~2] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn rpc_later_client_response_refusal_contains_earlier_committed_routes() {
    struct LimitedResponses {
        probe: Probe,
        response_checks: &'static AtomicUsize,
    }
    impl BindingSetup for LimitedResponses {
        type Running = NullHandoff;
        fn name(&self) -> &'static str {
            self.probe.name()
        }
        fn capabilities(&self) -> Capabilities {
            self.probe.capabilities()
        }
        fn qos_support(&self, kind: RouteKind) -> QosSupport {
            if kind.message == Some(uattributes::UMessageType::Response)
                && self.response_checks.fetch_add(1, Ordering::Relaxed) == 1
            {
                // Refuse before the second response reaches prepare_route.
                // The request and first response have both cleared their latch.
                QosSupport::NONE
            } else {
                self.probe.qos_support(kind)
            }
        }
        fn prepare_route(
            &mut self,
            route: &RouteDeclaration<'_>,
        ) -> Result<PreparedRoute, BindingRefusal> {
            self.probe.prepare_route(route)
        }
        fn into_running(self, port: RuntimePort) -> Result<NullHandoff, BindingRefusal> {
            self.probe.into_running(port)
        }
    }
    let memory = memory::StartupMemory::new(Box::leak(Box::new_uninit_slice(256 * 1024))).unwrap();
    let counts = Box::leak(Box::new(Counts::default()));
    let response_checks = Box::leak(Box::new(AtomicUsize::new(0)));
    let policy = Box::leak(Box::new(Policy {
        reject: false,
        response_only: false,
        counts,
    }));
    let setup = TransportSetup::handoff_on(
        &memory,
        &CLOCK,
        LimitedResponses {
            probe: Probe {
                counts,
                refuse: false,
            },
            response_checks,
        },
    )
    .unwrap()
    .strict(policy);
    let clients = [TestUri(1).get_source_uri(), TestUri(3).get_source_uri()];
    let error = RpcServer::<u32, u32>::on(&setup, &TestUri(2), 1)
        .wire(TestWire { fail_encode: false })
        .clients(&clients)
        .build()
        .unwrap_err();
    assert!(matches!(error, StartupError::QosNotHonored { .. }));
    assert_eq!(response_checks.load(Ordering::Relaxed), 2);
    assert_eq!(counts.preparations.load(Ordering::Relaxed), 2);
    assert_eq!(counts.policy_routes.load(Ordering::Relaxed), 2);
    refuses_retry_and_start(setup, counts);
}
