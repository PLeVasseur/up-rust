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

//! A route's QoS on pull bindings, for a plain `try_recv` reader, and what `pump`
//! does there.
//!
//! The re-check's N1 and the bindings' `Wait` finding on `a3b8396`: after H5
//! bounded the pull, a handoff route's policy reached only frames `pump` moved
//! into the inbox, and a pull taken with the inbox at depth dropped a `Wait`
//! frame the binding would have kept. Here a pull binding's queue is the route's
//! queue, and the SDK takes a frame only when it has room for it.
//!
//! The second re-check's R2-2: `pump` moved a typed route's frames into the
//! inbox, a second queue of the route's depth ahead of the binding's, so a route
//! could hold twice its depth. `pump` now serves RPC response routes only, in one
//! bounded step whose bound is the route's, never the peer's sending.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    missing_docs
)]

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

#[path = "support/review_doubles.rs"]
mod doubles;

use up_rust::startup::LEAKED_HEAP_REGIONS;
use up_rust::test_util::handoff::InMemoryBus;
use up_rust::test_util::lending::LendingBus;
use up_rust::test_util::wires::{DoorState, DoorWire};
use up_rust::two_mode::*;
use up_rust::{StaticUriProvider, UEntityId, UUri};

fn entity(authority: &str, type_id: u16) -> StaticUriProvider {
    StaticUriProvider::for_entity(authority, UEntityId::first_instance(type_id), 1).unwrap()
}

fn memory() -> StartupMemory {
    StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 64 * 1024).unwrap()
}

fn topic() -> UUri {
    entity("door", 1).get_resource_uri(0x8001)
}

fn door(id: u32) -> DoorState {
    DoorState { id, open: 0 }
}

struct Pair {
    publisher: Publisher<DoorState>,
    subscriber: Subscriber<DoorState>,
    rx: TransportRuntime,
}

fn pair_on(a: TransportSetup, b: TransportSetup, qos: Qos) -> Pair {
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .unwrap();
    let subscriber = Subscriber::<DoorState>::on(&b, topic())
        .wire(DoorWire)
        .qos(qos)
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    Pair {
        publisher: publisher.activate(&tx).unwrap(),
        subscriber: subscriber.activate(&rx).unwrap(),
        rx,
    }
}

fn handoff_pair(qos: Qos) -> Pair {
    let bus = InMemoryBus::new(32, 4096);
    pair_on(
        TransportSetup::handoff_in(&memory(), bus.participant(1)).unwrap(),
        TransportSetup::handoff_in(&memory(), bus.participant(2)).unwrap(),
        qos,
    )
}

/// A lending pair whose publisher has room for every frame of a burst, so the
/// route's own policy, and nothing else, decides what is kept.
fn lending_pair(qos: Qos) -> Pair {
    let bus = LendingBus::new();
    let (a, b) = (
        TransportSetup::lending_in(&memory(), bus.participant(1)).unwrap(),
        TransportSetup::lending_in(&memory(), bus.participant(2)).unwrap(),
    );
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .slots(16)
        .build()
        .unwrap();
    let subscriber = Subscriber::<DoorState>::on(&b, topic())
        .wire(DoorWire)
        .qos(qos)
        .build()
        .unwrap();
    let (tx, rx) = (a.enter_runtime().unwrap(), b.enter_runtime().unwrap());
    Pair {
        publisher: publisher.activate(&tx).unwrap(),
        subscriber: subscriber.activate(&rx).unwrap(),
        rx,
    }
}

fn read_all(subscriber: &Subscriber<DoorState>) -> Vec<u32> {
    let mut got = Vec::new();
    for _ in 0..64 {
        match subscriber.try_recv().unwrap() {
            Some(frame) => got.push(frame.id),
            None => break,
        }
    }
    got
}

fn depth(depth: u16, overflow: Overflow) -> Qos {
    Qos::DEFAULT.with_depth(depth).with_overflow(overflow)
}

const WAIT: Overflow = Overflow::Wait {
    timeout: Duration::from_secs(5),
};

const WALL: u64 = 2_000_000_000_000;

const METHOD: u16 = 0x0001;

