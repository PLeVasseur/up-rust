// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Real SDK RPC paths over the in-process lending fixture; no native claim.
#![cfg(feature = "test-util")]
use std::{alloc::System, time::Duration};
use up_rust::heap_guard::{observe, HeapGuard, Scope};
use up_rust::test_util::{
    lending::LendingBus,
    wires::{DoorState, DoorWire},
};
use up_rust::two_mode::*;
use up_rust::{StaticUriProvider, UCode, UEntityId, UUri};
use up_rust_core::wire::{
    EncodedSize, PayloadWriter, Ready, StartupSlot, Wire, WireError, WireIdentity, WireInfo,
};
#[global_allocator]
static HEAP: HeapGuard = HeapGuard::new(System);
fn entity(id: u16) -> StaticUriProvider {
    StaticUriProvider::for_entity("local", UEntityId::first_instance(id), 1).unwrap()
}
fn heap_free(body: impl FnOnce()) {
    let (_, report) = observe(Scope::AllThreads, body);
    let c = report.counts();
    assert_eq!((c.allocations, c.reallocations, c.deallocations), (0, 0, 0));
}
#[derive(Clone, Copy)]
struct TestWire(bool);
impl Wire<DoorState> for TestWire {
    fn describe(&self) -> WireInfo {
        let mut info = DoorWire.describe();
        if self.0 {
            info.wire_id = WireIdentity::new("test.literal-response", 0);
            info.payload_family = WireIdentity::new("test.literal-door-family", 0);
        }
        info
    }
    fn encoded_size(&self, v: &DoorState) -> EncodedSize {
        DoorWire.encoded_size(v)
    }
    fn encode(&self, v: &DoorState, o: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        DoorWire.encode(v, o)
    }
    fn init_slot<'s>(&self, s: StartupSlot<'s, DoorState>) -> Ready<'s, DoorState> {
        DoorWire.init_slot(s)
    }
    fn decode(
        &self,
        e: up_rust::PayloadEncoding,
        b: &[u8],
        v: &mut DoorState,
    ) -> Result<(), WireError> {
        DoorWire.decode(e, b, v)
    }
}
// [utest->req~ssr-peer-rpc-correlation~1] method: focused fixture regression; partial evidence, no G2 credit
// [utest->req~ssr-heap-none~1] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn first4a1_two_independent_clients_dynamic_wildcard_exact_compact_literal_and_status() {
    for literal in [false, true] {
        for filter in 0..3 {
            let bus = LendingBus::new();
            let a = TransportSetup::lending(bus.participant(1)).unwrap();
            let b = TransportSetup::lending(bus.participant(2)).unwrap();
            let s = TransportSetup::lending(bus.participant(3)).unwrap();
            let wire = TestWire(literal);
            let ca = RpcClient::<DoorState, DoorState>::on(
                &a,
                &entity(1),
                entity(3).get_resource_uri(1),
            )
            .wire(wire)
            .ttl(Duration::from_secs(30))
            .build()
            .unwrap();
            let cb = RpcClient::<DoorState, DoorState>::on(
                &b,
                &entity(2),
                entity(3).get_resource_uri(1),
            )
            .wire(wire)
            .ttl(Duration::from_secs(30))
            .build()
            .unwrap();
            let exact = [entity(1).get_source_uri(), entity(2).get_source_uri()];
            let any = [UUri::any()];
            let clients: &[UUri] = match filter {
                0 => &[],
                1 => &any,
                _ => &exact,
            };
            let server = RpcServer::<DoorState, DoorState>::on(&s, &entity(3), 1)
                .wire(wire)
                .clients(clients)
                .build()
                .unwrap();
            let (a, b, s) = (
                a.enter_runtime().unwrap(),
                b.enter_runtime().unwrap(),
                s.enter_runtime().unwrap(),
            );
            let (ca, cb, server) = (
                ca.activate(&a).unwrap(),
                cb.activate(&b).unwrap(),
                server.activate(&s).unwrap(),
            );
            heap_free(|| {
                let mut ac = ca.invoke(&DoorState { id: 1, open: 0 }).unwrap();
                let mut bc = cb.invoke(&DoorState { id: 2, open: 0 }).unwrap();
                let ar = server.try_recv().unwrap().unwrap();
                let br = server.try_recv().unwrap().unwrap();
                assert_eq!(ar.id, 1);
                assert_eq!(br.id, 2);
                // Reply out of order; no delivery to A until its own response.
                br.respond(&DoorState { id: 22, open: 1 }).unwrap();
                assert!(ac.try_response().unwrap().is_none());
                assert_eq!(bc.try_response().unwrap().unwrap().id, 22);
                ar.respond_failure(UCode::Unavailable).unwrap();
                assert!(matches!(
                    ac.try_response(),
                    Err(CallError::Rejected(UCode::Unavailable))
                ));
                drop(ac);
                drop(bc);
            });
        }
    }
}
// [utest->req~ssr-peer-rpc-correlation~1] method: focused fixture regression; partial evidence, no G2 credit
// [utest->req~ssr-heap-none~1] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn first4a1_unrelated_full_wait_queue_does_not_delay_dynamic_response() {
    use up_rust_core::transport::{CarriedWire, CarrierRoute, Overflow, Qos};
    let bus = LendingBus::new();
    let a = TransportSetup::lending(bus.participant(1)).unwrap();
    let b = TransportSetup::lending(bus.participant(2)).unwrap();
    let s = TransportSetup::lending(bus.participant(3)).unwrap();
    let tap = TransportSetup::lending(bus.participant(4)).unwrap();
    let method = entity(3).get_resource_uri(1);
    let sink = entity(2).get_source_uri();
    let wires = [CarriedWire::of(&DoorWire.describe())];
    let mut route = CarrierRoute::new(&method, &wires, 5);
    route.sink_filter = Some(&sink);
    route.kind = Some(up_rust::UMessageType::Response);
    route.qos = Qos::DEFAULT.with_depth(1).with_overflow(Overflow::Wait {
        timeout: Duration::from_millis(1),
    });
    let tap_route = tap.carrier_ingress(&route).unwrap();
    let ca = RpcClient::<DoorState, DoorState>::on(&a, &entity(1), method.clone())
        .wire(DoorWire)
        .build()
        .unwrap();
    let cb = RpcClient::<DoorState, DoorState>::on(&b, &entity(2), method)
        .wire(DoorWire)
        .build()
        .unwrap();
    let server = RpcServer::<DoorState, DoorState>::on(&s, &entity(3), 1)
        .wire(DoorWire)
        .build()
        .unwrap();
    let (a, b, s, tap) = (
        a.enter_runtime().unwrap(),
        b.enter_runtime().unwrap(),
        s.enter_runtime().unwrap(),
        tap.enter_runtime().unwrap(),
    );
    let (ca, cb, server) = (
        ca.activate(&a).unwrap(),
        cb.activate(&b).unwrap(),
        server.activate(&s).unwrap(),
    );
    heap_free(|| {
        let mut bc = cb.invoke(&DoorState { id: 2, open: 0 }).unwrap();
        server
            .try_recv()
            .unwrap()
            .unwrap()
            .respond(&DoorState { id: 20, open: 1 })
            .unwrap();
        // Keep B's tap queue full. A must still complete through the shared TX route.
        let mut ac = ca.invoke(&DoorState { id: 1, open: 0 }).unwrap();
        server
            .try_recv()
            .unwrap()
            .unwrap()
            .respond(&DoorState { id: 10, open: 1 })
            .unwrap();
        assert_eq!(ac.try_response().unwrap().unwrap().id, 10);
        assert_eq!(bc.try_response().unwrap().unwrap().id, 20);
        let mut seen = 0;
        let count = tap
            .receive(tap_route, &mut |frame| {
                assert_eq!(frame.sink(), Some(&sink));
                seen += 1;
                up_rust_core::transport::Flow::Continue
            })
            .unwrap();
        assert_eq!((count, seen), (1, 1));
        assert_eq!(
            tap.receive(tap_route, &mut |_| up_rust_core::transport::Flow::Continue)
                .unwrap(),
            0
        );
        drop(ac);
        drop(bc);
    });
}
