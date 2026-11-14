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

//! Frame storage is never released while a caller still borrows it.
//!
//! The class behind review finding B1 and the Miri report on `a3b8396`: user code
//! holds something whose drop or release frees or reuses storage that an SDK
//! frame, or the user's own by-value argument, still references. Every frame
//! here really gives its storage up on release, so Miri, under Stacked Borrows
//! and Tree Borrows, reports a release that lands while a reference is still
//! protected: a `HeapFrame` frees its `Vec`, and the in-memory lending bus with
//! one loan slot per route writes the next message into the bytes a returned
//! frame had. One test per path; the paths are listed in `FIXES.md`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    missing_docs
)]

use core::sync::atomic::Ordering;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;

#[path = "support/review_doubles.rs"]
mod doubles;
use doubles::*;

use up_rust::startup::LEAKED_HEAP_REGIONS;
use up_rust::test_util::lending::{LendingBus, LendingOptions};
use up_rust::two_mode::{native_agreement_table, NativeAgreement, NativeWire};
use up_rust::{NativeProfile, NativeProfileAgreement, StablePayloadExt as _};
use up_rust_core::communication::two_mode::{Publisher, Request, RpcClient, RpcServer, Subscriber};
use up_rust_core::communication::LocalUriProvider as _;
use up_rust_core::memory::StartupMemory;
use up_rust_core::transport::carrier::{
    CarriedFrame, CarriedWire, CarrierRoute, IngressRoute, ParkedFrame,
};
use up_rust_core::transport::qos::{Qos, SendReport};
use up_rust_core::transport::{Flow, Received, SendError, TransportRuntime, TransportSetup};
use up_rust_core::uattributes::{PayloadEncoding, UMessageType};
use up_rust_core::uri::UUri;
use up_rust_core::wire::{InPlace, WirePayloadIdentity};

const WALL: u64 = 2_000_000_000_000;
const PAYLOAD: [u8; 4] = [1, 2, 3, 4];

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

fn topic() -> UUri {
    uri(9, 0x8001)
}

fn frame_bytes(n: u64) -> Vec<u8> {
    publish_frame(&topic(), uuid_at(WALL, n), None, None, &PAYLOAD)
}

fn value_frame(n: u64, value: u32) -> Vec<u8> {
    publish_frame(&topic(), uuid_at(WALL, n), None, None, &value.to_le_bytes())
}

fn heap(shared: &'static Shared, bytes: Vec<u8>) -> HeapFrame {
    HeapFrame {
        bytes,
        peer: peer(7, 1),
        shared,
    }
}

fn dropped(shared: &Shared) -> usize {
    shared.frames_dropped.load(Ordering::SeqCst)
}

fn queue(shared: &Shared, bytes: Vec<u8>) {
    shared.queue.lock().unwrap().push_back((bytes, peer(7, 1)));
}

/// A lending carrier ingress whose frames free their bytes when dropped.
fn lending_ingress(slots: usize) -> (&'static Shared, TransportRuntime, IngressRoute) {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::lending_on(&memory, clock, LendingDouble(shared)).unwrap();
    let topic = topic();
    let mut route = CarrierRoute::new(&topic, carried(), 64);
    route.kind = Some(UMessageType::Publish);
    route.physical_source = Some(&topic);
    route.slots = slots;
    let ingress = setup.carrier_ingress(&route).unwrap();
    (shared, setup.enter_runtime().unwrap(), ingress)
}

// ------------------------------------------------- carrier, lending ingress --

/// A parked frame dropped on another thread while the sink still runs is not
/// returned then: the sink's argument still borrows its bytes. The carry returns
/// it when the sink returns.
#[test]
fn a_park_dropped_on_another_thread_during_the_sink_is_returned_when_the_sink_returns() {
    let (shared, runtime, ingress) = lending_ingress(2);
    queue(shared, frame_bytes(1));
    let handed = runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            assert_eq!(frame.payload().as_contiguous().unwrap(), &PAYLOAD);
            let parked = frame.park().expect("park");
            std::thread::spawn(move || drop(parked)).join().unwrap();
            assert_eq!(dropped(shared), 0, "the sink still runs: not returned yet");
            Flow::Continue
        })
        .unwrap();
    assert_eq!(handed, 1);
    assert_eq!(dropped(shared), 1, "returned once, before receive returned");
}

