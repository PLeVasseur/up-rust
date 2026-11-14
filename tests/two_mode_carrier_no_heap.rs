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

//! Proves that a carrier route does not touch the heap in runtime mode (R3).
//!
//! The windows open when `enter_runtime` returns: first traffic in all four
//! directions, steady forwarding, parking, every overflow policy, refusals, TTL
//! expiry, a broken clock and peer death. A window fails on any allocation, free
//! or reallocation, on any thread. Run with
//! `cargo test --test two_mode_carrier_no_heap`.
//!
//! When `UP_TWO_MODE_C_LEVEL` is set, each window also asserts the C allocator
//! counters; the process must then be started under the allocation observer.

#![allow(clippy::indexing_slicing, clippy::wildcard_enum_match_arm)]

use std::alloc::System;
use std::time::Duration;

use up_rust::heap_guard::{c_level, observe, HeapGuard, Scope};
use up_rust::startup::LEAKED_HEAP_REGIONS;
use up_rust::test_util::carrier::{door_frame, ScriptedControl, ScriptedFrame, ScriptedHandoff};
use up_rust::test_util::forwarder::TestForwarder;
use up_rust::test_util::handoff::InMemoryBus;
use up_rust::test_util::lending::{LendingBus, LendingOptions};
use up_rust::test_util::wires::{DoorState, DoorWire, LittleDoorWire};
use up_rust::test_util::TestClock;
use up_rust::two_mode::*;
use up_rust::{PayloadEncoding, StaticUriProvider, UEntityId, UUri};

#[global_allocator]
static HEAP: HeapGuard = HeapGuard::new(System);

const ROUNDS: u32 = 1000;

fn entity(authority: &str, type_id: u16) -> StaticUriProvider {
    StaticUriProvider::for_entity(authority, UEntityId::first_instance(type_id), 1).expect("uri")
}

fn memory() -> StartupMemory {
    StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 256 * 1024).expect("memory")
}

fn handoff(bus: InMemoryBus, peer: u64) -> TransportSetup {
    TransportSetup::handoff_in(&memory(), bus.participant(peer)).expect("setup")
}

fn lending(bus: LendingBus, peer: u64) -> TransportSetup {
    TransportSetup::lending_in(&memory(), bus.participant(peer)).expect("setup")
}

fn lending_with(bus: LendingBus, peer: u64, options: LendingOptions) -> TransportSetup {
    TransportSetup::lending_in(&memory(), bus.participant(peer).with_options(options))
        .expect("setup")
}

fn topic() -> UUri {
    entity("door", 1).get_resource_uri(0x8001)
}

fn door(id: u32, open: u8) -> DoorState {
    DoorState { id, open }
}

fn door_wires() -> [CarriedWire; 1] {
    [CarriedWire::of(&Wire::<DoorState>::describe(&DoorWire))]
}

fn route<'a>(filter: &'a UUri, carried: &'a [CarriedWire], qos: Qos) -> CarrierRoute<'a> {
    CarrierRoute {
        qos,
        slots: 4,
        ..CarrierRoute::new(filter, carried, 64)
    }
}

fn scripted(envelope: Envelope) -> (TransportSetup, ScriptedControl) {
    let (binding, control) = ScriptedHandoff::new(envelope);
    (
        TransportSetup::handoff_in(&memory(), binding).expect("setup"),
        control,
    )
}

fn scripted_on(clock: &'static TestClock) -> (TransportSetup, ScriptedControl) {
    let (binding, control) = ScriptedHandoff::new(Envelope::UFrameFields);
    (
        TransportSetup::handoff_on(&memory(), clock, binding).expect("setup"),
        control,
    )
}

/// The publisher upstream of a carrier and the subscriber downstream of it.
struct Rig {
    publisher: Publisher<DoorState>,
    subscriber: Subscriber<DoorState>,
    ingress: IngressRoute,
    egress: EgressRoute,
    in_rt: TransportRuntime,
    out_rt: TransportRuntime,
}

impl Rig {
    fn forwarder(&self) -> TestForwarder {
        TestForwarder::new(self.in_rt, self.ingress, self.out_rt, self.egress)
    }
}

/// Builds a carrier between two transports. `upstream` and `downstream` are the
/// end participants on the same buses as the ingress and egress legs.
fn rig(
    upstream: TransportSetup,
    carrier_in: TransportSetup,
    carrier_out: TransportSetup,
    downstream: TransportSetup,
    qos: Qos,
) -> Rig {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried, qos);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .slots(8)
        .build()
        .expect("publisher");
    let ingress = carrier_in.carrier_ingress(&carrier).expect("ingress");
    let egress = carrier_out.carrier_egress(&carrier).expect("egress");
    let subscriber = Subscriber::<DoorState>::on(&downstream, uri.clone())
        .wire(DoorWire)
        .qos(qos)
        .build()
        .expect("subscriber");
    let up = upstream.enter_runtime().expect("runtime");
    let in_rt = carrier_in.enter_runtime().expect("runtime");
    let out_rt = carrier_out.enter_runtime().expect("runtime");
    let down = downstream.enter_runtime().expect("runtime");
    Rig {
        publisher: publisher.activate(&up).expect("activate"),
        subscriber: subscriber.activate(&down).expect("activate"),
        ingress,
        egress,
        in_rt,
        out_rt,
    }
}