/// Queues `count` frames carrying `1..=count` in a double's queue.
fn queue_values(shared: &doubles::Shared, topic: &UUri, count: u32) {
    for value in 1..=count {
        shared.queue.lock().unwrap().push_back((
            doubles::publish_frame(
                topic,
                doubles::uuid_at(WALL, u64::from(value)),
                None,
                None,
                &value.to_le_bytes(),
            ),
            doubles::peer(7, 1),
        ));
    }
}

fn drain_values(subscriber: &Subscriber<u32>) -> Vec<u32> {
    let mut got = Vec::new();
    while let Some(value) = subscriber.try_recv().unwrap() {
        got.push(*value);
    }
    got
}

// ------------------------------------------------- N1: a plain `try_recv` --

/// N1, `DropOldest`. Eight frames on a depth-2 route of the SDK's own handoff bus,
/// read with `try_recv` alone: the two newest, and six counted drops. At
/// `a3b8396` the reader got all eight and nothing was counted.
#[test]
fn n1_a_try_recv_reader_gets_drop_oldest_on_the_handoff_bus() {
    let pair = handoff_pair(depth(2, Overflow::DropOldest));
    for id in 0..8 {
        pair.publisher.publish(&door(id)).unwrap();
    }
    assert_eq!(read_all(&pair.subscriber), vec![6, 7]);
    let counters = pair.subscriber.counters();
    assert_eq!(counters.dropped_oldest, 6);
    assert_eq!(counters.dropped_newest, 0);
}

/// N1, `DropNewest`: the first two, and six counted drops.
#[test]
fn n1_a_try_recv_reader_gets_drop_newest_on_the_handoff_bus() {
    let pair = handoff_pair(depth(2, Overflow::DropNewest));
    for id in 0..8 {
        pair.publisher.publish(&door(id)).unwrap();
    }
    assert_eq!(read_all(&pair.subscriber), vec![0, 1]);
    let counters = pair.subscriber.counters();
    assert_eq!(counters.dropped_newest, 6);
    assert_eq!(counters.dropped_oldest, 0);
}

// ---------------------------------------------------- `Wait` never drops --

/// The bindings' finding, on the SDK's lending bus: every frame of a `Wait` route
/// reaches the reader in order, and nothing is dropped. At `a3b8396` a second
/// `pump` took two frames and dropped them. `pump` now takes nothing from a typed
/// route (R2-2); at `2454e24` it moved two frames into the inbox.
#[test]
fn wait_pump_leaves_a_lending_route_to_its_reader() {
    let bus = LendingBus::new();
    let pair = pair_on(
        TransportSetup::lending_in(&memory(), bus.participant(1)).unwrap(),
        TransportSetup::lending_in(&memory(), bus.participant(2)).unwrap(),
        depth(2, WAIT),
    );
    pair.publisher.publish(&door(0)).unwrap();
    pair.publisher.publish(&door(1)).unwrap();
    assert_eq!(pair.rx.pump(), 0, "pump takes nothing from a typed route");
    assert_eq!(read_all(&pair.subscriber), vec![0, 1]);
    pair.publisher.publish(&door(2)).unwrap();
    pair.publisher.publish(&door(3)).unwrap();
    assert_eq!(pair.rx.pump(), 0);
    assert_eq!(read_all(&pair.subscriber), vec![2, 3]);
    let counters = pair.subscriber.counters();
    assert_eq!(counters.dropped_newest, 0, "Wait drops nothing");
    assert_eq!(counters.carried, 4);
}

/// The handoff pull path, on a double whose queue holds more than the route's
/// depth: `pump` leaves the frames in the binding, and the reader pulls each in
/// order with nothing dropped. At `a3b8396` the third frame was taken and
/// dropped; at `2454e24` `pump` pulled two of them into the inbox (R2-2).
#[test]
fn wait_handoff_pump_leaves_the_frames_in_the_binding() {
    let clock = doubles::Clock::leak(1_000_000_000, WALL);
    let shared = doubles::Shared::leak();
    let setup =
        TransportSetup::handoff_on(&doubles::memory(), clock, doubles::HandoffDouble(shared))
            .unwrap();
    let topic = doubles::uri(9, 0x8001);
    let subscriber = Subscriber::<u32>::on(&setup, topic.clone())
        .wire(doubles::U32Wire { hook: None })
        .qos(depth(2, WAIT))
        .build()
        .unwrap();
    let rx = setup.enter_runtime().unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    queue_values(shared, &topic, 4);
    assert_eq!(rx.pump(), 0, "pump takes nothing from a typed route");
    assert_eq!(
        shared.queue.lock().unwrap().len(),
        4,
        "the binding keeps them all"
    );
    assert_eq!(drain_values(&subscriber), vec![1, 2, 3, 4]);
    assert_eq!(subscriber.counters().dropped_newest, 0);
}