/// The other thread may drop the parked frame before or after the sink returns.
/// Either way the frame is returned exactly once and the route's slot, retained
/// budget and park budget all come back: a one-slot route carries again.
/// Threaded: run under Miri with many seeds.
#[test]
fn a_park_dropped_on_another_thread_racing_the_sink_is_returned_exactly_once() {
    let (shared, runtime, ingress) = lending_ingress(1);
    queue(shared, frame_bytes(1));
    let mut dropper = None;
    let handed = runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            let parked = frame.park().expect("park");
            dropper = Some(std::thread::spawn(move || drop(parked)));
            Flow::Continue
        })
        .unwrap();
    dropper.expect("spawned").join().unwrap();
    assert_eq!(handed, 1);
    assert_eq!(dropped(shared), 1, "returned exactly once");
    queue(shared, frame_bytes(2));
    let again = runtime.receive(ingress, &mut |_| Flow::Continue).unwrap();
    assert_eq!(again, 1, "the one slot and both budgets came back");
    assert_eq!(dropped(shared), 2);
}

/// A parked frame kept past the sink holds its bytes until it is dropped, and is
/// returned at once then.
#[test]
fn a_park_kept_past_the_sink_is_returned_when_it_is_dropped() {
    let (shared, runtime, ingress) = lending_ingress(2);
    queue(shared, frame_bytes(1));
    let mut kept: Option<ParkedFrame> = None;
    runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            kept = Some(frame.park().expect("park"));
            Flow::Continue
        })
        .unwrap();
    assert_eq!(dropped(shared), 0, "the parked frame holds it");
    let parked = kept.take().expect("parked");
    {
        let view = parked.frame();
        assert_eq!(view.payload().as_contiguous().unwrap(), &PAYLOAD);
        assert!(!view.metadata().is_empty());
    }
    drop(parked);
    assert_eq!(
        dropped(shared),
        1,
        "returned at once: its sink call is over"
    );
}

/// A frame parked in an earlier call and dropped inside a later sink is returned
/// at once, while the later sink's own frame stays held until that sink returns.
#[test]
fn a_park_from_an_earlier_call_dropped_in_a_later_sink_is_returned_at_once() {
    let (shared, runtime, ingress) = lending_ingress(2);
    queue(shared, frame_bytes(1));
    let mut first: Option<ParkedFrame> = None;
    runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            first = Some(frame.park().expect("park"));
            Flow::Continue
        })
        .unwrap();
    queue(shared, frame_bytes(2));
    let handed = runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            drop(first.take());
            assert_eq!(dropped(shared), 1, "the earlier frame went back at once");
            assert_eq!(frame.payload().as_contiguous().unwrap(), &PAYLOAD);
            Flow::Continue
        })
        .unwrap();
    assert_eq!(handed, 1);
    assert_eq!(
        dropped(shared),
        2,
        "this sink's frame went back when it returned"
    );
}

/// A sink that parks, drops the parked frame and then panics: the frame is
/// returned once, on the unwind, and the route keeps working.
#[test]
fn a_sink_that_panics_after_dropping_its_park_returns_the_frame_once() {
    let (shared, runtime, ingress) = lending_ingress(1);
    queue(shared, frame_bytes(1));
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        runtime.receive(ingress, &mut |frame: CarriedFrame<'_>| {
            drop(frame.park().expect("park"));
            assert_eq!(dropped(shared), 0);
            panic!("the sink fails after dropping its park");
        })
    }));
    assert!(outcome.is_err(), "the sink's panic reaches the caller");
    assert_eq!(dropped(shared), 1, "returned once, on the unwind");
    queue(shared, frame_bytes(2));
    assert_eq!(
        runtime.receive(ingress, &mut |_| Flow::Continue).unwrap(),
        1,
        "the slot and budgets came back"
    );
}