fn handoff_rig(qos: Qos) -> Rig {
    let (a, b) = (InMemoryBus::new(16, 4096), InMemoryBus::new(16, 4096));
    rig(
        handoff(a, 1),
        handoff(a, 2),
        handoff(b, 3),
        handoff(b, 4),
        qos,
    )
}

fn handoff_to_lending_rig(qos: Qos) -> Rig {
    let a = InMemoryBus::new(16, 4096);
    let b = LendingBus::new();
    rig(
        handoff(a, 1),
        handoff(a, 2),
        lending(b, 3),
        lending(b, 4),
        qos,
    )
}

fn lending_to_handoff_rig(qos: Qos) -> Rig {
    let a = LendingBus::new();
    let b = InMemoryBus::new(16, 4096);
    rig(
        lending(a, 1),
        lending(a, 2),
        handoff(b, 3),
        handoff(b, 4),
        qos,
    )
}

fn lending_rig(qos: Qos) -> Rig {
    let (a, b) = (LendingBus::new(), LendingBus::new());
    rig(
        lending(a, 1),
        lending(a, 2),
        lending(b, 3),
        lending(b, 4),
        qos,
    )
}

/// The bus under a carrier leg, for the windows that build one from scratch.
#[derive(Clone, Copy)]
enum Bus {
    Handoff(InMemoryBus),
    Lending(LendingBus),
}

impl Bus {
    fn setup(self, peer: u64) -> TransportSetup {
        match self {
            Self::Handoff(bus) => handoff(bus, peer),
            Self::Lending(bus) => lending(bus, peer),
        }
    }
}

struct Results {
    lines: Vec<String>,
    c_level: bool,
}

impl Results {
    /// Runs one window. Every subject must be clean: nothing acquired or released.
    fn window<T>(&mut self, subject: &str, operation: impl FnOnce() -> T) -> T {
        if self.c_level {
            let (value, report) = c_level::observe(operation);
            let rust = report.rust().counts();
            let c = report.c();
            self.lines.push(format!(
                "window={subject} rust_allocations={} rust_deallocations={} \
                 rust_reallocations={} c={c:?} clean={}",
                rust.allocations,
                rust.deallocations,
                rust.reallocations,
                report.is_clean()
            ));
            assert!(report.is_clean(), "{subject}: {report}");
            value
        } else {
            let (value, report) = observe(Scope::AllThreads, operation);
            let counts = report.counts();
            self.lines.push(format!(
                "window={subject} allocations={} deallocations={} reallocations={} clean={}",
                counts.allocations,
                counts.deallocations,
                counts.reallocations,
                counts.is_clean()
            ));
            assert!(counts.is_clean(), "{subject}: {report}");
            value
        }
    }

    fn note(&mut self, line: String) {
        self.lines.push(line);
    }
}

/// Windows that must see the heap, so clean windows mean something.
fn controls(results: &mut Results) {
    // A sink that allocates.
    let (setup, control) = scripted(Envelope::UFrameFields);
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried, Qos::DEFAULT);
    let ingress = setup.carrier_ingress(&carrier).expect("ingress");
    let runtime = setup.enter_runtime().expect("runtime");
    let (metadata, payload) = door_frame(Envelope::UFrameFields, &uri, 1, None);
    control.script([ScriptedFrame::contiguous(metadata, payload)]);
    let ((), report) = observe(Scope::AllThreads, || {
        runtime
            .receive(ingress, &mut |frame: CarriedFrame<'_>| {
                let staged = frame.metadata().to_vec();
                assert!(!staged.is_empty());
                Flow::Continue
            })
            .expect("receive");
    });
    assert!(
        report.counts().allocations >= 1 && report.counts().deallocations >= 1,
        "negative control: the guard must see the staged Vec: {report}"
    );
    results.note(format!(
        "control=allocating-sink allocations={} deallocations={} (expected nonzero)",
        report.counts().allocations,
        report.counts().deallocations
    ));

    // A binding that locks a never-primed mutex in its first send. On pthread
    // targets the first lock allocates, and the first-traffic window must see it.
    let bus = LendingBus::new();
    let upstream = lending(bus, 1);
    let carrier_in = lending(bus, 2);
    let lazy = LendingBus::new();
    let carrier_out = lending_with(
        lazy,
        3,
        LendingOptions {
            lazy_mutex: true,
            ..LendingOptions::default()
        },
    );
    let downstream = lending(lazy, 4);
    let carried = door_wires();
    let carrier = route(&uri, &carried, Qos::DEFAULT);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .expect("publisher");
    let ingress = carrier_in.carrier_ingress(&carrier).expect("ingress");
    let egress = carrier_out.carrier_egress(&carrier).expect("egress");
    let subscriber = Subscriber::<DoorState>::on(&downstream, uri.clone())
        .wire(DoorWire)
        .build()
        .expect("subscriber");
    let up = upstream.enter_runtime().expect("runtime");
    let in_rt = carrier_in.enter_runtime().expect("runtime");
    let out_rt = carrier_out.enter_runtime().expect("runtime");
    let down = downstream.enter_runtime().expect("runtime");
    let publisher = publisher.activate(&up).expect("activate");
    let _subscriber = subscriber.activate(&down).expect("activate");
    publisher.publish(&door(1, 1)).expect("publish");
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    let ((), report) = observe(Scope::AllThreads, || {
        forwarder.forward_once().expect("forward");
    });
    let saw_heap = !report.counts().is_clean();
    results.note(format!(
        "control=lazy-mutex-first-forward allocations={} saw_heap={saw_heap} expected=true",
        report.counts().allocations
    ));
    assert!(
        saw_heap,
        "positive control: a binding that allocates on first traffic must fail this \
         window, on every target: {report}"
    );
}

