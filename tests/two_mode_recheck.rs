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

//! The re-check's observations on `a3b8396` (`review-stage2/RECHECK.md`):
//! `.max_peers(0)` told the binding 0 while the SDK built one entry, and three new
//! paths had no test: the reply window's `ClockUnavailable`, `peer_lost` freeing
//! entries on carrier and client-response routes, and `try_recv` waking waiters
//! when it stops at its bound.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    missing_docs
)]

use core::time::Duration;

#[path = "support/review_doubles.rs"]
mod doubles;
use doubles::*;

use core::sync::atomic::Ordering;
use up_rust_core::communication::two_mode::{RpcClient, RpcServer, Subscriber};
use up_rust_core::signal::SignalWait;
use up_rust_core::transport::binding::{InboundFrame, PayloadBytes, PeerRef, ReplyToken};
use up_rust_core::transport::carrier::{CarriedWire, CarrierRoute};
use up_rust_core::transport::qos::Qos;
use up_rust_core::transport::{ReceiveError, SendError, StartupError, TransportSetup};
use up_rust_core::uattributes::UMessageType;
use up_rust_core::uri::UUri;
use up_rust_core::wire::WirePayloadIdentity;

const WALL: u64 = 2_000_000_000_000;

fn wire() -> U32Wire {
    U32Wire { hook: None }
}

fn carried() -> &'static [CarriedWire] {
    Box::leak(Box::new([CarriedWire {
        wire_id: WIRE_ID,
        payload_family: FAMILY_ID,
        identity: WirePayloadIdentity::serialized(ENCODING),
        alignment: 1,
    }]))
}

fn deliver(shared: &Shared, frame: &[u8], from: PeerRef) -> Result<(), ReceiveError> {
    shared.port().deliver(
        shared.route(0),
        InboundFrame {
            metadata: &[],
            payload: PayloadBytes::one(frame),
            peer: from,
            reply: ReplyToken::default(),
        },
    )
}

fn handoff_setup(clock: &'static Clock) -> (&'static Shared, TransportSetup) {
    let shared = Shared::leak();
    let setup = TransportSetup::handoff_on(&memory(), clock, HandoffDouble(shared)).unwrap();
    (shared, setup)
}

/// `.max_peers(0)` is refused at setup on every receive route, since the binding
/// would be told of a table the SDK does not build.
#[test]
fn a_zero_peer_table_is_refused_at_setup() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let (_, setup) = handoff_setup(clock);
    let subscriber = Subscriber::<u32>::on(&setup, uri(9, 0x8001))
        .wire(wire())
        .max_peers(0)
        .build();
    assert!(
        matches!(subscriber, Err(StartupError::InvalidConfig(_))),
        "{subscriber:?}"
    );
    let server = RpcServer::<u32, u32>::on(&setup, &Uri(3), 1_u16)
        .wire(wire())
        .max_peers(0)
        .build();
    assert!(
        matches!(server, Err(StartupError::InvalidConfig(_))),
        "{server:?}"
    );
    let topic = uri(9, 0x8002);
    let mut route = CarrierRoute::new(&topic, carried(), 64);
    route.kind = Some(UMessageType::Publish);
    route.max_peers = 0;
    let ingress = setup.carrier_ingress(&route);
    assert!(
        matches!(ingress, Err(StartupError::CarrierRefused { .. })),
        "{ingress:?}"
    );
}

