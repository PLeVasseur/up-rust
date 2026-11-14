// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! SDK-level async seam contract over encoded and genuinely loaned native fixtures.
#![cfg(feature = "test-util")]
use std::{
    alloc::System,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};
use up_rust::heap_guard::{observe, HeapGuard, Scope};
use up_rust::test_util::{
    lending::LendingBus,
    wires::{DoorState, DoorWire},
};
use up_rust::transport::wake::{RouteWake, WakeEvent};
use up_rust::two_mode::*;
use up_rust::{
    NativeProfile, NativeProfileAgreement, PayloadEncoding, StablePayloadExt, StaticUriProvider,
    UEntityId,
};
#[global_allocator]
static HEAP: HeapGuard = HeapGuard::new(System);
static WAKE: AtomicU32 = AtomicU32::new(0);
static CLEAR: AtomicU32 = AtomicU32::new(0);
fn hook(_: &RouteWake, event: WakeEvent) {
    match event {
        WakeEvent::Wake => {
            WAKE.fetch_add(1, Ordering::Relaxed);
        }
        WakeEvent::Clear => {
            CLEAR.fetch_add(1, Ordering::Relaxed);
        }
    }
}
fn entity(id: u16) -> StaticUriProvider {
    StaticUriProvider::for_entity("local", UEntityId::first_instance(id), 1).unwrap()
}
fn pair() -> (TransportSetup, TransportSetup) {
    let bus = LendingBus::new();
    (
        TransportSetup::lending(bus.participant(1)).unwrap(),
        TransportSetup::lending(bus.participant(2)).unwrap(),
    )
}
fn heap_free(body: impl FnOnce()) {
    let (_, report) = observe(Scope::AllThreads, body);
    let counts = report.counts();
    assert_eq!(
        (
            counts.allocations,
            counts.deallocations,
            counts.reallocations
        ),
        (0, 0, 0)
    );
}
// [utest->req~ssr-async-seam~1] method: requirements-based test, heap observation
#[test]
fn first4a1_encoded_declared_client_seam_borrows_until_drop_and_clears() {
    let (a, b) = pair();
    let client =
        RpcClient::<DoorState, DoorState>::on(&a, &entity(1), entity(2).get_resource_uri(1))
            .wire(DoorWire)
            .ttl(Duration::from_secs(30))
            .build()
            .unwrap();
    let known = entity(1).get_source_uri();
    let server = RpcServer::<DoorState, DoorState>::on(&b, &entity(2), 1)
        .wire(DoorWire)
        .clients(std::slice::from_ref(&known))
        .build()
        .unwrap();
    let (a, b) = (a.enter_runtime().unwrap(), b.enter_runtime().unwrap());
    let (client, server) = (client.activate(&a).unwrap(), server.activate(&b).unwrap());
    WAKE.store(0, Ordering::Relaxed);
    CLEAR.store(0, Ordering::Relaxed);
    heap_free(|| {
        assert_eq!(client.__ttl(), Duration::from_secs(30));
        let mut call = client.invoke(&DoorState { id: 4, open: 0 }).unwrap();
        let cell = call.__wake().unwrap();
        cell.install_hook(hook);
        assert!(call.__taken_value().is_none());
        assert!(call.try_response().unwrap().is_none());
        let receive = server.__route_wake();
        receive.install_hook(hook);
        let request = server.try_recv().unwrap().unwrap();
        request.respond(&DoorState { id: 5, open: 1 }).unwrap();
        {
            let reply = call.try_response().unwrap().unwrap();
            assert_eq!(reply.id, 5);
        }
        assert_eq!(call.__taken_value().unwrap().id, 5);
        drop(call);
        assert!(CLEAR.load(Ordering::Relaxed) > 0);
        receive.clear();
        // A cancelled/reused slot does not expose an earlier response.
        let next = client.invoke(&DoorState { id: 6, open: 0 }).unwrap();
        assert!(next.__taken_value().is_none());
        drop(next);
    });
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, up_rust::StablePayload, up_rust::StablePayloadInit)]
#[stable_payload(type_name = "uprotocol.test.First4A1Native")]
struct Native {
    id: u32,
    value: u32,
}
// [utest->req~ssr-async-seam~1] method: requirements-based test, heap observation
#[test]
fn first4a1_native_call_seam_retains_checked_payload() {
    let profile = NativeProfile::new(
        "first4a1",
        1,
        PayloadEncoding::private_use(0xf777),
        [Native::native_representation()],
    )
    .unwrap();
    let agreement = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
    let table = native_agreement_table(&agreement);
    let (a, b) = pair();
    let known = entity(1).get_source_uri();
    let client = RpcClient::<Native, Native>::on(&a, &entity(1), entity(2).get_resource_uri(1))
        .wire(NativeWire::new(table))
        .ttl(Duration::from_secs(30))
        .build()
        .unwrap();
    let server = RpcServer::<Native, Native>::on(&b, &entity(2), 1)
        .wire(NativeWire::new(table))
        .clients(std::slice::from_ref(&known))
        .build()
        .unwrap();
    let (a, b) = (a.enter_runtime().unwrap(), b.enter_runtime().unwrap());
    let (client, server) = (client.activate(&a).unwrap(), server.activate(&b).unwrap());
    heap_free(|| {
        let mut call = client
            .invoke_in_place(|slot| slot.into_initializer().id(10).value(20).finish())
            .unwrap();
        call.__wake().unwrap().install_hook(hook);
        assert!(call.try_response().unwrap().is_none());
        let request = server.try_recv().unwrap().unwrap();
        assert_eq!(request.payload().unwrap().id, 10);
        request
            .respond_in_place(|slot| slot.into_initializer().id(11).value(21).finish())
            .unwrap();
        {
            let reply = call.try_response().unwrap().unwrap();
            assert_eq!(reply.payload().unwrap().id, 11);
        }
        assert_eq!(call.__taken_payload().unwrap().value, 21);
        drop(call);
    });
}