/// Runs first traffic and steady forwarding on one rig, end to end.
fn forwarding(results: &mut Results, label: &str, rig: &Rig) {
    let mut forwarder = rig.forwarder();
    results.window(&format!("{label}_first_forward"), || {
        rig.publisher.publish(&door(1, 1)).expect("publish");
        assert_eq!(forwarder.forward_once().expect("forward"), 1);
        let received = rig.subscriber.try_recv().expect("recv").expect("a frame");
        assert_eq!(received.id, 1);
    });
    results.window(&format!("{label}_forward_{ROUNDS}"), || {
        for id in 0..ROUNDS {
            rig.publisher.publish(&door(id, 1)).expect("publish");
            assert_eq!(forwarder.forward_once().expect("forward"), 1);
            let received = rig.subscriber.try_recv().expect("recv").expect("a frame");
            assert_eq!(received.id, id);
        }
        let _ = rig.in_rt.counters(rig.ingress.id());
        let _ = rig.out_rt.counters(rig.egress.id());
        let _ = rig.in_rt.routes();
        let _ = rig.out_rt.waivers();
    });
    results.window(&format!("{label}_has_capacity_and_empty_receive"), || {
        for _ in 0..ROUNDS {
            assert!(rig.out_rt.has_capacity(rig.egress));
            assert_eq!(
                rig.in_rt
                    .receive(rig.ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
                    .expect("receive"),
                0
            );
        }
    });
}

/// From `enter_runtime` returning through activation and the first frame.
fn activation_to_first_frame(results: &mut Results) {
    for (name, ingress_bus, egress_bus) in [
        (
            "activate_and_first_forward_handoff",
            Bus::Handoff(InMemoryBus::new(8, 4096)),
            Bus::Handoff(InMemoryBus::new(8, 4096)),
        ),
        (
            "activate_and_first_forward_lending",
            Bus::Lending(LendingBus::new()),
            Bus::Lending(LendingBus::new()),
        ),
    ] {
        let uri = topic();
        let carried = door_wires();
        let carrier = route(&uri, &carried, Qos::DEFAULT);
        let upstream = ingress_bus.setup(1);
        let carrier_in = ingress_bus.setup(2);
        let carrier_out = egress_bus.setup(3);
        let downstream = egress_bus.setup(4);
        let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
            .wire(DoorWire)
            .build()
            .expect("publisher");
        let ingress = carrier_in.carrier_ingress(&carrier).expect("ingress");
        let egress = carrier_out.carrier_egress(&carrier).expect("egress");
        let subscriber = Subscriber::<DoorState>::on(&downstream, uri.clone())
            .wire(DoorWire)
            .build()
            .expect("subscriber");
        let up = upstream.enter_runtime().expect("runtime");
        let in_rt = carrier_in.enter_runtime().expect("runtime");
        let out_rt = carrier_out.enter_runtime().expect("runtime");
        let down = downstream.enter_runtime().expect("runtime");
        results.window(name, || {
            let publisher = publisher.activate(&up).expect("activate");
            let subscriber = subscriber.activate(&down).expect("activate");
            let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
            assert!(out_rt.has_capacity(egress));
            publisher.publish(&door(1, 1)).expect("publish");
            assert_eq!(forwarder.forward_once().expect("forward"), 1);
            let received = subscriber.try_recv().expect("recv").expect("a frame");
            assert_eq!(received.id, 1);
        });
    }
}

fn directions(results: &mut Results) {
    forwarding(results, "handoff_to_handoff", &handoff_rig(Qos::DEFAULT));
    forwarding(
        results,
        "handoff_to_lending",
        &handoff_to_lending_rig(Qos::DEFAULT),
    );
    forwarding(
        results,
        "lending_to_handoff",
        &lending_to_handoff_rig(Qos::DEFAULT),
    );
    forwarding(results, "lending_to_lending", &lending_rig(Qos::DEFAULT));
}

/// A frame with a real chunked payload and a re-encoded head, in every window.
fn envelopes(results: &mut Results) {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried, Qos::DEFAULT);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let (egress_setup, egress_control) = scripted(Envelope::NativeWholeFrame);
    let ingress = ingress_setup.carrier_ingress(&carrier).expect("ingress");
    let egress = egress_setup.carrier_egress(&carrier).expect("egress");
    let in_rt = ingress_setup.enter_runtime().expect("runtime");
    let out_rt = egress_setup.enter_runtime().expect("runtime");
    let (metadata, payload) = door_frame(Envelope::UFrameFields, &uri, 7, None);
    ingress_control
        .script((0..200).map(|_| ScriptedFrame::chunked(metadata.clone(), payload.clone(), 2)));
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    results.window("different_envelope_reencodes_metadata_only_200", || {
        let mut total = 0;
        while total < 200 {
            total += forwarder.forward_once().expect("forward");
        }
    });
    assert_eq!(egress_control.sends(), 200);
    assert_eq!(egress_control.last_chunks(), 2);
}

