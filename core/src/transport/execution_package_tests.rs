//! Narrow missing execution-package fixtures; controlled platform only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::communication::two_mode::Publisher;
use crate::memory::StartupRegion;
use crate::transport::pool::LifetimePolicy;
use crate::transport::tests::{NullHandoff, TestUri, TestWire, CLOCK};

#[test]
fn full_u64_policy_endpoints_survive_setup_publication() {
    static REGION: StartupRegion<262144> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    for value in [1, (1_u64 << 32) + 1, LifetimePolicy::MAX_REUSES] {
        let policy = LifetimePolicy::new(value).unwrap();
        let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap()
            .with_lifetime_policy(policy).unwrap();
        assert_eq!(setup.shared().lifetime_policy().warning_remaining(), value);
    }
    assert!(LifetimePolicy::new(0).is_err());
    assert!(LifetimePolicy::new(u64::MAX).is_err());
}

#[test]
fn forgotten_setup_identity_is_not_reused() {
    static REGION: StartupRegion<65536> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let abandoned = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    let identity = abandoned.id();
    core::mem::forget(abandoned);
    let next = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    assert_ne!(next.id(), identity);
}

#[test]
fn setup_seed_boundaries_refuse_bad_inputs_and_preserve_terminal_policy() {
    static REGION: StartupRegion<262144> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    let publisher = Publisher::<u32>::on(&setup, &TestUri(11), 0x8001)
        .wire(TestWire { fail_encode: false }).build().unwrap();
    // Only the safe initialization owner seeds, before any runtime/lease exists.
    assert!(setup.__seed_pool_free_for_test(usize::MAX, 0).is_err());
    assert!(setup.__seed_pool_free_for_test(0, 1).is_err());
    assert!(setup.__seed_pool_free_for_test(0, u64::MAX).is_err());
    setup.__seed_pool_free_for_test(0, u64::MAX - 1).unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let publisher = publisher.activate(&runtime).unwrap();
    assert!(matches!(publisher.publish(&7), Err(SendError::PoolRetired)));
}


use crate::communication::LocalUriProvider;
use crate::communication::two_mode::{NotificationListener, RpcClient, RpcServer, Subscriber};
use crate::transport::binding::{HandoffRuntime, InboundFrame, OutgoingFrame, PayloadBytes};
use crate::transport::carrier::{CarriedWire, CarrierRoute};
use crate::transport::pool::LifetimeState;
use crate::wire::Wire;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};

#[test]
fn receive_roles_refuse_another_runtime() {
    static REGION: StartupRegion<524288> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let first = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    let second = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    let subscriber = Subscriber::<u32>::on(&first, TestUri(1).get_resource_uri(0x8001))
        .wire(TestWire { fail_encode: false }).build().unwrap();
    let listener = NotificationListener::<u32>::on(&first, TestUri(1).get_resource_uri(0x8001), &TestUri(2))
        .wire(TestWire { fail_encode: false }).build().unwrap();
    let _first_runtime = first.enter_runtime().unwrap();
    let second_runtime = second.enter_runtime().unwrap();
    assert!(matches!(subscriber.activate(&second_runtime), Err(StartupError::WrongTransport)));
    assert!(matches!(listener.activate(&second_runtime), Err(StartupError::WrongTransport)));
}

#[test]
fn rpc_roles_refuse_another_runtime() {
    static REGION: StartupRegion<524288> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let first = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    let second = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    let server = RpcServer::<u32, u32>::on(&first, &TestUri(1), 1)
        .wire(TestWire { fail_encode: false }).build().unwrap();
    let client = RpcClient::<u32, u32>::on(&first, &TestUri(2), TestUri(1).get_resource_uri(1))
        .wire(TestWire { fail_encode: false }).build().unwrap();
    let _first_runtime = first.enter_runtime().unwrap();
    let second_runtime = second.enter_runtime().unwrap();
    assert!(matches!(server.activate(&second_runtime), Err(StartupError::WrongTransport)));
    assert!(matches!(client.activate(&second_runtime), Err(StartupError::WrongTransport)));
}

// Owns no mutable byte storage. Its one immutable startup frame is borrowed for
// the entire callback. Each raw receive attempt takes the provided budget first;
// there is no filtering loop, worker, native middleware, or concurrent observer.
struct OneFrameHandoff {
    bytes: &'static [u8],
    taken: AtomicBool,
    receives: &'static AtomicUsize,
    sends: &'static AtomicUsize,
    capacities: &'static AtomicUsize,
}
impl BindingSetup for OneFrameHandoff {
    type Running = Self;
    fn name(&self) -> &'static str { "execution-one-frame" }
    fn capabilities(&self) -> Capabilities { NullHandoff.capabilities() }
    fn qos_support(&self, kind: RouteKind) -> QosSupport { NullHandoff.qos_support(kind) }
    fn prepare_route(&mut self, route: &RouteDeclaration<'_>) -> Result<PreparedRoute, BindingRefusal> {
        NullHandoff.prepare_route(route)
    }
    fn into_running(self, _: RuntimePort) -> Result<Self, BindingRefusal> { Ok(self) }
}
impl HandoffRuntime for OneFrameHandoff {
    fn send(&self, _: BindingRoute, _: OutgoingFrame<'_>) -> Result<crate::transport::SendReport, crate::transport::HandoffFailure> {
        self.sends.fetch_add(1, AtomicOrdering::Relaxed);
        Ok(crate::transport::SendReport::exact(0, 0))
    }
    fn has_capacity(&self, _: BindingRoute) -> bool {
        self.capacities.fetch_add(1, AtomicOrdering::Relaxed);
        true
    }
    fn receive_bounded(&self, _: BindingRoute, budget: &ReceiveBudget,
        sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow) -> usize {
        self.receives.fetch_add(1, AtomicOrdering::Relaxed);
        if !budget.try_take() || self.taken.swap(true, AtomicOrdering::Relaxed) { return 0; }
        sink(InboundFrame { payload: PayloadBytes::one(self.bytes), ..InboundFrame::default() });
        1
    }
}

