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

//! Stage-2 review regressions that need the API the fixes added: the `CarriedFrame`
//! accessors, `Call::try_response(&mut self)`, the server's `.max_peers(n)`, the
//! binding's peer-count refusal. The compile-fail halves are in `tests/ui`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::wildcard_enum_match_arm,
    clippy::too_many_lines,
    missing_docs
)]

use core::sync::atomic::Ordering;
use core::time::Duration;

#[path = "support/review_doubles.rs"]
mod doubles;
use doubles::*;

use up_rust_core::communication::two_mode::{RpcClient, RpcServer};
use up_rust_core::transport::binding::{
    BindingRefusal, BindingRoute, BindingSetup, Capabilities, InboundFrame, PayloadBytes, PeerRef,
    PreparedRoute, ReplyToken, RouteDeclaration, RouteKind, RuntimePort,
};
use up_rust_core::transport::carrier::{CarriedWire, CarrierRoute, ParkedFrame};
use up_rust_core::transport::qos::{check_qos, Qos, QosSetting, QosSupport};
use up_rust_core::transport::{Flow, StartupError, TransportSetup};
use up_rust_core::uattributes::UMessageType;
use up_rust_core::uri::UUri;
use up_rust_core::wire::{RuntimeHeap, WirePayloadIdentity};

const WALL: u64 = 2_000_000_000_000;

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

fn deliver(
    shared: &Shared,
    frame: &[u8],
    from: PeerRef,
) -> Result<(), up_rust_core::transport::ReceiveError> {
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

fn request_frame(from: &UUri, method: &UUri, n: u64) -> Vec<u8> {
    whole_frame(&Spec {
        kind: UMessageType::Request,
        id: uuid_at(WALL, n),
        reqid: None,
        ttl: Some(Duration::from_secs(10)),
        source: from.clone(),
        sink: Some(method.clone()),
        token: None,
        payload: Some(&1u32.to_le_bytes()),
    })
}

/// B1. The forwarder's view methods borrow the frame, and a parked frame dropped
/// inside the sink really is freed, but only once the sink has returned: the
/// sink's argument still borrows the bytes until then. The frame frees its `Vec`,
/// so Miri reports a free that lands inside the call (as it did on `a3b8396`).
#[test]
fn b1_views_are_read_through_the_frame_and_a_dropped_park_frees_after_the_sink() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::lending_on(&memory, clock, LendingDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let mut route = CarrierRoute::new(&topic, carried(), 64);
    route.kind = Some(UMessageType::Publish);
    route.physical_source = Some(&topic);
    route.slots = 4;
    let ingress = setup.carrier_ingress(&route).unwrap();
    let runtime = setup.enter_runtime().unwrap();
    shared.queue.lock().unwrap().push_back((
        publish_frame(&topic, uuid_at(WALL, 1), None, None, &[1, 2, 3, 4]),
        peer(7, 1),
    ));
    let mut metadata_len = 0;
    let mut first_payload_byte = 0;
    let handed = runtime
        .receive(ingress, &mut |frame| {
            // Every view is read here, while the frame is held.
            metadata_len = frame.metadata().len();
            first_payload_byte = frame.payload().as_contiguous().unwrap()[0];
            assert_eq!(frame.encoding(), Some(ENCODING));
            let parked: ParkedFrame = frame.park().expect("park");
            assert_eq!(
                shared.frames_dropped.load(Ordering::SeqCst),
                0,
                "control: the frame is still held"
            );
            drop(parked);
            assert_eq!(
                shared.frames_dropped.load(Ordering::SeqCst),
                0,
                "not inside the sink: its argument still borrows the bytes"
            );
            Flow::Continue
        })
        .unwrap();
    assert_eq!(
        shared.frames_dropped.load(Ordering::SeqCst),
        1,
        "freed exactly once, after the sink and before receive returned"
    );
    assert_eq!(handed, 1);
    assert_eq!(first_payload_byte, 1);
    assert!(metadata_len > 0);
}

/// B2. A `Call` can be moved to another thread and completed there; the data race
/// needs a shared call, which `&mut self` rules out (compile-fail in `tests/ui`).
#[test]
fn b2_a_call_moved_to_another_thread_completes_there() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
    let local = Uri(2);
    let method = uri(3, 1);
    let client = RpcClient::<u32, u32>::on(&setup, &local, method.clone())
        .wire(wire())
        .max_in_flight(1)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let client = client.activate(&runtime).unwrap();
    let mut call = client.invoke(&5).expect("invoke");
    let request = shared.sent.lock().unwrap()[0].clone();
    let response = whole_frame(&Spec {
        kind: UMessageType::Response,
        id: uuid_at(WALL, 2),
        reqid: Some(sent_request_id(&request)),
        ttl: None,
        source: method,
        sink: Some(uri(2, 0)),
        token: None,
        payload: Some(&77u32.to_le_bytes()),
    });
    deliver(shared, &response, peer(9, 1)).unwrap();
    // Two threads may *hold* a `Call` in turn; one thread at a time touches it.
    let moved = std::thread::spawn(move || {
        let reply = call
            .try_response()
            .expect("no failure")
            .expect("the response");
        let value = *reply;
        assert!(matches!(
            call.try_response(),
            Err(up_rust_core::transport::CallError::AlreadyTaken)
        ));
        value
    });
    assert_eq!(moved.join().unwrap(), 77);
}

