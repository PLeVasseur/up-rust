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

//! Frames returned through the async layer are released exactly when their value
//! is dropped, and their storage is then reused. The in-memory lending bus has one
//! loan slot per send route here, so the next message is written into the bytes a
//! dropped frame had: a release that came early would be an overwrite under a
//! live borrow, which Miri reports, and one that never came fails the next send.

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Waker};
use std::time::Duration;

use up_rust::startup::LEAKED_HEAP_REGIONS;
use up_rust::test_util::lending::{LendingBus, LendingOptions};
use up_rust::two_mode::*;
use up_rust::{
    NativeProfile, NativeProfileAgreement, PayloadEncoding, StablePayloadExt, StaticUriProvider,
    UEntityId,
};
use up_rust_tokio::{AsyncCallInPlace, AsyncReceive};

const METHOD: u16 = 0x0042;
const TTL: Duration = Duration::from_secs(600);

#[derive(Clone, Copy, Debug, Default, up_rust::StablePayload, up_rust::StablePayloadInit)]
#[repr(C)]
#[stable_payload(type_name = "uprotocol.test.AsyncReleaseDoor")]
struct AsyncReleaseDoor {
    id: u32,
    open: u32,
}

fn entity(authority: &str, type_id: u16) -> StaticUriProvider {
    StaticUriProvider::for_entity(authority, UEntityId::first_instance(type_id), 1).unwrap()
}

fn agreement() -> &'static NativeAgreement {
    let profile = NativeProfile::new(
        "test",
        1,
        PayloadEncoding::private_use(0xF77A),
        [AsyncReleaseDoor::native_representation()],
    )
    .unwrap();
    let agreement = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
    native_agreement_table(&agreement)
}

/// A participant with one loan slot per send route.
fn one_slot(bus: LendingBus, peer: u64) -> TransportSetup {
    let memory = StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 64 * 1024).unwrap();
    let options = LendingOptions {
        slots: 1,
        ..LendingOptions::default()
    };
    TransportSetup::lending_in(&memory, bus.participant(peer).with_options(options)).unwrap()
}

type Client = RpcClient<AsyncReleaseDoor, AsyncReleaseDoor, InPlace>;
type Server = RpcServer<AsyncReleaseDoor, AsyncReleaseDoor, InPlace>;

/// Native RPC with one loan slot on each send route: `max_in_flight(1)` on the
/// request route and `slots(1)` on the response route. Also the client's runtime.
fn one_slot_rpc(bus: LendingBus) -> (Client, Server, TransportRuntime) {
    let agreement = agreement();
    let a = one_slot(bus, 1);
    let b = one_slot(bus, 2);
    let method = entity("door", 2).get_resource_uri(METHOD);
    let client =
        RpcClient::<AsyncReleaseDoor, AsyncReleaseDoor>::on(&a, &entity("door", 1), method)
            .wire(NativeWire::new(agreement))
            .ttl(TTL)
            .max_in_flight(1)
            .build()
            .unwrap();
    let server =
        RpcServer::<AsyncReleaseDoor, AsyncReleaseDoor>::on(&b, &entity("door", 2), METHOD)
            .wire(NativeWire::new(agreement))
            .clients(std::slice::from_ref(&entity("door", 1).get_source_uri()))
            .slots(1)
            .build()
            .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    (
        client.activate(&tx).unwrap(),
        server.activate(&rx).unwrap(),
        tx,
    )
}