fn parking(results: &mut Results) {
    let rig = lending_rig(Qos::DEFAULT.with_depth(8));
    results.window("park_resend_drop_500", || {
        for id in 0..500_u32 {
            rig.publisher.publish(&door(id, 0)).expect("publish");
            let mut parked = None;
            assert_eq!(
                rig.in_rt
                    .receive(rig.ingress, &mut |frame: CarriedFrame<'_>| {
                        parked = Some(frame.park().expect("parked"));
                        Flow::Stop
                    })
                    .expect("receive"),
                1
            );
            let parked = parked.expect("a frame was parked");
            rig.out_rt.send(rig.egress, &parked.frame()).expect("sent");
            drop(parked);
            assert_eq!(
                rig.subscriber.try_recv().expect("recv").expect("frame").id,
                id
            );
        }
    });

    // A budget of one: parking holds it, the next take finds nothing to do.
    let budget = route_rig_budget();
    results.window("a_parked_frame_holds_the_retained_budget", || {
        for id in 0..100_u32 {
            budget.publisher.publish(&door(id, 0)).expect("publish");
            budget.publisher.publish(&door(id + 1, 0)).expect("publish");
            let mut parked = None;
            let handed = budget
                .in_rt
                .receive(budget.ingress, &mut |frame: CarriedFrame<'_>| {
                    parked = Some(frame.park().expect("parked"));
                    Flow::Continue
                })
                .expect("receive");
            assert_eq!(handed, 1);
            assert_eq!(
                budget
                    .in_rt
                    .receive(budget.ingress, &mut |_frame: CarriedFrame<'_>| {
                        Flow::Continue
                    })
                    .expect("receive"),
                0
            );
            drop(parked);
            assert_eq!(
                budget
                    .in_rt
                    .receive(budget.ingress, &mut |_frame: CarriedFrame<'_>| {
                        Flow::Continue
                    })
                    .expect("receive"),
                1
            );
        }
    });

    // An egress that refuses one send: the frame parks and goes out on the retry.
    let bus = LendingBus::new();
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried, Qos::DEFAULT);
    let upstream = lending(bus, 1);
    let carrier_in = lending(bus, 2);
    let (egress_setup, egress_control) = scripted(Envelope::UFrameFields);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .slots(8)
        .build()
        .expect("publisher");
    let ingress = carrier_in.carrier_ingress(&carrier).expect("ingress");
    let egress = egress_setup.carrier_egress(&carrier).expect("egress");
    let up = upstream.enter_runtime().expect("runtime");
    let in_rt = carrier_in.enter_runtime().expect("runtime");
    let out_rt = egress_setup.enter_runtime().expect("runtime");
    let publisher = publisher.activate(&up).expect("activate");
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    results.window("egress_busy_parks_the_frame_and_retries_200", || {
        for id in 0..200_u32 {
            publisher.publish(&door(id, 0)).expect("publish");
            egress_control.refuse_sends(1);
            assert_eq!(forwarder.forward_once().expect("forward"), 0);
            assert!(forwarder.has_parked());
            assert_eq!(forwarder.forward_once().expect("forward"), 1);
            assert!(!forwarder.has_parked());
        }
    });
    assert_eq!(forwarder.stats().dropped, 0);
    assert_eq!(forwarder.stats().forwarded, 200);
}