/// A lending double whose pull holds each caller until a second caller is inside
/// it too, or 300 ms have passed: so two concurrent callers both pull, when both
/// may.
struct Gate {
    shared: &'static doubles::Shared,
    inside: AtomicUsize,
}

struct Gated(&'static Gate);
struct GatedRunning(&'static Gate);

impl BindingSetup for Gated {
    type Running = GatedRunning;
    fn name(&self) -> &'static str {
        "gated-lending"
    }
    fn capabilities(&self) -> Capabilities {
        doubles::capabilities()
    }
    fn qos_support(&self, _: RouteKind) -> QosSupport {
        doubles::all_qos()
    }
    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        if let Some(inbound) = route.inbound {
            self.0.shared.inbound.lock().unwrap().push(inbound);
        }
        Ok(PreparedRoute::new(BindingRoute(0), RuntimeHeap::None))
    }
    fn into_running(self, port: RuntimePort) -> Result<GatedRunning, BindingRefusal> {
        let _ = self.0.shared.port.set(port);
        Ok(GatedRunning(self.0))
    }
}

impl LendingRuntime for GatedRunning {
    // Explicit single-attempt adapter, justified by this retained implementation.
    fn receive_bounded(&self, route: up_rust::two_mode::BindingRoute, budget: &up_rust::two_mode::ReceiveBudget) -> Option<Self::Rx> {
        if !budget.try_take() { return None; }
        self.receive(route)
    }

    type Tx<'a> = doubles::NoTx;
    type Rx = doubles::HeapFrame;
    fn loan(&self, _: BindingRoute, _: TxFrameLayout) -> Result<doubles::NoTx, LoanError> {
        Err(LoanError::InvalidRoute)
    }
    fn publish(
        &self,
        _: BindingRoute,
        _: InitializedTx<doubles::NoTx>,
    ) -> Result<SendReport, SendError> {
        Err(SendError::Delivery)
    }
    fn receive(&self, _: BindingRoute) -> Option<doubles::HeapFrame> {
        self.0.inside.fetch_add(1, Ordering::SeqCst);
        let started = Instant::now();
        while self.0.inside.load(Ordering::SeqCst) < 2
            && started.elapsed() < Duration::from_millis(300)
        {
            std::thread::yield_now();
        }
        self.0
            .shared
            .queue
            .lock()
            .unwrap()
            .pop_front()
            .map(|(bytes, peer)| doubles::HeapFrame {
                bytes,
                peer,
                shared: self.0.shared,
            })
    }
}

/// A depth-1 `Wait` route on the gated double, holding two frames, decoded through
/// `hook`.
fn gated_wait_route(
    hook: Option<fn(&mut u32)>,
) -> (&'static doubles::Shared, TransportRuntime, Subscriber<u32>) {
    let clock = doubles::Clock::leak(1_000_000_000, WALL);
    let shared = doubles::Shared::leak();
    let gate: &'static Gate = Box::leak(Box::new(Gate {
        shared,
        inside: AtomicUsize::new(0),
    }));
    let setup = TransportSetup::lending_on(&doubles::memory(), clock, Gated(gate)).unwrap();
    let topic = doubles::uri(9, 0x8001);
    let subscriber = Subscriber::<u32>::on(&setup, topic.clone())
        .wire(doubles::U32Wire { hook })
        .qos(depth(1, WAIT))
        .max_held(2)
        .build()
        .unwrap();
    let rx = setup.enter_runtime().unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    queue_values(shared, &topic, 2);
    (shared, rx, subscriber)
}

