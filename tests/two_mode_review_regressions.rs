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

//! Regression tests for the stage-2 review findings, on the API both sides of the
//! fixes share, so each one can run against the commit the review reported on.
//! Findings needing new API are in `two_mode_review_new_api.rs`, and compile-fail
//! cases in `tests/ui/two_mode_merged`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::wildcard_enum_match_arm,
    clippy::too_many_lines,
    missing_docs
)]

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use core::time::Duration;

#[path = "support/review_doubles.rs"]
mod doubles;
use doubles::*;

use up_rust_core::communication::two_mode::{RpcClient, RpcServer, Subscriber};
use up_rust_core::memory::{RegionSource, StartupMemory};
use up_rust_core::signal::SignalWait;
use up_rust_core::transport::binding::{
    BindingRefusal, BindingRoute, BindingSetup, Capabilities, InboundFrame, LendingRuntime,
    PayloadBytes, PeerRef, PreparedRoute, ReplyToken, RouteDeclaration, RouteKind, RuntimePort,
};
use up_rust_core::transport::carrier::{CarriedWire, CarrierRoute};
use up_rust_core::transport::qos::{check_qos, Overflow, Qos, QosSupport, SendReport};
use up_rust_core::transport::{
    Flow, InitializedTx, LoanError, ReceiveError, SendError, StartupError, TransportRuntime,
    TransportSetup, TxFrameLayout,
};
use up_rust_core::uattributes::UMessageType;
use up_rust_core::uri::UUri;
use up_rust_core::wire::{RuntimeHeap, WirePayloadIdentity};

const WALL: u64 = 2_000_000_000_000;
const QOS_DEPTH: usize = Qos::DEFAULT.depth as usize;

fn carried() -> &'static [CarriedWire] {
    Box::leak(Box::new([CarriedWire {
        wire_id: WIRE_ID,
        payload_family: FAMILY_ID,
        identity: WirePayloadIdentity::serialized(ENCODING),
        alignment: 1,
    }]))
}

fn wire() -> U32Wire {
    U32Wire { hook: None }
}

fn deliver(shared: &Shared, route: usize, frame: &[u8], from: PeerRef) -> Result<(), ReceiveError> {
    shared.port().deliver(
        shared.route(route),
        InboundFrame {
            metadata: &[],
            payload: PayloadBytes::one(frame),
            peer: from,
            reply: ReplyToken::default(),
        },
    )
}

fn request_frame(from: &UUri, method: &UUri, n: u64, ttl: Duration) -> Vec<u8> {
    whole_frame(&Spec {
        kind: UMessageType::Request,
        id: uuid_at(WALL, n),
        reqid: None,
        ttl: Some(ttl),
        source: from.clone(),
        sink: Some(method.clone()),
        token: None,
        payload: Some(&1u32.to_le_bytes()),
    })
}

struct Server {
    shared: &'static Shared,
    server: RpcServer<u32, u32>,
}

fn server_on_handoff(clients: &[UUri], clock: &'static Clock) -> Server {
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
    let local = Uri(3);
    let server = RpcServer::<u32, u32>::on(&setup, &local, 1_u16)
        .wire(wire())
        .clients(clients)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let server = server.activate(&runtime).unwrap();
    Server { shared, server }
}

/// H1. Strangers must not consume a peer entry, so declared clients are never
/// refused because of them, and a stranger is refused and counted on arrival.
#[test]
fn h1_declared_clients_survive_strangers() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let method = uri(3, 1);
    let declared = [uri(100, 0)];
    let rig = server_on_handoff(&declared, clock);
    // Eight stranger source URIs, as many as the table has entries.
    for stranger in 0..8_u16 {
        let source = uri(1000 + stranger, 0);
        let frame = request_frame(
            &source,
            &method,
            u64::from(stranger),
            Duration::from_secs(10),
        );
        let _ = deliver(rig.shared, 0, &frame, peer(5000 + u64::from(stranger), 1));
    }
    while rig.server.try_recv().unwrap().is_some() {}
    let frame = request_frame(&declared[0], &method, 99, Duration::from_secs(10));
    assert_eq!(
        deliver(rig.shared, 0, &frame, peer(1, 1)),
        Ok(()),
        "a declared client must still be admitted after strangers"
    );
    assert!(
        rig.server.try_recv().unwrap().is_some(),
        "the declared client's request is served"
    );
    assert_eq!(rig.server.counters().unknown_client, 8);
}