fn route_rig_budget() -> Rig {
    let uri = topic();
    let carried = door_wires();
    let carrier = CarrierRoute {
        qos: Qos::DEFAULT.with_depth(8),
        slots: 1,
        ..CarrierRoute::new(&uri, &carried, 64)
    };
    let (a, b) = (LendingBus::new(), LendingBus::new());
    let upstream = lending(a, 1);
    let carrier_in = lending(a, 2);
    let carrier_out = lending(b, 3);
    let downstream = lending(b, 4);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .slots(8)
        .build()
        .expect("publisher");
    let ingress = carrier_in.carrier_ingress(&carrier).expect("ingress");
    let egress = carrier_out.carrier_egress(&carrier).expect("egress");
    let subscriber = Subscriber::<DoorState>::on(&downstream, uri.clone())
        .wire(DoorWire)
        .qos(Qos::DEFAULT.with_depth(8))
        .build()
        .expect("subscriber");
    let up = upstream.enter_runtime().expect("runtime");
    let in_rt = carrier_in.enter_runtime().expect("runtime");
    let out_rt = carrier_out.enter_runtime().expect("runtime");
    let down = downstream.enter_runtime().expect("runtime");
    Rig {
        publisher: publisher.activate(&up).expect("activate"),
        subscriber: subscriber.activate(&down).expect("activate"),
        ingress,
        egress,
        in_rt,
        out_rt,
    }
}

fn overflow(results: &mut Results) {
    let depth2 = |overflow: Overflow| Qos::DEFAULT.with_depth(2).with_overflow(overflow);

    // A receiver's full queue under DropNewest and DropOldest, at the send.
    let rig_newest = lending_rig(depth2(Overflow::DropNewest));
    let mut forwarder = rig_newest.forwarder();
    results.window("lending_egress_drop_newest_500", || {
        for id in 0..500_u32 {
            rig_newest.publisher.publish(&door(id, 0)).expect("publish");
            assert_eq!(forwarder.forward_once().expect("forward"), 1);
            if id % 4 == 0 {
                while rig_newest.subscriber.try_recv().expect("recv").is_some() {}
            }
        }
    });
    let rig_oldest = lending_rig(depth2(Overflow::DropOldest));
    let mut forwarder = rig_oldest.forwarder();
    results.window("lending_egress_drop_oldest_500", || {
        for id in 0..500_u32 {
            rig_oldest.publisher.publish(&door(id, 0)).expect("publish");
            assert_eq!(forwarder.forward_once().expect("forward"), 1);
        }
    });
    let rig_handoff = handoff_rig(depth2(Overflow::DropNewest));
    let mut forwarder = rig_handoff.forwarder();
    results.window("handoff_inbox_full_500", || {
        for id in 0..500_u32 {
            rig_handoff
                .publisher
                .publish(&door(id, 0))
                .expect("publish");
            let _ = forwarder.forward_once().expect("forward");
            if id % 8 == 0 {
                while rig_handoff.subscriber.try_recv().expect("recv").is_some() {}
            }
        }
    });

    // Pushed retained frames, on a full inbox, both policies.
    for (name, overflow) in [
        ("pushed_frames_drop_oldest_500", Overflow::DropOldest),
        ("pushed_frames_drop_newest_500", Overflow::DropNewest),
    ] {
        let options = LendingOptions {
            push: true,
            ..LendingOptions::default()
        };
        let bus = LendingBus::new();
        let uri = topic();
        let carried = door_wires();
        let carrier = route(&uri, &carried, depth2(overflow));
        let upstream = lending_with(bus, 1, options);
        let carrier_in = lending_with(bus, 2, options);
        let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
            .wire(DoorWire)
            .slots(8)
            .build()
            .expect("publisher");
        let ingress = carrier_in.carrier_ingress(&carrier).expect("ingress");
        let up = upstream.enter_runtime().expect("runtime");
        let in_rt = carrier_in.enter_runtime().expect("runtime");
        let publisher = publisher.activate(&up).expect("activate");
        results.window(name, || {
            for id in 0..500_u32 {
                publisher.publish(&door(id, 0)).expect("publish");
                publisher.publish(&door(id, 1)).expect("publish");
                publisher.publish(&door(id, 2)).expect("publish");
                in_rt
                    .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
                    .expect("receive");
            }
        });
    }

    // Wait on an ingress: the upstream sender waits, bounded, and the worker never
    // does. A full inbox with a Wait route times out the upstream sender.
    let bus = LendingBus::new();
    let uri = topic();
    let carried = door_wires();
    let wait = Qos::DEFAULT.with_depth(1).with_overflow(Overflow::Wait {
        timeout: Duration::from_millis(1),
    });
    let carrier = route(&uri, &carried, wait);
    let upstream = lending(bus, 1);
    let carrier_in = lending(bus, 2);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .expect("publisher");
    let _ingress = carrier_in.carrier_ingress(&carrier).expect("ingress");
    let up = upstream.enter_runtime().expect("runtime");
    let _in_rt = carrier_in.enter_runtime().expect("runtime");
    let publisher = publisher.activate(&up).expect("activate");
    publisher.publish(&door(1, 0)).expect("publish");
    results.window("ingress_wait_times_out_the_upstream_sender", || {
        assert!(matches!(
            publisher.publish(&door(2, 0)),
            Err(SendError::TimedOut)
        ));
    });
}

