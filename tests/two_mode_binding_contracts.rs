/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

//! The wire, binding and QoS contracts run over the in-memory bindings, with the
//! negative controls that show each contract can fail (R33, R46).
//!
//! Harness-free: every contract uses all-thread heap windows, so one runs at a time.

#![allow(clippy::indexing_slicing)]

use std::alloc::System;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use up_rust::heap_guard::HeapGuard;
use up_rust::startup::LEAKED_HEAP_REGIONS;
use up_rust::test_util::contract::{
    assert_binding_contract, assert_wire_contract, run_qos_contract, BindingFixture, ContractRoute,
    QosUnderTest,
};
use up_rust::test_util::forwarder::TestForwarder;
use up_rust::test_util::handoff::{InMemoryBus, InMemoryOptions};
use up_rust::test_util::lending::{LendingBus, LendingOptions};
use up_rust::test_util::wires::{
    DoorState, DoorWire, LittleDoorWire, VariableDoorWire, DOOR_ENCODING,
};
use up_rust::two_mode::*;
use up_rust::{PayloadEncoding, StaticUriProvider, UEntityId, UUri};

#[global_allocator]
static HEAP: HeapGuard = HeapGuard::new(System);

fn entity(authority: &str, type_id: u16) -> StaticUriProvider {
    StaticUriProvider::for_entity(authority, UEntityId::first_instance(type_id), 1).expect("uri")
}

fn memory() -> StartupMemory {
    StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 64 * 1024).expect("memory")
}

fn topic(resource: u16) -> UUri {
    entity("door", 1).get_resource_uri(resource)
}

fn door(id: u32) -> DoorState {
    DoorState { id, open: 0 }
}

// ------------------------------------------------------------- the wire contract

fn fill_sample(value: &mut DoorState) {
    *value = DoorState { id: 1234, open: 1 };
}

fn fill_other(value: &mut DoorState) {
    *value = DoorState { id: 99, open: 0 };
}

fn wire_contract() {
    assert_wire_contract(&DoorWire, fill_sample, fill_other);
    assert_wire_contract(&LittleDoorWire, fill_sample, fill_other);
    assert_wire_contract(&VariableDoorWire, fill_sample, fill_other);
}

/// A wire whose methods can each be made to break one rule.
#[derive(Clone, Copy, Debug, Default)]
struct Broken {
    unrepeatable: bool,
    lying_exact: bool,
    keeps_prior_contents: bool,
    panics_on_truncation: bool,
    accepts_truncation: bool,
    allocates: bool,
}

static ENCODE_CALLS: AtomicU32 = AtomicU32::new(0);

impl Wire<DoorState> for Broken {
    fn describe(&self) -> WireInfo {
        DoorWire.describe()
    }

    fn encoded_size(&self, value: &DoorState) -> EncodedSize {
        if self.lying_exact {
            EncodedSize::Exact(3)
        } else {
            DoorWire.encoded_size(value)
        }
    }

    fn encode(&self, value: &DoorState, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        if self.allocates {
            drop(vec![0_u8; 8]);
        }
        let calls = ENCODE_CALLS.fetch_add(1, Ordering::SeqCst);
        if self.unrepeatable {
            let mut varying = *value;
            varying.open = (calls % 200) as u8;
            return DoorWire.encode(&varying, out);
        }
        DoorWire.encode(value, out)
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, DoorState>) -> Ready<'s, DoorState> {
        Wire::init_slot(&DoorWire, slot)
    }

    fn decode(
        &self,
        encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut DoorState,
    ) -> Result<(), WireError> {
        if self.panics_on_truncation && bytes.len() < 5 {
            panic!("this wire panics on a truncated frame");
        }
        if self.accepts_truncation && bytes.len() < 5 {
            return Ok(());
        }
        if self.keeps_prior_contents {
            let mut decoded = *into;
            DoorWire.decode(encoding, bytes, &mut decoded)?;
            decoded.open = into.open;
            *into = decoded;
            return Ok(());
        }
        DoorWire.decode(encoding, bytes, into)
    }
}

/// Runs a contract that must fail, and returns its panic message.
fn expect_failure(run: impl FnOnce()) -> String {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = catch_unwind(AssertUnwindSafe(run));
    std::panic::set_hook(previous);
    match outcome {
        Err(payload) => payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                payload
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_owned())
            })
            .unwrap_or_default(),
        Ok(()) => panic!("the contract was expected to fail, and it passed"),
    }
}