/// H1 (second part). A peer reported gone gives its table entry back.
#[test]
fn h1_a_lost_peer_gives_its_table_entry_back() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let sub = Subscriber::<u32>::on(&setup, topic.clone())
        .wire(wire())
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let sub = sub.activate(&runtime).unwrap();
    let frame = publish_frame(&topic, uuid_at(WALL, 1), None, None, &1u32.to_le_bytes());
    // Fill the table: 8 peers.
    for id in 1..=8_u64 {
        deliver(shared, 0, &frame, peer(100 + id, 1)).unwrap();
        let _ = sub.try_recv();
    }
    assert_eq!(
        deliver(shared, 0, &frame, peer(999, 1)),
        Err(ReceiveError::PeerTableFull),
        "control: the table is full"
    );
    shared.port().peer_lost(shared.route(0), peer(103, 1));
    assert_eq!(
        deliver(shared, 0, &frame, peer(999, 1)),
        Ok(()),
        "the entry of a lost peer serves another peer"
    );
}

/// H2. The table is sized from the declared clients.
#[test]
fn h2_a_ninth_declared_client_is_admitted() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let method = uri(3, 1);
    let clients: Vec<UUri> = (0..9_u16).map(|n| uri(200 + n, 0)).collect();
    let rig = server_on_handoff(&clients, clock);
    for (n, client) in clients.iter().enumerate() {
        let frame = request_frame(client, &method, n as u64, Duration::from_secs(10));
        assert_eq!(
            deliver(rig.shared, 0, &frame, peer(300 + n as u64, 1)),
            Ok(()),
            "client {n} of 9 is declared and must be admitted"
        );
    }
}

/// H4. Several waiters on one idle signal stay asleep: nothing bumps it.
#[test]
#[cfg_attr(miri, ignore = "real timeouts on the native platform")]
fn h4_several_idle_waiters_do_not_wake_each_other() {
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_in(&memory, HandoffDouble(shared)).unwrap();
    let runtime: TransportRuntime = setup.enter_runtime().unwrap();
    let stop = AtomicBool::new(false);
    let notified = AtomicU64::new(0);
    let returns = AtomicU64::new(0);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    let token = runtime.wait_token();
                    if matches!(
                        runtime.wait(token, Duration::from_millis(50)),
                        SignalWait::Notified
                    ) {
                        notified.fetch_add(1, Ordering::Relaxed);
                    }
                    returns.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
        std::thread::sleep(Duration::from_millis(300));
        stop.store(true, Ordering::Relaxed);
    });
    assert_eq!(
        notified.load(Ordering::Relaxed),
        0,
        "with no traffic and no deadline, nothing may notify a waiter"
    );
    // 4 waiters, 50 ms each: about 24 returns; the base code gave thousands.
    assert!(
        returns.load(Ordering::Relaxed) <= 80,
        "idle waiters must sleep through their timeouts: {} returns",
        returns.load(Ordering::Relaxed)
    );
}

/// H4 (the positive case): a service tick that did something still wakes a waiter.
#[test]
#[cfg_attr(miri, ignore = "real wake-ups on the native platform")]
fn h4_a_service_tick_that_expired_a_deadline_wakes_a_waiter() {
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_in(&memory, HandoffDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let sub = Subscriber::<u32>::on(&setup, topic)
        .wire(wire())
        .qos(Qos::DEFAULT.with_deadline(Duration::from_millis(20)))
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let sub = sub.activate(&runtime).unwrap();
    let notified = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let token = runtime.wait_token();
            if matches!(
                runtime.wait(token, Duration::from_millis(1000)),
                SignalWait::Notified
            ) {
                notified.fetch_add(1, Ordering::Relaxed);
            }
        });
        let started = std::time::Instant::now();
        while runtime.service() == 0 && started.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    assert!(
        sub.counters().deadline_missed >= 1,
        "control: a miss happened"
    );
    assert_eq!(
        notified.load(Ordering::Relaxed),
        1,
        "a tick that counted a miss wakes the waiter"
    );
}

