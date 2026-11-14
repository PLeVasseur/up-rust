// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Force the dispatch-capacity failure after report capacity is available.
//! No setup in these controls enters runtime.
#[cfg(test)]
use crate::test_support::FixtureResult as _;
use super::*;
use crate::{
    communication::{two_mode::{Publisher, Subscriber}, LocalUriProvider},
    memory::StartupRegion,
    transport::tests::{NullHandoff, TestUri, TestWire, CLOCK},
};

fn exhaust(memory: &StartupMemory) {
    while memory.bytes(64).is_ok() {}
    while memory.bytes(1).is_ok() {}
}

#[test]
fn add_rx_capacity_failure_keeps_report_and_dispatch_lengths() {
    static REGION: StartupRegion<{ 1 << 18 }> = StartupRegion::new();
    let memory = REGION.claim().require_fixture();
    let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).require_fixture();
    let _role = Subscriber::<u32>::on(&setup, TestUri(1).get_resource_uri(0x8001))
        .wire(TestWire { fail_encode: false })
        .build()
        .require_fixture();
    let (route, reports, routes) = {
        let mut inner = setup.inner.borrow_mut();
        let route = *inner.rx.slice().first().require_fixture();
        // Test-only setup geometry: duplicate immutable references fill the
        // dispatch table. No runtime/pool operation is reachable from this setup.
        while inner.rx.len() < inner.rx.capacity {
            inner.rx.push(&setup.memory, route).require_fixture();
        }
        assert!(inner.reports.len() < inner.reports.capacity);
        (route, inner.reports.len(), inner.rx.len())
    };
    exhaust(&memory);
    assert!(setup.add_rx(route).is_err());
    let inner = setup.inner.borrow();
    assert_eq!(inner.reports.len(), reports);
    assert_eq!(inner.rx.len(), routes);
}

#[test]
fn add_tx_capacity_failure_keeps_report_and_dispatch_lengths() {
    static REGION: StartupRegion<{ 1 << 18 }> = StartupRegion::new();
    let memory = REGION.claim().require_fixture();
    let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).require_fixture();
    let _role = Publisher::<u32>::on(&setup, &TestUri(1), 0x8001_u16)
        .wire(TestWire { fail_encode: false })
        .build()
        .require_fixture();
    let (route, reports, routes) = {
        let mut inner = setup.inner.borrow_mut();
        let route = *inner.tx.slice().first().require_fixture();
        // As in the RX control, only the unavailable dispatch-capacity boundary
        // is synthesized. No duplicate route is activated or executed.
        while inner.tx.len() < inner.tx.capacity {
            inner.tx.push(&setup.memory, route).require_fixture();
        }
        assert!(inner.reports.len() < inner.reports.capacity);
        (route, inner.reports.len(), inner.tx.len())
    };
    exhaust(&memory);
    assert!(setup.add_tx(route).is_err());
    let inner = setup.inner.borrow();
    assert_eq!(inner.reports.len(), reports);
    assert_eq!(inner.tx.len(), routes);
}
