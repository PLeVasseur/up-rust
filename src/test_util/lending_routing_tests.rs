// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Metadata, routing, bounded history and slot-ownership fixture regressions.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use super::*;
use crate::heap_guard::{observe, HeapGuard, Scope};
use crate::two_mode::TransportSetupExt;
use up_rust_core::wire::{WireIdentity, PROTOBUF_PAYLOAD_FAMILY_ID, PROTOBUF_WIRE_ID};
use up_rust_core::{
    frame::header::{write_header, TxHeaderParts},
    uattributes::{PayloadEncoding, UPriority},
    ucode::UCode,
    uuid::UUID,
};
#[global_allocator]
static HEAP: HeapGuard = HeapGuard::new(std::alloc::System);
fn heap_free(body: impl FnOnce()) {
    let (_, r) = observe(Scope::AllThreads, body);
    let c = r.counts();
    assert_eq!((c.allocations, c.reallocations, c.deallocations), (0, 0, 0));
}
fn uri(entity: u32, resource: u16) -> UUri {
    UUri::try_from_wire_parts("local", 0x10000 | entity, 1, resource).unwrap()
}
fn identity(out: &mut Vec<u8>, id: WireIdentity, literal: bool) {
    if literal {
        out.push(ID_REF_LITERAL);
        out.extend_from_slice(&(id.literal_id().len() as u32).to_le_bytes());
        out.extend_from_slice(id.literal_id().as_bytes());
    } else {
        out.push(ID_REF_COMPACT);
        out.extend_from_slice(&id.compact_id().to_le_bytes());
    }
}
fn header(
    sink: &UUri,
    forms: u8,
    layout: WireIdentity,
    token: Option<&str>,
) -> (Vec<u8>, WrittenHeader) {
    let mut prefix = NATIVE_PREFIX_MAGIC.to_vec();
    identity(&mut prefix, layout, forms & 1 != 0);
    prefix.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    identity(&mut prefix, PROTOBUF_WIRE_ID, forms & 2 != 0);
    identity(&mut prefix, PROTOBUF_PAYLOAD_FAMILY_ID, forms & 4 != 0);
    prefix.extend_from_slice(&[0, 0]);
    let id = UUID::try_from_wire_pair(
        (2_000_000_000_000_u64 << 16) | 0x7000,
        0x8000_0000_0000_0001,
    )
    .unwrap();
    let source = uri(3, 1);
    let parts = TxHeaderParts {
        kind: UMessageType::Response,
        priority: Some(UPriority::CS4),
        id: &id,
        reqid: Some(&id),
        ttl: None,
        comm_status: Some(UCode::Unavailable),
        permission_level: None,
        source: &source,
        sink: Some(sink),
        token,
        traceparent: None,
        payload_encoding: None,
        native_type_token: None,
    };
    let mut bytes = vec![0; 8192];
    let written = write_header(&prefix, &parts, &mut bytes).unwrap();
    bytes.truncate(written.len);
    (bytes, written)
}
fn participant(bus: &LendingBus, id: u64, late: bool) -> LendingParticipant {
    bus.participant(id).with_options(LendingOptions {
        slots: 4,
        late_joiner: late,
        ..LendingOptions::default()
    })
}
fn inbound_for_fixture() -> InboundRoute {
    // Obtain an actual SDK-issued index. No private-field construction or unsafe
    // forged handle; this low-level fixture has no RuntimePort to dispatch into.
    let bus = LendingBus::new();
    let setup = crate::two_mode::TransportSetup::lending(bus.participant(99)).unwrap();
    crate::two_mode::Subscriber::<crate::test_util::wires::DoorState>::on(&setup, uri(9, 0x8001))
        .wire(crate::test_util::wires::DoorWire)
        .build()
        .unwrap();
    let inbound = lock(&bus.shared.state).rx.first().unwrap().inbound;
    inbound
}
fn prepare(
    p: &mut LendingParticipant,
    receive: bool,
    source: &UUri,
    sink: Option<&UUri>,
    kind: UMessageType,
    qos: Qos,
) -> BindingRoute {
    p.prepare_route(&RouteDeclaration {
            lifetime_policy: Default::default(),
        direction: if receive {
            RouteDirection::Receive
        } else {
            RouteDirection::Send
        },
        kind: Some(kind),
        source,
        sink,
        inbound: receive.then(inbound_for_fixture),
        envelope: Envelope::UFrameFields,
        encodings: &[PayloadEncoding::PROTOBUF],
        families: &[],
        max_metadata_len: 8192,
        max_payload_len: 0,
        payload_alignment: 1,
        slots: 4,
        qos,
        native: false,
        max_peers: 8,
    })
    .unwrap()
    .route
}
fn running(p: LendingParticipant) -> RunningLender {
    // Private fixture route tests need no SDK port; preparation and all actual
    // loan/publish/receive/drop implementations are the shipping test fixture's.
    RunningLender {
        shared: p.shared,
        participant: p.participant,
        lazy: None,
        lazy_allocated: AtomicBool::new(false),
    }
}
fn send(
    r: &RunningLender,
    route: BindingRoute,
    bytes: &[u8],
    written: WrittenHeader,
) -> Result<SendReport, SendError> {
    let mut tx = r
        .loan(
            route,
            TxFrameLayout {
                metadata_len: bytes.len(),
                payload_len: None,
                payload_alignment: 1,
            },
        )
        .unwrap();
    for (slot, byte) in tx.metadata_uninit_mut().iter_mut().zip(bytes) {
        slot.write(*byte);
    }
    tx.record_header(written).unwrap();
    // SAFETY: write_header validated and initialized every metadata byte under
    // the selected prefix. There is no payload/token owner. The alternate layout
    // test uses the same valid field-block codec with a private layout identity;
    // publication may reject that unsupported identity without violating storage.
    r.publish(route, unsafe { InitializedTx::from_initialized(tx) })
}
// [utest->req~ssr-binding-no-heap~1] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn first4a1_routing_compact_literal_mixed_prefixes_and_truncated_metadata() {
    let sink = uri(1, 0);
    for form in 0..8 {
        let (bytes, _) = header(&sink, form, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
        heap_free(|| {
            let actual = Routing::read(&bytes).unwrap();
            assert_eq!(actual.kind, UMessageType::Response);
            assert_eq!(actual.source, uri(3, 1));
            assert_eq!(actual.sink, Some(sink.clone()));
            for end in 0..bytes.len() {
                assert!(Routing::read(&bytes[..end]).is_err());
            }
        });
    }
    let long = "x".repeat(4096);
    let (bytes, _) = header(&sink, 7, UFRAME_FIELDS_METADATA_LAYOUT_ID, Some(&long));
    heap_free(|| {
        assert_eq!(Routing::read(&bytes).unwrap().sink, Some(sink));
    });
}
// [utest->req~ssr-binding-no-heap~1] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn first4a1_actual_address_matches_exact_wildcard_kind_and_unrelated_wait() {
    let bus = LendingBus::new();
    let source = uri(3, 1);
    let a = uri(1, 0);
    let b = uri(2, 0);
    let any = UUri::any();
    let mut p = participant(&bus, 3, false);
    let tx = prepare(
        &mut p,
        false,
        &source,
        None,
        UMessageType::Response,
        Qos::DEFAULT,
    );
    let p = running(p);
    let mut ra = participant(&bus, 1, false);
    let ar = prepare(
        &mut ra,
        true,
        &source,
        Some(&a),
        UMessageType::Response,
        Qos::DEFAULT,
    );
    let ra = running(ra);
    let mut rb = participant(&bus, 2, false);
    let br = prepare(
        &mut rb,
        true,
        &source,
        Some(&b),
        UMessageType::Response,
        Qos::DEFAULT.with_depth(1).with_overflow(Overflow::Wait {
            timeout: Duration::from_millis(1),
        }),
    );
    let rb = running(rb);
    let mut rw = participant(&bus, 4, false);
    let wr = prepare(
        &mut rw,
        true,
        &any,
        Some(&any),
        UMessageType::Response,
        Qos::DEFAULT,
    );
    let rw = running(rw);
    let mut wrong = participant(&bus, 5, false);
    let kr = prepare(
        &mut wrong,
        true,
        &source,
        Some(&any),
        UMessageType::Request,
        Qos::DEFAULT,
    );
    let sr = prepare(
        &mut wrong,
        true,
        &uri(8, 1),
        Some(&any),
        UMessageType::Response,
        Qos::DEFAULT,
    );
    let nr = prepare(
        &mut wrong,
        true,
        &source,
        None,
        UMessageType::Response,
        Qos::DEFAULT,
    );
    let wrong = running(wrong);
    let (bh, bw) = header(&b, 0, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
    let (ah, aw) = header(&a, 7, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
    heap_free(|| {
        assert_eq!(send(&p, tx, &bh, bw).unwrap().delivered, 2);
        // B's Wait queue is now full, yet A's response succeeds.
        assert_eq!(send(&p, tx, &ah, aw).unwrap().delivered, 2);
        assert_eq!(
            Routing::read(ra.receive(ar).unwrap().encoded_metadata())
                .unwrap()
                .sink,
            Some(a.clone())
        );
        assert_eq!(
            Routing::read(rb.receive(br).unwrap().encoded_metadata())
                .unwrap()
                .sink,
            Some(b.clone())
        );
        for sink in [&b, &a] {
            assert_eq!(
                Routing::read(rw.receive(wr).unwrap().encoded_metadata())
                    .unwrap()
                    .sink
                    .as_ref(),
                Some(sink)
            );
        }
        assert!(ra.receive(ar).is_none());
        assert!(rb.receive(br).is_none());
        assert!(rw.receive(wr).is_none());
        assert!(wrong.receive(kr).is_none());
        assert!(wrong.receive(sr).is_none());
        assert!(wrong.receive(nr).is_none());
    });
}
// [utest->req~ssr-binding-no-heap~1] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn first4a1_history_routes_each_slot_after_recycling_shared_response_route() {
    let bus = LendingBus::new();
    let source = uri(3, 1);
    let a = uri(1, 0);
    let b = uri(2, 0);
    let qos = Qos::DEFAULT.with_depth(2).with_history(2);
    let mut p = participant(&bus, 3, false);
    let tx = prepare(&mut p, false, &source, None, UMessageType::Response, qos);
    let p = running(p);
    let mut ra = participant(&bus, 1, true);
    let ar = prepare(
        &mut ra,
        true,
        &source,
        Some(&a),
        UMessageType::Response,
        qos,
    );
    let ra = running(ra);
    let mut rb = participant(&bus, 2, true);
    let br = prepare(
        &mut rb,
        true,
        &source,
        Some(&b),
        UMessageType::Response,
        qos,
    );
    let rb = running(rb);
    let (ah, aw) = header(&a, 0, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
    let (bh, bw) = header(&b, 7, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
    heap_free(|| {
        for _ in 0..3 {
            send(&p, tx, &ah, aw).unwrap();
            send(&p, tx, &bh, bw).unwrap();
        }
        assert!(ra.receive(ar).is_none());
        assert!(rb.receive(br).is_none());
        bus.join_late(1, ar);
        bus.join_late(2, br);
        assert_eq!(
            Routing::read(ra.receive(ar).unwrap().encoded_metadata())
                .unwrap()
                .sink,
            Some(a)
        );
        assert_eq!(
            Routing::read(rb.receive(br).unwrap().encoded_metadata())
                .unwrap()
                .sink,
            Some(b)
        );
        bus.join_late(1, ar);
        bus.join_late(2, br);
        assert!(ra.receive(ar).is_none());
        assert!(rb.receive(br).is_none());
        let state = lock(&bus.shared.state);
        let route = state.tx.first().unwrap();
        assert_eq!(route.history.len(), 2);
        assert_eq!(route.free.len(), 2);
        assert_eq!(route.slots.iter().map(|s| s.refs).sum::<u32>(), 2);
    });
}
// [utest->req~ssr-binding-no-heap~1] method: focused fixture regression; partial evidence, no G2 credit
#[test]
fn first4a1_rejected_metadata_returns_unpublished_loan_slot() {
    let bus = LendingBus::new();
    let source = uri(3, 1);
    let a = uri(1, 0);
    let mut p = participant(&bus, 3, false);
    let tx = prepare(
        &mut p,
        false,
        &source,
        None,
        UMessageType::Response,
        Qos::DEFAULT,
    );
    let p = running(p);
    let (bad, bw) = header(
        &a,
        1,
        WireIdentity::new("test.private.field-block", 0xf321),
        None,
    );
    let (good, gw) = header(&a, 0, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
    heap_free(|| {
        for _ in 0..12 {
            assert!(matches!(
                send(&p, tx, &bad, bw),
                Err(SendError::PayloadStorage)
            ));
            assert_eq!(lock(&bus.shared.state).tx.first().unwrap().free.len(), 4);
        }
        assert!(send(&p, tx, &good, gw).is_ok());
        let state = lock(&bus.shared.state);
        let route = state.tx.first().unwrap();
        assert_eq!(route.free.len(), 4);
        assert!(route.slots.iter().all(|s| s.refs == 0));
    });
}

// [utest->req~ssr-binding-no-heap~1] method: fixture history boundary, not native credit
#[test]
fn first4a1_r2_history_bounds_all_transmitters_and_preserves_publication_identity() {
    for overflow in [
        Overflow::DropOldest,
        Overflow::DropNewest,
        Overflow::Wait {
            timeout: Duration::from_millis(1),
        },
    ] {
        for history in [1_u16, 2] {
            let bus = LendingBus::new();
            let source = uri(3, 1);
            let a = uri(1, 0);
            let b = uri(2, 0);
            let any = UUri::any();
            let tx_qos = Qos::DEFAULT.with_depth(2).with_history(2);
            let mut pa = participant(&bus, 31, false);
            let ta = prepare(
                &mut pa,
                false,
                &source,
                None,
                UMessageType::Response,
                tx_qos,
            );
            let pa = running(pa);
            let mut pb = participant(&bus, 32, false);
            let tb = prepare(
                &mut pb,
                false,
                &source,
                None,
                UMessageType::Response,
                tx_qos,
            );
            let pb = running(pb);
            let mut rx = participant(&bus, 7, true);
            let rr = prepare(
                &mut rx,
                true,
                &source,
                Some(&any),
                UMessageType::Response,
                Qos::DEFAULT
                    .with_depth(2)
                    .with_history(history)
                    .with_overflow(overflow),
            );
            let rx = running(rx);
            let (ah, aw) = header(&a, 0, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
            let (bh, bw) = header(&b, 7, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
            let queue_capacity = lock(&bus.shared.state).rx[0].queue.capacity();
            heap_free(|| {
                send(&pa, ta, &ah, aw).unwrap();
                send(&pb, tb, &bh, bw).unwrap();
                assert_eq!(bus.restart(32), 2);
                send(&pb, tb, &bh, bw).unwrap();
                assert_eq!(bus.restart(31), 2);
                send(&pa, ta, &ah, aw).unwrap();
                // Replayed A must retain incarnation 2, neither 1 nor current 3.
                assert_eq!(bus.restart(31), 3);
                bus.join_late(7, rr);
                {
                    let state = lock(&bus.shared.state);
                    assert_eq!(state.rx[0].queue.len(), usize::from(history));
                    assert_eq!(state.rx[0].queue.capacity(), queue_capacity);
                }
                // Publications were A1, B1, B2, A2: global newest history is
                // B2/A2, despite A's transmitter being registered first.
                if history == 2 {
                    let frame = rx.receive(rr).unwrap();
                    assert_eq!((frame.peer().peer, frame.peer().incarnation), (32, 2));
                    assert_eq!(
                        Routing::read(frame.encoded_metadata()).unwrap().sink,
                        Some(b.clone())
                    );
                }
                let frame = rx.receive(rr).unwrap();
                assert_eq!((frame.peer().peer, frame.peer().incarnation), (31, 2));
                assert_eq!(
                    Routing::read(frame.encoded_metadata()).unwrap().sink,
                    Some(a.clone())
                );
                drop(frame);
                bus.join_late(7, rr);
                assert!(rx.receive(rr).is_none());
                let state = lock(&bus.shared.state);
                for tx in &state.tx {
                    assert_eq!(tx.history.len(), 2);
                    assert_eq!(tx.free.len(), 2);
                    assert_eq!(tx.slots.iter().map(|s| s.refs).sum::<u32>(), 2);
                }
            });
        }
    }
}

// [utest->req~ssr-binding-no-heap~1] method: retained-frame ownership, not native credit
#[test]
fn first4a1_r2_drop_oldest_releases_the_queued_transmitter_without_aliasing() {
    let bus = LendingBus::new();
    let source = uri(3, 1);
    let a = uri(1, 0);
    let b = uri(2, 0);
    let any = UUri::any();
    let qos = Qos::DEFAULT
        .with_depth(1)
        .with_overflow(Overflow::DropOldest);
    let mut pa = participant(&bus, 31, false);
    let ta = prepare(&mut pa, false, &source, None, UMessageType::Response, qos);
    let pa = running(pa);
    let mut pb = participant(&bus, 32, false);
    let tb = prepare(&mut pb, false, &source, None, UMessageType::Response, qos);
    let pb = running(pb);
    let mut rx = participant(&bus, 7, false);
    let rr = prepare(
        &mut rx,
        true,
        &source,
        Some(&any),
        UMessageType::Response,
        qos,
    );
    let rx = running(rx);
    let (ah, aw) = header(&a, 0, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
    let (bh, bw) = header(&b, 7, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
    heap_free(|| {
        assert_eq!(send(&pa, ta, &ah, aw).unwrap().delivered, 1);
        assert_eq!(send(&pb, tb, &bh, bw).unwrap().dropped_full, 1);
        {
            let state = lock(&bus.shared.state);
            assert_eq!(state.tx[0].free.len(), 4);
            assert_eq!(state.tx[1].free.len(), 3);
        }
        let held = rx.receive(rr).unwrap();
        let held_bytes = held.encoded_metadata();
        assert_eq!(held_bytes, bh.as_slice());
        // A wrong-owner release puts B's referenced slot back on B's free list.
        // A second B loan would then overwrite this still-held frame.
        send(&pb, tb, &ah, aw).unwrap();
        assert_eq!(held_bytes, bh.as_slice());
        assert_eq!(send(&pa, ta, &ah, aw).unwrap().dropped_full, 1);
        assert_eq!(held_bytes, bh.as_slice());
        let current = rx.receive(rr).unwrap();
        assert_eq!(current.peer().peer, 31);
        assert_eq!(current.encoded_metadata(), ah.as_slice());
        drop(current);
        drop(held);
        let state = lock(&bus.shared.state);
        for tx in &state.tx {
            assert_eq!(tx.free.len(), 4);
            assert!(tx.slots.iter().all(|s| s.refs == 0));
            for (index, free) in tx.free.iter().enumerate() {
                assert!(!tx.free[..index].contains(free));
            }
        }
    });
}

// [utest->req~ssr-binding-no-heap~1] method: bounded publication counter refusal
#[test]
fn first4a1_r2_publication_order_exhaustion_returns_the_unpublished_slot() {
    let bus = LendingBus::new();
    let source = uri(3, 1);
    let sink = uri(1, 0);
    let mut p = participant(&bus, 31, false);
    let tx = prepare(
        &mut p,
        false,
        &source,
        None,
        UMessageType::Response,
        Qos::DEFAULT.with_depth(2).with_history(2),
    );
    let p = running(p);
    let (bytes, written) = header(&sink, 0, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
    lock(&bus.shared.state).publication_sequence = u64::MAX;
    heap_free(|| {
        for _ in 0..8 {
            assert!(matches!(
                send(&p, tx, &bytes, written),
                Err(SendError::PayloadStorage)
            ));
        }
        let state = lock(&bus.shared.state);
        assert_eq!(state.tx[0].free.len(), 4);
        assert!(state.tx[0].history.is_empty());
        assert!(state.tx[0].slots.iter().all(|s| s.refs == 0));
    });
}

// [utest->req~ssr-binding-no-heap~1] method: shared readers and history ownership
#[test]
fn first4a1_r2_shared_readers_keep_borrows_across_history_eviction_and_reuse() {
    let bus = LendingBus::new();
    let source = uri(3, 1);
    let sink = uri(1, 0);
    let other = uri(2, 0);
    let any = UUri::any();
    let qos = Qos::DEFAULT.with_depth(1).with_history(1);
    let mut p = participant(&bus, 31, false);
    let tx = prepare(&mut p, false, &source, None, UMessageType::Response, qos);
    let p = running(p);
    let mut a = participant(&bus, 7, false);
    let ar = prepare(
        &mut a,
        true,
        &source,
        Some(&any),
        UMessageType::Response,
        qos,
    );
    let a = running(a);
    let mut b = participant(&bus, 8, false);
    let br = prepare(
        &mut b,
        true,
        &source,
        Some(&any),
        UMessageType::Response,
        qos,
    );
    let b = running(b);
    let (bytes, written) = header(&sink, 0, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
    let (later, lw) = header(&other, 7, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
    heap_free(|| {
        send(&p, tx, &bytes, written).unwrap();
        let first = a.receive(ar).unwrap();
        let held_bytes = first.encoded_metadata();
        let second = b.receive(br).unwrap();
        assert_eq!(held_bytes.as_ptr(), second.encoded_metadata().as_ptr());
        assert_eq!(held_bytes, bytes.as_slice());
        drop(second);
        for _ in 0..8 {
            send(&p, tx, &later, lw).unwrap();
            assert_eq!(held_bytes, bytes.as_slice());
        }
        // Calling join on already-active routes must not replay retained history.
        drop(a.receive(ar).unwrap());
        drop(b.receive(br).unwrap());
        bus.join_late(7, ar);
        bus.join_late(8, br);
        assert!(a.receive(ar).is_none());
        assert!(b.receive(br).is_none());
        assert_eq!(held_bytes, bytes.as_slice());
        drop(first);
        let state = lock(&bus.shared.state);
        let tx = &state.tx[0];
        assert_eq!(tx.history.len(), 1);
        assert_eq!(tx.free.len(), 3);
        assert_eq!(tx.slots.iter().map(|s| s.refs).sum::<u32>(), 1);
        for (index, free) in tx.free.iter().enumerate() {
            assert!(!tx.free[..index].contains(free));
            assert!(!tx.history.contains(free));
        }
    });
}

// [utest->req~ssr-binding-no-heap~1] method: repeated timed-out unpublished loans
#[test]
fn first4a1_r2_wait_timeout_returns_each_unpublished_loan_once() {
    let bus = LendingBus::new();
    let source = uri(3, 1);
    let sink = uri(1, 0);
    let qos = Qos::DEFAULT.with_depth(1).with_overflow(Overflow::Wait {
        timeout: Duration::from_millis(1),
    });
    let mut p = participant(&bus, 31, false);
    let tx = prepare(&mut p, false, &source, None, UMessageType::Response, qos);
    let p = running(p);
    let mut r = participant(&bus, 7, false);
    let rr = prepare(
        &mut r,
        true,
        &source,
        Some(&sink),
        UMessageType::Response,
        qos,
    );
    let r = running(r);
    let (bytes, written) = header(&sink, 0, UFRAME_FIELDS_METADATA_LAYOUT_ID, None);
    heap_free(|| {
        send(&p, tx, &bytes, written).unwrap();
        for _ in 0..12 {
            assert!(matches!(
                send(&p, tx, &bytes, written),
                Err(SendError::TimedOut)
            ));
            let state = lock(&bus.shared.state);
            assert_eq!(state.publication_sequence, 1);
            assert_eq!(state.rx[0].queue.len(), 1);
            assert_eq!(state.tx[0].free.len(), 3);
            assert_eq!(state.tx[0].slots.iter().map(|s| s.refs).sum::<u32>(), 1);
        }
        drop(r.receive(rr).unwrap());
        assert!(send(&p, tx, &bytes, written).is_ok());
        drop(r.receive(rr).unwrap());
        let state = lock(&bus.shared.state);
        assert_eq!(state.publication_sequence, 2);
        let tx = &state.tx[0];
        assert_eq!(tx.free.len(), 4);
        assert!(tx.slots.iter().all(|s| s.refs == 0));
        for (index, free) in tx.free.iter().enumerate() {
            assert!(!tx.free[..index].contains(free));
        }
    });
}