/// H5. A flood from an undeclared client holds `try_recv` for at most the route's
/// bound of frames; the rest stay with the binding.
#[test]
fn h5_one_try_recv_examines_at_most_the_route_bound_of_a_flood() {
    for lending in [true, false] {
        let clock = Clock::leak(1_000_000_000, WALL);
        let shared = Shared::leak();
        let memory = memory();
        let setup = if lending {
            TransportSetup::lending_on(&memory, clock, LendingDouble(shared)).unwrap()
        } else {
            TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap()
        };
        let local = Uri(3);
        let method = uri(3, 1);
        let declared = [uri(100, 0)];
        let server = RpcServer::<u32, u32>::on(&setup, &local, 1_u16)
            .wire(wire())
            .clients(&declared)
            .build()
            .unwrap();
        let runtime = setup.enter_runtime().unwrap();
        let server = server.activate(&runtime).unwrap();
        let stranger = uri(4000, 0);
        let frame = request_frame(&stranger, &method, 1, Duration::from_secs(10));
        let flood = if cfg!(miri) { 64 } else { 200_000 };
        *shared.flood_frame.lock().unwrap() = (frame, peer(4000, 1));
        shared.flood.store(flood, Ordering::SeqCst);
        assert!(server.try_recv().unwrap().is_none());
        let examined = flood - shared.flood.load(Ordering::SeqCst);
        assert!(
            examined <= QOS_DEPTH + 1,
            "lending={lending}: one try_recv examined {examined} frames; the bound is depth + 1"
        );
    }
}

/// H5. `pump` is bounded per route, on both binding families: against a flood,
/// one call examines at most a response route's bound, its call table plus one.
/// A subscriber on the same runtime is left to its reader (R2-2), so the flood
/// reaches it only through `try_recv`. At `2454e24` `pump` also drained the
/// subscriber's bound from the flood.
#[test]
fn h5_one_pump_examines_at_most_the_route_bound_of_a_flood() {
    const CALLS: usize = 4;
    for lending in [true, false] {
        let clock = Clock::leak(1_000_000_000, WALL);
        let shared = Shared::leak();
        let memory = memory();
        let setup = if lending {
            TransportSetup::lending_on(&memory, clock, LendingDouble(shared)).unwrap()
        } else {
            TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap()
        };
        let topic = uri(9, 0x8001);
        let sub = Subscriber::<u32>::on(&setup, topic.clone())
            .wire(wire())
            .build()
            .unwrap();
        let client = RpcClient::<u32, u32>::on(&setup, &Uri(3), uri(4, 0x0001))
            .wire(wire())
            .ttl(Duration::from_secs(30))
            .max_in_flight(CALLS)
            .build()
            .unwrap();
        let runtime = setup.enter_runtime().unwrap();
        let _sub = sub.activate(&runtime).unwrap();
        let _client = client.activate(&runtime).unwrap();
        let frame = publish_frame(&topic, uuid_at(WALL, 1), None, None, &1u32.to_le_bytes());
        let flood = if cfg!(miri) { 64 } else { 200_000 };
        *shared.flood_frame.lock().unwrap() = (frame, peer(7, 1));
        shared.flood.store(flood, Ordering::SeqCst);
        let pumped = runtime.pump();
        let examined = flood - shared.flood.load(Ordering::SeqCst);
        assert_eq!(
            examined,
            CALLS + 1,
            "lending={lending}: one pump examined {examined} frames"
        );
        assert_eq!(pumped, examined, "pump reports what it examined");
    }
}

/// M1. A TTL below one millisecond expires; it never fails open.
#[test]
fn m1_a_sub_millisecond_ttl_expires() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let sub = Subscriber::<u32>::on(&setup, topic.clone())
        .wire(wire())
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let sub = sub.activate(&runtime).unwrap();
    let old = uuid_at(WALL - 3_600_000, 1);
    let ms = publish_frame(
        &topic,
        old.clone(),
        Some(Duration::from_millis(1)),
        None,
        &1u32.to_le_bytes(),
    );
    deliver(shared, 0, &ms, peer(7, 1)).unwrap();
    assert!(
        sub.try_recv().unwrap().is_none(),
        "control: a 1 ms TTL an hour old is refused"
    );
    let us = publish_frame(
        &topic,
        old,
        Some(Duration::from_micros(500)),
        None,
        &2u32.to_le_bytes(),
    );
    deliver(shared, 0, &us, peer(7, 1)).unwrap();
    assert!(
        sub.try_recv().unwrap().is_none(),
        "a 500 us TTL an hour old must be expired, not delivered"
    );
}