/// A response is refused, failing closed, when the wall clock cannot be read to
/// judge the request's window (the L2 fix's `ClockUnavailable`).
#[test]
fn a_response_is_refused_when_the_wall_clock_cannot_be_read() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let (shared, setup) = handoff_setup(clock);
    let server = RpcServer::<u32, u32>::on(&setup, &Uri(3), 1_u16)
        .wire(wire())
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let server = server.activate(&runtime).unwrap();
    let request = whole_frame(&Spec {
        kind: UMessageType::Request,
        id: uuid_at(WALL, 1),
        reqid: None,
        ttl: Some(Duration::from_secs(10)),
        source: uri(2, 0),
        sink: Some(uri(3, 1)),
        token: None,
        payload: Some(&5u32.to_le_bytes()),
    });
    deliver(shared, &request, peer(7, 1)).unwrap();
    let request = server.try_recv().unwrap().expect("a request");
    clock.wall.store(u64::MAX, Ordering::SeqCst);
    assert_eq!(
        request.respond(&6).map(|_| ()),
        Err(SendError::ClockUnavailable)
    );
    assert!(shared.sent.lock().unwrap().is_empty(), "nothing went out");
}

/// `peer_lost` frees a carrier ingress entry: a one-entry table refuses a second
/// peer until the first is reported gone, then serves it.
#[test]
fn peer_lost_frees_a_carrier_ingress_entry() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let setup = TransportSetup::lending_on(&memory(), clock, LendingDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let mut route = CarrierRoute::new(&topic, carried(), 64);
    route.kind = Some(UMessageType::Publish);
    route.physical_source = Some(&topic);
    route.max_peers = 1;
    route.slots = 4;
    let _ingress = setup.carrier_ingress(&route).unwrap();
    let _runtime = setup.enter_runtime().unwrap();
    let frame = |n| HeapFrame {
        bytes: publish_frame(&topic, uuid_at(WALL, n), None, None, &[1, 2, 3, 4]),
        peer: peer(100 + n, 1),
        shared,
    };
    let port = shared.port();
    assert_eq!(port.deliver_frame(shared.route(0), frame(1)), Ok(()));
    assert_eq!(
        port.deliver_frame(shared.route(0), frame(2)),
        Err(ReceiveError::PeerTableFull),
        "control: the one entry is taken"
    );
    port.peer_lost(shared.route(0), peer(101, 1));
    assert_eq!(
        port.deliver_frame(shared.route(0), frame(2)),
        Ok(()),
        "the lost peer's entry serves the next peer"
    );
}

/// `peer_lost` on a client's response route frees the server's entry; it does
/// not infer outstanding calls' destinations. After eight servers take the table, a ninth
/// is refused until one is reported gone, then its response completes a call.
#[test]
fn peer_lost_frees_a_client_response_entry() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let (shared, setup) = handoff_setup(clock);
    let method = uri(3, 1);
    let client = RpcClient::<u32, u32>::on(&setup, &Uri(2), method.clone())
        .wire(wire())
        .max_in_flight(10)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let client = client.activate(&runtime).unwrap();
    let response_to = |request: &[u8], value: u32| {
        whole_frame(&Spec {
            kind: UMessageType::Response,
            id: uuid_at(WALL, 900),
            reqid: Some(sent_request_id(request)),
            ttl: None,
            source: method.clone(),
            sink: Some(uri(2, 0)),
            token: None,
            payload: Some(&value.to_le_bytes()),
        })
    };
    let mut calls = Vec::new();
    for n in 0..9_u64 {
        calls.push(client.invoke(&1).expect("invoke"));
        let request = shared.sent.lock().unwrap().last().unwrap().clone();
        let outcome = deliver(shared, &response_to(&request, 1), peer(100 + n, 1));
        if n < 8 {
            assert_eq!(outcome, Ok(()), "server {n} takes an entry");
        } else {
            assert_eq!(
                outcome,
                Err(ReceiveError::PeerTableFull),
                "control: a ninth server finds the table full"
            );
        }
    }
    shared.port().peer_lost(shared.route(0), peer(100, 1));
    let mut call = client.invoke(&2).expect("invoke");
    let request = shared.sent.lock().unwrap().last().unwrap().clone();
    assert_eq!(
        deliver(shared, &response_to(&request, 42), peer(108, 1)),
        Ok(()),
        "the lost server's entry serves the ninth"
    );
    assert_eq!(*call.try_response().unwrap().expect("a response"), 42);
    drop(calls);
}