/// Two `pump` callers at once on a depth-1 `Wait` route. At `a3b8396` both pulled
/// a frame, and the one that found the depth taken dropped its frame; at
/// `2454e24` one of them pulled a frame into the inbox. `pump` now takes nothing
/// from a typed route, from any number of threads (R2-2), and the reader gets
/// both frames. Threaded: Miri runs this with many seeds.
#[test]
fn wait_two_pumps_on_a_depth_one_route_take_nothing() {
    let (shared, rx, subscriber) = gated_wait_route(None);
    std::thread::scope(|scope| {
        let first = scope.spawn(|| rx.pump());
        let second = scope.spawn(|| rx.pump());
        let pulled = first.join().unwrap() + second.join().unwrap();
        assert_eq!(pulled, 0, "pump takes nothing from a typed route");
    });
    assert_eq!(drain_values(&subscriber), vec![1, 2], "no frame was lost");
    let counters = subscriber.counters();
    assert_eq!(counters.dropped_newest, 0);
    assert_eq!(shared.frames_dropped.load(Ordering::SeqCst), 2);
}

/// Decodes slowly, so a reader that reserved depth still holds it while a second
/// reader would try to reserve.
fn slow_decode(_: &mut u32) {
    std::thread::sleep(Duration::from_millis(50));
}

/// Two `try_recv` callers at once on a depth-1 `Wait` route, the race the bindings
/// read from the L1 fix. At `a3b8396` both pulled; one reserved the depth while
/// decoding, and the other found it taken and dropped its frame. Threaded: Miri
/// runs this with many seeds.
#[test]
fn wait_two_readers_on_a_depth_one_route_lose_nothing() {
    let (shared, _rx, subscriber) = gated_wait_route(Some(slow_decode));
    let taken: Vec<u32> = std::thread::scope(|scope| {
        let readers: Vec<_> = (0..2)
            .map(|_| scope.spawn(|| subscriber.try_recv().unwrap().map(|value| *value)))
            .collect();
        readers
            .into_iter()
            .filter_map(|reader| reader.join().unwrap())
            .collect()
    });
    let mut got = taken;
    got.extend(drain_values(&subscriber));
    got.sort_unstable();
    assert_eq!(got, vec![1, 2], "no frame was lost");
    assert_eq!(subscriber.counters().dropped_newest, 0);
    assert_eq!(shared.frames_dropped.load(Ordering::SeqCst), 2);
}

// ----------------------------------------------------- N3: `pump` is bounded --

/// N3. On a conforming pull binding, one `pump` call is the whole drain: it hands
/// each response the bus holds to its call, and the next call finds nothing.
#[test]
fn n3_one_pump_call_drains_a_conforming_binding() {
    let bus = InMemoryBus::new(8, 4096);
    let a = TransportSetup::handoff_in(&memory(), bus.participant(1)).unwrap();
    let b = TransportSetup::handoff_in(&memory(), bus.participant(2)).unwrap();
    let client = RpcClient::<DoorState, DoorState>::on(
        &a,
        &entity("door", 1),
        entity("door", 2).get_resource_uri(METHOD),
    )
    .wire(DoorWire)
    .ttl(Duration::from_secs(30))
    .max_in_flight(3)
    .build()
    .unwrap();
    let server = RpcServer::<DoorState, DoorState>::on(&b, &entity("door", 2), METHOD)
        .wire(DoorWire)
        .clients(&[])
        .build()
        .unwrap();
    let (client_rt, server_rt) = (a.enter_runtime().unwrap(), b.enter_runtime().unwrap());
    let (client, server) = (
        client.activate(&client_rt).unwrap(),
        server.activate(&server_rt).unwrap(),
    );
    let mut calls: Vec<_> = (0..3).map(|id| client.invoke(&door(id)).unwrap()).collect();
    for _ in 0..3 {
        let request = server.try_recv().unwrap().expect("a request");
        let asked = request.id;
        request.respond(&door(asked + 10)).unwrap();
    }
    assert_eq!(client_rt.pump(), 3, "the bus held the three responses");
    assert_eq!(client_rt.pump(), 0, "nothing left: no loop is needed");
    for (id, call) in (0..).zip(calls.iter_mut()) {
        let reply = call.try_response().unwrap().expect("pump delivered it");
        assert_eq!(reply.id, id + 10);
    }
}