/// M2. A wake hint is not an arrival, on a typed route.
#[test]
fn m2_a_wake_hint_is_not_an_arrival() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::lending_on(&memory, clock, LendingDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let qos = Qos::DEFAULT.with_deadline(Duration::from_millis(100));
    let sub = Subscriber::<u32>::on(&setup, topic.clone())
        .wire(wire())
        .qos(qos)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let sub = sub.activate(&runtime).unwrap();
    for _ in 0..20 {
        clock.advance_ms(60);
        shared.port().readable(shared.route(0));
        let _ = runtime.service();
    }
    assert_eq!(sub.counters().carried, 0, "control: no frame arrived");
    assert!(
        sub.counters().deadline_missed >= 1,
        "hints without frames must count as silence: missed={}",
        sub.counters().deadline_missed
    );
}

/// M2. The same, on a carrier ingress.
#[test]
fn m2_a_wake_hint_is_not_an_arrival_on_a_carrier_ingress() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::lending_on(&memory, clock, LendingDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let qos = Qos::DEFAULT.with_deadline(Duration::from_millis(100));
    let mut route = CarrierRoute::new(&topic, carried(), 64);
    route.kind = Some(UMessageType::Publish);
    route.physical_source = Some(&topic);
    route.qos = qos;
    let ingress = setup.carrier_ingress(&route).unwrap();
    let runtime = setup.enter_runtime().unwrap();
    for _ in 0..20 {
        clock.advance_ms(60);
        shared.port().readable(shared.route(0));
        let _ = runtime.service();
    }
    assert!(
        runtime.counters(ingress.id()).deadline_missed >= 1,
        "hints without frames must count as silence on a carrier"
    );
}

/// M2. A real admitted frame resets the monitor; a frame taken by this very poll
/// counts as an arrival, not a missed deadline.
#[test]
fn m2_a_frame_taken_by_this_poll_is_an_arrival_not_a_miss() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::lending_on(&memory, clock, LendingDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let qos = Qos::DEFAULT.with_deadline(Duration::from_millis(100));
    let sub = Subscriber::<u32>::on(&setup, topic.clone())
        .wire(wire())
        .qos(qos)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let sub = sub.activate(&runtime).unwrap();
    clock.advance_ms(150);
    shared.queue.lock().unwrap().push_back((
        publish_frame(&topic, uuid_at(WALL, 1), None, None, &7u32.to_le_bytes()),
        peer(7, 1),
    ));
    let got = sub.try_recv().unwrap().expect("the queued frame");
    assert_eq!(*got, 7);
    assert_eq!(
        sub.counters().deadline_missed,
        0,
        "the deadline is judged after the pull: this frame arrived"
    );
}

/// M2. A frame already past its time-to-live on arrival is not an arrival.
#[test]
fn m2_an_already_expired_frame_is_not_an_arrival() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let qos = Qos::DEFAULT.with_deadline(Duration::from_millis(100));
    let sub = Subscriber::<u32>::on(&setup, topic.clone())
        .wire(wire())
        .qos(qos)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let sub = sub.activate(&runtime).unwrap();
    for n in 0..20_u64 {
        clock.advance_ms(60);
        let old = publish_frame(
            &topic,
            uuid_at(WALL - 3_600_000, n),
            Some(Duration::from_secs(1)),
            None,
            &1u32.to_le_bytes(),
        );
        deliver(shared, 0, &old, peer(7, 1)).unwrap();
        let _ = runtime.service();
    }
    assert!(
        sub.counters().deadline_missed >= 1,
        "frames an hour old must not reset the monitor"
    );
}

/// M3. The same-envelope carrier path enforces the egress token limit.
#[test]
fn m3_a_same_envelope_egress_refuses_a_token_it_does_not_allow() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared_in = Shared::leak();
    let shared_out = Shared::leak();
    let memory_in = memory();
    let memory_out = memory();
    let ingress_setup =
        TransportSetup::handoff_on(&memory_in, clock, HandoffDouble(shared_in)).unwrap();
    let egress_setup =
        TransportSetup::handoff_on(&memory_out, clock, HandoffDouble(shared_out)).unwrap();
    let topic = uri(9, 0x8001);
    let wires = carried();
    let mut ingress_route = CarrierRoute::new(&topic, wires, 64);
    ingress_route.kind = Some(UMessageType::Publish);
    ingress_route.max_token = 64;
    let mut egress_route = CarrierRoute::new(&topic, wires, 64);
    egress_route.kind = Some(UMessageType::Publish);
    egress_route.max_token = 0;
    let ingress = ingress_setup.carrier_ingress(&ingress_route).unwrap();
    let egress = egress_setup.carrier_egress(&egress_route).unwrap();
    let ingress_rt = ingress_setup.enter_runtime().unwrap();
    let egress_rt = egress_setup.enter_runtime().unwrap();
    let secret = "SECRET-ACCESS-TOKEN";
    shared_in.queue.lock().unwrap().push_back((
        publish_frame(&topic, uuid_at(WALL, 1), None, Some(secret), &[1, 2, 3, 4]),
        peer(7, 1),
    ));
    let mut result = None;
    ingress_rt
        .receive(ingress, &mut |frame| {
            result = Some(egress_rt.send(egress, &frame));
            Flow::Continue
        })
        .unwrap();
    assert_eq!(
        result.expect("handed"),
        Err(SendError::TokenTooLong),
        "a token must not cross an egress that allows none"
    );
    assert!(shared_out.sent.lock().unwrap().is_empty());
    assert_eq!(egress_rt.counters(egress.id()).refused, 1);
}

