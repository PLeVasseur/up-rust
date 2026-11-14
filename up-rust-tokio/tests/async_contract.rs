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

//! The async layer over the in-memory bindings: wakes, deadlines, concurrent calls
//! and the promise that nothing stays registered after a future drops.

use std::future::Future;
use std::pin::{pin, Pin};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

use up_rust::startup::LEAKED_HEAP_REGIONS;
use up_rust::test_util::handoff::InMemoryBus;
use up_rust::test_util::lending::LendingBus;
use up_rust::test_util::wires::{DoorState, DoorWire};
use up_rust::two_mode::*;
use up_rust::{
    NativeProfile, NativeProfileAgreement, PayloadEncoding, StablePayloadExt, StaticUriProvider,
    UEntityId, UUri,
};
use up_rust_tokio::{service_loop, AsyncCall, AsyncCallInPlace, AsyncReceive};

const METHOD: u16 = 0x0042;

fn entity(authority: &str, type_id: u16) -> StaticUriProvider {
    StaticUriProvider::for_entity(authority, UEntityId::first_instance(type_id), 1).unwrap()
}

fn memory() -> StartupMemory {
    StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 64 * 1024).unwrap()
}

fn door(id: u32) -> DoorState {
    DoorState { id, open: 0 }
}

fn topic() -> UUri {
    entity("door", 1).get_resource_uri(0x8001)
}

struct Roles {
    publisher: Publisher<DoorState>,
    subscriber: Subscriber<DoorState>,
    client: RpcClient<DoorState, DoorState>,
    server: RpcServer<DoorState, DoorState>,
    client_rt: TransportRuntime,
}

fn roles_on(a: TransportSetup, b: TransportSetup, ttl: Duration, in_flight: usize) -> Roles {
    let method = entity("door", 2).get_resource_uri(METHOD);
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .unwrap();
    let subscriber = Subscriber::<DoorState>::on(&b, topic())
        .wire(DoorWire)
        .build()
        .unwrap();
    let client = RpcClient::<DoorState, DoorState>::on(&a, &entity("door", 1), method)
        .wire(DoorWire)
        .ttl(ttl)
        .max_in_flight(in_flight)
        .build()
        .unwrap();
    let server = RpcServer::<DoorState, DoorState>::on(&b, &entity("door", 2), METHOD)
        .wire(DoorWire)
        .qos(Qos::DEFAULT.with_depth(64))
        .max_held(32)
        .clients(std::slice::from_ref(&entity("door", 1).get_source_uri()))
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    Roles {
        publisher: publisher.activate(&tx).unwrap(),
        subscriber: subscriber.activate(&rx).unwrap(),
        client: client.activate(&tx).unwrap(),
        server: server.activate(&rx).unwrap(),
        client_rt: tx,
    }
}

fn handoff_roles(ttl: Duration, in_flight: usize) -> Roles {
    let bus = InMemoryBus::new(256, 4096);
    roles_on(
        TransportSetup::handoff_in(&memory(), bus.participant(1)).unwrap(),
        TransportSetup::handoff_in(&memory(), bus.participant(2)).unwrap(),
        ttl,
        in_flight,
    )
}

fn lending_roles(ttl: Duration, in_flight: usize) -> Roles {
    let bus = LendingBus::new();
    roles_on(
        TransportSetup::lending_in(&memory(), bus.participant(1)).unwrap(),
        TransportSetup::lending_in(&memory(), bus.participant(2)).unwrap(),
        ttl,
        in_flight,
    )
}

const TTL: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Default, up_rust::StablePayload, up_rust::StablePayloadInit)]
#[repr(C)]
#[stable_payload(type_name = "uprotocol.test.NativeDoor")]
struct NativeDoor {
    id: u32,
    open: u32,
}