/// N3. Against a peer that never stops sending, one `pump` call examines at most a
/// response route's bound, its call table plus one: the bound is the route's,
/// not the peer's. The flood's frames are refused, and counted as examined.
#[test]
fn n3_one_pump_call_is_bounded_by_the_route_against_a_flood() {
    let clock = doubles::Clock::leak(1_000_000_000, WALL);
    let shared = doubles::Shared::leak();
    let setup =
        TransportSetup::handoff_on(&doubles::memory(), clock, doubles::HandoffDouble(shared))
            .unwrap();
    let topic = doubles::uri(9, 0x8001);
    let client = RpcClient::<u32, u32>::on(&setup, &doubles::Uri(3), doubles::uri(4, METHOD))
        .wire(doubles::U32Wire { hook: None })
        .ttl(Duration::from_secs(30))
        .max_in_flight(4)
        .build()
        .unwrap();
    let rx = setup.enter_runtime().unwrap();
    let _client = client.activate(&rx).unwrap();
    *shared.flood_frame.lock().unwrap() = (
        doubles::publish_frame(
            &topic,
            doubles::uuid_at(WALL, 1),
            None,
            None,
            &1_u32.to_le_bytes(),
        ),
        doubles::peer(7, 1),
    );
    shared.flood.store(1_000_000, Ordering::SeqCst);
    for _ in 0..3 {
        assert_eq!(rx.pump(), 5, "the call table plus one");
    }
}

// ---------------------------------------- R2-2: `pump` and a typed route --

/// R2-2, the second re-check's case C: `pump` once between two bursts, then read,
/// on a depth-2 `DropOldest` route, on both of the SDK's buses. The route has one
/// queue, so the reader gets the newest two, and six drops are counted. At
/// `2454e24` `pump` moved the first burst's two survivors into the inbox, a second
/// queue ahead of the binding's: the reader got `[2, 3, 6, 7]`, and 4 and 5 had
/// been dropped as oldest while older frames were kept.
#[test]
fn r2_2_pump_between_bursts_keeps_a_drop_oldest_route_at_its_depth() {
    for (bus, pair) in [
        ("handoff", handoff_pair(depth(2, Overflow::DropOldest))),
        ("lending", lending_pair(depth(2, Overflow::DropOldest))),
    ] {
        for id in 0..4 {
            pair.publisher.publish(&door(id)).unwrap();
        }
        let pumped = pair.rx.pump();
        for id in 4..8 {
            pair.publisher.publish(&door(id)).unwrap();
        }
        assert_eq!(read_all(&pair.subscriber), vec![6, 7], "{bus}");
        assert_eq!(pair.subscriber.counters().dropped_oldest, 6, "{bus}");
        assert_eq!(pumped, 0, "{bus}: pump takes nothing from a typed route");
    }
}

/// R2-2 on a depth-2 `DropNewest` route: with `pump` between two bursts, the
/// route's one queue keeps the first two, and six arrivals are refused and
/// counted. At `2454e24` the reader got `[0, 1, 4, 5]`: `pump` had emptied the
/// binding's queue into the inbox, so the second burst found room.
#[test]
fn r2_2_pump_between_bursts_keeps_a_drop_newest_route_at_its_depth() {
    let pair = handoff_pair(depth(2, Overflow::DropNewest));
    for id in 0..4 {
        pair.publisher.publish(&door(id)).unwrap();
    }
    let pumped = pair.rx.pump();
    for id in 4..8 {
        pair.publisher.publish(&door(id)).unwrap();
    }
    assert_eq!(read_all(&pair.subscriber), vec![0, 1]);
    assert_eq!(pair.subscriber.counters().dropped_newest, 6);
    assert_eq!(pumped, 0, "pump takes nothing from a typed route");
}

/// R2-2 on a depth-2 `Wait` route with nobody reading: `pump` does not let the
/// sender run ahead of the route's depth, so the third publish waits for room
/// and runs out of time. At `2454e24` one `pump` moved two frames out of the
/// binding's queue, and the third publish went through at once.
#[test]
fn r2_2_pump_does_not_let_a_wait_sender_run_ahead_of_the_depth() {
    let short = Overflow::Wait {
        timeout: Duration::from_millis(50),
    };
    let pair = lending_pair(depth(2, short));
    pair.publisher.publish(&door(0)).unwrap();
    pair.publisher.publish(&door(1)).unwrap();
    let pumped = pair.rx.pump();
    let third = pair.publisher.publish(&door(2));
    assert!(
        matches!(third, Err(SendError::TimedOut)),
        "the third publish waits for room: {third:?}"
    );
    assert_eq!(read_all(&pair.subscriber), vec![0, 1]);
    assert_eq!(pumped, 0, "pump takes nothing from a typed route");
}

