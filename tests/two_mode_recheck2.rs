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

//! The second re-check's findings on `85dae7d` (`review-stage2/RECHECK-2.md`) that
//! the review doubles show:
//! - R2-4: a wire's `decode` that unwinds left a typed route's slot, queue depth
//!   and lease taken for good, the class `b4fea94` closed for the carrier sink;
//! - R2-5.2: nothing tested that a refused `Wait` frame on a receive route counts
//!   as `busy`.
//!
//! And the third re-check's (`RECHECK-3.md`) R3-4.1: nothing could fail when the
//! send side of R2-4, the wipe of a value slot whose send unwinds, was removed.
//!
//! Each test that panics has its own hook and switch, because the tests of one
//! binary run at once.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use core::time::Duration;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[path = "support/review_doubles.rs"]
mod doubles;
use doubles::*;

use up_rust_core::communication::two_mode::{Publisher, Subscriber};
use up_rust_core::transport::binding::{
    InboundFrame, InboundRoute, PayloadBytes, ReplyToken, RuntimePort,
};
use up_rust_core::transport::qos::{Overflow, Qos};
use up_rust_core::transport::{MessageFields, ReceiveError, TransportSetup};
use up_rust_core::uattributes::PayloadEncoding;
use up_rust_core::uri::UUri;
use up_rust_core::wire::{
    EncodedSize, PayloadWriter, Ready, StartupSlot, Wire, WireError, WireInfo,
};

const WALL: u64 = 2_000_000_000_000;

fn frame(topic: &UUri, value: u32) -> Vec<u8> {
    publish_frame(
        topic,
        uuid_at(WALL, u64::from(value)),
        None,
        None,
        &value.to_le_bytes(),
    )
}

/// Delivers one frame the way a push binding does, from its own thread.
fn push(
    port: RuntimePort,
    route: InboundRoute,
    topic: &UUri,
    value: u32,
) -> Result<(), ReceiveError> {
    let bytes = frame(topic, value);
    port.deliver(
        route,
        InboundFrame {
            metadata: &[],
            payload: PayloadBytes::one(&bytes),
            peer: peer(7, 1),
            reply: ReplyToken::default(),
        },
    )
}

/// One `try_recv`, with an unwind caught and reported as "panicked".
fn read(subscriber: Subscriber<u32>) -> String {
    match catch_unwind(AssertUnwindSafe(|| {
        subscriber.try_recv().map(|m| m.map(|v| *v))
    })) {
        Ok(Ok(Some(value))) => value.to_string(),
        Ok(Ok(None)) => "none".to_owned(),
        Ok(Err(error)) => format!("{error:?}"),
        Err(_) => "panicked".to_owned(),
    }
}

static PANIC_PULLED: AtomicBool = AtomicBool::new(false);

fn panic_pulled(_: &mut u32) {
    if PANIC_PULLED.swap(false, Ordering::SeqCst) {
        panic!("the wire's decode fails once");
    }
}

/// R2-4, a pull binding. A depth-1 `DropNewest` route that lends one lease, three
/// frames waiting in the binding, and a wire whose first `decode` panics inside
/// `try_recv`. The panic costs that frame only: the next calls read the other two,
/// and the lost frame counts as refused. At `85dae7d` the route never delivered
/// again: the frame's depth, its slot and the lease reserved for it stayed taken.
#[test]
fn r2_4_a_decode_that_unwinds_on_a_pull_costs_its_frame_not_the_route() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let setup = TransportSetup::lending_on(&memory(), clock, LendingDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let subscriber = Subscriber::<u32>::on(&setup, topic.clone())
        .wire(U32Wire {
            hook: Some(panic_pulled),
        })
        .qos(
            Qos::DEFAULT
                .with_depth(1)
                .with_overflow(Overflow::DropNewest),
        )
        .max_held(1)
        .build()
        .unwrap();
    let rx = setup.enter_runtime().unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    for value in 1..=3 {
        shared
            .queue
            .lock()
            .unwrap()
            .push_back((frame(&topic, value), peer(7, 1)));
    }
    PANIC_PULLED.store(true, Ordering::SeqCst);
    let outcomes: Vec<String> = (0..5).map(|_| read(subscriber)).collect();
    assert_eq!(outcomes, ["panicked", "2", "3", "none", "none"]);
    assert_eq!(shared.queue.lock().unwrap().len(), 0, "nothing left behind");
    assert_eq!(
        subscriber.counters().refused,
        1,
        "the lost frame is counted"
    );
}