/// `try_recv` that stops at its bound with frames still waiting wakes waiters, so
/// a parked caller looks again. The frames here are refused before they are
/// queued, so only that wake can bump the signal.
#[test]
fn try_recv_stopping_at_its_bound_wakes_waiters() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let (shared, setup) = handoff_setup(clock);
    let topic = uri(9, 0x8001);
    let subscriber = Subscriber::<u32>::on(&setup, topic.clone())
        .wire(wire())
        .qos(Qos::DEFAULT.with_depth(1))
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let subscriber = subscriber.activate(&runtime).unwrap();
    let elsewhere: UUri = uri(10, 0x8001);
    *shared.flood_frame.lock().unwrap() = (
        publish_frame(
            &elsewhere,
            uuid_at(WALL, 1),
            None,
            None,
            &1u32.to_le_bytes(),
        ),
        peer(7, 1),
    );
    shared.flood.store(1_000, Ordering::SeqCst);
    let token = runtime.wait_token();
    assert!(subscriber.try_recv().unwrap().is_none());
    assert_eq!(
        shared.flood.load(Ordering::SeqCst),
        1_000 - 2,
        "depth 1 plus one frames examined"
    );
    assert_eq!(
        runtime.wait(token, Duration::ZERO),
        SignalWait::Notified,
        "stopping at the bound woke the waiters"
    );
}


/// Exact loss has observable outcomes; stale/unknown/wrong-peer events cannot
/// fail a call without an authenticated destination; observed peers are insufficient.
// [utest->req~ssr-peer-stale-rejected~1]
// [utest->req~ssr-peer-rpc-ends~1]
#[test]
fn peer_loss_outcome_preserves_newer_call_and_rejects_unknown_identity() {
    use up_rust_core::transport::{CallError, PeerLossOutcome};
    let clock = Clock::leak(1_000_000_000, WALL);
    let (shared, setup) = handoff_setup(clock);
    let method = uri(3, 1);
    let client = RpcClient::<u32, u32>::on(&setup, &Uri(2), method.clone())
        .wire(wire())
        .max_in_flight(2)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let client = client.activate(&runtime).unwrap();
    let mut seed = client.invoke(&1).unwrap();
    let request = shared.sent.lock().unwrap().last().unwrap().clone();
    let response = whole_frame(&Spec {
        kind: UMessageType::Response,
        id: uuid_at(WALL, 900),
        reqid: Some(sent_request_id(&request)),
        ttl: None,
        source: method,
        sink: Some(uri(2, 0)),
        token: None,
        payload: Some(&1_u32.to_le_bytes()),
    });
    assert_eq!(deliver(shared, &response, peer(100, 7)), Ok(()));
    assert!(seed.try_response().unwrap().is_some());
    drop(seed);
    let port = shared.port();
    let route = shared.route(0);
    port.peer_restarted(route, peer(100, 8));
    let mut current = client.invoke(&2).unwrap();
    for lost in [peer(100, 7), peer(101, 8)] {
        assert_eq!(port.report_peer_lost(route, lost), PeerLossOutcome::Applied {
            peer_removed: false, calls_failed: 0,
        });
        assert!(current.try_response().unwrap().is_none());
    }
    assert_eq!(port.report_peer_lost(route, peer(100, 0)), PeerLossOutcome::Unidentified);
    assert!(current.try_response().unwrap().is_none());
    assert_eq!(port.report_peer_lost(route, peer(100, 8)), PeerLossOutcome::Applied {
        peer_removed: true, calls_failed: 0,
    });
    assert!(current.try_response().unwrap().is_none());
    clock.advance_ms(31_000);
    assert!(matches!(current.try_response(), Err(CallError::TimedOut)));
    drop(current);
    let mut unknown = client.invoke(&3).unwrap();
    assert_eq!(port.report_peer_lost(route, peer(100, 8)), PeerLossOutcome::Applied {
        peer_removed: false, calls_failed: 0,
    });
    assert!(unknown.try_response().unwrap().is_none());
}