/// An awaited native response, read in place and dropped, gives its loan back:
/// the second round's response is written into the same slot.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_awaited_native_response_dropped_has_its_bytes_reused_by_the_next_call() {
    let bus = LendingBus::new();
    let (client, server, _client_runtime) = one_slot_rpc(bus);
    let serving = tokio::spawn(async move {
        for _ in 0..2 {
            let request = server.recv().await.unwrap();
            let asked = request.payload().unwrap().id;
            // With one response slot this fails unless the last response came back.
            request
                .respond_in_place(|init| init.into_initializer().id(asked + 100).open(0).finish())
                .unwrap();
        }
    });
    for round in 1..=2_u32 {
        let response = client
            .call_in_place(|init| init.into_initializer().id(round).open(1).finish())
            .await
            .unwrap();
        assert_eq!(response.payload().unwrap().id, round + 100);
        drop(response);
    }
    serving.await.unwrap();
    assert!(bus.frame_drops() >= 4, "every frame went back to the bus");
}

/// A frame awaited from `recv` and dropped in the task gives its loan back to the
/// one-slot publisher, whose next message is written into the same bytes.
#[tokio::test]
async fn an_awaited_native_frame_dropped_in_the_task_has_its_bytes_reused() {
    let bus = LendingBus::new();
    let agreement = agreement();
    let a = one_slot(bus, 1);
    let b = one_slot(bus, 2);
    let topic = entity("door", 1).get_resource_uri(0x8001);
    let publisher = Publisher::<AsyncReleaseDoor>::on(&a, &entity("door", 1), 0x8001)
        .wire(NativeWire::new(agreement))
        .slots(1)
        .build()
        .unwrap();
    let subscriber = Subscriber::<AsyncReleaseDoor>::on(&b, topic)
        .wire(NativeWire::new(agreement))
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let publisher: Publisher<AsyncReleaseDoor, InPlace> = publisher.activate(&tx).unwrap();
    let subscriber: Subscriber<AsyncReleaseDoor, InPlace> = subscriber.activate(&rx).unwrap();
    for id in 1..=2_u32 {
        publisher
            .publish_in_place(|init| init.into_initializer().id(id).open(1).finish())
            .expect("the one loan slot is free");
        let frame = subscriber.recv().await.unwrap();
        assert!(
            publisher
                .publish_in_place(|init| init.into_initializer().id(99).open(9).finish())
                .is_err(),
            "control: the lease holds the one loan slot"
        );
        assert_eq!(frame.payload().unwrap().id, id);
        drop(frame);
    }
    assert_eq!(bus.frame_drops(), 2);
}

/// A call future dropped after its response was delivered to its call, without
/// being polled again, returns the response: the server's one response slot then
/// serves the next call. (On this pull binding a response stays in the binding's
/// queue until the client's transport pumps, so the test pumps before the drop.)
#[test]
fn a_cancelled_call_future_returns_its_delivered_response() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let _inside = runtime.enter();
    let bus = LendingBus::new();
    let (client, server, client_runtime) = one_slot_rpc(bus);
    let mut context = Context::from_waker(Waker::noop());
    let delivered;
    {
        let mut future =
            pin!(client.call_in_place(|init| init.into_initializer().id(1).open(1).finish()));
        assert!(
            future.as_mut().poll(&mut context).is_pending(),
            "request sent"
        );
        let request = server.try_recv().unwrap().expect("the request");
        request
            .respond_in_place(|init| init.into_initializer().id(101).open(0).finish())
            .unwrap();
        // The response reaches the call, which nobody polls again. One `pump` is
        // the whole drain (N3): it takes the one response the binding holds.
        assert_eq!(client_runtime.pump(), 1, "the response was delivered");
        delivered = bus.frame_drops();
    }
    assert_eq!(
        bus.frame_drops(),
        delivered + 1,
        "dropping the future dropped its call, which returned the response"
    );
    let mut call = client
        .invoke_in_place(|init| init.into_initializer().id(2).open(1).finish())
        .expect("the call slot came back");
    let request = server.try_recv().unwrap().expect("the second request");
    request
        .respond_in_place(|init| init.into_initializer().id(102).open(0).finish())
        .expect("the response slot came back");
    let reply = call.try_response().unwrap().expect("the response");
    assert_eq!(reply.payload().unwrap().id, 102);
}
