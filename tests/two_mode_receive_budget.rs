/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/
//! Focused SDK regressions for the call-local raw receive allowance.
//! They exercise the real SDK receive paths with scripted binding rejections.
//! These heap-backed doubles are NOT native, allocator or no-heap evidence.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic,
    clippy::indexing_slicing, clippy::too_many_lines, missing_docs)]

#[path = "support/review_doubles.rs"]
mod doubles;

use std::collections::VecDeque;
use std::sync::{Mutex, atomic::{AtomicUsize, Ordering}};
use std::time::Duration;
use up_rust::two_mode::*;
use up_rust_core::{uattributes::UMessageType, uri::UUri};

const WALL: u64 = 2_000_000_000_000;

enum Step {
    Layout,
    Malformed,
    Frame(Vec<u8>, PeerRef),
    Empty,
    NativeError,
    BorrowCapacity,
}

struct Script {
    shared: &'static doubles::Shared,
    steps: Mutex<VecDeque<Step>>,
    attempts: AtomicUsize,
    raw: AtomicUsize,
    layouts: AtomicUsize,
    malformed: AtomicUsize,
    native_errors: AtomicUsize,
    borrow_capacity: AtomicUsize,
}

impl Script {
    fn leak(steps: Vec<Step>) -> &'static Self {
        Box::leak(Box::new(Self {
            shared: doubles::Shared::leak(), steps: Mutex::new(steps.into()),
            attempts: AtomicUsize::new(0), raw: AtomicUsize::new(0),
            layouts: AtomicUsize::new(0), malformed: AtomicUsize::new(0),
            native_errors: AtomicUsize::new(0), borrow_capacity: AtomicUsize::new(0),
        }))
    }

    fn next(&self, mut take: impl FnMut() -> bool) -> Option<doubles::HeapFrame> {
        while take() {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            let next = self.steps.lock().unwrap().pop_front()?;
            let frame = match next {
                Step::Frame(bytes, peer) => doubles::HeapFrame { bytes, peer, shared: self.shared },
                Step::Layout | Step::Malformed => {
                    self.raw.fetch_add(1, Ordering::SeqCst);
                    match next {
                        Step::Layout => { self.layouts.fetch_add(1, Ordering::SeqCst); }
                        _ => { self.malformed.fetch_add(1, Ordering::SeqCst); }
                    }
                    // One ownership return for every rejected raw sample.
                    drop(doubles::HeapFrame { bytes: Vec::new(), peer: PeerRef::UNKNOWN,
                        shared: self.shared });
                    continue;
                }
                Step::Empty => return None,
                Step::NativeError => {
                    self.native_errors.fetch_add(1, Ordering::SeqCst);
                    return None;
                }
                Step::BorrowCapacity => {
                    self.borrow_capacity.fetch_add(1, Ordering::SeqCst);
                    return None;
                }
            };
            self.raw.fetch_add(1, Ordering::SeqCst);
            return Some(frame);
        }
        None
    }

    fn legacy(&self) -> Option<doubles::HeapFrame> {
        let mut left = 32_usize;
        self.next(|| {
            let Some(next) = left.checked_sub(1) else { return false; };
            left = next;
            true
        })
    }

    fn raw(&self) -> usize { self.raw.load(Ordering::SeqCst) }
    fn dropped(&self) -> usize { self.shared.frames_dropped.load(Ordering::SeqCst) }
    fn pending(&self) -> usize { self.steps.lock().unwrap().len() }
}