static PANIC_PUSHED: AtomicBool = AtomicBool::new(false);

fn panic_pushed(_: &mut u32) {
    if PANIC_PUSHED.swap(false, Ordering::SeqCst) {
        panic!("the wire's decode fails once");
    }
}

/// R2-4, a push binding. A depth-1 `DropNewest` route; the wire's first `decode`
/// panics inside a delivery on the binding's thread. The next frame is admitted
/// and read. At `85dae7d` it was refused, because the panicked frame's depth
/// stayed reserved.
#[test]
fn r2_4_a_decode_that_unwinds_on_a_push_costs_its_frame_not_the_route() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let setup = TransportSetup::handoff_on(&memory(), clock, HandoffDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let subscriber = Subscriber::<u32>::on(&setup, topic.clone())
        .wire(U32Wire {
            hook: Some(panic_pushed),
        })
        .qos(
            Qos::DEFAULT
                .with_depth(1)
                .with_overflow(Overflow::DropNewest),
        )
        .build()
        .unwrap();
    let rx = setup.enter_runtime().unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    let (port, route) = (shared.port(), shared.route(0));
    PANIC_PUSHED.store(true, Ordering::SeqCst);
    assert!(
        catch_unwind(AssertUnwindSafe(|| push(port, route, &topic, 1))).is_err(),
        "the first decode panics"
    );
    assert_eq!(
        push(port, route, &topic, 2),
        Ok(()),
        "the route takes the next frame"
    );
    assert_eq!(read(subscriber), "2");
    assert_eq!(
        subscriber.counters().refused,
        1,
        "the lost frame is counted"
    );
}

static PANICS_LEFT: AtomicU32 = AtomicU32::new(0);

/// Only its own test, on one thread, touches `PANICS_LEFT`.
fn panic_while_left(_: &mut u32) {
    if PANICS_LEFT.load(Ordering::SeqCst) > 0 {
        PANICS_LEFT.fetch_sub(1, Ordering::SeqCst);
        panic!("the wire's decode fails");
    }
}

/// R2-4, `DropOldest`, which reserves no depth: a decode that unwinds still holds
/// a slot. The route has three (depth 1, one lease, one spare). After three panics
/// in a row the route still takes and delivers a frame. At `85dae7d` it refused
/// it: all three slots had been lost.
#[test]
fn r2_4_decodes_that_unwind_give_their_slots_back() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let setup = TransportSetup::handoff_on(&memory(), clock, HandoffDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let subscriber = Subscriber::<u32>::on(&setup, topic.clone())
        .wire(U32Wire {
            hook: Some(panic_while_left),
        })
        .qos(
            Qos::DEFAULT
                .with_depth(1)
                .with_overflow(Overflow::DropOldest),
        )
        .max_held(1)
        .build()
        .unwrap();
    let rx = setup.enter_runtime().unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    let (port, route) = (shared.port(), shared.route(0));
    PANICS_LEFT.store(3, Ordering::SeqCst);
    for value in 1..=3 {
        assert!(
            catch_unwind(AssertUnwindSafe(|| push(port, route, &topic, value))).is_err(),
            "decode {value} panics"
        );
    }
    assert_eq!(
        push(port, route, &topic, 4),
        Ok(()),
        "the route still has slots"
    );
    assert_eq!(read(subscriber), "4");
    assert_eq!(
        subscriber.counters().refused,
        3,
        "each lost frame is counted"
    );
}