fn wire_contract_negative_controls() {
    let cases: [(Broken, &str); 6] = [
        (
            Broken {
                unrepeatable: true,
                ..Broken::default()
            },
            "repeatable",
        ),
        (
            Broken {
                lying_exact: true,
                ..Broken::default()
            },
            "Exact",
        ),
        (
            Broken {
                keeps_prior_contents: true,
                ..Broken::default()
            },
            "prior slot contents",
        ),
        (
            Broken {
                panics_on_truncation: true,
                ..Broken::default()
            },
            "must not panic",
        ),
        (
            Broken {
                accepts_truncation: true,
                ..Broken::default()
            },
            "refused",
        ),
        (
            Broken {
                allocates: true,
                ..Broken::default()
            },
            "touched the heap",
        ),
    ];
    for (broken, expected) in cases {
        let message = expect_failure(move || {
            assert_wire_contract(&broken, fill_sample, fill_other);
        });
        assert!(
            message.contains(expected),
            "{broken:?} failed for the wrong reason: {message}"
        );
    }
}

// ---------------------------------------------------------- the binding contract

struct LendingFixture {
    bus: LendingBus,
    setups: Option<(TransportSetup, TransportSetup)>,
    publisher: Option<Publisher<DoorState>>,
    subscriber: Option<Subscriber<DoorState>>,
    variable: Option<Publisher<DoorState>>,
    incarnation: u64,
}

impl LendingFixture {
    fn new() -> Self {
        let bus = LendingBus::new();
        let a = TransportSetup::lending_in(&memory(), bus.participant(1)).unwrap();
        let b = TransportSetup::lending_in(&memory(), bus.participant(2)).unwrap();
        Self {
            bus,
            setups: Some((a, b)),
            publisher: None,
            subscriber: None,
            variable: None,
            incarnation: 1,
        }
    }
}

impl BindingFixture for LendingFixture {
    fn name(&self) -> &'static str {
        "in-memory-lending"
    }

    fn start(&mut self) {
        let (a, b) = self.setups.take().expect("not started");
        let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
            .wire(DoorWire)
            .build()
            .unwrap();
        let variable = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8002)
            .wire(VariableDoorWire)
            .build()
            .unwrap();
        let subscriber = Subscriber::<DoorState>::on(&b, topic(0x8001))
            .wire(DoorWire)
            .build()
            .unwrap();
        let _variable_rx = Subscriber::<DoorState>::on(&b, topic(0x8002))
            .wire(VariableDoorWire)
            .build()
            .unwrap();
        let tx = a.enter_runtime().unwrap();
        let rx = b.enter_runtime().unwrap();
        self.publisher = Some(publisher.activate(&tx).unwrap());
        self.variable = Some(variable.activate(&tx).unwrap());
        self.subscriber = Some(subscriber.activate(&rx).unwrap());
    }

    fn publish(&mut self, value: u32) -> bool {
        self.publisher.unwrap().publish(&door(value)).is_ok()
    }

    fn receive(&mut self) -> Option<u32> {
        self.subscriber
            .unwrap()
            .try_recv()
            .ok()
            .flatten()
            .map(|f| f.id)
    }

    fn deliver_malformed(&mut self) {
        // A frame whose bytes do not decode, carried through the binding's receive path.
        self.publisher
            .unwrap()
            .publish_encoded(DOOR_ENCODING, PayloadBytes::one(&[1, 2]))
            .expect("the frame is carried");
    }

    fn restart_peer(&mut self) -> ((u64, u64), (u64, u64)) {
        let before = self.incarnation;
        self.incarnation = self.bus.restart(1);
        ((1, before), (1, self.incarnation))
    }

    fn check_truncation(&mut self) -> Option<bool> {
        let publisher = self.variable.unwrap();
        publisher.publish(&door(7)).ok()?;
        let (_, payload) = self.bus.take_raw(2)?;
        // The declared bound is 12; "7" with its flag and count bytes uses 3.
        Some(payload.as_deref().map(<[u8]>::len) == Some(3))
    }

    fn qos_support(&self) -> QosSupport {
        LendingOptions::default_support()
    }

    fn try_route(&mut self, qos: Qos) -> Result<(), QosRefused> {
        try_lending_route(self.bus, qos)
    }
}

