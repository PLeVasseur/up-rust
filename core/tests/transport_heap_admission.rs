// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Public setup regressions for transport allocation classification only.
//! These doubles establish no middleware, panic/OOM, or native runtime claim.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]
pub use up_rust_core::{
    communication, frame, memory, transport, uattributes, uri, uuid, wire,
};
// The shared test doubles use the core's private atomic facade. This host-only
// integration fixture supplies the public AtomicU32 used by Platform on this
// target without accessing or changing that private production module.
pub mod platform {
    pub use up_rust_core::platform::*;
    pub(crate) mod atomic {
        pub(crate) use core::sync::atomic::AtomicU32;
    }
}
#[path = "../src/transport/tests.rs"]
mod doubles;

use communication::two_mode::{Publisher, Subscriber};
use communication::LocalUriProvider;
use doubles::{PushLending, TestUri, TestWire, CLOCK};
use std::sync::atomic::{AtomicUsize, Ordering};
use transport::{
    BindingRefusal, BindingRoute, BindingSetup, Capabilities, CarriedWire, CarrierRoute, Envelope,
    PreparedRoute, QosSupport, RouteDeclaration, RouteKind, RuntimePermit, RuntimePort,
    StartupError, TransportSetup,
};
use wire::{RuntimeHeap, Wire};

struct Classified {
    heap: RuntimeHeap,
    starts: &'static AtomicUsize,
}

impl BindingSetup for Classified {
    type Running = PushLending;
    fn name(&self) -> &'static str {
        "heap-admission-probe"
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::new(Envelope::UFrameFields, &[Envelope::UFrameFields], 1 << 20)
            .with_per_message_encoding(true)
            .with_truncate_payload(true)
            .with_native_loans(true)
    }
    fn qos_support(&self, kind: RouteKind) -> QosSupport {
        PushLending.qos_support(kind)
    }
    fn prepare_route(&mut self, _: &RouteDeclaration<'_>) -> Result<PreparedRoute, BindingRefusal> {
        Ok(PreparedRoute::new(BindingRoute(0), self.heap))
    }
    fn into_running(self, _: RuntimePort) -> Result<PushLending, BindingRefusal> {
        self.starts.fetch_add(1, Ordering::Relaxed);
        Ok(PushLending)
    }
}

fn setup(heap: RuntimeHeap) -> (TransportSetup, &'static AtomicUsize) {
    let memory = memory::StartupMemory::new(Box::leak(Box::new_uninit_slice(256 * 1024))).unwrap();
    let starts = Box::leak(Box::new(AtomicUsize::new(0)));
    (
        TransportSetup::lending_on(&memory, &CLOCK, Classified { heap, starts }).unwrap(),
        starts,
    )
}

fn carried(native: bool) -> CarriedWire {
    let mut value = CarriedWire::of(&TestWire { fail_encode: false }.describe());
    if native {
        value.identity.native_type_token = Some(frame::native::NativeTypeToken::from_u32(7));
    }
    value
}

fn prepare_carrier(setup: &TransportSetup, wires: &[CarriedWire], receive: bool) {
    let source = TestUri(1).get_resource_uri(0x8001);
    let route = CarrierRoute::new(&source, wires, 4);
    if receive {
        setup.carrier_ingress(&route).unwrap();
    } else {
        setup.carrier_egress(&route).unwrap();
    }
}

#[test]
fn serialized_publisher_and_subscriber_admit_declared_transport_allocation() {
    for receive in [false, true] {
        let (setup, starts) = setup(RuntimeHeap::Allocates("transport serialization"));
        if receive {
            Subscriber::<u32>::on(&setup, TestUri(1).get_resource_uri(0x8001))
                .wire(TestWire { fail_encode: false })
                .build()
                .unwrap();
        } else {
            Publisher::<u32>::on(&setup, &TestUri(1), 0x8001)
                .wire(TestWire { fail_encode: false })
                .build()
                .unwrap();
        }
        let runtime = setup.enter_runtime().unwrap();
        assert_eq!(starts.load(Ordering::Relaxed), 1);
        assert_eq!(runtime.routes().first().map(|route| route.native_payload), Some(false));
        assert!(matches!(
            runtime.routes().first().map(|route| route.heap),
            Some(RuntimeHeap::Allocates(_))
        ));
    }
}

#[test]
fn serialized_carrier_legs_admit_declared_transport_allocation() {
    for receive in [false, true] {
        let (setup, starts) = setup(RuntimeHeap::Allocates("serialized carrier"));
        prepare_carrier(&setup, &[carried(false)], receive);
        let runtime = setup.enter_runtime().unwrap();
        assert_eq!(starts.load(Ordering::Relaxed), 1);
        assert_eq!(runtime.routes().first().map(|route| route.native_payload), Some(false));
    }
}

#[test]
fn native_and_mixed_carrier_legs_refuse_allocation_before_start() {
    for receive in [false, true] {
        for wires in [vec![carried(true)], vec![carried(false), carried(true)]] {
            let (setup, starts) = setup(RuntimeHeap::Allocates("native must refuse"));
            prepare_carrier(&setup, &wires, receive);
            assert!(matches!(
                setup.enter_runtime(),
                Err(StartupError::RouteNeedsHeapAtRuntime { .. })
            ));
            assert_eq!(starts.load(Ordering::Relaxed), 0);
        }
    }
}

#[test]
fn native_and_mixed_carrier_reports_remain_native_with_heap_free_binding() {
    for receive in [false, true] {
        for wires in [vec![carried(true)], vec![carried(false), carried(true)]] {
            let (setup, _) = setup(RuntimeHeap::None);
            prepare_carrier(&setup, &wires, receive);
            assert_eq!(setup.enter_runtime().unwrap().routes().first().map(|route| route.native_payload), Some(true));
        }
    }
}

#[test]
fn named_middleware_waiver_is_still_required_for_every_carrier_kind() {
    static PERMIT: RuntimePermit = RuntimePermit {
        id: "UPW-test-1",
        rule: "runtime.no_heap_acquire",
        binding: "test",
        operations: &[transport::HeapOperation::Send],
        bound: "fixture only",
    };
    for receive in [false, true] {
        for wires in [
            vec![carried(false)],
            vec![carried(true)],
            vec![carried(false), carried(true)],
        ] {
            let (setup, starts) = setup(RuntimeHeap::Middleware(&PERMIT));
            prepare_carrier(&setup, &wires, receive);
            assert!(matches!(
                setup.enter_runtime(),
                Err(StartupError::MiddlewareHeapNeedsWaiver { .. })
            ));
            assert_eq!(starts.load(Ordering::Relaxed), 0);
        }
    }
}