/// A sink that panics without parking: the frame is returned on the unwind.
/// On `a3b8396` this leaked the slot and the retained budget, so a one-slot
/// route never carried again and the frame was never returned.
#[test]
fn a_sink_that_panics_without_parking_returns_the_frame() {
    let (shared, runtime, ingress) = lending_ingress(1);
    queue(shared, frame_bytes(1));
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        runtime.receive(ingress, &mut |frame: CarriedFrame<'_>| {
            assert!(!frame.metadata().is_empty());
            panic!("the sink fails");
        })
    }));
    assert!(outcome.is_err());
    assert_eq!(dropped(shared), 1, "the frame went back on the unwind");
    queue(shared, frame_bytes(2));
    assert_eq!(
        runtime.receive(ingress, &mut |_| Flow::Continue).unwrap(),
        1,
        "the route still carries"
    );
}

/// A sink that keeps its park and then panics: the frame stays parked, readable,
/// and is returned when the parked frame drops.
#[test]
fn a_sink_that_panics_with_its_park_kept_leaves_the_frame_parked() {
    let (shared, runtime, ingress) = lending_ingress(2);
    queue(shared, frame_bytes(1));
    let mut kept: Option<ParkedFrame> = None;
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        runtime.receive(ingress, &mut |frame: CarriedFrame<'_>| {
            kept = Some(frame.park().expect("park"));
            panic!("the sink fails with its park kept");
        })
    }));
    assert!(outcome.is_err());
    assert_eq!(dropped(shared), 0, "the parked frame still holds it");
    let parked = kept.take().expect("parked");
    assert_eq!(
        parked.frame().payload().as_contiguous().unwrap(),
        &PAYLOAD,
        "and its bytes are still there"
    );
    drop(parked);
    assert_eq!(dropped(shared), 1);
}

/// A frame the binding pushed with `deliver_frame` takes the same carry: parked
/// and dropped in the sink, it is returned when the sink returns.
#[test]
fn a_pushed_frame_parked_and_dropped_in_the_sink_is_returned_when_the_sink_returns() {
    let (shared, runtime, ingress) = lending_ingress(2);
    shared
        .port()
        .deliver_frame(shared.route(0), heap(shared, frame_bytes(1)))
        .unwrap();
    let handed = runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            drop(frame.park().expect("park"));
            assert_eq!(dropped(shared), 0, "not during the sink call");
            Flow::Continue
        })
        .unwrap();
    assert_eq!(handed, 1);
    assert_eq!(dropped(shared), 1);
}

// A carried frame has no destructor: only `park` and its parked frame release.
const _: () = assert!(!core::mem::needs_drop::<CarriedFrame<'static>>());

/// On a handoff ingress the binding owns the bytes for its `receive` call: the
/// sink cannot park the frame, and the refused frame it gets back has no
/// destructor, so nothing the sink does with it releases the binding's bytes.
#[test]
fn a_handoff_frame_cannot_be_parked_and_stays_lent_for_the_whole_sink() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
    let topic = topic();
    let mut route = CarrierRoute::new(&topic, carried(), 64);
    route.kind = Some(UMessageType::Publish);
    let ingress = setup.carrier_ingress(&route).unwrap();
    let runtime = setup.enter_runtime().unwrap();
    queue(shared, frame_bytes(1));
    let handed = runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            let before = frame.payload().as_contiguous().unwrap()[0];
            let back = frame.park().expect_err("a handoff frame cannot be parked");
            assert_eq!(back.payload().as_contiguous().unwrap()[0], before);
            Flow::Continue
        })
        .unwrap();
    assert_eq!(handed, 1);
}

// ---------------------------------------------------------- carrier, egress --