fn refusals(results: &mut Results) {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried, Qos::DEFAULT);
    let (setup, control) = scripted(Envelope::UFrameFields);
    let ingress = setup.carrier_ingress(&carrier).expect("ingress");
    let rt = setup.enter_runtime().expect("runtime");
    let (other_setup, other_control) = scripted(Envelope::UFrameFields);
    let other_ingress = other_setup.carrier_ingress(&carrier).expect("ingress");
    let other_egress = other_setup.carrier_egress(&carrier).expect("egress");
    let other_rt = other_setup.enter_runtime().expect("runtime");

    // Frames refused at admission: outside the filter, foreign wire, malformed.
    let elsewhere = entity("elsewhere", 7).get_resource_uri(0x8001);
    let (metadata, payload) = door_frame(Envelope::UFrameFields, &elsewhere, 1, None);
    let out_of_filter = ScriptedFrame::contiguous(metadata, payload);
    let foreign_wire = wire_frame(&uri);
    let malformed = ScriptedFrame::contiguous(vec![1, 2, 3], vec![4]);
    let mut stale_first = scripted_door(&uri, 1);
    stale_first.peer = PeerRef {
        peer: 9,
        incarnation: 2,
    };
    let mut stale_old = scripted_door(&uri, 2);
    stale_old.peer = PeerRef {
        peer: 9,
        incarnation: 1,
    };
    control.script((0..40).flat_map(|_| {
        [
            out_of_filter,
            foreign_wire,
            malformed,
            stale_first,
            stale_old,
        ]
    }));
    results.window("admission_refusals_200", || {
        for _ in 0..200 {
            rt.receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
                .expect("receive");
        }
    });
    let counters = rt.counters(ingress.id());
    assert_eq!(counters.refused, 120, "{counters:?}");
    assert_eq!(counters.stale_peer, 40, "{counters:?}");
    assert_eq!(counters.carried, 40, "{counters:?}");

    // Route tokens used on the wrong transport, never a panic.
    results.window("wrong_transport_tokens_are_refused", || {
        for _ in 0..100 {
            assert!(!rt.has_capacity(other_egress));
            assert!(matches!(
                rt.receive(other_ingress, &mut |_frame: CarriedFrame<'_>| {
                    Flow::Continue
                }),
                Err(ReceiveError::UnknownRoute)
            ));
        }
    });
    let (metadata, payload) = door_frame(Envelope::UFrameFields, &uri, 1, None);
    other_control
        .script((0..100).map(|_| ScriptedFrame::contiguous(metadata.clone(), payload.clone())));
    results.window("a_send_on_a_foreign_route_is_refused_100", || {
        for _ in 0..100 {
            other_rt
                .receive(other_ingress, &mut |frame: CarriedFrame<'_>| {
                    assert!(matches!(
                        rt.send(other_egress, &frame),
                        Err(SendError::Rejected { .. })
                    ));
                    Flow::Continue
                })
                .expect("receive");
        }
    });

    // A borrowed delivery on a carrier ingress is refused, and a borrowed egress
    // refusal on a handoff ingress cannot park.
    let (metadata, payload) = door_frame(Envelope::UFrameFields, &uri, 1, None);
    results.window("borrowed_delivery_is_refused_100", || {
        for _ in 0..100 {
            assert!(control.deliver_borrowed(&metadata, &payload).is_err());
        }
    });
    let (setup_in, control_in) = scripted(Envelope::UFrameFields);
    let (setup_out, control_out) = scripted(Envelope::UFrameFields);
    let handoff_in = setup_in.carrier_ingress(&carrier).expect("ingress");
    let handoff_out = setup_out.carrier_egress(&carrier).expect("egress");
    let in_rt = setup_in.enter_runtime().expect("runtime");
    let out_rt = setup_out.enter_runtime().expect("runtime");
    control_in
        .script((0..100).map(|_| ScriptedFrame::contiguous(metadata.clone(), payload.clone())));
    let mut forwarder = TestForwarder::new(in_rt, handoff_in, out_rt, handoff_out);
    results.window("egress_busy_with_a_handoff_ingress_drops_100", || {
        for _ in 0..100 {
            control_out.refuse_sends(1);
            let _ = forwarder.forward_once().expect("forward");
        }
    });
    assert_eq!(forwarder.stats().dropped, 100);
}

fn wire_frame(source: &UUri) -> ScriptedFrame {
    // A door frame of the wrong wire: the little-door wire is not in the closed set.
    let wire = Wire::<DoorState>::describe(&LittleDoorWire);
    let (metadata, payload) =
        up_rust::test_util::carrier::frame_of(Envelope::UFrameFields, &wire, source, 1, None);
    ScriptedFrame::contiguous(metadata, payload)
}