/// R2-3. The SDK's handoff bus refuses at setup a route deeper than its buffers,
/// since that queue could not hold the route's depth (R44: honoured or refused).
/// The limit covers every route on the bus, whose `qos_support` cannot tell a
/// sender from a receiver. A route as deep as its buffers is admitted and gets
/// its policy. At `85dae7d` the deeper route was admitted: on a 2-frame bus a
/// depth-4 `DropOldest` route refused publishes 3 to 6 `Busy` and counted no drop.
#[test]
fn r2_3_the_handoff_bus_refuses_a_route_deeper_than_its_buffers() {
    let bus = InMemoryBus::new(2, 4096);
    let setup = TransportSetup::handoff_in(&memory(), bus.participant(2)).unwrap();
    let deeper = Subscriber::<DoorState>::on(&setup, topic())
        .wire(DoorWire)
        .qos(depth(4, Overflow::DropOldest))
        .build();
    assert!(
        matches!(
            deeper,
            Err(StartupError::QosNotHonored {
                setting: QosSetting::Depth,
                requested: 4,
                available: 2,
                ..
            })
        ),
        "{deeper:?}"
    );
    let bus = InMemoryBus::new(2, 4096);
    let qos = depth(2, Overflow::DropOldest);
    let a = TransportSetup::handoff_in(&memory(), bus.participant(1)).unwrap();
    let b = TransportSetup::handoff_in(&memory(), bus.participant(2)).unwrap();
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .qos(qos)
        .build()
        .unwrap();
    let subscriber = Subscriber::<DoorState>::on(&b, topic())
        .wire(DoorWire)
        .qos(qos)
        .build()
        .unwrap();
    let (tx, rx) = (a.enter_runtime().unwrap(), b.enter_runtime().unwrap());
    let (publisher, subscriber) = (
        publisher.activate(&tx).unwrap(),
        subscriber.activate(&rx).unwrap(),
    );
    for id in 0..6 {
        publisher.publish(&door(id)).unwrap();
    }
    assert_eq!(read_all(&subscriber), vec![4, 5]);
    assert_eq!(subscriber.counters().dropped_oldest, 4);
}

/// The handoff bus's medium, apart from any route's depth: `with_medium` bounds
/// the frames in flight, so a send beyond it is refused `Busy`, and the sender
/// sees no capacity, until a receiver takes a frame.
#[test]
fn a_full_handoff_medium_refuses_a_send_until_a_frame_is_taken() {
    let bus = InMemoryBus::with_medium(8, 4096, 1);
    let pair = pair_on(
        TransportSetup::handoff_in(&memory(), bus.participant(1)).unwrap(),
        TransportSetup::handoff_in(&memory(), bus.participant(2)).unwrap(),
        Qos::DEFAULT,
    );
    pair.publisher.publish(&door(1)).unwrap();
    assert!(matches!(
        pair.publisher.publish(&door(2)),
        Err(SendError::Busy)
    ));
    assert_eq!(read_all(&pair.subscriber), vec![1]);
    pair.publisher.publish(&door(3)).unwrap();
    assert_eq!(read_all(&pair.subscriber), vec![3]);
}