/// A parked frame resent inside a later sink and then dropped: the egress gets
/// the ingress bytes unchanged, both times, and both frames go back.
#[test]
fn a_parked_frame_resent_then_dropped_in_a_later_sink_forwards_its_bytes() {
    let (shared, runtime, ingress) = lending_ingress(2);
    let clock = Clock::leak(1_000_000_000, WALL);
    let out = Shared::leak();
    let out_memory = memory();
    let out_setup = TransportSetup::handoff_on(&out_memory, clock, HandoffDouble(out)).unwrap();
    let topic = topic();
    let mut route = CarrierRoute::new(&topic, carried(), 64);
    route.kind = Some(UMessageType::Publish);
    let egress = out_setup.carrier_egress(&route).unwrap();
    let out_runtime = out_setup.enter_runtime().unwrap();
    let first_bytes = frame_bytes(1);
    let second_bytes = frame_bytes(2);
    queue(shared, first_bytes.clone());
    let mut first: Option<ParkedFrame> = None;
    runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            // As if the egress had been busy.
            first = Some(frame.park().expect("park"));
            Flow::Continue
        })
        .unwrap();
    queue(shared, second_bytes.clone());
    runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            let parked = first.take().expect("parked");
            out_runtime.send(egress, &parked.frame()).expect("resend");
            drop(parked);
            assert_eq!(dropped(shared), 1, "the resent frame went back at once");
            out_runtime.send(egress, &frame).expect("send");
            Flow::Continue
        })
        .unwrap();
    assert_eq!(dropped(shared), 2);
    let sent = out.sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0], first_bytes, "the parked frame's bytes, unchanged");
    assert_eq!(sent[1], second_bytes);
}

fn bus_setup(bus: LendingBus, peer: u64) -> TransportSetup {
    let memory = StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 64 * 1024).unwrap();
    TransportSetup::lending_in(&memory, bus.participant(peer)).unwrap()
}

/// The same over a lending egress, whose send copies the ingress bytes once into
/// its loan, here with the metadata re-encoded for the egress's own envelope: the
/// payload arrives unchanged, and both freeing ingress frames go back.
#[test]
fn a_parked_frame_resent_then_dropped_through_a_lending_egress_forwards_its_payload() {
    let (shared, runtime, ingress) = lending_ingress(2);
    let bus = LendingBus::new();
    let topic = topic();
    let mut route = CarrierRoute::new(&topic, carried(), 64);
    route.kind = Some(UMessageType::Publish);
    route.physical_source = Some(&topic);
    let out_setup = bus_setup(bus, 1);
    let egress = out_setup.carrier_egress(&route).unwrap();
    let reader_setup = bus_setup(bus, 2);
    let _reader = reader_setup.carrier_ingress(&route).unwrap();
    let out_runtime = out_setup.enter_runtime().unwrap();
    let _reader_runtime = reader_setup.enter_runtime().unwrap();
    queue(shared, frame_bytes(1));
    let mut first: Option<ParkedFrame> = None;
    runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            first = Some(frame.park().expect("park"));
            Flow::Continue
        })
        .unwrap();
    queue(shared, frame_bytes(2));
    runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            let parked = first.take().expect("parked");
            out_runtime
                .send(egress, &parked.frame())
                .expect("resend into the loan");
            drop(parked);
            assert_eq!(dropped(shared), 1, "the resent frame went back at once");
            out_runtime
                .send(egress, &frame)
                .expect("send into the loan");
            Flow::Continue
        })
        .unwrap();
    assert_eq!(dropped(shared), 2);
    for _ in 0..2 {
        let (_, payload) = bus.take_raw(2).expect("forwarded");
        assert_eq!(
            payload.as_deref(),
            Some(&PAYLOAD[..]),
            "the ingress payload, unchanged"
        );
    }
}