trait DefaultSupport {
    fn default_support() -> QosSupport;
}

impl DefaultSupport for LendingOptions {
    fn default_support() -> QosSupport {
        let options = LendingOptions::default();
        QosSupport::new()
            .with_reliable(true)
            .with_drop_oldest(options.drop_oldest)
            .with_drop_newest(options.drop_newest)
            .with_wait(options.wait)
            .with_max_depth(options.max_depth)
            .with_max_history(options.max_history)
            .with_priority_levels(options.priority_levels)
    }
}

#[allow(clippy::wildcard_enum_match_arm)] // Any other refusal fails this test.
fn refused_of(error: StartupError) -> QosRefused {
    match error {
        StartupError::QosNotHonored {
            setting,
            requested,
            available,
            ..
        } => QosRefused {
            setting,
            requested,
            available,
        },
        other => panic!("expected a QoS refusal, got {other}"),
    }
}

fn try_lending_route(bus: LendingBus, qos: Qos) -> Result<(), QosRefused> {
    let setup = TransportSetup::lending_in(&memory(), bus.participant(50)).unwrap();
    Publisher::<DoorState>::on(&setup, &entity("probe", 1), 0x8F01)
        .wire(DoorWire)
        .qos(qos)
        .build()
        .map(|_| ())
        .map_err(refused_of)
}

struct HandoffFixture {
    bus: InMemoryBus,
    setups: Option<(TransportSetup, TransportSetup)>,
    publisher: Option<Publisher<DoorState>>,
    subscriber: Option<Subscriber<DoorState>>,
    incarnation: u64,
}

impl HandoffFixture {
    fn new() -> Self {
        let bus = InMemoryBus::new(16, 4096);
        let a = TransportSetup::handoff_in(&memory(), bus.participant(1)).unwrap();
        let b = TransportSetup::handoff_in(&memory(), bus.participant(2)).unwrap();
        Self {
            bus,
            setups: Some((a, b)),
            publisher: None,
            subscriber: None,
            incarnation: 1,
        }
    }
}

impl BindingFixture for HandoffFixture {
    fn name(&self) -> &'static str {
        "in-memory-handoff"
    }

    fn start(&mut self) {
        let (a, b) = self.setups.take().expect("not started");
        let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
            .wire(DoorWire)
            .build()
            .unwrap();
        let subscriber = Subscriber::<DoorState>::on(&b, topic(0x8001))
            .wire(DoorWire)
            .build()
            .unwrap();
        let tx = a.enter_runtime().unwrap();
        let rx = b.enter_runtime().unwrap();
        self.publisher = Some(publisher.activate(&tx).unwrap());
        self.subscriber = Some(subscriber.activate(&rx).unwrap());
    }

    fn publish(&mut self, value: u32) -> bool {
        self.publisher.unwrap().publish(&door(value)).is_ok()
    }

    fn receive(&mut self) -> Option<u32> {
        self.subscriber
            .unwrap()
            .try_recv()
            .ok()
            .flatten()
            .map(|f| f.id)
    }

    fn deliver_malformed(&mut self) {
        self.bus.inject(2, &[1, 2, 3], Some(&[4]));
    }

    fn restart_peer(&mut self) -> ((u64, u64), (u64, u64)) {
        let before = self.incarnation;
        self.incarnation = self.bus.restart(1);
        ((1, before), (1, self.incarnation))
    }

    fn qos_support(&self) -> QosSupport {
        QosSupport::new()
            .with_reliable(true)
            .with_drop_oldest(true)
            .with_drop_newest(true)
            .with_wait(false)
            .with_max_depth(1024)
            .with_max_history(0)
            .with_priority_levels(0)
    }

    fn try_route(&mut self, qos: Qos) -> Result<(), QosRefused> {
        let setup = TransportSetup::handoff_in(&memory(), self.bus.participant(50)).unwrap();
        Publisher::<DoorState>::on(&setup, &entity("probe", 1), 0x8F01)
            .wire(DoorWire)
            .qos(qos)
            .build()
            .map(|_| ())
            .map_err(refused_of)
    }
}

/// A binding that allocates in its first send: the first-traffic check must catch it.
struct LazyFixture {
    bus: InMemoryBus,
    publisher: Option<Publisher<DoorState>>,
    subscriber: Option<Subscriber<DoorState>>,
}