fn scripted_door(source: &UUri, door_id: u32) -> ScriptedFrame {
    let (metadata, payload) = door_frame(Envelope::UFrameFields, source, door_id, None);
    ScriptedFrame::contiguous(metadata, payload)
}

fn ttl_and_clock(results: &mut Results) {
    let clock = TestClock::leak(1_000, u64::MAX / 4);
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried, Qos::DEFAULT);
    let (setup, control) = scripted_on(clock);
    let ingress = setup.carrier_ingress(&carrier).expect("ingress");
    let rt = setup.enter_runtime().expect("runtime");
    let (metadata, payload) = door_frame(Envelope::UFrameFields, &uri, 1, Some(1));
    control.script((0..500).map(|_| ScriptedFrame::contiguous(metadata.clone(), payload.clone())));
    results.window("ttl_expired_frames_are_not_forwarded_500", || {
        for _ in 0..500 {
            assert_eq!(
                rt.receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
                    .expect("receive"),
                0
            );
        }
    });
    assert_eq!(rt.counters(ingress.id()).expired, 500);

    let clock = TestClock::leak(1_000, 1_000_000);
    let (setup, control) = scripted_on(clock);
    let ingress = setup.carrier_ingress(&carrier).expect("ingress");
    let rt = setup.enter_runtime().expect("runtime");
    let (metadata, payload) = door_frame(Envelope::UFrameFields, &uri, 1, Some(3_600_000));
    control.script((0..500).map(|_| ScriptedFrame::contiguous(metadata.clone(), payload.clone())));
    clock.break_wall_clock();
    results.window("broken_clock_refuses_frames_with_a_ttl_500", || {
        for _ in 0..500 {
            assert_eq!(
                rt.receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
                    .expect("receive"),
                0
            );
        }
    });
    assert_eq!(rt.counters(ingress.id()).clock_unavailable, 500);

    // A parked frame that runs out of time while parked, and a broken clock then.
    let clock = TestClock::leak(1_000, 1_000_000);
    let bus = LendingBus::new();
    let carrier_ttl = route(&uri, &carried, Qos::DEFAULT);
    let upstream = TransportSetup::lending_on(&memory(), clock, bus.participant(1)).expect("setup");
    let carrier_in =
        TransportSetup::lending_on(&memory(), clock, bus.participant(2)).expect("setup");
    let (egress_setup, _egress_control) = scripted(Envelope::UFrameFields);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .ttl(Duration::from_millis(50))
        .slots(8)
        .build()
        .expect("publisher");
    let ingress = carrier_in.carrier_ingress(&carrier_ttl).expect("ingress");
    let egress = egress_setup.carrier_egress(&carrier_ttl).expect("egress");
    let up = upstream.enter_runtime().expect("runtime");
    let in_rt = carrier_in.enter_runtime().expect("runtime");
    let out_rt = egress_setup.enter_runtime().expect("runtime");
    let publisher = publisher.activate(&up).expect("activate");
    results.window("parked_frame_expires_while_parked_100", || {
        for id in 0..100_u32 {
            publisher.publish(&door(id, 0)).expect("publish");
            let mut parked = None;
            in_rt
                .receive(ingress, &mut |frame: CarriedFrame<'_>| {
                    parked = Some(frame.park().expect("parked"));
                    Flow::Stop
                })
                .expect("receive");
            let parked = parked.expect("a frame was parked");
            clock.advance_wall_ms(60);
            assert!(matches!(
                out_rt.send(egress, &parked.frame()),
                Err(SendError::Expired)
            ));
            drop(parked);
        }
    });
    assert_eq!(in_rt.counters(ingress.id()).expired, 100);
    clock.break_wall_clock();
    publisher.publish(&door(1, 0)).ok();
    // The frame published above was admitted before the clock broke; the check
    // applies at the send of a parked frame and at the next receive.
    let clock2 = TestClock::leak(1_000, 1_000_000);
    let bus2 = LendingBus::new();
    let upstream2 =
        TransportSetup::lending_on(&memory(), clock2, bus2.participant(1)).expect("setup");
    let carrier_in2 =
        TransportSetup::lending_on(&memory(), clock2, bus2.participant(2)).expect("setup");
    let (egress_setup2, _control2) = scripted(Envelope::UFrameFields);
    let publisher2 = Publisher::<DoorState>::on(&upstream2, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .ttl(Duration::from_secs(60))
        .slots(8)
        .build()
        .expect("publisher");
    let ingress2 = carrier_in2.carrier_ingress(&carrier_ttl).expect("ingress");
    let egress2 = egress_setup2.carrier_egress(&carrier_ttl).expect("egress");
    let up2 = upstream2.enter_runtime().expect("runtime");
    let in_rt2 = carrier_in2.enter_runtime().expect("runtime");
    let out_rt2 = egress_setup2.enter_runtime().expect("runtime");
    let publisher2 = publisher2.activate(&up2).expect("activate");
    publisher2.publish(&door(1, 0)).expect("publish");
    let mut parked = None;
    in_rt2
        .receive(ingress2, &mut |frame: CarriedFrame<'_>| {
            parked = Some(frame.park().expect("parked"));
            Flow::Stop
        })
        .expect("receive");
    let parked = parked.expect("a frame was parked");
    clock2.break_wall_clock();
    results.window("parked_frame_with_a_broken_clock_is_refused", || {
        assert!(matches!(
            out_rt2.send(egress2, &parked.frame()),
            Err(SendError::ClockUnavailable)
        ));
    });
    drop(parked);
}