/// The re-check's B1-class note: a parked frame dropped inside the sink, then a
/// nested `receive` on the same ingress from inside that sink. The outer frame's
/// slot is not free until the outer sink returns, so the nested call takes
/// another slot and cannot rewrite the bytes the outer argument still borrows.
#[test]
fn a_nested_receive_inside_the_sink_cannot_reuse_the_outer_frames_slot() {
    let (shared, runtime, ingress) = lending_ingress(2);
    queue(shared, frame_bytes(1));
    let mut nested = 0;
    let handed = runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            drop(frame.park().expect("park"));
            queue(shared, frame_bytes(2));
            nested = runtime
                .receive(ingress, &mut |inner: CarriedFrame<'_>| {
                    assert_eq!(inner.payload().as_contiguous().unwrap(), &PAYLOAD);
                    Flow::Continue
                })
                .unwrap();
            assert_eq!(
                dropped(shared),
                1,
                "the nested frame went back; the outer one waits for its sink"
            );
            Flow::Continue
        })
        .unwrap();
    assert_eq!((handed, nested), (1, 1));
    assert_eq!(dropped(shared), 2);
}

// ------------------------------------------------------------ typed receive --

fn lending_subscriber(depth: u16, held: usize) -> (&'static Shared, Subscriber<u32>) {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::lending_on(&memory, clock, LendingDouble(shared)).unwrap();
    let sub = Subscriber::<u32>::on(&setup, topic())
        .wire(wire())
        .qos(Qos::DEFAULT.with_depth(depth))
        .max_held(held)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    (shared, sub.activate(&runtime).unwrap())
}

/// A handler that takes the lease by value, reads it and drops it.
fn take_value(message: Received<u32>) -> u32 {
    let value = *message;
    drop(message);
    value
}

/// On an encoded lending route a retained frame is decoded where it is and goes
/// back at once, inside delivery or inside `try_recv`; the value outlives it.
#[test]
fn a_retained_frame_is_returned_at_delivery_and_its_value_outlives_it() {
    let (shared, sub) = lending_subscriber(4, 4);
    shared
        .port()
        .deliver_frame(shared.route(0), heap(shared, value_frame(1, 7)))
        .unwrap();
    assert_eq!(dropped(shared), 1, "decoded at delivery, then returned");
    queue(shared, value_frame(2, 8));
    let pushed = sub.try_recv().unwrap().expect("the pushed frame");
    assert_eq!(*pushed, 7);
    let pulled = sub.try_recv().unwrap().expect("the pulled frame");
    assert_eq!(
        dropped(shared),
        2,
        "pulled, decoded and returned in try_recv"
    );
    assert_eq!(*pulled, 8);
}

/// A lease handled by value and dropped returns its slot, and the next frame is
/// decoded into that same slot.
#[test]
fn a_lease_dropped_in_a_handler_returns_its_slot_to_the_next_frame() {
    let (shared, sub) = lending_subscriber(1, 1);
    for (n, value) in [(1_u64, 11_u32), (2, 22), (3, 33)] {
        shared
            .port()
            .deliver_frame(shared.route(0), heap(shared, value_frame(n, value)))
            .unwrap();
        assert_eq!(take_value(sub.try_recv().unwrap().expect("a frame")), value);
    }
    assert_eq!(dropped(shared), 3);
}

/// Leases dropped on another thread while frames keep arriving. Threaded: run
/// under Miri with many seeds.
#[test]
fn leases_dropped_on_another_thread_while_frames_arrive() {
    let (shared, sub) = lending_subscriber(2, 2);
    let mut taken = 0_u32;
    std::thread::scope(|scope| {
        let producer = scope.spawn(|| {
            for n in 0..3_u64 {
                let _ = shared
                    .port()
                    .deliver_frame(shared.route(0), heap(shared, value_frame(n, 5)));
            }
        });
        for _ in 0..64 {
            if let Ok(Some(message)) = sub.try_recv() {
                taken += 1;
                std::thread::spawn(move || take_value(message))
                    .join()
                    .unwrap();
            }
            if producer.is_finished() && taken == 3 {
                break;
            }
            std::thread::yield_now();
        }
    });
    while let Ok(Some(message)) = sub.try_recv() {
        taken += 1;
        take_value(message);
    }
    let counters = sub.counters();
    assert_eq!(dropped(shared), 3, "every frame went back once");
    assert_eq!(
        u64::from(taken)
            + counters.dropped_oldest
            + counters.dropped_newest
            + counters.slots_exhausted
            + counters.refused_contention,
        3,
        "every frame was taken or counted"
    );
}