struct LazyBinding {
    inner: up_rust::test_util::handoff::InMemoryParticipant,
    log: &'static Mutex<Vec<u8>>,
}

struct LazyRunning {
    inner: up_rust::test_util::handoff::RunningParticipant,
    log: &'static Mutex<Vec<u8>>,
}

impl BindingSetup for LazyBinding {
    type Running = LazyRunning;

    fn name(&self) -> &'static str {
        "lazy"
    }

    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn qos_support(&self, kind: RouteKind) -> QosSupport {
        self.inner.qos_support(kind)
    }

    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        self.inner.prepare_route(route)
    }

    fn into_running(self, port: RuntimePort) -> Result<LazyRunning, BindingRefusal> {
        Ok(LazyRunning {
            inner: self.inner.into_running(port)?,
            log: self.log,
        })
    }
}

impl HandoffRuntime for LazyRunning {
    fn receive_bounded(
        &self, route: up_rust::two_mode::BindingRoute, budget: &up_rust::two_mode::ReceiveBudget,
        sink: &mut dyn FnMut(up_rust::two_mode::InboundFrame<'_>) -> up_rust::two_mode::Flow,
    ) -> usize {
        self.inner.receive_bounded(route, budget, sink)
    }

    fn send(&self, route: BindingRoute, frame: OutgoingFrame<'_>) -> Result<up_rust_core::transport::SendReport, up_rust_core::transport::HandoffFailure> {
        self.log.lock().unwrap().push(1);
        self.inner.send(route, frame)
    }

    fn receive(
        &self,
        route: BindingRoute,
        sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow,
    ) -> usize {
        self.inner.receive(route, sink)
    }
}

impl BindingFixture for LazyFixture {
    fn name(&self) -> &'static str {
        "lazy"
    }

    fn start(&mut self) {
        let log: &'static Mutex<Vec<u8>> = Box::leak(Box::new(Mutex::new(Vec::new())));
        let a = TransportSetup::handoff_in(
            &memory(),
            LazyBinding {
                inner: self
                    .bus
                    .participant(1)
                    .with_options(InMemoryOptions::default()),
                log,
            },
        )
        .unwrap();
        let b = TransportSetup::handoff_in(&memory(), self.bus.participant(2)).unwrap();
        let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
            .wire(DoorWire)
            .build()
            .unwrap();
        let subscriber = Subscriber::<DoorState>::on(&b, topic(0x8001))
            .wire(DoorWire)
            .build()
            .unwrap();
        let tx = a.enter_runtime().unwrap();
        let rx = b.enter_runtime().unwrap();
        self.publisher = Some(publisher.activate(&tx).unwrap());
        self.subscriber = Some(subscriber.activate(&rx).unwrap());
    }

    fn publish(&mut self, value: u32) -> bool {
        self.publisher.unwrap().publish(&door(value)).is_ok()
    }

    fn receive(&mut self) -> Option<u32> {
        self.subscriber
            .unwrap()
            .try_recv()
            .ok()
            .flatten()
            .map(|f| f.id)
    }

    fn deliver_malformed(&mut self) {}

    fn restart_peer(&mut self) -> ((u64, u64), (u64, u64)) {
        ((1, 1), (1, 2))
    }

    fn qos_support(&self) -> QosSupport {
        QosSupport::new()
            .with_reliable(true)
            .with_drop_oldest(true)
            .with_drop_newest(true)
            .with_wait(false)
            .with_max_depth(1024)
            .with_max_history(0)
            .with_priority_levels(0)
    }

    fn try_route(&mut self, _qos: Qos) -> Result<(), QosRefused> {
        Ok(())
    }
}

fn binding_contracts() {
    assert_binding_contract(&mut LendingFixture::new());
    assert_binding_contract(&mut HandoffFixture::new());
}

fn binding_contract_negative_control() {
    let message = expect_failure(|| {
        assert_binding_contract(&mut LazyFixture {
            bus: InMemoryBus::new(16, 4096),
            publisher: None,
            subscriber: None,
        });
    });
    assert!(
        message.contains("first send touched the heap"),
        "the negative control failed for the wrong reason: {message}"
    );
}

// -------------------------------------------------------------- the QoS contract

struct LendingUnderTest {
    options: LendingOptions,
}