fn peers(results: &mut Results) {
    // A peer killed while a frame is in flight, then restarted, in the window.
    let (a, b) = (InMemoryBus::new(16, 4096), InMemoryBus::new(16, 4096));
    let rig = rig(
        handoff(a, 1),
        handoff(a, 2),
        handoff(b, 3),
        handoff(b, 4),
        Qos::DEFAULT,
    );
    let mut forwarder = rig.forwarder();
    results.window("peer_killed_and_restarted_200", || {
        for id in 0..200_u32 {
            rig.publisher.publish(&door(id, 0)).expect("publish");
            if id % 3 == 0 {
                a.kill(1);
                let _ = forwarder.forward_once().expect("forward");
                a.restart(1);
            } else {
                let _ = forwarder.forward_once().expect("forward");
            }
            let _ = rig.subscriber.try_recv().expect("recv");
        }
    });

    // The ingress transport's own sender dies: nothing arrives, nothing leaks.
    let (a, b) = (LendingBus::new(), LendingBus::new());
    let rig = rig_lending_peers(a, b);
    let mut forwarder = rig.forwarder();
    results.window("lending_peer_death_and_restart_200", || {
        for id in 0..200_u32 {
            rig.publisher.publish(&door(id, 0)).expect("publish");
            if id % 5 == 0 {
                a.kill(1);
                let _ = forwarder.forward_once().expect("forward");
                a.restart(1);
            } else {
                let _ = forwarder.forward_once().expect("forward");
            }
            let _ = rig.subscriber.try_recv().expect("recv");
        }
    });

    // A restarted peer's stale frame on a lending carrier, inside the window.
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried, Qos::DEFAULT);
    let (setup, control) = scripted(Envelope::UFrameFields);
    let ingress = setup.carrier_ingress(&carrier).expect("ingress");
    let rt = setup.enter_runtime().expect("runtime");
    let mut current = scripted_door(&uri, 1);
    current.peer = PeerRef {
        peer: 42,
        incarnation: 3,
    };
    let mut stale = scripted_door(&uri, 2);
    stale.peer = PeerRef {
        peer: 42,
        incarnation: 2,
    };
    control.script((0..100).flat_map(|_| [current, stale]));
    results.window("restarted_peer_stale_frame_is_refused_200", || {
        for _ in 0..200 {
            rt.receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
                .expect("receive");
        }
    });
    assert_eq!(rt.counters(ingress.id()).stale_peer, 100);
}

fn rig_lending_peers(a: LendingBus, b: LendingBus) -> Rig {
    rig(
        lending(a, 1),
        lending(a, 2),
        lending(b, 3),
        lending(b, 4),
        Qos::DEFAULT,
    )
}

fn waiting(results: &mut Results) {
    static WAKE: Signal = Signal::new();
    let (a, b) = (LendingBus::new(), LendingBus::new());
    let upstream = lending(a, 1);
    let carrier_in = lending(a, 2);
    let carrier_out = lending(b, 3);
    let downstream = lending(b, 4);
    carrier_in.signal_on(&WAKE);
    carrier_out.signal_on(&WAKE);
    let rig = rig(upstream, carrier_in, carrier_out, downstream, Qos::DEFAULT);
    results.window("wait_on_a_shared_signal_times_out", || {
        let token = WAKE.token();
        assert_eq!(
            WAKE.wait(token, Duration::from_millis(1)),
            SignalWait::TimedOut
        );
        let token = rig.in_rt.wait_token();
        let _ = rig.in_rt.wait(token, Duration::from_millis(1));
        let _ = rig.in_rt.service();
        let _ = rig.out_rt.service();
    });
}

fn main() {
    let mut results = Results {
        lines: Vec::with_capacity(64),
        c_level: std::env::var_os("UP_TWO_MODE_C_LEVEL").is_some(),
    };
    controls(&mut results);
    activation_to_first_frame(&mut results);
    directions(&mut results);
    envelopes(&mut results);
    parking(&mut results);
    overflow(&mut results);
    refusals(&mut results);
    ttl_and_clock(&mut results);
    peers(&mut results);
    waiting(&mut results);
    for line in &results.lines {
        println!("{line}");
    }
    let windows = results
        .lines
        .iter()
        .filter(|line| line.starts_with("window="))
        .count();
    println!("two_mode_carrier_no_heap: all {windows} windows clean, plus the controls");
    let _ = PayloadEncoding::private_use(0xF001);
}