// --------------------------------------------------------------------- RPC --

const METHOD_RESOURCE: u16 = 1;

fn method() -> UUri {
    uri(3, METHOD_RESOURCE)
}

fn client_on_handoff(in_flight: usize) -> (&'static Shared, RpcClient<u32, u32>) {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
    let client = RpcClient::<u32, u32>::on(&setup, &Uri(2), method())
        .wire(wire())
        .max_in_flight(in_flight)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    (shared, client.activate(&runtime).unwrap())
}

/// The response to the last request this client sent.
fn response_to_last(shared: &Shared, value: u32) -> Vec<u8> {
    let request = shared
        .sent
        .lock()
        .unwrap()
        .last()
        .expect("a request")
        .clone();
    whole_frame(&Spec {
        kind: UMessageType::Response,
        id: uuid_at(WALL, 500),
        reqid: Some(sent_request_id(&request)),
        ttl: None,
        source: method(),
        sink: Some(uri(2, 0)),
        token: None,
        payload: Some(&value.to_le_bytes()),
    })
}

/// A retained response stays with its call until `try_response` decodes it, and
/// goes back inside that call; the decoded value outlives it.
#[test]
fn a_retained_response_is_returned_inside_try_response_once_decoded() {
    let (shared, client) = client_on_handoff(1);
    let mut call = client.invoke(&5).expect("invoke");
    let response = response_to_last(shared, 77);
    shared
        .port()
        .deliver_frame(shared.route(0), heap(shared, response))
        .unwrap();
    assert_eq!(dropped(shared), 0, "the call holds its response frame");
    let reply = call.try_response().unwrap().expect("the response");
    assert_eq!(dropped(shared), 1, "returned inside try_response");
    assert_eq!(*reply, 77);
}

/// A call dropped with its response still untaken returns the response frame.
#[test]
fn a_call_dropped_with_an_untaken_retained_response_returns_it() {
    let (shared, client) = client_on_handoff(1);
    let call = client.invoke(&5).expect("invoke");
    let response = response_to_last(shared, 77);
    shared
        .port()
        .deliver_frame(shared.route(0), heap(shared, response))
        .unwrap();
    assert_eq!(dropped(shared), 0);
    drop(call);
    assert_eq!(dropped(shared), 1);
}

/// A call dropped on one thread while its response is delivered on another: the
/// frame goes back exactly once, from whichever side finishes, and the slot is
/// reusable. Threaded: run under Miri with many seeds.
#[test]
fn a_call_dropped_while_its_response_is_delivered_returns_the_frame_once() {
    let (shared, client) = client_on_handoff(1);
    let call = client.invoke(&5).expect("invoke");
    let response = response_to_last(shared, 77);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let _ = shared
                .port()
                .deliver_frame(shared.route(0), heap(shared, response));
        });
        scope.spawn(move || drop(call));
    });
    assert_eq!(dropped(shared), 1, "returned exactly once");
    let mut again = client.invoke(&6).expect("the one call slot came back");
    let response = response_to_last(shared, 66);
    shared
        .port()
        .deliver_frame(shared.route(0), heap(shared, response))
        .unwrap();
    assert_eq!(*again.try_response().unwrap().expect("response"), 66);
    assert_eq!(dropped(shared), 2);
}