/// A heap-backed region source that counts what it hands out.
struct CountingHeap(AtomicUsize);

impl RegionSource for CountingHeap {
    fn region(&'static self, min_len: usize) -> Option<&'static mut [core::mem::MaybeUninit<u8>]> {
        self.0.fetch_add(1, Ordering::SeqCst);
        // Uninitialized and unwritten: no per-byte store for Miri to interpret.
        Some(Box::leak(Box::new_uninit_slice(min_len)))
    }
}

fn counting_memory() -> (StartupMemory, &'static CountingHeap) {
    let source: &'static CountingHeap = Box::leak(Box::new(CountingHeap(AtomicUsize::new(0))));
    (
        StartupMemory::from_source(source, 64 * 1024).unwrap(),
        source,
    )
}

/// M6. No placement succeeds after the last setup entered runtime, and the
/// refused placement asks the heap for nothing.
#[test]
fn m6_no_placement_succeeds_after_the_last_enter_runtime() {
    let (memory, source) = counting_memory();
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
    let _runtime = setup.enter_runtime().unwrap();
    let before = source.0.load(Ordering::SeqCst);
    let refused = memory.bytes(1 << 20);
    let after = source.0.load(Ordering::SeqCst);
    assert_eq!(
        refused.map(|_| ()),
        Err(up_rust_core::memory::StartupError::RegionSealed),
        "placement after runtime starts must be refused"
    );
    assert_eq!(before, after, "the refused path must not request a region");
}

/// M6. A shared memory serves every setup until the last one enters runtime.
#[test]
fn m6_shared_memory_serves_every_setup_until_the_last_enters_runtime() {
    let (memory, _source) = counting_memory();
    let clock = Clock::leak(1_000_000_000, WALL);
    let first = TransportSetup::handoff_on(&memory, clock, HandoffDouble(Shared::leak())).unwrap();
    let second = TransportSetup::handoff_on(&memory, clock, HandoffDouble(Shared::leak())).unwrap();
    let topic = uri(9, 0x8001);
    let sub = Subscriber::<u32>::on(&first, topic.clone())
        .wire(wire())
        .build()
        .unwrap();
    let runtime = first.enter_runtime().unwrap();
    // The other setup has not entered runtime: it is still served.
    let other = Subscriber::<u32>::on(&second, topic)
        .wire(wire())
        .build()
        .expect("a setup that has not entered runtime is still served");
    let second_runtime = second.enter_runtime().unwrap();
    let _ = (
        sub.activate(&runtime).unwrap(),
        other.activate(&second_runtime).unwrap(),
    );
    assert_eq!(
        memory.bytes(64).map(|_| ()),
        Err(up_rust_core::memory::StartupError::RegionSealed),
        "sealed after the last setup entered runtime"
    );
}

/// M6. A setup dropped without entering runtime neither seals the memory nor
/// blocks the seal once runtime has started. The caller's own handle stops at the
/// first `enter_runtime` (N5), for placements and new setups alike; a setup started
/// before then is served through its own handle until it enters runtime.
#[test]
fn m6_a_dropped_setup_only_counts_once_runtime_has_started() {
    let (memory, _source) = counting_memory();
    let clock = Clock::leak(1_000_000_000, WALL);
    let only = TransportSetup::handoff_on(&memory, clock, HandoffDouble(Shared::leak())).unwrap();
    drop(only);
    assert!(
        memory.bytes(64).is_ok(),
        "nothing started yet: a setup dropped during initialization frees nothing"
    );
    let first = TransportSetup::handoff_on(&memory, clock, HandoffDouble(Shared::leak())).unwrap();
    let second = TransportSetup::handoff_on(&memory, clock, HandoffDouble(Shared::leak())).unwrap();
    let _runtime = first.enter_runtime().unwrap();
    assert_eq!(
        memory.bytes(64).map(|_| ()),
        Err(up_rust_core::memory::StartupError::RegionSealed),
        "the caller's handle places nothing once a transport using it entered runtime"
    );
    assert!(
        Subscriber::<u32>::on(&second, uri(9, 0x8001))
            .wire(wire())
            .build()
            .is_ok(),
        "the other setup has not entered runtime and is still served"
    );
    assert!(
        matches!(
            TransportSetup::handoff_on(&memory, clock, HandoffDouble(Shared::leak())),
            Err(StartupError::Memory(
                up_rust_core::memory::StartupError::RegionSealed
            ))
        ),
        "starting a setup is initialization: the caller's handle refuses it once a transport \
         using this memory entered runtime, though another setup is still initializing"
    );
    drop(second);
    assert!(
        matches!(
            TransportSetup::handoff_on(&memory, clock, HandoffDouble(Shared::leak())),
            Err(StartupError::Memory(
                up_rust_core::memory::StartupError::RegionSealed
            ))
        ),
        "runtime started and the last setup is gone: no setup is served any more"
    );
}

/// N5. A setup that is never dropped holds nothing open for the caller: once the
/// only live transport has entered runtime, the caller's handle neither places nor
/// starts a setup, and the refusals ask the heap for nothing. At `a3b8396` a
/// forgotten setup kept the claim alive, and 1 MiB was placed from a new heap region
/// after runtime had started; at `d4d20af` the placement was refused, but every new
/// setup was still served from the memory.
#[test]
fn n5_a_forgotten_setup_keeps_nothing_open_for_the_caller() {
    let (memory, source) = counting_memory();
    let clock = Clock::leak(1_000_000_000, WALL);
    let first = TransportSetup::handoff_on(&memory, clock, HandoffDouble(Shared::leak())).unwrap();
    let second = TransportSetup::handoff_on(&memory, clock, HandoffDouble(Shared::leak())).unwrap();
    core::mem::forget(second);
    let _runtime = first.enter_runtime().unwrap();
    let before = source.0.load(Ordering::SeqCst);
    let placed = memory.bytes(1 << 20).map(|_| ());
    // Enough setups to outgrow the 64 KiB region if they are served.
    let attempts = 256;
    let shared = Shared::leak();
    let served = (0..attempts)
        .filter(|_| TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).is_ok())
        .count();
    let after = source.0.load(Ordering::SeqCst);
    assert_eq!(
        placed,
        Err(up_rust_core::memory::StartupError::RegionSealed),
        "a transport using this memory is in runtime: the caller's handle places nothing"
    );
    assert_eq!(
        served,
        0,
        "and starts no setup: {served} of {attempts} new setups were served, and the heap \
         gave {} more region(s)",
        after - before
    );
    assert_eq!(before, after, "and the refusals ask the heap for nothing");
}