struct LendingSetup(&'static Script);
struct LendingRunning(&'static Script);
struct HandoffSetup(&'static Script);
struct HandoffRunning(&'static Script);

macro_rules! setup {
    ($setup:ident, $running:ident) => {
        impl BindingSetup for $setup {
            type Running = $running;
            fn name(&self) -> &'static str { "receive-budget-script" }
            fn capabilities(&self) -> Capabilities { doubles::capabilities() }
            fn qos_support(&self, _: RouteKind) -> QosSupport { doubles::all_qos() }
            fn prepare_route(&mut self, route: &RouteDeclaration<'_>) -> Result<PreparedRoute, BindingRefusal> {
                if let Some(inbound) = route.inbound {
                    self.0.shared.inbound.lock().unwrap().push(inbound);
                }
                Ok(PreparedRoute::new(BindingRoute(0), RuntimeHeap::None))
            }
            fn into_running(self, port: RuntimePort) -> Result<$running, BindingRefusal> {
                let _ = self.0.shared.port.set(port);
                Ok($running(self.0))
            }
        }
    }
}
setup!(LendingSetup, LendingRunning);
setup!(HandoffSetup, HandoffRunning);

impl LendingRuntime for LendingRunning {
    type Tx<'a> = doubles::NoTx;
    type Rx = doubles::HeapFrame;
    fn loan(&self, _: BindingRoute, _: TxFrameLayout) -> Result<Self::Tx<'_>, LoanError> {
        Err(LoanError::InvalidRoute)
    }
    fn publish<'a>(&'a self, _: BindingRoute, _: InitializedTx<Self::Tx<'a>>) -> Result<SendReport, SendError> {
        Err(SendError::Delivery)
    }
    fn receive(&self, _: BindingRoute) -> Option<Self::Rx> { self.0.legacy() }
    fn receive_bounded(&self, _: BindingRoute, budget: &ReceiveBudget) -> Option<Self::Rx> {
        self.0.next(|| budget.try_take())
    }
}

impl HandoffRuntime for HandoffRunning {
    fn send(&self, route: BindingRoute, frame: OutgoingFrame<'_>) -> Result<up_rust_core::transport::SendReport, up_rust_core::transport::HandoffFailure> {
        doubles::HandoffRunning(self.0.shared).send(route, frame)
    }
    fn receive(&self, _: BindingRoute, sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow) -> usize {
        let mut count = 0;
        while let Some(frame) = self.0.legacy() {
            count += 1;
            if matches!(sink(borrow(&frame)), Flow::Stop) { break; }
        }
        count
    }
    fn receive_bounded(&self, _: BindingRoute, budget: &ReceiveBudget,
        sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow) -> usize {
        let mut count = 0;
        while let Some(frame) = self.0.next(|| budget.try_take()) {
            count += 1;
            if matches!(sink(borrow(&frame)), Flow::Stop) { break; }
        }
        count
    }
}

fn borrow(frame: &doubles::HeapFrame) -> InboundFrame<'_> {
    InboundFrame { metadata: &[], payload: PayloadBytes::one(&frame.bytes),
        peer: frame.peer, reply: ReplyToken::default() }
}

fn setup_for(script: &'static Script, handoff: bool) -> TransportSetup {
    let clock = doubles::Clock::leak(1_000_000_000, WALL);
    if handoff {
        TransportSetup::handoff_on(&doubles::memory(), clock, HandoffSetup(script)).unwrap()
    } else {
        TransportSetup::lending_on(&doubles::memory(), clock, LendingSetup(script)).unwrap()
    }
}

fn typed(script: &'static Script, handoff: bool, depth: u16) -> (TransportRuntime, Subscriber<u32>) {
    let setup = setup_for(script, handoff);
    let sub = Subscriber::<u32>::on(&setup, doubles::uri(9, 0x8001))
        .wire(doubles::U32Wire { hook: None })
        .qos(Qos::DEFAULT.with_depth(depth)).max_held(1).build().unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let sub = sub.activate(&runtime).unwrap();
    (runtime, sub)
}

fn publish(topic: &UUri, publisher: u64, value: u32, malformed_payload: bool) -> Step {
    let payload = value.to_le_bytes();
    Step::Frame(doubles::publish_frame(topic, doubles::uuid_at(WALL, u64::from(value)),
        None, None, if malformed_payload { &payload[..1] } else { &payload }),
        doubles::peer(publisher, 1))
}

fn valid(publisher: u64, value: u32) -> Step {
    publish(&doubles::uri(9, 0x8001), publisher, value, false)
}

#[test]
fn mixed_binding_and_sdk_refusals_share_one_allowance_for_both_families() {
    for handoff in [false, true] {
        let s = Script::leak(vec![Step::Layout,
            publish(&doubles::uri(10, 0x8001), 8, 1, false), valid(7, 11), valid(8, 22)]);
        let (runtime, sub) = typed(s, handoff, 2);
        assert_eq!(runtime.pump(), 0, "pump must leave ordinary typed routes alone");
        assert_eq!(s.raw(), 0);
        assert_eq!(*sub.try_recv().unwrap().unwrap(), 11);
        assert_eq!(s.raw(), 3);
        assert_eq!(s.layouts.load(Ordering::SeqCst), 1);
        assert_eq!(sub.counters().refused, 1);
        assert_eq!(s.dropped(), 3, "native-refused, SDK-refused and decoded owners returned once");
        assert_eq!(s.pending(), 1);
        assert_eq!(*sub.try_recv().unwrap().unwrap(), 22);
        assert_eq!(s.raw(), 4);
        assert_eq!(s.dropped(), 4);
    }
}

#[test]
fn an_exhausted_prefix_keeps_the_following_valid_frame_for_the_next_call() {
    for handoff in [false, true] {
        let s = Script::leak(vec![Step::Malformed, Step::Layout,
            publish(&doubles::uri(9, 0x8001), 7, 10, true), valid(8, 22)]);
        let (_, sub) = typed(s, handoff, 2);
        assert!(sub.try_recv().unwrap().is_none());
        assert_eq!(s.raw(), 3);
        assert_eq!(s.dropped(), 3);
        assert_eq!(s.pending(), 1);
        assert_eq!(s.malformed.load(Ordering::SeqCst), 1);
        assert_eq!(s.layouts.load(Ordering::SeqCst), 1);
        assert_eq!(sub.counters().refused, 1);
        assert_eq!(*sub.try_recv().unwrap().unwrap(), 22);
        assert_eq!(s.dropped(), 4);
    }
}

#[test]
fn empty_native_error_and_borrow_exhaustion_stop_without_swallowing_a_valid_successor() {
    for handoff in [false, true] {
        for stop in [Step::Empty, Step::NativeError, Step::BorrowCapacity] {
            let expected_error = usize::from(matches!(stop, Step::NativeError));
            let expected_capacity = usize::from(matches!(stop, Step::BorrowCapacity));
            let s = Script::leak(vec![Step::Malformed, stop, valid(8, 22)]);
            let (_, sub) = typed(s, handoff, 8);
            assert!(sub.try_recv().unwrap().is_none());
            assert_eq!(s.raw(), 1);
            assert_eq!(s.attempts.load(Ordering::SeqCst), 2);
            assert_eq!(s.native_errors.load(Ordering::SeqCst), expected_error);
            assert_eq!(s.borrow_capacity.load(Ordering::SeqCst), expected_capacity);
            assert_eq!(*sub.try_recv().unwrap().unwrap(), 22);
            assert_eq!(s.dropped(), 2);
        }
    }
}

#[test]
fn held_lease_refusal_does_no_raw_work_and_releases_capacity_normally() {
    let s = Script::leak(vec![valid(7, 11), Step::Malformed, valid(8, 22)]);
    let (_, sub) = typed(s, false, 2);
    let held = sub.try_recv().unwrap().unwrap();
    assert!(matches!(sub.try_recv(), Err(ReceiveError::MaxHeldExceeded)));
    assert_eq!(s.raw(), 1);
    drop(held);
    assert_eq!(*sub.try_recv().unwrap().unwrap(), 22);
    assert_eq!(s.raw(), 3);
    assert_eq!(s.dropped(), 3);
}

#[test]
fn binding_rejection_flood_stops_at_depth_plus_one_and_preserves_the_tail() {
    let mut steps: Vec<_> = (0..40).map(|_| Step::Malformed).collect();
    steps.push(valid(8, 22));
    let s = Script::leak(steps);
    let (_, sub) = typed(s, false, 8);
    assert!(sub.try_recv().unwrap().is_none());
    assert_eq!(s.raw(), 9);
    assert_eq!(s.pending(), 32);
    assert_eq!(s.dropped(), 9);
}

fn response(reqid: up_rust_core::uuid::UUID, id: u64, value: u32) -> Step {
    Step::Frame(doubles::whole_frame(&doubles::Spec {
        kind: UMessageType::Response, id: doubles::uuid_at(WALL, id), reqid: Some(reqid),
        ttl: None, source: doubles::uri(4, 1), sink: Some(doubles::uri(3, 0)),
        token: None, payload: Some(&value.to_le_bytes()),
    }), doubles::peer(8, 1))
}

#[test]
fn rpc_pump_counts_deliveries_separately_and_polling_can_finish_the_remaining_response() {
    let s = Script::leak(Vec::new());
    let setup = setup_for(s, true);
    let client = RpcClient::<u32, u32>::on(&setup, &doubles::Uri(3), doubles::uri(4, 1))
        .wire(doubles::U32Wire { hook: None }).ttl(Duration::from_secs(30))
        .qos(Qos::DEFAULT.with_depth(7)).max_in_flight(2).build().unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let client = client.activate(&runtime).unwrap();
    let mut call = client.invoke(&1).unwrap();
    let reqid = doubles::sent_request_id(&s.shared.sent.lock().unwrap()[0]);
    s.steps.lock().unwrap().extend([Step::Layout, Step::Malformed,
        response(doubles::uuid_at(WALL, 999), 10, 100), response(reqid, 11, 101)]);
    assert_eq!(runtime.pump(), 1, "only one frame reached SDK response admission");
    assert_eq!(s.raw(), 3, "response depth is derived from max_in_flight, not request QoS");
    assert_eq!(s.pending(), 1);
    assert_eq!(*call.try_response().unwrap().unwrap(), 101);
    assert_eq!(s.raw(), 4);
    assert_eq!(s.dropped(), 4);
}

#[test]
fn carrier_uses_the_same_budget_and_sink_stop_keeps_successors_queued() {
    let topic = doubles::uri(9, 0x8001);
    let s = Script::leak(vec![Step::Malformed,
        publish(&doubles::uri(10, 0x8001), 7, 1, false), valid(8, 22), valid(7, 33)]);
    let setup = setup_for(s, false);
    let wires = Box::leak(Box::new([CarriedWire {
        wire_id: doubles::WIRE_ID, payload_family: doubles::FAMILY_ID,
        identity: WirePayloadIdentity::serialized(doubles::ENCODING), alignment: 1,
    }]));
    let mut route = CarrierRoute::new(&topic, wires, 64);
    route.kind = Some(UMessageType::Publish);
    route.physical_source = Some(&topic);
    route.qos = Qos::DEFAULT.with_depth(2);
    route.slots = 4;
    let ingress = setup.carrier_ingress(&route).unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let mut seen = 0;
    assert_eq!(runtime.receive(ingress, &mut |frame| {
        assert_eq!(frame.payload().as_contiguous(), Some(&22_u32.to_le_bytes()[..]));
        seen += 1;
        Flow::Stop
    }).unwrap(), 1);
    assert_eq!(seen, 1);
    assert_eq!(s.raw(), 3);
    assert_eq!(s.pending(), 1);
    assert_eq!(s.dropped(), 3);
}


#[test]
fn early_carrier_stop_leaves_budget_and_successor_for_both_families() {
    for handoff in [false, true] {
        let topic = doubles::uri(9, 0x8001);
        let s = Script::leak(vec![valid(8, 22), valid(7, 33)]);
        let setup = setup_for(s, handoff);
        let wires = Box::leak(Box::new([CarriedWire {
            wire_id: doubles::WIRE_ID, payload_family: doubles::FAMILY_ID,
            identity: WirePayloadIdentity::serialized(doubles::ENCODING), alignment: 1,
        }]));
        let mut route = CarrierRoute::new(&topic, wires, 64);
        route.kind = Some(UMessageType::Publish);
        route.physical_source = Some(&topic);
        route.qos = Qos::DEFAULT.with_depth(8);
        route.slots = 2;
        let ingress = setup.carrier_ingress(&route).unwrap();
        let runtime = setup.enter_runtime().unwrap();
        for (value, attempts, pending) in [(22_u32, 1, 1), (33, 2, 0)] {
            assert_eq!(runtime.receive(ingress, &mut |frame| {
                assert_eq!(frame.payload().as_contiguous(), Some(&value.to_le_bytes()[..]));
                Flow::Stop
            }).unwrap(), 1);
            assert_eq!(s.attempts.load(Ordering::SeqCst), attempts);
            assert_eq!(s.pending(), pending);
            assert_eq!(s.dropped(), attempts);
        }
    }
}

struct MeteredSetup<B> { inner: B, spent: &'static AtomicUsize }
struct MeteredRunning<B> { inner: B, spent: &'static AtomicUsize }
impl<B: BindingSetup> BindingSetup for MeteredSetup<B> {
    type Running = MeteredRunning<B::Running>;
    fn name(&self) -> &'static str { self.inner.name() }
    fn capabilities(&self) -> Capabilities { self.inner.capabilities() }
    fn qos_support(&self, kind: RouteKind) -> QosSupport { self.inner.qos_support(kind) }
    fn recover_predecessor(&mut self) -> Result<(), BindingRefusal> { self.inner.recover_predecessor() }
    fn prepare_route(&mut self, route: &RouteDeclaration<'_>) -> Result<PreparedRoute, BindingRefusal> {
        self.inner.prepare_route(route)
    }
    fn into_running(self, port: RuntimePort) -> Result<Self::Running, BindingRefusal> {
        Ok(MeteredRunning { inner: self.inner.into_running(port)?, spent: self.spent })
    }
}
impl<B: HandoffRuntime> HandoffRuntime for MeteredRunning<B> {
    fn send(&self, route: BindingRoute, frame: OutgoingFrame<'_>) -> Result<up_rust_core::transport::SendReport, up_rust_core::transport::HandoffFailure> {
        self.inner.send(route, frame)
    }
    fn has_capacity(&self, route: BindingRoute) -> bool { self.inner.has_capacity(route) }
    fn receive(&self, route: BindingRoute, sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow) -> usize {
        self.inner.receive(route, sink)
    }
    fn receive_bounded(&self, route: BindingRoute, budget: &ReceiveBudget,
        sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow) -> usize {
        let before = budget.remaining();
        let count = self.inner.receive_bounded(route, budget, sink);
        self.spent.fetch_add(before - budget.remaining(), Ordering::SeqCst);
        count
    }
}

#[test]
fn sustained_dead_participant_discards_charge_every_pop_and_return_medium_credit() {
    use up_rust::test_util::handoff::InMemoryBus;
    let bus = InMemoryBus::with_medium(8, 4096, 8);
    let spent = Box::leak(Box::new(AtomicUsize::new(0)));
    let clock = doubles::Clock::leak(1_000_000_000, WALL);
    let tx = TransportSetup::handoff_on(&doubles::memory(), clock, bus.participant(1)).unwrap();
    let rx = TransportSetup::handoff_on(&doubles::memory(), clock,
        MeteredSetup { inner: bus.participant(2), spent }).unwrap();
    let publisher = Publisher::<u32>::on(&tx, &doubles::Uri(9), 0x8001_u16)
        .wire(doubles::U32Wire { hook: None }).build().unwrap();
    let subscriber = Subscriber::<u32>::on(&rx, doubles::uri(9, 0x8001))
        .wire(doubles::U32Wire { hook: None }).qos(Qos::DEFAULT.with_depth(8))
        .max_held(1).build().unwrap();
    let tx = tx.enter_runtime().unwrap();
    let rx = rx.enter_runtime().unwrap();
    let publisher = publisher.activate(&tx).unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    bus.kill(2);
    // Refill at the accepted depth between calls. Each batch returns all eight
    // medium credits; the ninth (empty) pop is also charged. No queue overfill.
    for round in 0..32 {
        for value in 0..8 { publisher.publish(&value).unwrap(); }
        assert!(matches!(publisher.publish(&99), Err(SendError::Busy)));
        assert!(subscriber.try_recv().unwrap().is_none());
        assert_eq!(spent.load(Ordering::SeqCst), (round + 1) * 9);
        assert!(bus.take_raw(2).is_none());
        assert_eq!(subscriber.counters().refused, 0, "dead frames never reach SDK admission");
    }
    bus.restart(2);
    for value in 0..8 { publisher.publish(&value).unwrap(); }
    for value in 0..8 { assert_eq!(*subscriber.try_recv().unwrap().unwrap(), value); }
}