/// R2-5.2. A `Wait` route at depth refuses a pushed frame as backpressure: the
/// binding keeps the frame and retries it, so the refusal counts in `busy`, not
/// as a drop, and the retried frame is admitted once there is room. At `a3b8396`
/// the refusal counted in `dropped_newest`.
#[test]
fn r2_5_a_wait_refusal_on_a_receive_route_counts_as_busy() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let setup = TransportSetup::handoff_on(&memory(), clock, HandoffDouble(shared)).unwrap();
    let topic = uri(9, 0x8001);
    let subscriber = Subscriber::<u32>::on(&setup, topic.clone())
        .wire(U32Wire { hook: None })
        .qos(Qos::DEFAULT.with_depth(1).with_overflow(Overflow::Wait {
            timeout: Duration::from_secs(1),
        }))
        .build()
        .unwrap();
    let rx = setup.enter_runtime().unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    let (port, route) = (shared.port(), shared.route(0));
    assert_eq!(push(port, route, &topic, 1), Ok(()));
    assert_eq!(
        push(port, route, &topic, 2),
        Err(ReceiveError::SlotsExhausted),
        "at depth: refused, and the binding keeps the frame"
    );
    let counters = subscriber.counters();
    assert_eq!(
        (counters.busy, counters.dropped_newest),
        (1, 0),
        "backpressure, not a drop"
    );
    assert_eq!(read(subscriber), "1");
    assert_eq!(
        push(port, route, &topic, 2),
        Ok(()),
        "the retried frame is admitted"
    );
    assert_eq!(read(subscriber), "2");
}

static ENCODE_UNWINDS: AtomicBool = AtomicBool::new(false);

/// The doubles' `u32` wire, except that its `encode` unwinds once when armed.
#[derive(Clone, Copy)]
struct UnwindingEncode;

const PLAIN: U32Wire = U32Wire { hook: None };

impl Wire<u32> for UnwindingEncode {
    fn describe(&self) -> WireInfo {
        PLAIN.describe()
    }
    fn encoded_size(&self, value: &u32) -> EncodedSize {
        PLAIN.encoded_size(value)
    }
    fn encode(&self, value: &u32, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        if ENCODE_UNWINDS.swap(false, Ordering::SeqCst) {
            panic!("the wire's encode fails");
        }
        PLAIN.encode(value, out)
    }
    fn init_slot<'s>(&self, slot: StartupSlot<'s, u32>) -> Ready<'s, u32> {
        PLAIN.init_slot(slot)
    }
    fn decode(
        &self,
        encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut u32,
    ) -> Result<(), WireError> {
        PLAIN.decode(encoding, bytes, into)
    }
}

/// R3-4.1, the send side of R2-4 (L4). A handoff publisher with one value slot
/// sends with an access token, through a wire whose `encode` unwinds. The send
/// wrote the token into the slot's head before `encode` ran; the slot it gives
/// back holds none of it, and the next send goes out. Without the guard on the
/// send path (the reviewer's m9), the token stays in the freed slot.
#[test]
fn r3_4_1_a_send_whose_encode_unwinds_leaves_no_token_in_its_slot() {
    let clock = Clock::leak(1_000_000_000, WALL);
    let shared = Shared::leak();
    let setup = TransportSetup::handoff_on(&memory(), clock, HandoffDouble(shared)).unwrap();
    let publisher = Publisher::<u32>::on(&setup, &Uri(3), 0x8001_u16)
        .wire(UnwindingEncode)
        .max_token_len(16)
        .slots(1)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let publisher = publisher.activate(&runtime).unwrap();
    let fields = MessageFields {
        token: Some("SECRET-TOKEN"),
        ..MessageFields::default()
    };
    let has_token = |bytes: &[u8]| bytes.windows(6).any(|window| window == b"SECRET");
    ENCODE_UNWINDS.store(true, Ordering::SeqCst);
    let unwound = catch_unwind(AssertUnwindSafe(|| publisher.publish_with(&fields, &7)));
    assert!(unwound.is_err(), "the wire's encode unwound");
    assert_eq!(
        publisher.__free_value_slot(has_token),
        Some(false),
        "the slot came back, and the token did not survive in it"
    );
    assert!(shared.sent.lock().unwrap().is_empty(), "nothing was sent");
    assert!(
        publisher.publish_with(&fields, &8).is_ok(),
        "the next send goes out"
    );
    let sent = shared.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert!(has_token(&sent[0]), "it carries its token");
}