/// L1. `Overflow::Wait` honours the depth on a typed receive route.
#[test]
fn l1_wait_overflow_honours_depth_on_a_typed_route() {
    let run = |overflow: Overflow| {
        let clock = Clock::leak(1_000_000_000, WALL);
        let shared = Shared::leak();
        let memory = memory();
        let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
        let topic = uri(9, 0x8001);
        let qos = Qos::DEFAULT.with_overflow(overflow).with_depth(2);
        let sub = Subscriber::<u32>::on(&setup, topic.clone())
            .wire(wire())
            .qos(qos)
            .max_held(4)
            .build()
            .unwrap();
        let runtime = setup.enter_runtime().unwrap();
        let _sub = sub.activate(&runtime).unwrap();
        let mut accepted = 0;
        for n in 0..10 {
            let frame = publish_frame(&topic, uuid_at(WALL, n), None, None, &1u32.to_le_bytes());
            if deliver(shared, 0, &frame, peer(7, 1)).is_ok() {
                accepted += 1;
            }
        }
        accepted
    };
    assert_eq!(
        run(Overflow::Wait {
            timeout: Duration::from_secs(1)
        }),
        2,
        "a queue at depth refuses the frame for Wait as for DropNewest"
    );
}

/// L2. The reply window is the request's own time-to-live, from its creation.
#[test]
fn l2_a_request_cannot_be_answered_after_its_ttl() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
    let local = Uri(3);
    let method = uri(3, 1);
    let server = RpcServer::<u32, u32>::on(&setup, &local, 1_u16)
        .wire(wire())
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let server = server.activate(&runtime).unwrap();
    let client = uri(100, 0);
    let frame = request_frame(&client, &method, 1, Duration::from_millis(100));
    deliver(shared, 0, &frame, peer(1, 1)).unwrap();
    clock.advance_ms(90);
    let request = server.try_recv().unwrap().expect("still inside its TTL");
    clock.advance_ms(90); // 180 ms after creation: 80 ms past the TTL.
    assert_eq!(
        request.respond(&2),
        Err(SendError::Expired),
        "a 100 ms request cannot be answered at 180 ms"
    );
    assert!(shared.sent.lock().unwrap().is_empty());
}