// V7: the SDK public URI path is the same value type as core, with its full model.
#[test]
fn first4a1_sdk_uri_model_spelling() {
    let uri = up_rust::UUri::try_from_wire_parts("local", 0x1234_5678, 2, 0x8001).unwrap();
    assert_eq!(uri.authority_name(), "local");
    assert_eq!(
        (
            uri.ue_type_id(),
            uri.ue_instance_id(),
            uri.ue_version_major(),
            uri.resource_id()
        ),
        (0x5678, 0x1234, 2, 0x8001)
    );
}

// Regression for the preserved sdk-targeted-02 dynamic response failure.
// [utest->req~ssr-async-seam~1] method: requirements-based test, heap observation
#[test]
fn first4a1_encoded_dynamic_client_seam_borrows_until_drop_and_clears() {
    let (a, b) = pair();
    let client =
        RpcClient::<DoorState, DoorState>::on(&a, &entity(1), entity(2).get_resource_uri(1))
            .wire(DoorWire)
            .ttl(Duration::from_secs(30))
            .build()
            .unwrap();
    let server = RpcServer::<DoorState, DoorState>::on(&b, &entity(2), 1)
        .wire(DoorWire)
        .build()
        .unwrap();
    let (a, b) = (a.enter_runtime().unwrap(), b.enter_runtime().unwrap());
    let (client, server) = (client.activate(&a).unwrap(), server.activate(&b).unwrap());
    WAKE.store(0, Ordering::Relaxed);
    CLEAR.store(0, Ordering::Relaxed);
    heap_free(|| {
        assert_eq!(client.__ttl(), Duration::from_secs(30));
        let mut call = client.invoke(&DoorState { id: 4, open: 0 }).unwrap();
        let cell = call.__wake().unwrap();
        cell.install_hook(hook);
        assert!(call.__taken_value().is_none());
        assert!(call.try_response().unwrap().is_none());
        let receive = server.__route_wake();
        receive.install_hook(hook);
        let request = server.try_recv().unwrap().unwrap();
        request.respond(&DoorState { id: 5, open: 1 }).unwrap();
        {
            let reply = call.try_response().unwrap().unwrap();
            assert_eq!(reply.id, 5);
        }
        assert_eq!(call.__taken_value().unwrap().id, 5);
        drop(call);
        assert!(CLEAR.load(Ordering::Relaxed) > 0);
        receive.clear();
        // A cancelled/reused slot does not expose an earlier response.
        let next = client.invoke(&DoorState { id: 6, open: 0 }).unwrap();
        assert!(next.__taken_value().is_none());
        drop(next);
    });
}