struct LendingRoute {
    publisher: Publisher<DoorState>,
    subscriber: Subscriber<DoorState>,
    late: Subscriber<DoorState>,
    bus: LendingBus,
}

impl ContractRoute for LendingRoute {
    fn publish(&mut self, value: u32) -> Result<SendReport, SendError> {
        self.publisher.publish(&door(value))
    }

    fn receive(&mut self) -> Option<u32> {
        self.subscriber.try_recv().ok().flatten().map(|f| f.id)
    }

    fn receive_late(&mut self) -> Option<u32> {
        self.late.try_recv().ok().flatten().map(|f| f.id)
    }

    fn join_late(&mut self) {
        self.bus.join_late(3, BindingRoute(0));
    }

    fn kill_main_receiver(&mut self) {
        self.bus.kill(2);
    }

    fn drain_main_after(&mut self, delay: Duration) {
        let subscriber = self.subscriber;
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            while subscriber.try_recv().ok().flatten().is_some() {}
        });
    }

    fn counters(&self) -> Option<RouteCounters> {
        Some(self.subscriber.counters())
    }
}

impl QosUnderTest for LendingUnderTest {
    type Route = LendingRoute;

    fn support(&self) -> QosSupport {
        QosSupport::new()
            .with_reliable(true)
            .with_drop_oldest(self.options.drop_oldest)
            .with_drop_newest(self.options.drop_newest)
            .with_wait(self.options.wait)
            .with_max_depth(self.options.max_depth)
            .with_max_history(self.options.max_history)
            .with_priority_levels(self.options.priority_levels)
    }

    fn open(&mut self, resource: u16, qos: &Qos) -> Result<LendingRoute, QosRefused> {
        let bus = LendingBus::new();
        let provider = entity("door", 1);
        let topic = provider.get_resource_uri(resource);
        let a =
            TransportSetup::lending_in(&memory(), bus.participant(1).with_options(self.options))
                .unwrap();
        let b =
            TransportSetup::lending_in(&memory(), bus.participant(2).with_options(self.options))
                .unwrap();
        let c = TransportSetup::lending_in(
            &memory(),
            bus.participant(3).with_options(LendingOptions {
                late_joiner: true,
                ..self.options
            }),
        )
        .unwrap();
        let publisher = Publisher::<DoorState>::on(&a, &provider, resource)
            .wire(DoorWire)
            .qos(*qos)
            .slots(usize::from(qos.depth) + usize::from(qos.history) + 8)
            .build()
            .map_err(refused_of)?;
        let subscriber = Subscriber::<DoorState>::on(&b, topic.clone())
            .wire(DoorWire)
            .qos(*qos)
            .build()
            .map_err(refused_of)?;
        let late = Subscriber::<DoorState>::on(&c, topic)
            .wire(DoorWire)
            .qos(*qos)
            .build()
            .map_err(refused_of)?;
        let tx = a.enter_runtime().unwrap();
        let rx = b.enter_runtime().unwrap();
        let rx_late = c.enter_runtime().unwrap();
        Ok(LendingRoute {
            publisher: publisher.activate(&tx).unwrap(),
            subscriber: subscriber.activate(&rx).unwrap(),
            late: late.activate(&rx_late).unwrap(),
            bus,
        })
    }
}

struct HandoffUnderTest;

struct HandoffRoute {
    publisher: Publisher<DoorState>,
    subscriber: Subscriber<DoorState>,
    bus: InMemoryBus,
}

impl ContractRoute for HandoffRoute {
    fn publish(&mut self, value: u32) -> Result<SendReport, SendError> {
        self.publisher.publish(&door(value))
    }

    fn receive(&mut self) -> Option<u32> {
        self.subscriber.try_recv().ok().flatten().map(|f| f.id)
    }

    fn receive_late(&mut self) -> Option<u32> {
        None
    }

    fn join_late(&mut self) {}

    fn kill_main_receiver(&mut self) {
        self.bus.kill(2);
    }

    fn drain_main_after(&mut self, delay: Duration) {
        let subscriber = self.subscriber;
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            while subscriber.try_recv().ok().flatten().is_some() {}
        });
    }

    fn counters(&self) -> Option<RouteCounters> {
        Some(self.subscriber.counters())
    }
}

impl QosUnderTest for HandoffUnderTest {
    type Route = HandoffRoute;