#[test]
fn carrier_wrong_runtime_gates_preserve_frame_and_skip_binding_calls() {
    static REGION: StartupRegion<524288> = StartupRegion::new();
    static RECEIVES: AtomicUsize = AtomicUsize::new(0);
    static SENDS: AtomicUsize = AtomicUsize::new(0);
    static CAPACITIES: AtomicUsize = AtomicUsize::new(0);
    let memory = REGION.claim().unwrap();
    let topic = TestUri(1).get_resource_uri(0x8001);
    let bytes = crate::transport::tests::whole_frame(&memory, &topic, &17_u32.to_le_bytes());
    let first = TransportSetup::handoff_on(&memory, &CLOCK, OneFrameHandoff {
        bytes, taken: AtomicBool::new(false), receives: &RECEIVES, sends: &SENDS, capacities: &CAPACITIES,
    }).unwrap();
    let second = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
    let wires = [CarriedWire::of(&TestWire { fail_encode: false }.describe())];
    let route = CarrierRoute::new(&topic, &wires, 4);
    let ingress = first.carrier_ingress(&route).unwrap();
    let egress = first.carrier_egress(&route).unwrap();
    let first_runtime = first.enter_runtime().unwrap();
    let second_runtime = second.enter_runtime().unwrap();
    assert!(!second_runtime.has_capacity(egress));
    assert_eq!(CAPACITIES.load(AtomicOrdering::Relaxed), 0);
    assert!(matches!(second_runtime.receive(ingress, &mut |_| panic!("wrong runtime reached sink")),
        Err(ReceiveError::UnknownRoute)));
    assert_eq!(RECEIVES.load(AtomicOrdering::Relaxed), 0);
    assert!(first_runtime.has_capacity(egress));
    assert_eq!(CAPACITIES.load(AtomicOrdering::Relaxed), 1);
    let mut callbacks = 0;
    assert_eq!(first_runtime.receive(ingress, &mut |frame| {
        callbacks += 1;
        assert!(matches!(second_runtime.send(egress, &frame), Err(SendError::Rejected {
            code: UCode::InvalidArgument, ..
        })));
        assert_eq!(SENDS.load(AtomicOrdering::Relaxed), 0);
        assert_eq!(frame.payload().len(), 4);
        // The refusal leaves the borrowed frame available for its actual owner.
        first_runtime.send(egress, &frame).unwrap();
        Flow::Stop
    }).unwrap(), 1);
    assert_eq!(callbacks, 1);
    assert_eq!(SENDS.load(AtomicOrdering::Relaxed), 1);
    assert_eq!(first_runtime.counters(egress.id()).refused, 1);
}

#[test]
fn receive_pool_uses_the_published_full_u64_warning_margin() {
    static REGION: StartupRegion<524288> = StartupRegion::new();
    static RECEIVES: AtomicUsize = AtomicUsize::new(0);
    static SENDS: AtomicUsize = AtomicUsize::new(0);
    static CAPACITIES: AtomicUsize = AtomicUsize::new(0);
    let memory = REGION.claim().unwrap();
    let topic = TestUri(1).get_resource_uri(0x8001);
    let bytes = crate::transport::tests::whole_frame(&memory, &topic, &29_u32.to_le_bytes());
    let margin = (1_u64 << 32) + 17;
    let policy = LifetimePolicy::new(margin).unwrap();
    let setup = TransportSetup::handoff_on(&memory, &CLOCK, OneFrameHandoff {
        bytes, taken: AtomicBool::new(false), receives: &RECEIVES, sends: &SENDS, capacities: &CAPACITIES,
    }).unwrap().with_lifetime_policy(policy).unwrap();
    let role = Subscriber::<u32>::on(&setup, topic).wire(TestWire { fail_encode: false }).build().unwrap();
    setup.__seed_pool_free_for_test(0, u64::MAX - 1 - 2 * (margin - 1)).unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let report = &runtime.routes()[0];
    assert_eq!(report.lifetime_policy, policy);
    assert_eq!(runtime.pool_lifetime(report.id), Some(LifetimeState::Healthy));
    let role = role.activate(&runtime).unwrap();
    let held = role.try_recv().unwrap().unwrap();
    assert_eq!(*held, 29);
    assert_eq!(runtime.pool_lifetime(report.id), Some(LifetimeState::LifetimeMarginLow));
}

#[test]
fn rpc_call_pool_uses_the_published_full_u64_warning_margin() {
    static REGION: StartupRegion<524288> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let margin = (1_u64 << 32) + 17;
    let policy = LifetimePolicy::new(margin).unwrap();
    let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap()
        .with_lifetime_policy(policy).unwrap();
    let client = RpcClient::<u32, u32>::on(&setup, &TestUri(2), TestUri(1).get_resource_uri(1))
        .wire(TestWire { fail_encode: false }).build().unwrap();
    setup.__seed_pool_free_for_test(0, u64::MAX - 1 - 2 * (margin - 1)).unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let response_id = runtime.core.rx[0].id();
    for report in runtime.routes() { assert_eq!(report.lifetime_policy, policy); }
    assert_eq!(runtime.pool_lifetime(response_id), Some(LifetimeState::Healthy));
    let client = client.activate(&runtime).unwrap();
    let held = client.invoke(&41).unwrap();
    assert_eq!(runtime.pool_lifetime(response_id), Some(LifetimeState::LifetimeMarginLow));
    drop(held);
}
