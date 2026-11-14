// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! White-box faults at private admission boundaries; no production fault hooks.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::communication::{
    two_mode::{Publisher, Subscriber},
    LocalUriProvider,
};
use crate::memory::StartupRegion;
use crate::transport::tests::{NullHandoff, TestUri, TestWire, CLOCK};
use crate::transport::{BindingSetup, Capabilities, QosSupport, RouteKind, RuntimePort};
use crate::platform::atomic::AtomicUsize;

struct Probe {
    counts: &'static Counts,
}
struct Counts {
    preparations: AtomicUsize,
    starts: AtomicUsize,
}
impl BindingSetup for Probe {
    type Running = NullHandoff;
    fn name(&self) -> &'static str {
        "insertion-probe"
    }
    fn capabilities(&self) -> Capabilities {
        NullHandoff.capabilities()
    }
    fn qos_support(&self, k: RouteKind) -> QosSupport {
        NullHandoff.qos_support(k)
    }
    fn prepare_route(&mut self, r: &RouteDeclaration<'_>) -> Result<PreparedRoute, BindingRefusal> {
        self.counts.preparations.fetch_add(1, Ordering::Relaxed);
        NullHandoff.prepare_route(r)
    }
    fn into_running(self, _: RuntimePort) -> Result<NullHandoff, BindingRefusal> {
        self.counts.starts.fetch_add(1, Ordering::Relaxed);
        Ok(NullHandoff)
    }
}
fn exhaust(memory: &StartupMemory) {
    let crate::memory::StartupError::MemoryExhausted { remaining, .. } =
        memory.bytes(usize::MAX).unwrap_err()
    else {
        panic!("expected real region exhaustion")
    };
    memory.bytes(remaining).unwrap();
}
fn incomplete(error: &StartupError) -> bool {
    matches!(
        error,
        StartupError::InvalidConfig(
            "incomplete route admission: discard and recreate this transport setup"
        )
    )
}
// [utest->req~ssr-strict-init-only~1] method: focused fixture regression; partial evidence, no G2 credit
// [utest->req~ssr-mode-init-failure~2] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn first4a1_snapshot_report_dispatch_and_refcell_faults_remain_latched() {
    static REGION: StartupRegion<2_097_152> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    // 0 snapshot growth; 1 report growth; 2 dispatch growth;
    // 3 snapshot RefCell; 4 dispatch/report RefCell. Both send and receive.
    for receive in [false, true] {
        for stage in 0..5 {
            let counts: &'static Counts = memory
                .place(Counts {
                    preparations: AtomicUsize::new(0),
                    starts: AtomicUsize::new(0),
                })
                .unwrap();
            // Independent real fixed region, so exhausting this setup leaves the
            // outer fixture arena usable for the next boundary.
            let bytes = memory.bytes(128 * 1024).unwrap();
            let local = StartupMemory::new(bytes).unwrap();
            let setup = TransportSetup::handoff_on(&local, &CLOCK, Probe { counts }).unwrap();
            Publisher::<u32>::on(&setup, &TestUri(1), 0x8001)
                .wire(TestWire { fail_encode: false })
                .build()
                .unwrap();
            Subscriber::<u32>::on(&setup, TestUri(1).get_resource_uri(0x8001))
                .wire(TestWire { fail_encode: false })
                .build()
                .unwrap();
            let (tx, rx, view) = {
                let inner = setup.inner.borrow();
                (
                    *inner.tx.slice().first().unwrap(),
                    *inner.rx.slice().first().unwrap(),
                    inner.views.slice().first().unwrap().clone(),
                )
            };
            let source = TestUri(1).get_resource_uri(0x8002);
            let declaration = RouteDeclaration {
            lifetime_policy: Default::default(),
                direction: if receive {
                    RouteDirection::Receive
                } else {
                    RouteDirection::Send
                },
                kind: Some(UMessageType::Publish),
                source: &source,
                sink: None,
                inbound: receive.then_some(InboundRoute(1)),
                envelope: Envelope::NativeWholeFrame,
                encodings: &[],
                families: &[],
                max_metadata_len: 256,
                max_payload_len: 4,
                payload_alignment: 1,
                slots: 1,
                qos: Qos::DEFAULT,
                native: false,
                max_peers: 1,
            };
            setup.prepare_route(&declaration).unwrap();
            assert_eq!(counts.preparations.load(Ordering::Relaxed), 3);
            {
                let mut inner = setup.inner.borrow_mut();
                if stage == 0 {
                    inner.views = GrowTable::new();
                }
                if stage == 1 {
                    inner.reports = GrowTable::new();
                }
                if stage == 2 {
                    if receive {
                        inner.rx = GrowTable::new();
                    } else {
                        inner.tx = GrowTable::new();
                    }
                }
            }
            let before_reports = setup.inner.borrow().reports.len();
            let error = if stage >= 3 {
                let _held = setup.inner.borrow();
                if stage == 3 {
                    setup.check_route(view).unwrap_err()
                } else if receive {
                    setup.add_rx(rx).unwrap_err()
                } else {
                    setup.add_tx(tx).unwrap_err()
                }
            } else {
                exhaust(&local);
                if stage == 0 {
                    setup.check_route(view).unwrap_err()
                } else if receive {
                    setup.add_rx(rx).unwrap_err()
                } else {
                    setup.add_tx(tx).unwrap_err()
                }
            };
            if stage >= 3 {
                assert!(matches!(
                    error,
                    StartupError::InvalidConfig("re-entrant setup")
                ));
            } else {
                assert!(matches!(
                    error,
                    StartupError::Memory(crate::memory::StartupError::MemoryExhausted { .. })
                ));
            }
            let after_reports = setup.inner.borrow().reports.len();
            assert_eq!(after_reports, before_reports + usize::from(stage == 2));
            assert!(incomplete(&setup.prepare_route(&declaration).unwrap_err()));
            assert_eq!(counts.preparations.load(Ordering::Relaxed), 3);
            assert!(incomplete(&setup.enter_runtime().unwrap_err()));
            assert_eq!(counts.starts.load(Ordering::Relaxed), 0);
        }
    }
}