fn request_bytes(n: u64, value: u32) -> Vec<u8> {
    whole_frame(&Spec {
        kind: UMessageType::Request,
        id: uuid_at(WALL, n),
        reqid: None,
        ttl: Some(Duration::from_secs(10)),
        source: uri(2, 0),
        sink: Some(method()),
        token: None,
        payload: Some(&value.to_le_bytes()),
    })
}

/// A handler that takes the request by value and answers it.
fn answer(request: Request<u32, u32>) -> Result<SendReport, SendError> {
    let asked = *request;
    request.respond(&(asked + 1))
}

/// Requests handled by value and answered: each frame went back at delivery, and
/// each answered request returns its slot to the next.
#[test]
fn requests_answered_by_value_return_their_slot_to_the_next_request() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
    let server = RpcServer::<u32, u32>::on(&setup, &Uri(3), METHOD_RESOURCE)
        .wire(wire())
        .qos(Qos::DEFAULT.with_depth(1))
        .max_held(1)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let server = server.activate(&runtime).unwrap();
    for (n, value) in [(1_u64, 10_u32), (2, 20), (3, 30)] {
        shared
            .port()
            .deliver_frame(shared.route(0), heap(shared, request_bytes(n, value)))
            .unwrap();
        answer(server.try_recv().unwrap().expect("a request")).expect("answered");
    }
    assert_eq!(dropped(shared), 3);
    assert_eq!(
        shared.sent.lock().unwrap().len(),
        3,
        "three responses went out"
    );
}

// -------------------------------------------------------------- slot pools --

/// Two threads publishing through one value slot take turns: a sender writes a
/// slot only while it holds it. Threaded: run under Miri with many seeds.
#[test]
fn two_threads_sending_through_one_value_slot_take_turns() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
    let publisher = Publisher::<u32>::on(&setup, &Uri(5), 0x8001_u16)
        .wire(wire())
        .slots(1)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let publisher = publisher.activate(&runtime).unwrap();
    let outcomes: Vec<Result<SendReport, SendError>> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..2_u32)
            .map(|worker| {
                let publisher = &publisher;
                scope.spawn(move || {
                    (0..2_u32)
                        .map(|n| publisher.publish(&(worker * 10 + n)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect()
    });
    let sent = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
    let busy = outcomes
        .iter()
        .filter(|outcome| matches!(outcome, Err(SendError::SlotsExhausted)))
        .count();
    assert_eq!(
        sent + busy,
        4,
        "every send went out or found the slot taken"
    );
    assert_eq!(shared.sent.lock().unwrap().len(), sent);
}

// ------------------------------------------- native, lending bus, one slot --

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, up_rust::StablePayload, up_rust::StablePayloadInit,
)]
#[repr(C)]
#[stable_payload(type_name = "uprotocol.test.ReleaseDoor")]
struct ReleaseDoor {
    id: u32,
    open: u32,
}

fn release_agreement() -> &'static NativeAgreement {
    let profile = NativeProfile::new(
        "test",
        1,
        PayloadEncoding::private_use(0xF779),
        [ReleaseDoor::native_representation()],
    )
    .unwrap();
    let agreement = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
    native_agreement_table(&agreement)
}

fn one_slot_bus_setup(bus: LendingBus, peer: u64) -> TransportSetup {
    let memory = StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 64 * 1024).unwrap();
    let options = LendingOptions {
        slots: 1,
        ..LendingOptions::default()
    };
    TransportSetup::lending_in(&memory, bus.participant(peer).with_options(options)).unwrap()
}

/// A native handler that takes the lease by value, reads it in place and drops it.
fn read_in_place(message: Received<ReleaseDoor, InPlace>) -> ReleaseDoor {
    let value = *message.payload().unwrap();
    drop(message);
    value
}