    fn sender_learns_of_drops(&self) -> bool {
        false
    }

    fn support(&self) -> QosSupport {
        QosSupport::new()
            .with_reliable(true)
            .with_drop_oldest(true)
            .with_drop_newest(true)
            .with_wait(false)
            .with_max_depth(256)
            .with_max_history(0)
            .with_priority_levels(0)
    }

    fn open(&mut self, resource: u16, qos: &Qos) -> Result<HandoffRoute, QosRefused> {
        // The handoff bus's queue is the route's queue: it applies the route's depth
        // and policy, and reports each discard to the route.
        let bus = InMemoryBus::new(256, 4096);
        let provider = entity("door", 1);
        let a = TransportSetup::handoff_in(&memory(), bus.participant(1)).unwrap();
        let b = TransportSetup::handoff_in(&memory(), bus.participant(2)).unwrap();
        let publisher = Publisher::<DoorState>::on(&a, &provider, resource)
            .wire(DoorWire)
            .qos(*qos)
            .build()
            .map_err(refused_of)?;
        let subscriber = Subscriber::<DoorState>::on(&b, provider.get_resource_uri(resource))
            .wire(DoorWire)
            .qos(*qos)
            .build()
            .map_err(refused_of)?;
        let tx = a.enter_runtime().unwrap();
        let rx = b.enter_runtime().unwrap();
        Ok(HandoffRoute {
            publisher: publisher.activate(&tx).unwrap(),
            subscriber: subscriber.activate(&rx).unwrap(),
            bus,
        })
    }
}

/// A binding that claims capabilities it does not have. The contract must say so.
struct LyingUnderTest;

impl QosUnderTest for LyingUnderTest {
    type Route = HandoffRoute;

    fn sender_learns_of_drops(&self) -> bool {
        false
    }

    fn support(&self) -> QosSupport {
        QosSupport::new()
            .with_reliable(true)
            .with_drop_oldest(true)
            .with_drop_newest(true)
            .with_wait(true)
            .with_max_depth(1024)
            .with_max_history(8)
            .with_priority_levels(3)
    }

    fn open(&mut self, resource: u16, qos: &Qos) -> Result<HandoffRoute, QosRefused> {
        // Silently drops the QoS it was given and runs a default route.
        let quiet = Qos {
            overflow: Overflow::DropOldest,
            history: 0,
            priority: None,
            ..*qos
        };
        HandoffUnderTest.open(resource, &quiet)
    }
}

// ------------------------------------------------ the QoS contract through a carrier

/// The carrier route of the QoS contract: an upstream publisher, a carrier ingress
/// and egress forwarding each frame at once, and the receivers beyond it.
struct CarrierQosRoute {
    publisher: Publisher<DoorState>,
    subscriber: Subscriber<DoorState>,
    late: Option<Subscriber<DoorState>>,
    forwarder: TestForwarder,
    egress_bus: CarrierBus,
}

#[derive(Clone, Copy)]
enum CarrierBus {
    Lending(LendingBus),
    Handoff(InMemoryBus),
}

impl ContractRoute for CarrierQosRoute {
    fn publish(&mut self, value: u32) -> Result<SendReport, SendError> {
        let upstream = self.publisher.publish(&door(value))?;
        self.forwarder.forward_once().expect("forward");
        // What the receiver's queue did is what a sender can learn: the send's report.
        Ok(self.forwarder.last_report().unwrap_or(upstream))
    }

    fn receive(&mut self) -> Option<u32> {
        self.subscriber.try_recv().ok().flatten().map(|f| f.id)
    }

    fn receive_late(&mut self) -> Option<u32> {
        self.late
            .and_then(|late| late.try_recv().ok().flatten().map(|f| f.id))
    }

    fn join_late(&mut self) {
        if let CarrierBus::Lending(bus) = self.egress_bus {
            bus.join_late(5, BindingRoute(0));
        }
    }

    fn kill_main_receiver(&mut self) {
        match self.egress_bus {
            CarrierBus::Lending(bus) => bus.kill(4),
            CarrierBus::Handoff(bus) => bus.kill(4),
        }
    }

    fn drain_main_after(&mut self, delay: Duration) {
        let subscriber = self.subscriber;
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            while subscriber.try_recv().ok().flatten().is_some() {}
        });
    }

    fn counters(&self) -> Option<RouteCounters> {
        Some(self.subscriber.counters())
    }
}