/// R3-3. The bus limits a receive route's depth to its buffers, and only a receive
/// route's: a send route has no queue here, so its depth sizes nothing. A
/// publisher at the default depth on a 4-frame bus is admitted and delivers, while
/// a subscriber at that depth is still refused. At `2454e24` the publisher was
/// refused too, with `QosNotHonored { Depth, 8, 4 }` (the reviewer's
/// `bus_unwind3.rs`).
#[test]
fn r3_3_the_handoff_bus_limits_only_receive_routes_to_its_buffers() {
    assert_eq!(Qos::DEFAULT.depth, 8, "the default depth");
    let bus = InMemoryBus::new(4, 4096);
    let a = TransportSetup::handoff_in(&memory(), bus.participant(1)).unwrap();
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build();
    assert!(publisher.is_ok(), "a default publisher: {publisher:?}");
    let other = TransportSetup::handoff_in(&memory(), bus.participant(3)).unwrap();
    let deeper = Subscriber::<DoorState>::on(&other, topic())
        .wire(DoorWire)
        .build();
    assert!(
        matches!(
            deeper,
            Err(StartupError::QosNotHonored {
                setting: QosSetting::Depth,
                requested: 8,
                available: 4,
                ..
            })
        ),
        "a default subscriber: {deeper:?}"
    );
    let b = TransportSetup::handoff_in(&memory(), bus.participant(2)).unwrap();
    let subscriber = Subscriber::<DoorState>::on(&b, topic())
        .wire(DoorWire)
        .qos(depth(4, Overflow::DropOldest))
        .build()
        .unwrap();
    let (tx, rx) = (a.enter_runtime().unwrap(), b.enter_runtime().unwrap());
    let (publisher, subscriber) = (
        publisher.unwrap().activate(&tx).unwrap(),
        subscriber.activate(&rx).unwrap(),
    );
    for id in 0..6 {
        publisher.publish(&door(id)).unwrap();
    }
    assert_eq!(read_all(&subscriber), vec![2, 3, 4, 5]);
}

/// Decodes left to unwind, for `r3_2_…`.
static DECODE_UNWINDS_LEFT: AtomicU32 = AtomicU32::new(0);

fn unwind_while_left(_: &mut u32) {
    if DECODE_UNWINDS_LEFT.load(Ordering::SeqCst) > 0 {
        DECODE_UNWINDS_LEFT.fetch_sub(1, Ordering::SeqCst);
        panic!("the wire's decode fails");
    }
}

/// R3-2, R2-4's class on the SDK's own handoff bus. A depth-2 `DropOldest` route on
/// a 2-frame bus, read through a wire whose `decode` unwinds twice. Each unwind
/// costs its frame, counted as refused, and the bus gets its buffer back: two
/// more publishes are taken, and both frames are read. At `2454e24` each unwind
/// kept a buffer for good: after two, both publishes were `Busy` and nothing was
/// read (the reviewer's `bus_unwind3.rs`).
#[test]
fn r3_2_a_decode_that_unwinds_gives_the_handoff_bus_its_buffer_back() {
    let bus = InMemoryBus::new(2, 4096);
    let a = TransportSetup::handoff_in(&memory(), bus.participant(1)).unwrap();
    let b = TransportSetup::handoff_in(&memory(), bus.participant(2)).unwrap();
    let qos = depth(2, Overflow::DropOldest);
    let publisher = Publisher::<u32>::on(&a, &doubles::Uri(9), 0x8001_u16)
        .wire(doubles::U32Wire { hook: None })
        .qos(qos)
        .build()
        .unwrap();
    let subscriber = Subscriber::<u32>::on(&b, doubles::uri(9, 0x8001))
        .wire(doubles::U32Wire {
            hook: Some(unwind_while_left),
        })
        .qos(qos)
        .build()
        .unwrap();
    let (tx, rx) = (a.enter_runtime().unwrap(), b.enter_runtime().unwrap());
    let (publisher, subscriber) = (
        publisher.activate(&tx).unwrap(),
        subscriber.activate(&rx).unwrap(),
    );
    DECODE_UNWINDS_LEFT.store(2, Ordering::SeqCst);
    for value in [100_u32, 101] {
        publisher.publish(&value).unwrap();
        let read = catch_unwind(AssertUnwindSafe(|| {
            subscriber
                .try_recv()
                .map(|received| received.map(|got| *got))
        }));
        assert!(read.is_err(), "the decode of {value} unwinds");
    }
    assert_eq!(
        subscriber.counters().refused,
        2,
        "each lost frame is counted"
    );
    let sent: Vec<_> = (1..=2_u32)
        .map(|value| publisher.publish(&value).map(|_| ()))
        .collect();
    assert_eq!(sent, vec![Ok(()), Ok(())], "the bus has both buffers back");
    assert_eq!(drain_values(&subscriber), vec![1, 2]);
}