/// A native lease dropped in a handler returns the transport's one loan slot, and
/// the next publish writes its message into those same bytes.
#[test]
fn a_native_lease_dropped_in_a_handler_has_its_bytes_reused_by_the_next_publish() {
    let bus = LendingBus::new();
    let agreement = release_agreement();
    let a = one_slot_bus_setup(bus, 1);
    let b = one_slot_bus_setup(bus, 2);
    // The bus gives a send route the larger of its own slot count and the
    // route's, so the route asks for one too.
    let publisher = Publisher::<ReleaseDoor>::on(&a, &Uri(1), 0x8001_u16)
        .wire(NativeWire::new(agreement))
        .slots(1)
        .build()
        .unwrap();
    let subscriber = Subscriber::<ReleaseDoor>::on(&b, Uri(1).get_resource_uri(0x8001))
        .wire(NativeWire::new(agreement))
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let publisher: Publisher<ReleaseDoor, InPlace> = publisher.activate(&tx).unwrap();
    let subscriber: Subscriber<ReleaseDoor, InPlace> = subscriber.activate(&rx).unwrap();
    publisher
        .publish_in_place(|init| init.into_initializer().id(1).open(1).finish())
        .unwrap();
    let first = subscriber.try_recv().unwrap().expect("a frame");
    assert!(
        publisher
            .publish_in_place(|init| init.into_initializer().id(9).open(9).finish())
            .is_err(),
        "control: the one loan slot is held by the lease"
    );
    assert_eq!(read_in_place(first), ReleaseDoor { id: 1, open: 1 });
    assert_eq!(bus.frame_drops(), 1);
    publisher
        .publish_in_place(|init| init.into_initializer().id(2).open(0).finish())
        .expect("the slot came back");
    let second = subscriber.try_recv().unwrap().expect("a frame");
    assert_eq!(read_in_place(second), ReleaseDoor { id: 2, open: 0 });
}

/// Native RPC over one loan slot per route: a request dropped in a handler and a
/// call dropped after borrowing its reply each give their slot to the next
/// message, which is written into the same bytes.
#[test]
fn native_requests_and_replies_dropped_by_value_have_their_bytes_reused() {
    fn read_request(
        request: Request<ReleaseDoor, ReleaseDoor, InPlace>,
    ) -> (ReleaseDoor, Request<ReleaseDoor, ReleaseDoor, InPlace>) {
        (*request.payload().unwrap(), request)
    }
    let bus = LendingBus::new();
    let agreement = release_agreement();
    let a = one_slot_bus_setup(bus, 1);
    let b = one_slot_bus_setup(bus, 2);
    let known = Uri(1).get_source_uri();
    // One loan slot on each send route: the request route has `max_in_flight`
    // slots and the response route the server's `slots`.
    let client = RpcClient::<ReleaseDoor, ReleaseDoor>::on(&a, &Uri(1), Uri(2).get_resource_uri(1))
        .wire(NativeWire::new(agreement))
        .max_in_flight(1)
        .build()
        .unwrap();
    let server = RpcServer::<ReleaseDoor, ReleaseDoor>::on(&b, &Uri(2), 1_u16)
        .wire(NativeWire::new(agreement))
        .clients(core::slice::from_ref(&known))
        .slots(1)
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let client: RpcClient<ReleaseDoor, ReleaseDoor, InPlace> = client.activate(&tx).unwrap();
    let server: RpcServer<ReleaseDoor, ReleaseDoor, InPlace> = server.activate(&rx).unwrap();
    for round in 1..=2_u32 {
        let mut call = client
            .invoke_in_place(|init| init.into_initializer().id(round).open(1).finish())
            .expect("the request slot is free");
        let (asked, request) = read_request(server.try_recv().unwrap().expect("a request"));
        assert_eq!(asked, ReleaseDoor { id: round, open: 1 });
        request
            .respond_in_place(|init| init.into_initializer().id(asked.id + 100).open(0).finish())
            .expect("the response slot is free");
        {
            let reply = call.try_response().unwrap().expect("a response");
            assert_eq!(
                *reply.payload().unwrap(),
                ReleaseDoor {
                    id: round + 100,
                    open: 0
                }
            );
        }
        drop(call);
    }
    assert!(bus.frame_drops() >= 4, "every frame went back to the bus");
}