/// A carrier leg refuses `Wait` on its egress: map that to the setting, for the contract.
#[allow(clippy::wildcard_enum_match_arm)] // Any other refusal fails this test.
fn carrier_refused(error: StartupError) -> QosRefused {
    match error {
        StartupError::CarrierRefused { reason, .. } if reason.contains("Overflow::Wait") => {
            QosRefused {
                setting: QosSetting::Overflow,
                requested: 1,
                available: 0,
            }
        }
        other => refused_of(other),
    }
}

fn carrier_route_of<'a>(uri: &'a UUri, carried: &'a [CarriedWire], qos: &Qos) -> CarrierRoute<'a> {
    CarrierRoute {
        qos: *qos,
        slots: 16,
        ..CarrierRoute::new(uri, carried, 64)
    }
}

/// Lending on both sides: a late joiner and a sender that learns of drops work.
struct LendingCarrierUnderTest;

impl QosUnderTest for LendingCarrierUnderTest {
    type Route = CarrierQosRoute;

    fn support(&self) -> QosSupport {
        // A carrier egress refuses `Wait`; everything else is what the legs offer.
        QosSupport::new()
            .with_reliable(true)
            .with_drop_oldest(true)
            .with_drop_newest(true)
            .with_wait(false)
            .with_max_depth(1024)
            .with_max_history(64)
            .with_priority_levels(0)
    }

    fn open(&mut self, resource: u16, qos: &Qos) -> Result<CarrierQosRoute, QosRefused> {
        let (a, b) = (LendingBus::new(), LendingBus::new());
        let provider = entity("door", 1);
        let uri = provider.get_resource_uri(resource);
        let carried = [CarriedWire::of(&Wire::<DoorState>::describe(&DoorWire))];
        let carrier = carrier_route_of(&uri, &carried, qos);
        let lending = |bus: LendingBus, peer: u64, late: bool| {
            TransportSetup::lending_in(
                &memory(),
                bus.participant(peer).with_options(LendingOptions {
                    late_joiner: late,
                    ..LendingOptions::default()
                }),
            )
            .unwrap()
        };
        let upstream = lending(a, 1, false);
        let carrier_in = lending(a, 2, false);
        let carrier_out = lending(b, 3, false);
        let downstream = lending(b, 4, false);
        let late_downstream = lending(b, 5, true);
        let publisher = Publisher::<DoorState>::on(&upstream, &provider, resource)
            .wire(DoorWire)
            .qos(*qos)
            .slots(usize::from(qos.depth) + usize::from(qos.history) + 8)
            .build()
            .map_err(refused_of)?;
        let ingress = carrier_in
            .carrier_ingress(&carrier)
            .map_err(carrier_refused)?;
        let egress = carrier_out
            .carrier_egress(&carrier)
            .map_err(carrier_refused)?;
        let subscriber = Subscriber::<DoorState>::on(&downstream, uri.clone())
            .wire(DoorWire)
            .qos(*qos)
            .build()
            .map_err(refused_of)?;
        let late = Subscriber::<DoorState>::on(&late_downstream, uri.clone())
            .wire(DoorWire)
            .qos(*qos)
            .build()
            .map_err(refused_of)?;
        let up = upstream.enter_runtime().unwrap();
        let in_rt = carrier_in.enter_runtime().unwrap();
        let out_rt = carrier_out.enter_runtime().unwrap();
        let down = downstream.enter_runtime().unwrap();
        let late_rt = late_downstream.enter_runtime().unwrap();
        Ok(CarrierQosRoute {
            publisher: publisher.activate(&up).unwrap(),
            subscriber: subscriber.activate(&down).unwrap(),
            late: Some(late.activate(&late_rt).unwrap()),
            forwarder: TestForwarder::new(in_rt, ingress, out_rt, egress),
            egress_bus: CarrierBus::Lending(b),
        })
    }
}

/// Handoff on both sides: the SDK's inbox applies the policy.
struct HandoffCarrierUnderTest;

impl QosUnderTest for HandoffCarrierUnderTest {
    type Route = CarrierQosRoute;

    fn sender_learns_of_drops(&self) -> bool {
        false
    }