/// H2. A server can raise its table and serves that many declared clients.
#[test]
fn h2_a_server_can_raise_its_peer_table() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
    let local = Uri(3);
    let method = uri(3, 1);
    let server = RpcServer::<u32, u32>::on(&setup, &local, 1_u16)
        .wire(wire())
        .max_peers(16)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let _server = server.activate(&runtime).unwrap();
    for n in 0..16_u64 {
        let source = uri(200 + n as u16, 0);
        let frame = request_frame(&source, &method, n);
        assert_eq!(deliver(shared, &frame, peer(300 + n, 1)), Ok(()));
    }
    let extra = request_frame(&uri(250, 0), &method, 99);
    assert_eq!(
        deliver(shared, &extra, peer(400, 1)),
        Err(up_rust_core::transport::ReceiveError::PeerTableFull),
        "control: the raised table holds 16"
    );
}

/// H2. Asking for fewer peers than the declared clients is a refused configuration.
#[test]
fn h2_a_table_smaller_than_the_client_list_is_refused() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let memory = memory();
    let setup = TransportSetup::handoff_on(&memory, clock, HandoffDouble(shared)).unwrap();
    let local = Uri(3);
    let clients: Vec<UUri> = (0..3_u16).map(|n| uri(200 + n, 0)).collect();
    let refused = RpcServer::<u32, u32>::on(&setup, &local, 1_u16)
        .wire(wire())
        .clients(&clients)
        .max_peers(2)
        .build();
    assert!(
        matches!(refused, Err(StartupError::InvalidConfig(_))),
        "two entries cannot hold three declared clients: {refused:?}"
    );
}

/// H2. A binding that cannot serve the requested peers refuses the route at setup.
#[test]
fn h2_a_binding_that_cannot_honour_the_size_refuses_at_setup() {
    struct LimitedBinding {
        limit: usize,
        seen: &'static std::sync::Mutex<Vec<usize>>,
    }
    struct LimitedRunning;
    impl BindingSetup for LimitedBinding {
        type Running = LimitedRunning;
        fn name(&self) -> &'static str {
            "limited"
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
            self.seen.lock().unwrap().push(route.max_peers);
            if route.max_peers > self.limit {
                return Err(BindingRefusal("this binding serves fewer peers"));
            }
            Ok(PreparedRoute::new(BindingRoute(0), RuntimeHeap::None))
        }
        fn into_running(self, _: RuntimePort) -> Result<LimitedRunning, BindingRefusal> {
            Ok(LimitedRunning)
        }
    }
    impl up_rust_core::transport::binding::HandoffRuntime for LimitedRunning {
    fn receive_bounded(
        &self, route: up_rust::two_mode::BindingRoute, budget: &up_rust::two_mode::ReceiveBudget,
        sink: &mut dyn FnMut(up_rust::two_mode::InboundFrame<'_>) -> up_rust::two_mode::Flow,
    ) -> usize {
        // Explicitly selected sink-visible adapter; no hidden filtering loop.
        if !budget.try_take() { return 0; }
        self.receive(route, &mut |frame| {
            let flow = sink(frame);
            if matches!(flow, up_rust::two_mode::Flow::Stop) || !budget.try_take() {
                up_rust::two_mode::Flow::Stop
            } else { up_rust::two_mode::Flow::Continue }
        })
    }

        fn send(
            &self,
            _: BindingRoute,
            _: up_rust_core::transport::binding::OutgoingFrame<'_>,
        ) -> Result<up_rust_core::transport::SendReport, up_rust_core::transport::HandoffFailure> {
            Ok(up_rust_core::transport::SendReport::unknown())
        }
    }
    let clock = Clock::leak(1_000_000_000, WALL);
    let memory = memory();
    let seen: &'static std::sync::Mutex<Vec<usize>> =
        Box::leak(Box::new(std::sync::Mutex::new(Vec::new())));
    let setup =
        TransportSetup::handoff_on(&memory, clock, LimitedBinding { limit: 4, seen }).unwrap();
    let local = Uri(3);
    let clients: Vec<UUri> = (0..5_u16).map(|n| uri(200 + n, 0)).collect();
    let refused = RpcServer::<u32, u32>::on(&setup, &local, 1_u16)
        .wire(wire())
        .clients(&clients)
        .build();
    assert!(
        matches!(
            refused,
            Err(StartupError::Binding {
                reason: "this binding serves fewer peers",
                ..
            })
        ),
        "{refused:?}"
    );
    assert_eq!(
        *seen.lock().unwrap(),
        vec![5],
        "the declaration carries the table size: the five declared clients"
    );
}

/// L3 (exact refusal).
#[test]
fn l3_a_zero_deadline_is_refused_as_the_deadline_setting() {
    let qos = Qos::DEFAULT.with_deadline(Duration::ZERO);
    let refused = check_qos(&qos, &all_qos()).unwrap_err();
    assert_eq!(refused.setting, QosSetting::Deadline);
}