/// L3. A zero deadline is refused at setup, not read as "not monitored".
#[test]
fn l3_a_zero_deadline_is_refused_at_setup() {
    let qos = Qos::DEFAULT.with_deadline(Duration::ZERO);
    assert!(check_qos(&qos, &all_qos()).is_err());
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::lending_on(&memory, clock, LendingDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let built = Subscriber::<u32>::on(&setup, topic)
        .wire(wire())
        .qos(qos)
        .build();
    assert!(matches!(built, Err(StartupError::QosNotHonored { .. })));
}

/// L6. Concurrent consumers cannot exceed `max_held`. The binding holds both
/// callers inside their pull so both pass any check that precedes taking a lease.
#[test]
fn l6_two_consumers_cannot_exceed_max_held() {
    struct GateShared {
        shared: &'static Shared,
        inside: AtomicUsize,
    }
    struct GatedLending(&'static GateShared);
    struct GatedRunning(&'static GateShared);

    impl BindingSetup for GatedLending {
        type Running = GatedRunning;
        fn name(&self) -> &'static str {
            "gated-lending"
        }
        fn capabilities(&self) -> Capabilities {
            capabilities()
        }
        fn qos_support(&self, _: RouteKind) -> QosSupport {
            all_qos()
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

        type Tx<'a> = NoTx;
        type Rx = HeapFrame;
        fn loan(&self, _: BindingRoute, _: TxFrameLayout) -> Result<NoTx, LoanError> {
            Err(LoanError::InvalidRoute)
        }
        fn publish(
            &self,
            _: BindingRoute,
            _: InitializedTx<NoTx>,
        ) -> Result<SendReport, SendError> {
            Err(SendError::Delivery)
        }
        fn receive(&self, _: BindingRoute) -> Option<HeapFrame> {
            self.0.inside.fetch_add(1, Ordering::SeqCst);
            // Wait, bounded, for the other consumer to arrive here too.
            let started = std::time::Instant::now();
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
                .map(|(bytes, peer)| HeapFrame {
                    bytes,
                    peer,
                    shared: self.0.shared,
                })
        }
    }

    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let gate: &'static GateShared = Box::leak(Box::new(GateShared {
        shared,
        inside: AtomicUsize::new(0),
    }));
    let memory = memory();
    let setup = TransportSetup::lending_on(&memory, clock, GatedLending(gate)).unwrap();
    let topic = uri(9, 0x8001);
    let sub = Subscriber::<u32>::on(&setup, topic.clone())
        .wire(wire())
        .max_held(1)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let sub = sub.activate(&runtime).unwrap();
    for n in 0..2_u64 {
        shared.queue.lock().unwrap().push_back((
            publish_frame(&topic, uuid_at(WALL, n), None, None, &1u32.to_le_bytes()),
            peer(7, 1),
        ));
    }
    let results = std::thread::scope(|scope| {
        let first = scope.spawn(|| sub.try_recv());
        let second = scope.spawn(|| sub.try_recv());
        [first.join().unwrap(), second.join().unwrap()]
    });
    let leases = results
        .iter()
        .filter(|result| matches!(result, Ok(Some(_))))
        .count();
    let refused = results
        .iter()
        .filter(|result| matches!(result, Err(ReceiveError::MaxHeldExceeded)))
        .count();
    assert_eq!(leases, 1, "max_held(1) grants one lease, got {leases}");
    assert_eq!(refused, 1, "the other consumer is refused, not parked");
}