    fn support(&self) -> QosSupport {
        QosSupport::new()
            .with_reliable(true)
            .with_drop_oldest(true)
            .with_drop_newest(true)
            .with_wait(false)
            .with_max_depth(256)
            .with_max_history(0)
            .with_priority_levels(0)
    }

    fn open(&mut self, resource: u16, qos: &Qos) -> Result<CarrierQosRoute, QosRefused> {
        let (a, b) = (InMemoryBus::new(256, 4096), InMemoryBus::new(256, 4096));
        let provider = entity("door", 1);
        let uri = provider.get_resource_uri(resource);
        let carried = [CarriedWire::of(&Wire::<DoorState>::describe(&DoorWire))];
        let carrier = carrier_route_of(&uri, &carried, qos);
        let handoff = |bus: InMemoryBus, peer: u64| {
            TransportSetup::handoff_in(&memory(), bus.participant(peer)).unwrap()
        };
        let upstream = handoff(a, 1);
        let carrier_in = handoff(a, 2);
        let carrier_out = handoff(b, 3);
        let downstream = handoff(b, 4);
        let publisher = Publisher::<DoorState>::on(&upstream, &provider, resource)
            .wire(DoorWire)
            .qos(*qos)
            .build()
            .map_err(refused_of)?;
        let ingress = carrier_in
            .carrier_ingress(&carrier)
            .map_err(carrier_refused)?;
        let egress = carrier_out
            .carrier_egress(&carrier)
            .map_err(carrier_refused)?;
        let subscriber = Subscriber::<DoorState>::on(&downstream, uri.clone())
            .wire(DoorWire)
            .qos(*qos)
            .build()
            .map_err(refused_of)?;
        let up = upstream.enter_runtime().unwrap();
        let in_rt = carrier_in.enter_runtime().unwrap();
        let out_rt = carrier_out.enter_runtime().unwrap();
        let down = downstream.enter_runtime().unwrap();
        Ok(CarrierQosRoute {
            publisher: publisher.activate(&up).unwrap(),
            subscriber: subscriber.activate(&down).unwrap(),
            late: None,
            forwarder: TestForwarder::new(in_rt, ingress, out_rt, egress),
            egress_bus: CarrierBus::Handoff(b),
        })
    }
}

fn qos_contracts() {
    // Every binding's outcome is printed and collected, so one run shows them all.
    let mut failures = Vec::new();
    for (name, results) in [
        (
            "lending",
            run_qos_contract(&mut LendingUnderTest {
                options: LendingOptions::default(),
            }),
        ),
        (
            "lending-with-priority",
            run_qos_contract(&mut LendingUnderTest {
                options: LendingOptions {
                    priority_levels: 3,
                    ..LendingOptions::default()
                },
            }),
        ),
        ("handoff", run_qos_contract(&mut HandoffUnderTest)),
        (
            "carrier-lending-to-lending",
            run_qos_contract(&mut LendingCarrierUnderTest),
        ),
        (
            "carrier-handoff-to-handoff",
            run_qos_contract(&mut HandoffCarrierUnderTest),
        ),
    ] {
        for (check, outcome) in &results {
            println!("qos {name} {check}: {outcome:?}");
            if !outcome.is_ok() {
                failures.push(format!("{name} {check}: {outcome:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "QoS contract failures: {failures:#?}");

    // A binding lying about its capabilities fails, on the checks it lied about.
    let results = run_qos_contract(&mut LyingUnderTest);
    let failed: Vec<&str> = results
        .iter()
        .filter(|(_, outcome)| !outcome.is_ok())
        .map(|(name, _)| *name)
        .collect();
    println!("qos lying-binding failed checks: {failed:?}");
    for must_fail in [
        "wait-times-out-with-an-error",
        "history-replays-to-a-late-joiner",
        "drop-newest-keeps-oldest-depth",
    ] {
        assert!(
            failed.contains(&must_fail),
            "the lying binding should fail {must_fail}; failed: {failed:?}"
        );
    }
}

fn main() {
    wire_contract();
    println!("wire_contract: ok");
    wire_contract_negative_controls();
    println!("wire_contract_negative_controls: ok");
    binding_contracts();
    println!("binding_contracts: ok");
    binding_contract_negative_control();
    println!("binding_contract_negative_control: ok");
    qos_contracts();
    println!("qos_contracts: ok");
    println!("two_mode_binding_contracts: all contracts held");
}