/// The future of a native call ends with the response in transport memory, read
/// through its checked borrow.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_native_call_future_completes_in_place() {
    let profile = NativeProfile::new(
        "test",
        1,
        PayloadEncoding::private_use(0xF777),
        [NativeDoor::native_representation()],
    )
    .unwrap();
    let agreement = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
    let agreement = native_agreement_table(&agreement);
    let bus = LendingBus::new();
    let a = TransportSetup::lending_in(&memory(), bus.participant(1)).unwrap();
    let b = TransportSetup::lending_in(&memory(), bus.participant(2)).unwrap();
    let method = entity("door", 2).get_resource_uri(METHOD);
    let client = RpcClient::<NativeDoor, NativeDoor>::on(&a, &entity("door", 1), method)
        .wire(NativeWire::new(agreement))
        .ttl(TTL)
        .build()
        .unwrap();
    let server = RpcServer::<NativeDoor, NativeDoor>::on(&b, &entity("door", 2), METHOD)
        .wire(NativeWire::new(agreement))
        .clients(std::slice::from_ref(&entity("door", 1).get_source_uri()))
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let client: RpcClient<NativeDoor, NativeDoor, InPlace> = client.activate(&tx).unwrap();
    let server: RpcServer<NativeDoor, NativeDoor, InPlace> = server.activate(&rx).unwrap();
    let serving = tokio::spawn(async move {
        let request = server.recv().await.unwrap();
        let asked = request.payload().unwrap().id;
        request
            .respond_in_place(|init| init.into_initializer().id(asked + 1).open(0).finish())
            .unwrap();
    });
    let response = client
        .call_in_place(|init| init.into_initializer().id(3).open(1).finish())
        .await
        .unwrap();
    assert_eq!(response.payload().unwrap().id, 4);
    serving.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_receive_future_wakes_when_a_frame_arrives() {
    for roles in [handoff_roles(TTL, 4), lending_roles(TTL, 4)] {
        let subscriber = roles.subscriber;
        let waiter = tokio::spawn(async move {
            let frame = subscriber.recv().await.unwrap();
            frame.id
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        roles.publisher.publish(&door(7)).unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), waiter)
                .await
                .unwrap()
                .unwrap(),
            7
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_future_completes_by_response() {
    for roles in [handoff_roles(TTL, 4), lending_roles(TTL, 4)] {
        let server = roles.server;
        let serving = tokio::spawn(async move {
            let request = server.recv().await.unwrap();
            let id = request.id;
            request.respond(&door(id + 1)).unwrap();
        });
        let reply = roles.client.call(&door(41)).await.unwrap();
        assert_eq!(reply.id, 42);
        serving.await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_future_completes_by_the_service_tick() {
    let roles = handoff_roles(Duration::from_millis(5), 4);
    let ticking = tokio::spawn(service_loop(roles.client_rt, Duration::from_millis(1)));
    let outcome = tokio::time::timeout(Duration::from_secs(5), roles.client.call(&door(1)))
        .await
        .expect("the tick must complete the call");
    assert!(matches!(outcome, Err(CallError::TimedOut)));
    ticking.abort();
}

#[tokio::test]
async fn a_call_future_ends_at_its_deadline_with_no_service_tick() {
    let roles = handoff_roles(Duration::from_millis(5), 4);
    let outcome = tokio::time::timeout(Duration::from_secs(5), roles.client.call(&door(1)))
        .await
        .expect("the timer must complete the call");
    assert!(matches!(outcome, Err(CallError::TimedOut)));
    // The slot came back when the future finished.
    let _ = roles.client.invoke(&door(2)).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_calls_each_get_their_own_response() {
    // Under Miri a run is thousands of times slower.
    let (calls, ttl) = if cfg!(miri) {
        (8_u32, Duration::from_secs(900))
    } else {
        (64_u32, TTL)
    };
    for roles in [handoff_roles(ttl, 16), lending_roles(ttl, 16)] {
        let started = std::time::Instant::now();
        let server = roles.server;
        let serving = tokio::spawn(async move {
            for _ in 0..calls {
                let request = server.recv().await.unwrap();
                let id = request.id;
                request.respond(&door(id + 1000)).unwrap();
            }
        });
        let client = roles.client;
        let mut tasks = Vec::new();
        for id in 0..calls {
            tasks.push(tokio::spawn(async move {
                loop {
                    match client.call(&door(id)).await {
                        Ok(reply) => return (id, reply.id),
                        // More tasks than slots: a real caller would apply back-pressure.
                        Err(CallError::SlotsExhausted) => tokio::task::yield_now().await,
                        Err(error) => panic!("call {id}: {error}"),
                    }
                }
            }));
        }
        for task in tasks {
            let (id, reply) = task.await.unwrap();
            assert_eq!(reply, id + 1000);
        }
        serving.await.unwrap();
        // A lost wake is seen as a call that ends only when its deadline passes.
        assert!(
            cfg!(miri) || started.elapsed() < ttl,
            "every call completed by a wake, not by its deadline"
        );
    }
}

struct Counting {
    wakes: AtomicUsize,
}

impl Wake for Counting {
    fn wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

fn counting() -> (Arc<Counting>, Waker) {
    let state = Arc::new(Counting {
        wakes: AtomicUsize::new(0),
    });
    (state.clone(), Waker::from(state))
}

/// A future dropped after being polled leaves nothing behind (design: cleared on drop).
#[test]
fn a_dropped_receive_future_leaves_no_waker_registered() {
    let roles = handoff_roles(TTL, 4);
    let (state, waker) = counting();
    let mut context = Context::from_waker(&waker);
    {
        let mut future = pin!(roles.subscriber.recv());
        assert!(future.as_mut().poll(&mut context).is_pending());
        assert_eq!(
            Arc::strong_count(&state),
            3,
            "registered: test, waker, route cell"
        );
    }
    assert_eq!(
        Arc::strong_count(&state),
        2,
        "the route cell holds none now"
    );
    roles.publisher.publish(&door(1)).unwrap();
    assert_eq!(state.wakes.load(Ordering::SeqCst), 0, "no stale wake");
    drop(waker);
    assert_eq!(Arc::strong_count(&state), 1);
}

#[test]
fn a_dropped_call_future_clears_its_waker_and_slot() {
    let roles = handoff_roles(TTL, 1);
    let (state, waker) = counting();
    let mut context = Context::from_waker(&waker);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let _guard = runtime.enter();
    let request = door(1);
    {
        let mut future = pin!(roles.client.call(&request));
        assert!(future.as_mut().poll(&mut context).is_pending());
        assert!(
            Arc::strong_count(&state) >= 3,
            "registered with the call slot"
        );
    }
    assert!(matches!(
        roles.client.invoke(&door(2)),
        Ok(_) | Err(CallError::SlotsExhausted)
    ));
    assert_eq!(Arc::strong_count(&state), 2, "the call slot holds none now");
    // The slot came back: a new call can use the only slot.
    let call = roles.client.invoke(&door(3)).expect("slot was returned");
    drop(call);
}

/// A completion wakes only the task currently registered.
#[test]
fn a_new_registration_replaces_the_previous_waker() {
    let roles = handoff_roles(TTL, 4);
    let (first, first_waker) = counting();
    let (second, second_waker) = counting();
    let mut future = roles.subscriber.recv();
    let mut future = Pin::new(&mut future);
    assert!(matches!(
        future.as_mut().poll(&mut Context::from_waker(&first_waker)),
        Poll::Pending
    ));
    assert!(matches!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(&second_waker)),
        Poll::Pending
    ));
    roles.publisher.publish(&door(1)).unwrap();
    assert_eq!(first.wakes.load(Ordering::SeqCst), 0, "replaced");
    assert_eq!(second.wakes.load(Ordering::SeqCst), 1);
}
