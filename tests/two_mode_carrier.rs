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

//! Carrier routes on the in-memory bindings (design 3.4, 3.7, 9.3).
//!
//! All four directions keep identifier and payload bytes unchanged (R24). The
//! wire set is closed and nothing is negotiated. Heap windows live in
//! `two_mode_carrier_no_heap.rs`.

#![allow(clippy::indexing_slicing, clippy::wildcard_enum_match_arm)]

use std::time::Duration;

use up_rust::prelude::UuidExt as _;
use up_rust::startup::LEAKED_HEAP_REGIONS;
use up_rust::test_util::carrier::{door_frame, ScriptedControl, ScriptedFrame, ScriptedHandoff};
use up_rust::test_util::forwarder::TestForwarder;
use up_rust::test_util::handoff::InMemoryBus;
use up_rust::test_util::lending::{LendingBus, LendingOptions};
use up_rust::test_util::wires::{DoorState, DoorWire, LittleDoorWire, DOOR_ENCODING};
use up_rust::test_util::{EnvelopeHarness, TestClock};
use up_rust::two_mode::*;
use up_rust::{StaticUriProvider, TxHeaderParts, UEntityId, UMessageType, UUri, UUID};

#[path = "support/review_doubles.rs"]
mod doubles;

fn entity(authority: &str, type_id: u16) -> StaticUriProvider {
    StaticUriProvider::for_entity(authority, UEntityId::first_instance(type_id), 1).expect("uri")
}

fn memory() -> StartupMemory {
    StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 256 * 1024).expect("memory")
}

fn topic() -> UUri {
    entity("door", 1).get_resource_uri(0x8001)
}

fn door(id: u32, open: u8) -> DoorState {
    DoorState { id, open }
}

fn door_wires() -> [CarriedWire; 1] {
    [CarriedWire::of(&Wire::<DoorState>::describe(&DoorWire))]
}

fn wires_of<W: Wire<DoorState>>(wire: W) -> [CarriedWire; 1] {
    [CarriedWire::of(&Wire::<DoorState>::describe(&wire))]
}

fn route<'a>(filter: &'a UUri, carried: &'a [CarriedWire]) -> CarrierRoute<'a> {
    CarrierRoute::new(filter, carried, 64)
}

fn handoff(bus: InMemoryBus, peer: u64) -> TransportSetup {
    TransportSetup::handoff_in(&memory(), bus.participant(peer)).expect("setup")
}

fn lending(bus: LendingBus, peer: u64) -> TransportSetup {
    TransportSetup::lending_in(&memory(), bus.participant(peer)).expect("setup")
}

fn lending_with(bus: LendingBus, peer: u64, options: LendingOptions) -> TransportSetup {
    TransportSetup::lending_in(&memory(), bus.participant(peer).with_options(options))
        .expect("setup")
}

fn scripted(envelope: Envelope) -> (TransportSetup, ScriptedControl) {
    let (binding, control) = ScriptedHandoff::new(envelope);
    (
        TransportSetup::handoff_in(&memory(), binding).expect("setup"),
        control,
    )
}

fn scripted_door_frame(source: &UUri, door_id: u32) -> ScriptedFrame {
    let (metadata, payload) = door_frame(Envelope::UFrameFields, source, door_id, None);
    ScriptedFrame::contiguous(metadata, payload)
}

// ------------------------------------------------------------- setup refusals

#[test]
fn a_wildcard_source_filter_on_a_lending_leg_needs_a_physical_address() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let any = UUri::any();
    let carried = door_wires();
    let refused = a.carrier_ingress(&route(&any, &carried)).unwrap_err();
    match &refused {
        StartupError::CarrierRefused {
            route,
            binding,
            reason,
        } => {
            assert_eq!(route.direction, RouteDirection::Receive);
            assert_eq!(*binding, "in-memory-lending");
            assert!(reason.contains("physical_source"), "{reason}");
        }
        other => panic!("not named: {other}"),
    }
    assert!(refused.to_string().contains("physical_source"));
    assert!(
        a.carrier_egress(&route(&any, &carried)).is_err(),
        "either leg"
    );
    // An exact filter on the same leg is the physical address.
    let exact = topic();
    a.carrier_ingress(&route(&exact, &carried)).unwrap();
}

#[test]
fn a_wildcard_filter_on_a_lending_leg_works_through_a_physical_address() {
    let bus = LendingBus::new();
    let upstream = lending(bus, 1);
    let carrier_in = lending(bus, 2);
    let downstream = lending(bus, 3);
    let any = UUri::any();
    let physical = topic();
    let carried = door_wires();
    let carrier = CarrierRoute {
        physical_source: Some(&physical),
        ..route(&any, &carried)
    };
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .unwrap();
    let ingress = carrier_in.carrier_ingress(&carrier).unwrap();
    let egress = carrier_in.carrier_egress(&carrier).unwrap();
    let subscriber = Subscriber::<DoorState>::on(&downstream, physical.clone())
        .wire(DoorWire)
        .build()
        .unwrap();
    let up = upstream.enter_runtime().unwrap();
    let mid = carrier_in.enter_runtime().unwrap();
    let down = downstream.enter_runtime().unwrap();
    let publisher = publisher.activate(&up).unwrap();
    let subscriber = subscriber.activate(&down).unwrap();
    let mut forwarder = TestForwarder::new(mid, ingress, mid, egress);
    publisher.publish(&door(7, 1)).unwrap();
    assert_eq!(forwarder.forward_once().unwrap(), 1);
    assert_eq!(subscriber.try_recv().unwrap().expect("a frame").id, 7);
}

#[test]
fn a_physical_address_outside_the_filter_is_refused() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let filter = entity("door", 1).get_resource_uri(0x8001);
    let elsewhere = entity("door", 2).get_resource_uri(0x8001);
    let carried = door_wires();
    let carrier = CarrierRoute {
        physical_source: Some(&elsewhere),
        ..route(&filter, &carried)
    };
    assert!(matches!(
        a.carrier_ingress(&carrier),
        Err(StartupError::CarrierRefused { reason, .. }) if reason.contains("outside")
    ));
}

#[test]
fn a_wildcard_filter_is_fine_on_a_byte_only_leg() {
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let any = UUri::any();
    let carried = door_wires();
    a.carrier_ingress(&route(&any, &carried)).unwrap();
    a.carrier_egress(&route(&any, &carried)).unwrap();
}

#[test]
fn an_empty_wire_set_is_refused_at_setup() {
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let uri = topic();
    let refused = a.carrier_ingress(&route(&uri, &[])).unwrap_err();
    assert!(
        matches!(&refused, StartupError::CarrierRefused { reason, .. } if reason.contains("wires")),
        "{refused}"
    );
}

#[test]
fn wait_on_a_carrier_egress_is_refused_naming_the_route() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let uri = topic();
    let carried = door_wires();
    let carrier = CarrierRoute {
        qos: Qos::DEFAULT.with_overflow(Overflow::Wait {
            timeout: Duration::from_millis(1),
        }),
        ..route(&uri, &carried)
    };
    let refused = a.carrier_egress(&carrier).unwrap_err();
    match &refused {
        StartupError::CarrierRefused { route, reason, .. } => {
            assert_eq!(route.direction, RouteDirection::Send);
            assert_eq!(route.source, uri);
            assert!(reason.contains("Overflow::Wait"), "{reason}");
        }
        other => panic!("not named: {other}"),
    }
    // Wait on the ingress is fine: the upstream sender is the one who waits.
    a.carrier_ingress(&carrier).unwrap();
}

#[test]
fn a_qos_the_leg_cannot_honor_is_refused_naming_the_leg() {
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let uri = topic();
    let carried = door_wires();
    let carrier = CarrierRoute {
        qos: Qos::DEFAULT.with_overflow(Overflow::Wait {
            timeout: Duration::from_millis(1),
        }),
        ..route(&uri, &carried)
    };
    match a.carrier_ingress(&carrier).unwrap_err() {
        StartupError::QosNotHonored { route, setting, .. } => {
            assert_eq!(route.direction, RouteDirection::Receive);
            assert_eq!(setting, QosSetting::Overflow);
        }
        other => panic!("not named: {other}"),
    }
}

#[test]
fn a_kind_the_binding_cannot_carry_is_refused_naming_the_route() {
    let (setup, control) = scripted(Envelope::UFrameFields);
    control.refuse_carrier_kinds();
    let uri = topic();
    let carried = door_wires();
    let refused = setup.carrier_ingress(&route(&uri, &carried)).unwrap_err();
    match &refused {
        StartupError::CarrierRefused {
            binding,
            reason,
            route,
        } => {
            assert_eq!(*binding, "scripted-handoff");
            assert!(reason.contains("cannot carry every kind"), "{reason}");
            assert_eq!(route.kind, None);
        }
        other => panic!("not named: {other}"),
    }
    let one_kind = CarrierRoute {
        kind: Some(UMessageType::Publish),
        ..route(&uri, &carried)
    };
    setup.carrier_ingress(&one_kind).unwrap();
}

#[test]
fn carrier_routes_declare_exactly_what_the_binding_should_reserve() {
    let (setup, control) = scripted(Envelope::UFrameFields);
    let uri = topic();
    let carried = door_wires();
    let carrier = CarrierRoute {
        slots: 5,
        qos: Qos::DEFAULT.with_depth(3),
        ..route(&uri, &carried)
    };
    setup.carrier_ingress(&carrier).unwrap();
    setup.carrier_egress(&carrier).unwrap();
    let routes = control.routes();
    assert_eq!(routes.len(), 2);
    assert_eq!(routes[0].0, RouteDirection::Receive);
    assert_eq!(routes[0].4, 3 + 5 + 1, "ingress: depth + slots + 1");
    assert_eq!(routes[1].0, RouteDirection::Send);
    assert_eq!(routes[1].4, 5, "egress: slots");
}

// ------------------------------------------------------------ bytes unchanged

/// The bus a carrier leg sits on, for the four directions.
#[derive(Clone, Copy)]
enum Bus {
    Handoff(InMemoryBus),
    Lending(LendingBus),
}

impl Bus {
    fn setup(self, peer: u64) -> TransportSetup {
        match self {
            Self::Handoff(bus) => handoff(bus, peer),
            Self::Lending(bus) => lending(bus, peer),
        }
    }

    fn take_raw(self, peer: u64) -> (Vec<u8>, Option<Vec<u8>>) {
        match self {
            Self::Handoff(bus) => bus.take_raw(peer),
            Self::Lending(bus) => bus.take_raw(peer),
        }
        .expect("a queued frame")
    }
}

const LONG_TOKEN: &str = "access-token-1";
const TRACEPARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";

/// Publishes one frame through a carrier and checks both legs, byte for byte.
fn direction_preserves_bytes(ingress_bus: Bus, egress_bus: Bus, name: &str) {
    let provider = entity("door", 1);
    let uri = topic();
    let carried = door_wires();
    let carrier = CarrierRoute {
        max_token: 16,
        ..route(&uri, &carried)
    };
    let upstream = ingress_bus.setup(1);
    let carrier_in = ingress_bus.setup(2);
    let original_tap = ingress_bus.setup(5);
    let carrier_out = egress_bus.setup(3);
    let downstream = egress_bus.setup(4);
    let forwarded_tap = egress_bus.setup(6);
    let publisher = Publisher::<DoorState>::on(&upstream, &provider, 0x8001)
        .wire(DoorWire)
        .max_token_len(16)
        .allow_traceparent(true)
        .ttl(Duration::from_secs(600))
        .build()
        .unwrap();
    let ingress = carrier_in.carrier_ingress(&carrier).unwrap();
    let egress = carrier_out.carrier_egress(&carrier).unwrap();
    let subscriber = Subscriber::<DoorState>::on(&downstream, uri.clone())
        .wire(DoorWire)
        .max_token_len(16)
        .allow_traceparent(true)
        .build()
        .unwrap();
    // These subscribers are never polled: `take_raw` reads what reached them.
    let _tap_in = Subscriber::<DoorState>::on(&original_tap, uri.clone())
        .wire(DoorWire)
        .max_token_len(16)
        .allow_traceparent(true)
        .build()
        .unwrap();
    let _tap_out = Subscriber::<DoorState>::on(&forwarded_tap, uri.clone())
        .wire(DoorWire)
        .max_token_len(16)
        .allow_traceparent(true)
        .build()
        .unwrap();
    let up = upstream.enter_runtime().unwrap();
    let mid_in = carrier_in.enter_runtime().unwrap();
    let mid_out = carrier_out.enter_runtime().unwrap();
    let down = downstream.enter_runtime().unwrap();
    let _tap_in_rt = original_tap.enter_runtime().unwrap();
    let _tap_out_rt = forwarded_tap.enter_runtime().unwrap();
    let publisher = publisher.activate(&up).unwrap();
    let subscriber = subscriber.activate(&down).unwrap();

    let id = UUID::build();
    let fields = MessageFields {
        id: Some(&id),
        token: Some(LONG_TOKEN),
        traceparent: Some(TRACEPARENT),
        ..MessageFields::default()
    };
    publisher.publish_with(&fields, &door(42, 1)).unwrap();

    let (original_meta, original_payload) = ingress_bus.take_raw(5);
    assert!(!original_meta.is_empty(), "{name}");

    let mut forwarder = TestForwarder::new(mid_in, ingress, mid_out, egress);
    assert_eq!(forwarder.forward_once().unwrap(), 1, "{name}");

    let received = subscriber.try_recv().unwrap().expect("a forwarded frame");
    assert_eq!((received.id, received.open), (42, 1), "{name}");
    let header = received.header();
    assert_eq!(header.id, id, "{name}: identifier unchanged");
    assert_eq!(header.source, uri, "{name}: source unchanged");
    assert_eq!(header.payload_encoding, Some(DOOR_ENCODING), "{name}");
    assert_eq!(header.token(), Some(LONG_TOKEN), "{name}");
    assert_eq!(header.traceparent(), Some(TRACEPARENT), "{name}");
    drop(received);

    let (forwarded_meta, forwarded_payload) = egress_bus.take_raw(6);
    assert_eq!(
        forwarded_meta, original_meta,
        "{name}: metadata byte for byte"
    );
    assert_eq!(
        forwarded_payload, original_payload,
        "{name}: payload byte for byte"
    );
    assert_eq!(
        forwarded_payload.as_deref(),
        Some(&[0, 0, 0, 42, 1][..]),
        "{name}"
    );
}

#[test]
fn a_frame_keeps_every_byte_in_all_four_directions() {
    direction_preserves_bytes(
        Bus::Handoff(InMemoryBus::new(8, 4096)),
        Bus::Handoff(InMemoryBus::new(8, 4096)),
        "handoff-to-handoff",
    );
    direction_preserves_bytes(
        Bus::Handoff(InMemoryBus::new(8, 4096)),
        Bus::Lending(LendingBus::new()),
        "handoff-to-lending",
    );
    direction_preserves_bytes(
        Bus::Lending(LendingBus::new()),
        Bus::Handoff(InMemoryBus::new(8, 4096)),
        "lending-to-handoff",
    );
    direction_preserves_bytes(
        Bus::Lending(LendingBus::new()),
        Bus::Lending(LendingBus::new()),
        "lending-to-lending",
    );
}

#[test]
fn a_carrier_dispatches_ingress_bytes_straight_to_the_egress() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let (egress_setup, egress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let egress = egress_setup.carrier_egress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();
    assert_eq!(
        out_rt.routes()[0].send_mode,
        Some(SendMode::Direct),
        "a carrier egress has no value slots"
    );

    let frame = scripted_door_frame(&uri, 9);
    let metadata_ptr = frame.metadata.as_ptr() as usize;
    let payload_ptr = frame.chunks[0].as_ptr() as usize;
    let original_metadata = frame.metadata.to_vec();
    let original_payload = frame.chunks[0].to_vec();
    ingress_control.script([frame]);

    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    assert_eq!(forwarder.forward_once().unwrap(), 1);
    assert_eq!(egress_control.sends(), 1);
    assert_eq!(
        egress_control.last_pointers(),
        (metadata_ptr, payload_ptr),
        "no staged copy: the egress borrowed the ingress bytes"
    );
    assert_eq!(egress_control.last_metadata(), original_metadata);
    assert_eq!(egress_control.last_payload(), Some(original_payload));
}

#[test]
fn a_lending_ingress_frame_reaches_a_byte_only_egress_directly() {
    let bus = LendingBus::new();
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let upstream = lending(bus, 1);
    let carrier_in = lending(bus, 2);
    let (egress_setup, egress_control) = scripted(Envelope::UFrameFields);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .unwrap();
    let ingress = carrier_in.carrier_ingress(&carrier).unwrap();
    let egress = egress_setup.carrier_egress(&carrier).unwrap();
    let up = upstream.enter_runtime().unwrap();
    let in_rt = carrier_in.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();
    publisher
        .activate(&up)
        .unwrap()
        .publish(&door(11, 0))
        .unwrap();

    let mut seen = (0, 0);
    let handed = in_rt
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            seen = (
                frame.metadata().as_ptr() as usize,
                frame.payload().as_contiguous().unwrap().as_ptr() as usize,
            );
            assert_eq!(frame.encoding(), Some(DOOR_ENCODING));
            assert_eq!(frame.kind(), UMessageType::Publish);
            assert_eq!(frame.source(), &uri);
            assert_eq!(frame.sink(), None);
            out_rt.send(egress, &frame).unwrap();
            Flow::Continue
        })
        .unwrap();
    assert_eq!(handed, 1);
    assert_eq!(
        egress_control.last_pointers(),
        seen,
        "the byte-only egress got the retained frame's own bytes"
    );
    assert_eq!(egress_control.last_chunks(), 1);
}

#[test]
fn a_handoff_ingress_to_a_lending_egress_takes_one_copy_into_the_loan() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let bus = LendingBus::new();
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let egress_setup = lending(bus, 3);
    let downstream = lending(bus, 4);
    let tap = lending(bus, 5);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let egress = egress_setup.carrier_egress(&carrier).unwrap();
    let subscriber = Subscriber::<DoorState>::on(&downstream, uri.clone())
        .wire(DoorWire)
        .build()
        .unwrap();
    let _tap = Subscriber::<DoorState>::on(&tap, uri.clone())
        .wire(DoorWire)
        .build()
        .unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();
    let down = downstream.enter_runtime().unwrap();
    let _tap_rt = tap.enter_runtime().unwrap();
    let subscriber = subscriber.activate(&down).unwrap();
    assert_eq!(out_rt.routes()[0].send_mode, Some(SendMode::Lend));

    let frame = scripted_door_frame(&uri, 33);
    let original_metadata = frame.metadata.to_vec();
    let original_payload = frame.chunks[0].to_vec();
    ingress_control.script([frame]);
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    assert_eq!(forwarder.forward_once().unwrap(), 1);
    assert_eq!(subscriber.try_recv().unwrap().expect("frame").id, 33);
    let (metadata, payload) = bus.take_raw(5).expect("a queued frame");
    assert_eq!(metadata, original_metadata);
    assert_eq!(payload, Some(original_payload));
}

#[test]
fn a_chunked_frame_forwards_chunks_and_a_different_envelope_reencodes_only_metadata() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let (egress_setup, egress_control) = scripted(Envelope::NativeWholeFrame);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let egress = egress_setup.carrier_egress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();

    let (metadata, payload) = door_frame(Envelope::UFrameFields, &uri, 21, None);
    let frame = ScriptedFrame::chunked(metadata.clone(), payload.clone(), 2);
    let first_chunk_ptr = frame.chunks[0].as_ptr() as usize;
    ingress_control.script([frame]);
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    assert_eq!(forwarder.forward_once().unwrap(), 1);
    assert_eq!(
        egress_control.last_pointers().1,
        first_chunk_ptr,
        "no staged copy"
    );
    assert_eq!(egress_control.last_chunks(), 2, "chunk structure kept");
    assert_eq!(
        egress_control.last_payload(),
        Some(payload),
        "payload unchanged"
    );
    let head = egress_control.last_metadata();
    assert_ne!(head, metadata, "another envelope: metadata re-encoded");
    assert_eq!(&head[..4], b"UPFE");
}

/// The whole-frame envelope travels unchanged too: head and tail are the frame.
#[test]
fn a_whole_frame_envelope_passes_through_unchanged() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (ingress_setup, ingress_control) = scripted(Envelope::NativeWholeFrame);
    let (egress_setup, egress_control) = scripted(Envelope::NativeWholeFrame);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let egress = egress_setup.carrier_egress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();
    let (head, payload) = door_frame(Envelope::NativeWholeFrame, &uri, 9, None);
    let mut whole = head.clone();
    whole.extend_from_slice(&payload);
    ingress_control.script([ScriptedFrame::whole(whole.clone())]);
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    assert_eq!(forwarder.forward_once().unwrap(), 1);
    let mut out = egress_control.last_metadata();
    out.extend_from_slice(&egress_control.last_payload().expect("a tail"));
    assert_eq!(
        out, whole,
        "head and tail are the original frame, unchanged"
    );
}

/// The closed set applies to both legs: an egress outside it refuses the frame.
#[test]
fn a_frame_outside_the_egress_closed_set_is_refused_and_counted() {
    let uri = topic();
    let door = door_wires();
    let little = wires_of(LittleDoorWire);
    let ingress_route = route(&uri, &door);
    let egress_route = route(&uri, &little);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let (egress_setup, egress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&ingress_route).unwrap();
    let egress = egress_setup.carrier_egress(&egress_route).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();
    ingress_control.script([scripted_door_frame(&uri, 1)]);
    let mut refused = None;
    in_rt
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            refused = Some(out_rt.send(egress, &frame));
            Flow::Continue
        })
        .unwrap();
    assert!(matches!(
        refused,
        Some(Err(SendError::EncodingNotAdmitted(_)))
    ));
    assert_eq!(egress_control.sends(), 0);
    assert_eq!(out_rt.counters(egress.id()).refused, 1);
}

/// A handoff egress whose medium is full reports no capacity, and a frame is not
/// taken from the ingress until the receiver drains. The downstream medium holds
/// one frame in flight; the receiver's route is as deep as the bus allows.
#[test]
fn a_full_handoff_egress_reports_no_capacity_until_its_receiver_drains() {
    let (a, b) = (
        InMemoryBus::new(8, 4096),
        InMemoryBus::with_medium(8, 4096, 1),
    );
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let upstream = handoff(a, 1);
    let carrier_in = handoff(a, 2);
    let carrier_out = handoff(b, 3);
    let downstream = handoff(b, 4);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .unwrap();
    let ingress = carrier_in.carrier_ingress(&carrier).unwrap();
    let egress = carrier_out.carrier_egress(&carrier).unwrap();
    let subscriber = Subscriber::<DoorState>::on(&downstream, uri.clone())
        .wire(DoorWire)
        .build()
        .unwrap();
    let up = upstream.enter_runtime().unwrap();
    let in_rt = carrier_in.enter_runtime().unwrap();
    let out_rt = carrier_out.enter_runtime().unwrap();
    let down = downstream.enter_runtime().unwrap();
    let publisher = publisher.activate(&up).unwrap();
    let subscriber = subscriber.activate(&down).unwrap();
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    publisher.publish(&door(1, 0)).unwrap();
    assert_eq!(
        forwarder.forward_once().unwrap(),
        1,
        "the medium's one frame"
    );
    assert!(!out_rt.has_capacity(egress), "the medium is full");
    publisher.publish(&door(2, 0)).unwrap();
    assert_eq!(forwarder.forward_once().unwrap(), 0, "nothing was taken");
    assert_eq!(forwarder.stats().no_capacity, 1);
    assert_eq!(subscriber.try_recv().unwrap().expect("frame").id, 1);
    assert!(out_rt.has_capacity(egress));
    assert_eq!(
        forwarder.forward_once().unwrap(),
        1,
        "the frame that waited moves"
    );
    assert_eq!(subscriber.try_recv().unwrap().expect("frame").id, 2);
}

/// The egress `r3_4_2_…` asks for capacity from inside a decode.
static CAPACITY_PROBE: std::sync::OnceLock<(TransportRuntime, EgressRoute)> =
    std::sync::OnceLock::new();
/// What that ask answered: 0 not asked, 1 no capacity, 2 capacity.
static CAPACITY_SEEN: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// `DoorWire`, except that its `decode` first asks the probed egress for capacity.
#[derive(Clone, Copy)]
struct ProbingDoorWire;

impl Wire<DoorState> for ProbingDoorWire {
    fn describe(&self) -> WireInfo {
        DoorWire.describe()
    }
    fn encoded_size(&self, value: &DoorState) -> EncodedSize {
        DoorWire.encoded_size(value)
    }
    fn encode(&self, value: &DoorState, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        DoorWire.encode(value, out)
    }
    fn init_slot<'s>(&self, slot: StartupSlot<'s, DoorState>) -> Ready<'s, DoorState> {
        DoorWire.init_slot(slot)
    }
    fn decode(
        &self,
        encoding: up_rust_core::uattributes::PayloadEncoding,
        bytes: &[u8],
        into: &mut DoorState,
    ) -> Result<(), WireError> {
        if let Some((runtime, egress)) = CAPACITY_PROBE.get() {
            let seen = if runtime.has_capacity(*egress) { 2 } else { 1 };
            CAPACITY_SEEN.store(seen, std::sync::atomic::Ordering::SeqCst);
        }
        DoorWire.decode(encoding, bytes, into)
    }
}

/// R3-4.2. What a forwarder sees on a handoff egress whose receiver holds every
/// buffer. The downstream route is one frame deep on a one-frame bus, and the
/// receive that decodes its frame holds the one buffer: a send at that moment
/// would be refused `Busy`, so the egress must report no capacity. Before the
/// read, a full `DropOldest` queue still takes a frame, and after it the buffer
/// is back, so both times it reports capacity. With the bus's `accepts` always
/// answering yes (the reviewer's m13), the egress claims room it does not have.
/// The re-check asked for a full `Wait` receiver, but this bus declares no `Wait`
/// support and refuses such a route at setup.
#[test]
fn r3_4_2_a_handoff_egress_sees_no_capacity_while_its_receiver_holds_every_buffer() {
    let (a, b) = (InMemoryBus::new(8, 4096), InMemoryBus::new(1, 4096));
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let upstream = handoff(a, 1);
    let carrier_in = handoff(a, 2);
    let carrier_out = handoff(b, 3);
    let downstream = handoff(b, 4);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .unwrap();
    let ingress = carrier_in.carrier_ingress(&carrier).unwrap();
    let egress = carrier_out.carrier_egress(&carrier).unwrap();
    let subscriber = Subscriber::<DoorState>::on(&downstream, uri.clone())
        .wire(ProbingDoorWire)
        .qos(
            Qos::DEFAULT
                .with_depth(1)
                .with_overflow(Overflow::DropOldest),
        )
        .build()
        .unwrap();
    let up = upstream.enter_runtime().unwrap();
    let in_rt = carrier_in.enter_runtime().unwrap();
    let out_rt = carrier_out.enter_runtime().unwrap();
    let down = downstream.enter_runtime().unwrap();
    let publisher = publisher.activate(&up).unwrap();
    let subscriber = subscriber.activate(&down).unwrap();
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    publisher.publish(&door(1, 0)).unwrap();
    assert_eq!(forwarder.forward_once().unwrap(), 1, "the frame moves");
    assert!(
        out_rt.has_capacity(egress),
        "a full DropOldest queue still takes a frame"
    );
    assert!(CAPACITY_PROBE.set((out_rt, egress)).is_ok());
    assert_eq!(subscriber.try_recv().unwrap().expect("frame").id, 1);
    assert_eq!(
        CAPACITY_SEEN.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "no capacity while the receive holds the route's one buffer"
    );
    assert!(
        out_rt.has_capacity(egress),
        "capacity once the buffer is back"
    );
}

#[test]
fn a_chunked_frame_is_copied_once_into_a_lending_egress_with_the_same_envelope() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let bus = LendingBus::new();
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let egress_setup = lending(bus, 3);
    let tap = lending(bus, 5);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let egress = egress_setup.carrier_egress(&carrier).unwrap();
    let _tap = Subscriber::<DoorState>::on(&tap, uri.clone())
        .wire(DoorWire)
        .build()
        .unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();
    let _tap_rt = tap.enter_runtime().unwrap();
    let (metadata, payload) = door_frame(Envelope::UFrameFields, &uri, 22, None);
    ingress_control.script([ScriptedFrame::chunked(metadata.clone(), payload.clone(), 3)]);
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    assert_eq!(forwarder.forward_once().unwrap(), 1);
    let (carried_meta, carried_payload) = bus.take_raw(5).expect("a queued frame");
    assert_eq!(carried_meta, metadata);
    assert_eq!(carried_payload, Some(payload));
}

#[test]
fn a_frame_with_no_payload_keeps_its_absence() {
    let uri = topic();
    let carried = door_wires();
    let wire = Wire::<DoorState>::describe(&DoorWire);
    let harness = EnvelopeHarness::new(
        Envelope::UFrameFields,
        wire.wire_id,
        wire.payload_family,
        wire.identity,
    );
    let id = UUID::build();
    let parts = TxHeaderParts {
        kind: UMessageType::Publish,
        priority: None,
        id: &id,
        reqid: None,
        ttl: None,
        comm_status: None,
        permission_level: None,
        source: &uri,
        sink: None,
        token: None,
        traceparent: None,
        payload_encoding: None,
        native_type_token: None,
    };
    let (head, bytes) = harness.encode(&parts, None).unwrap();
    assert_eq!(head, bytes.len());
    let carrier = route(&uri, &carried);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let (egress_setup, egress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let egress = egress_setup.carrier_egress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();
    let mut frame = ScriptedFrame::contiguous(bytes.clone(), Vec::new());
    frame.present = false;
    ingress_control.script([frame]);
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    assert_eq!(forwarder.forward_once().unwrap(), 1);
    assert_eq!(egress_control.last_metadata(), bytes);
    assert_eq!(egress_control.last_payload(), None, "absent stays absent");
}

/// A frame with no payload, arriving as a whole-frame envelope and leaving on a
/// selected-wire egress, still has a head the other side can parse.
#[test]
fn a_no_payload_frame_reencoded_for_a_selected_wire_egress_stays_parseable() {
    let uri = topic();
    let carried = door_wires();
    let wire = Wire::<DoorState>::describe(&DoorWire);
    let whole = EnvelopeHarness::new(
        Envelope::NativeWholeFrame,
        wire.wire_id,
        wire.payload_family,
        wire.identity,
    );
    let id = UUID::build();
    let parts = TxHeaderParts {
        kind: UMessageType::Publish,
        priority: None,
        id: &id,
        reqid: None,
        ttl: None,
        comm_status: None,
        permission_level: None,
        source: &uri,
        sink: None,
        token: None,
        traceparent: None,
        payload_encoding: None,
        native_type_token: None,
    };
    let (_, frame) = whole.encode(&parts, None).unwrap();
    let carrier = route(&uri, &carried);
    let (ingress_setup, ingress_control) = scripted(Envelope::NativeWholeFrame);
    let (egress_setup, egress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let egress = egress_setup.carrier_egress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();
    let mut scripted_frame = ScriptedFrame::whole(frame);
    scripted_frame.present = true;
    ingress_control.script([scripted_frame]);
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    assert_eq!(forwarder.forward_once().unwrap(), 1);
    assert_eq!(egress_control.last_payload(), None, "absent stays absent");
    let metadata = egress_control.last_metadata();
    let target = EnvelopeHarness::new(
        Envelope::UFrameFields,
        wire.wire_id,
        wire.payload_family,
        wire.identity,
    );
    let (header, _) = target
        .parse(
            &metadata,
            PayloadBytes::Absent,
            &up_rust_core::frame::header::HeaderLimits::default(),
        )
        .expect("the re-encoded head parses");
    assert_eq!(header.id, id);
}

#[test]
fn a_reply_token_and_the_kind_go_through() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let (egress_setup, egress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let egress = egress_setup.carrier_egress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();
    let mut frame = scripted_door_frame(&uri, 3);
    frame.reply = ReplyToken(77);
    ingress_control.script([frame]);
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    assert_eq!(forwarder.forward_once().unwrap(), 1);
    assert_eq!(egress_control.last_reply(), ReplyToken(77));
    assert_eq!(egress_control.last_kind(), Some(UMessageType::Publish));
}

// -------------------------------------------------------- admission and limits

#[test]
fn a_frame_outside_the_filter_is_dropped_and_counted() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let other = entity("elsewhere", 7).get_resource_uri(0x8001);
    ingress_control.script([scripted_door_frame(&other, 1)]);
    let handed = in_rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
        .unwrap();
    assert_eq!(handed, 0);
    let counters = in_rt.counters(ingress.id());
    assert_eq!(counters.refused, 1);
    assert_eq!(counters.carried, 0);
}

#[test]
fn a_wildcard_filter_on_a_byte_only_leg_carries_any_source() {
    let any = UUri::any();
    let carried = door_wires();
    let carrier = route(&any, &carried);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let elsewhere = entity("elsewhere", 7).get_resource_uri(0x8001);
    ingress_control.script([
        scripted_door_frame(&elsewhere, 1),
        scripted_door_frame(&topic(), 2),
    ]);
    let handed = in_rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
        .unwrap();
    assert_eq!(handed, 2);
}

#[test]
fn a_frame_of_a_wire_outside_the_closed_set_is_refused() {
    let uri = topic();
    // The carrier carries the little-door wire only; a door frame is not in the set.
    let carried = wires_of(LittleDoorWire);
    let carrier = route(&uri, &carried);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    ingress_control.script([scripted_door_frame(&uri, 1)]);
    let handed = in_rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
        .unwrap();
    assert_eq!(handed, 0);
    assert_eq!(in_rt.counters(ingress.id()).refused, 1);
}

#[test]
fn a_frame_above_the_route_payload_limit_is_refused() {
    let uri = topic();
    let carried = door_wires();
    let carrier = CarrierRoute::new(&uri, &carried, 2);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    ingress_control.script([scripted_door_frame(&uri, 1)]);
    let handed = in_rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
        .unwrap();
    assert_eq!(handed, 0);
    assert_eq!(in_rt.counters(ingress.id()).refused, 1);
}

#[test]
fn a_malformed_frame_is_refused_without_panic_and_counted() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    ingress_control.script([ScriptedFrame::contiguous(vec![1, 2, 3], vec![4])]);
    let handed = in_rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
        .unwrap();
    assert_eq!(handed, 0);
    assert_eq!(in_rt.counters(ingress.id()).refused, 1);
}

#[test]
fn a_full_peer_table_refuses_the_frame_and_counts_it() {
    let uri = topic();
    let carried = door_wires();
    let carrier = CarrierRoute {
        max_peers: 1,
        ..route(&uri, &carried)
    };
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let mut first = scripted_door_frame(&uri, 1);
    first.peer = PeerRef {
        peer: 5,
        incarnation: 1,
    };
    let mut second = scripted_door_frame(&uri, 2);
    second.peer = PeerRef {
        peer: 6,
        incarnation: 1,
    };
    ingress_control.script([first, second]);
    let handed = in_rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
        .unwrap();
    assert_eq!(handed, 1, "the second sender does not fit the fixed table");
    assert_eq!(in_rt.counters(ingress.id()).refused, 1);
}

#[test]
fn a_restarted_peers_stale_frame_is_refused() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let mut first = scripted_door_frame(&uri, 1);
    first.peer = PeerRef {
        peer: 9,
        incarnation: 2,
    };
    let mut stale = scripted_door_frame(&uri, 2);
    stale.peer = PeerRef {
        peer: 9,
        incarnation: 1,
    };
    ingress_control.script([first, stale]);
    let mut seen = Vec::new();
    in_rt
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            seen.push(frame.payload().len());
            Flow::Continue
        })
        .unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(in_rt.counters(ingress.id()).stale_peer, 1);
}

#[test]
fn flow_stop_leaves_the_rest_with_the_transport() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    ingress_control.script([
        scripted_door_frame(&uri, 1),
        scripted_door_frame(&uri, 2),
        scripted_door_frame(&uri, 3),
    ]);
    let mut taken = 0;
    in_rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| {
            taken += 1;
            Flow::Stop
        })
        .unwrap();
    assert_eq!(taken, 1);
    in_rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| {
            taken += 1;
            Flow::Continue
        })
        .unwrap();
    assert_eq!(taken, 3, "frames not taken were still queued");
}

// ------------------------------------------------------------------- TTL and clock

fn carrier_with_clock(
    clock: &'static TestClock,
) -> (TransportRuntime, IngressRoute, ScriptedControl, UUri) {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (binding, control) = ScriptedHandoff::new(Envelope::UFrameFields);
    let setup = TransportSetup::handoff_on(&memory(), clock, binding).unwrap();
    let ingress = setup.carrier_ingress(&carrier).unwrap();
    (setup.enter_runtime().unwrap(), ingress, control, uri)
}

#[test]
fn an_expired_frame_is_not_forwarded_and_is_counted() {
    let clock = TestClock::leak(1_000, u64::MAX / 4);
    let (in_rt, ingress, control, uri) = carrier_with_clock(clock);
    // A frame with a 1 ms TTL, built from a real UUID, is expired on this far-future clock.
    let (metadata, payload) = door_frame(Envelope::UFrameFields, &uri, 1, Some(1));
    control.script([ScriptedFrame::contiguous(metadata, payload)]);
    let handed = in_rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
        .unwrap();
    assert_eq!(handed, 0, "an expired message must not be processed");
    assert_eq!(in_rt.counters(ingress.id()).expired, 1);
}

#[test]
fn a_frame_within_its_ttl_is_forwarded() {
    let clock = TestClock::leak(1_000, 1_000_000);
    let (in_rt, ingress, control, uri) = carrier_with_clock(clock);
    // UUID time is the real wall clock, so a ttl long enough for this test passes
    // when the test clock agrees with it.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    clock.advance_wall_ms(now.saturating_sub(1_000_000));
    let (metadata, payload) = door_frame(Envelope::UFrameFields, &uri, 1, Some(3_600_000));
    control.script([ScriptedFrame::contiguous(metadata, payload)]);
    let mut handed = 0;
    in_rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| {
            handed += 1;
            Flow::Continue
        })
        .unwrap();
    assert_eq!(handed, 1);
}

#[test]
fn a_frame_with_a_ttl_is_refused_when_the_clock_cannot_be_read() {
    let clock = TestClock::leak(1_000, 1_000_000);
    let (in_rt, ingress, control, uri) = carrier_with_clock(clock);
    clock.break_wall_clock();
    let (metadata, payload) = door_frame(Envelope::UFrameFields, &uri, 1, Some(3_600_000));
    let (no_ttl_meta, no_ttl_payload) = door_frame(Envelope::UFrameFields, &uri, 2, None);
    control.script([
        ScriptedFrame::contiguous(metadata, payload),
        ScriptedFrame::contiguous(no_ttl_meta, no_ttl_payload),
    ]);
    let mut handed = 0;
    in_rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| {
            handed += 1;
            Flow::Continue
        })
        .unwrap();
    assert_eq!(handed, 1, "the frame with no ttl needs no clock");
    assert_eq!(in_rt.counters(ingress.id()).clock_unavailable, 1);
}

// ------------------------------------------------------- capacity and parking

/// A lending carrier: frames published upstream are taken from a real ingress.
struct LendingCarrier {
    publisher: Publisher<DoorState>,
    ingress: IngressRoute,
    egress: EgressRoute,
    in_rt: TransportRuntime,
    out_rt: TransportRuntime,
    subscriber: Subscriber<DoorState>,
}

fn lending_carrier(qos: Qos, slots: usize, egress_options: LendingOptions) -> LendingCarrier {
    let uri = topic();
    let carried = door_wires();
    let carrier = CarrierRoute {
        qos,
        slots,
        ..route(&uri, &carried)
    };
    let upstream_bus = LendingBus::new();
    let downstream_bus = LendingBus::new();
    let upstream = lending(upstream_bus, 1);
    let carrier_in = lending(upstream_bus, 2);
    let carrier_out = lending_with(downstream_bus, 3, egress_options);
    let downstream = lending(downstream_bus, 4);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .qos(qos)
        .slots(16)
        .build()
        .unwrap();
    let ingress = carrier_in.carrier_ingress(&carrier).unwrap();
    let egress = carrier_out.carrier_egress(&carrier).unwrap();
    let subscriber = Subscriber::<DoorState>::on(&downstream, uri.clone())
        .wire(DoorWire)
        .qos(qos)
        .build()
        .unwrap();
    let up = upstream.enter_runtime().unwrap();
    let in_rt = carrier_in.enter_runtime().unwrap();
    let out_rt = carrier_out.enter_runtime().unwrap();
    let down = downstream.enter_runtime().unwrap();
    LendingCarrier {
        publisher: publisher.activate(&up).unwrap(),
        ingress,
        egress,
        in_rt,
        out_rt,
        subscriber: subscriber.activate(&down).unwrap(),
    }
}

#[test]
fn a_full_loan_pool_is_no_capacity_and_a_take_is_not_attempted() {
    let carrier = lending_carrier(
        Qos::DEFAULT
            .with_depth(1)
            .with_overflow(Overflow::DropOldest),
        1,
        LendingOptions {
            slots: 1,
            ..LendingOptions::default()
        },
    );
    carrier.publisher.publish(&door(1, 0)).unwrap();
    let mut forwarder = TestForwarder::new(
        carrier.in_rt,
        carrier.ingress,
        carrier.out_rt,
        carrier.egress,
    );
    assert_eq!(forwarder.forward_once().unwrap(), 1);
    // The one loan is now held by the queued frame at the receiver.
    assert!(!carrier.out_rt.has_capacity(carrier.egress));
    carrier.publisher.publish(&door(2, 0)).unwrap();
    assert_eq!(forwarder.forward_once().unwrap(), 0, "nothing was taken");
    assert_eq!(forwarder.stats().no_capacity, 1);
    assert_eq!(carrier.subscriber.try_recv().unwrap().expect("frame").id, 1);
    // Reading frees the loan, and the frame that stayed in the ingress queue moves.
    assert!(carrier.out_rt.has_capacity(carrier.egress));
    assert_eq!(forwarder.forward_once().unwrap(), 1);
    assert_eq!(carrier.subscriber.try_recv().unwrap().expect("frame").id, 2);
}

#[test]
fn true_from_has_capacity_is_not_a_promise_on_a_lending_egress() {
    // Plenty of loans, and a receiver whose queue is full under DropNewest: the
    // binding can only tell at the send, where the route's overflow policy applies.
    let carrier = lending_carrier(
        Qos::DEFAULT
            .with_depth(1)
            .with_overflow(Overflow::DropNewest),
        2,
        LendingOptions {
            slots: 8,
            ..LendingOptions::default()
        },
    );
    let mut forwarder = TestForwarder::new(
        carrier.in_rt,
        carrier.ingress,
        carrier.out_rt,
        carrier.egress,
    );
    for id in 1..=3 {
        carrier.publisher.publish(&door(id, 0)).unwrap();
        assert!(carrier.out_rt.has_capacity(carrier.egress));
        assert_eq!(forwarder.forward_once().unwrap(), 1);
    }
    // DropNewest applied at the send: the first frame stayed, the others were dropped.
    assert_eq!(carrier.subscriber.try_recv().unwrap().expect("frame").id, 1);
    assert!(carrier.subscriber.try_recv().unwrap().is_none());
    assert_eq!(forwarder.stats().forwarded, 3);
}

#[test]
fn drop_oldest_on_the_egress_keeps_the_newest_at_the_receiver() {
    let carrier = lending_carrier(
        Qos::DEFAULT
            .with_depth(2)
            .with_overflow(Overflow::DropOldest),
        2,
        LendingOptions::default(),
    );
    let mut forwarder = TestForwarder::new(
        carrier.in_rt,
        carrier.ingress,
        carrier.out_rt,
        carrier.egress,
    );
    for id in 1..=5 {
        carrier.publisher.publish(&door(id, 0)).unwrap();
        assert_eq!(forwarder.forward_once().unwrap(), 1);
    }
    assert_eq!(carrier.subscriber.try_recv().unwrap().expect("frame").id, 4);
    assert_eq!(carrier.subscriber.try_recv().unwrap().expect("frame").id, 5);
    assert!(carrier.subscriber.try_recv().unwrap().is_none());
}

/// The rule from the QoS design: no worker ever blocks. A full receiver downstream
/// is an overflow outcome, never a wait.
#[test]
#[cfg_attr(
    miri,
    ignore = "a wall-clock bound: interpreted code says nothing about it"
)]
fn a_full_receiver_never_blocks_the_worker() {
    let carrier = lending_carrier(
        Qos::DEFAULT
            .with_depth(1)
            .with_overflow(Overflow::DropNewest),
        2,
        LendingOptions::default(),
    );
    let mut forwarder = TestForwarder::new(
        carrier.in_rt,
        carrier.ingress,
        carrier.out_rt,
        carrier.egress,
    );
    carrier.publisher.publish(&door(0, 0)).unwrap();
    assert_eq!(
        forwarder.forward_once().unwrap(),
        1,
        "fills the receiver's queue"
    );
    let started = std::time::Instant::now();
    for id in 1..200 {
        carrier.publisher.publish(&door(id, 0)).unwrap();
        assert_eq!(forwarder.forward_once().unwrap(), 1);
    }
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the worker never waited: {:?}",
        started.elapsed()
    );
}

/// Wait on the ingress: the upstream sender waits, within its bound. The worker
/// does not wait for it; it simply has not taken the frame yet.
#[test]
fn wait_on_the_ingress_makes_the_upstream_sender_wait_not_the_worker() {
    let uri = topic();
    let carried = door_wires();
    let wait = Qos::DEFAULT.with_depth(1).with_overflow(Overflow::Wait {
        timeout: Duration::from_millis(20),
    });
    let ingress_route = CarrierRoute {
        qos: wait,
        ..route(&uri, &carried)
    };
    let egress_route = CarrierRoute {
        qos: Qos::DEFAULT
            .with_depth(2)
            .with_overflow(Overflow::DropOldest),
        ..route(&uri, &carried)
    };
    let (a, b) = (LendingBus::new(), LendingBus::new());
    let upstream = lending(a, 1);
    let carrier_in = lending(a, 2);
    let carrier_out = lending(b, 3);
    let downstream = lending(b, 4);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .unwrap();
    let ingress = carrier_in.carrier_ingress(&ingress_route).unwrap();
    let egress = carrier_out.carrier_egress(&egress_route).unwrap();
    let subscriber = Subscriber::<DoorState>::on(&downstream, uri.clone())
        .wire(DoorWire)
        .qos(
            Qos::DEFAULT
                .with_depth(2)
                .with_overflow(Overflow::DropOldest),
        )
        .build()
        .unwrap();
    let up = upstream.enter_runtime().unwrap();
    let in_rt = carrier_in.enter_runtime().unwrap();
    let out_rt = carrier_out.enter_runtime().unwrap();
    let down = downstream.enter_runtime().unwrap();
    let publisher = publisher.activate(&up).unwrap();
    let subscriber = subscriber.activate(&down).unwrap();
    publisher.publish(&door(1, 0)).unwrap();
    // The ingress queue is full: the upstream sender waits, to its bound.
    let started = std::time::Instant::now();
    assert!(matches!(
        publisher.publish(&door(2, 0)),
        Err(SendError::TimedOut)
    ));
    assert!(started.elapsed() >= Duration::from_millis(20));
    // The worker takes the queued frame at once and forwards it.
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    assert_eq!(forwarder.forward_once().unwrap(), 1);
    assert_eq!(subscriber.try_recv().unwrap().expect("frame").id, 1);
    // With room again, the upstream send succeeds.
    publisher.publish(&door(3, 0)).unwrap();
}

/// One `receive` call examines a bounded number of frames, so a flood cannot hold
/// a worker on one route (a stalled route cannot delay others, design 7).
#[test]
fn a_flood_cannot_hold_the_worker_on_one_route() {
    let uri = topic();
    let carried = door_wires();
    let carrier = CarrierRoute {
        qos: Qos::DEFAULT.with_depth(2),
        ..route(&uri, &carried)
    };
    let (setup, control) = scripted(Envelope::UFrameFields);
    let ingress = setup.carrier_ingress(&carrier).unwrap();
    let rt = setup.enter_runtime().unwrap();
    control.script((1..=20).map(|id| scripted_door_frame(&uri, id)));
    let handed = rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
        .unwrap();
    assert_eq!(handed, 3, "depth + 1 frames per call");
    let handed = rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
        .unwrap();
    assert_eq!(handed, 3, "the rest stayed in the binding's queue");
}

#[test]
fn has_capacity_reads_the_binding_and_is_false_for_a_foreign_route() {
    let (setup, control) = scripted(Envelope::UFrameFields);
    let uri = topic();
    let carried = door_wires();
    let egress = setup.carrier_egress(&route(&uri, &carried)).unwrap();
    let (other_setup, _other_control) = scripted(Envelope::UFrameFields);
    let foreign = other_setup.carrier_egress(&route(&uri, &carried)).unwrap();
    let rt = setup.enter_runtime().unwrap();
    assert!(rt.has_capacity(egress));
    control.set_capacity(false);
    assert!(!rt.has_capacity(egress));
    assert!(!rt.has_capacity(foreign), "a route on another transport");
}

#[test]
fn a_frame_can_be_parked_and_resent_when_capacity_returns() {
    let carrier = lending_carrier(Qos::DEFAULT, 2, LendingOptions::default());
    carrier.publisher.publish(&door(77, 1)).unwrap();
    let mut parked = None;
    carrier
        .in_rt
        .receive(carrier.ingress, &mut |frame: CarriedFrame<'_>| {
            parked = Some(frame.park().expect("parked"));
            Flow::Stop
        })
        .unwrap();
    let parked = parked.expect("a frame was parked");
    assert_eq!(carrier.in_rt.counters(carrier.ingress.id()).parked, 1);
    // The parked frame's bytes stay valid across other traffic.
    let bytes_before = parked.frame().payload().as_contiguous().unwrap().to_vec();
    carrier.publisher.publish(&door(78, 1)).unwrap();
    assert_eq!(
        parked.frame().payload().as_contiguous().unwrap(),
        bytes_before
    );
    let report = carrier
        .out_rt
        .send(carrier.egress, &parked.frame())
        .expect("sent");
    assert!(report.counts_exact);
    assert_eq!(report.delivered, 1);
    drop(parked);
    assert_eq!(
        carrier.subscriber.try_recv().unwrap().expect("frame").id,
        77
    );
    // The slot went back: the next frame moves too.
    let mut forwarder = TestForwarder::new(
        carrier.in_rt,
        carrier.ingress,
        carrier.out_rt,
        carrier.egress,
    );
    assert_eq!(forwarder.forward_once().unwrap(), 1);
    assert_eq!(
        carrier.subscriber.try_recv().unwrap().expect("frame").id,
        78
    );
}

/// Over a frame that really frees its bytes when dropped. A parked frame dropped
/// inside the sink is not freed there: the sink's argument borrows the bytes until
/// the sink returns, and a free inside the call is undefined behaviour that Miri
/// reports (it did on `a3b8396`). It is freed once the sink returns, before
/// `receive` does, and the route keeps working. The API has no way to hold a view
/// across the park (`tests/ui/two_mode_merged/carrier_view_held_across_park.rs`).
#[test]
fn a_parked_frame_dropped_inside_the_sink_is_freed_when_the_sink_returns() {
    use core::sync::atomic::Ordering;
    let clock = doubles::Clock::leak(1_000_000_000, 2_000_000_000_000);
    let shared = doubles::Shared::leak();
    let memory = doubles::memory();
    let setup = TransportSetup::lending_on(&memory, clock, doubles::LendingDouble(shared)).unwrap();
    let uri = doubles::uri(9, 0x8001);
    let carried: &'static [CarriedWire] = Box::leak(Box::new([CarriedWire {
        wire_id: doubles::WIRE_ID,
        payload_family: doubles::FAMILY_ID,
        identity: up_rust_core::wire::WirePayloadIdentity::serialized(doubles::ENCODING),
        alignment: 1,
    }]));
    let mut carrier = CarrierRoute::new(&uri, carried, 64);
    carrier.kind = Some(UMessageType::Publish);
    carrier.physical_source = Some(&uri);
    let ingress = setup.carrier_ingress(&carrier).unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let frame = doubles::publish_frame(
        &uri,
        doubles::uuid_at(2_000_000_000_000, 1),
        None,
        None,
        &[1, 2, 3, 4],
    );
    shared
        .queue
        .lock()
        .unwrap()
        .push_back((frame.clone(), doubles::peer(7, 1)));
    runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            let kept = frame.park().expect("parked");
            assert_eq!(
                shared.frames_dropped.load(Ordering::SeqCst),
                0,
                "control: the lease holds the frame"
            );
            drop(kept);
            assert_eq!(
                shared.frames_dropped.load(Ordering::SeqCst),
                0,
                "not inside the sink: its argument still borrows the bytes"
            );
            Flow::Stop
        })
        .unwrap();
    assert_eq!(
        shared.frames_dropped.load(Ordering::SeqCst),
        1,
        "freed exactly once, when the sink returned"
    );
    // The slot is back, so the route still takes frames.
    shared
        .queue
        .lock()
        .unwrap()
        .push_back((frame, doubles::peer(7, 1)));
    let handed = runtime
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            assert!(!frame.metadata().is_empty());
            Flow::Continue
        })
        .unwrap();
    assert_eq!(handed, 1, "the route still works");
}

#[test]
fn a_handoff_ingress_frame_cannot_be_parked() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (setup, control) = scripted(Envelope::UFrameFields);
    let ingress = setup.carrier_ingress(&carrier).unwrap();
    let rt = setup.enter_runtime().unwrap();
    control.script([scripted_door_frame(&uri, 1)]);
    let mut refused = 0;
    rt.receive(ingress, &mut |frame: CarriedFrame<'_>| {
        if frame.park().is_err() {
            refused += 1;
        }
        Flow::Continue
    })
    .unwrap();
    assert_eq!(refused, 1);
}

#[test]
fn a_frame_whose_egress_refused_can_still_be_parked() {
    // Lending ingress, an egress that is busy for one send: the frame that lost
    // the race between the check and the send is parked, then forwarded.
    let bus = LendingBus::new();
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let upstream = lending(bus, 1);
    let carrier_in = lending(bus, 2);
    let (egress_setup, egress_control) = scripted(Envelope::UFrameFields);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .unwrap();
    let ingress = carrier_in.carrier_ingress(&carrier).unwrap();
    let egress = egress_setup.carrier_egress(&carrier).unwrap();
    let up = upstream.enter_runtime().unwrap();
    let in_rt = carrier_in.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();
    publisher
        .activate(&up)
        .unwrap()
        .publish(&door(5, 1))
        .unwrap();
    egress_control.refuse_sends(1);
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    assert_eq!(forwarder.forward_once().unwrap(), 0);
    assert!(
        forwarder.has_parked(),
        "egress filled between check and send"
    );
    assert_eq!(
        forwarder.forward_once().unwrap(),
        1,
        "the parked frame goes out"
    );
    assert!(!forwarder.has_parked());
    assert_eq!(egress_control.sends(), 1);
    assert_eq!(forwarder.stats().dropped, 0);
    let _ = bus;
}

#[test]
fn an_egress_refusal_on_a_handoff_ingress_drops_the_frame_and_counts_it() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let (egress_setup, egress_control) = scripted(Envelope::UFrameFields);
    let ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let egress = egress_setup.carrier_egress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();
    ingress_control.script([scripted_door_frame(&uri, 1)]);
    egress_control.refuse_sends(1);
    let mut forwarder = TestForwarder::new(in_rt, ingress, out_rt, egress);
    assert_eq!(forwarder.forward_once().unwrap(), 0);
    assert!(!forwarder.has_parked(), "a borrowed frame cannot be parked");
    assert_eq!(forwarder.stats().dropped, 1);
    assert_eq!(out_rt.counters(egress.id()).busy, 1);
}

#[test]
fn parked_frames_hold_the_retained_budget() {
    // One slot beyond the frame in hand: parking the frame leaves no room to take
    // another, so the next frame stays in the binding's queue, safely.
    let carrier = lending_carrier(Qos::DEFAULT.with_depth(4), 1, LendingOptions::default());
    carrier.publisher.publish(&door(1, 0)).unwrap();
    carrier.publisher.publish(&door(2, 0)).unwrap();
    let mut parked = None;
    let handed = carrier
        .in_rt
        .receive(carrier.ingress, &mut |frame: CarriedFrame<'_>| {
            parked = Some(frame.park().expect("parked"));
            Flow::Continue
        })
        .unwrap();
    assert_eq!(handed, 1, "budget spent: nothing else is taken");
    let empty_handed = carrier
        .in_rt
        .receive(carrier.ingress, &mut |_frame: CarriedFrame<'_>| {
            Flow::Continue
        })
        .unwrap();
    assert_eq!(
        empty_handed, 0,
        "the second frame stays in the binding's queue"
    );
    drop(parked);
    let mut got = 0;
    carrier
        .in_rt
        .receive(carrier.ingress, &mut |_frame: CarriedFrame<'_>| {
            got += 1;
            Flow::Continue
        })
        .unwrap();
    assert_eq!(got, 1, "with the slot back, the next frame arrives");
}

#[test]
fn the_park_budget_is_bounded() {
    let carrier = lending_carrier(Qos::DEFAULT.with_depth(8), 2, LendingOptions::default());
    for id in 0..4 {
        carrier.publisher.publish(&door(id, 0)).unwrap();
    }
    // The budget with two slots is one parked frame.
    let mut parked: Vec<ParkedFrame> = Vec::new();
    let mut refused = 0;
    carrier
        .in_rt
        .receive(carrier.ingress, &mut |frame: CarriedFrame<'_>| {
            match frame.park() {
                Ok(kept) => parked.push(kept),
                Err(_) => refused += 1,
            }
            Flow::Continue
        })
        .unwrap();
    assert_eq!(parked.len(), 1, "only one frame fits the budget");
    assert_eq!(
        refused, 3,
        "the others were refused and dropped, never a panic"
    );
}

#[test]
fn a_parked_frame_that_expires_is_refused_at_send_and_counted() {
    let clock = TestClock::leak(1_000, 1_000_000);
    let bus = LendingBus::new();
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let upstream = TransportSetup::lending_on(&memory(), clock, bus.participant(1)).unwrap();
    let carrier_in = TransportSetup::lending_on(&memory(), clock, bus.participant(2)).unwrap();
    let (egress_setup, egress_control) = scripted(Envelope::UFrameFields);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .ttl(Duration::from_millis(50))
        .build()
        .unwrap();
    let ingress = carrier_in.carrier_ingress(&carrier).unwrap();
    let egress = egress_setup.carrier_egress(&carrier).unwrap();
    let up = upstream.enter_runtime().unwrap();
    let in_rt = carrier_in.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();
    let publisher = publisher.activate(&up).unwrap();
    // The publisher's IDs come from this clock's wall time, so 50 ms is measured against it.
    publisher.publish(&door(1, 0)).unwrap();
    let mut parked = None;
    in_rt
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            parked = Some(frame.park().expect("parked"));
            Flow::Stop
        })
        .unwrap();
    let parked = parked.expect("parked");
    clock.advance_wall_ms(60);
    assert!(matches!(
        out_rt.send(egress, &parked.frame()),
        Err(SendError::Expired)
    ));
    assert_eq!(
        in_rt.counters(ingress.id()).expired,
        1,
        "counted on the ingress"
    );
    assert_eq!(egress_control.sends(), 0, "nothing was forwarded");
    drop(parked);
}

#[test]
fn a_parked_frame_with_a_ttl_is_refused_when_the_clock_cannot_be_read() {
    let clock = TestClock::leak(1_000, 1_000_000);
    let bus = LendingBus::new();
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let upstream = TransportSetup::lending_on(&memory(), clock, bus.participant(1)).unwrap();
    let carrier_in = TransportSetup::lending_on(&memory(), clock, bus.participant(2)).unwrap();
    let (egress_setup, _egress_control) = scripted(Envelope::UFrameFields);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .ttl(Duration::from_secs(60))
        .build()
        .unwrap();
    let ingress = carrier_in.carrier_ingress(&carrier).unwrap();
    let egress = egress_setup.carrier_egress(&carrier).unwrap();
    let up = upstream.enter_runtime().unwrap();
    let in_rt = carrier_in.enter_runtime().unwrap();
    let out_rt = egress_setup.enter_runtime().unwrap();
    publisher
        .activate(&up)
        .unwrap()
        .publish(&door(1, 0))
        .unwrap();
    let mut parked = None;
    in_rt
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            parked = Some(frame.park().expect("parked"));
            Flow::Stop
        })
        .unwrap();
    let parked = parked.expect("parked");
    clock.break_wall_clock();
    assert!(matches!(
        out_rt.send(egress, &parked.frame()),
        Err(SendError::ClockUnavailable)
    ));
    assert_eq!(in_rt.counters(ingress.id()).clock_unavailable, 1);
}

// ------------------------------------------------------ retained delivery, push

#[test]
fn a_retained_push_frame_is_carried_and_parked() {
    let options = LendingOptions {
        push: true,
        ..LendingOptions::default()
    };
    let bus = LendingBus::new();
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let upstream = lending_with(bus, 1, options);
    let carrier_in = lending_with(bus, 2, options);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .unwrap();
    let ingress = carrier_in.carrier_ingress(&carrier).unwrap();
    let up = upstream.enter_runtime().unwrap();
    let in_rt = carrier_in.enter_runtime().unwrap();
    let publisher = publisher.activate(&up).unwrap();
    publisher.publish(&door(1, 1)).unwrap();
    publisher.publish(&door(2, 1)).unwrap();
    let mut parked = None;
    let handed = in_rt
        .receive(ingress, &mut |frame: CarriedFrame<'_>| {
            let bytes = frame.payload().as_contiguous().unwrap().to_vec();
            if bytes == [0, 0, 0, 1, 1] {
                parked = Some(frame.park().expect("parked"));
            }
            Flow::Continue
        })
        .unwrap();
    assert_eq!(handed, 2, "both pushed frames were carried");
    assert!(parked.is_some());
}

#[test]
fn a_full_inbox_of_retained_frames_applies_the_overflow_policy() {
    let options = LendingOptions {
        push: true,
        ..LendingOptions::default()
    };
    let run = |overflow: Overflow| {
        let bus = LendingBus::new();
        let uri = topic();
        let carried = door_wires();
        let carrier = CarrierRoute {
            qos: Qos::DEFAULT.with_depth(2).with_overflow(overflow),
            ..route(&uri, &carried)
        };
        let upstream = lending_with(bus, 1, options);
        let carrier_in = lending_with(bus, 2, options);
        let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
            .wire(DoorWire)
            .slots(8)
            .build()
            .unwrap();
        let ingress = carrier_in.carrier_ingress(&carrier).unwrap();
        let up = upstream.enter_runtime().unwrap();
        let in_rt = carrier_in.enter_runtime().unwrap();
        let publisher = publisher.activate(&up).unwrap();
        for id in 1..=4_u32 {
            publisher.publish(&door(id, 1)).unwrap();
        }
        let mut seen = Vec::new();
        in_rt
            .receive(ingress, &mut |frame: CarriedFrame<'_>| {
                seen.push(frame.payload().as_contiguous().unwrap()[3]);
                Flow::Continue
            })
            .unwrap();
        (seen, in_rt.counters(ingress.id()))
    };
    let (seen, counters) = run(Overflow::DropOldest);
    assert_eq!(seen, [3, 4], "newest two kept");
    assert_eq!(counters.dropped_oldest, 2);
    let (seen, counters) = run(Overflow::DropNewest);
    assert_eq!(seen, [1, 2], "oldest two kept");
    assert_eq!(counters.dropped_newest, 2);
}

#[test]
fn a_borrowed_delivery_on_a_carrier_ingress_is_refused_and_counted() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (setup, control) = scripted(Envelope::UFrameFields);
    let ingress = setup.carrier_ingress(&carrier).unwrap();
    let rt = setup.enter_runtime().unwrap();
    let (metadata, payload) = door_frame(Envelope::UFrameFields, &uri, 1, None);
    let refused = control.deliver_borrowed(&metadata, &payload).unwrap_err();
    assert!(
        matches!(refused, ReceiveError::Unsupported(reason) if reason.contains("carrier ingress"))
    );
    assert_eq!(rt.counters(ingress.id()).refused, 1);
}

#[test]
fn pump_leaves_a_carrier_ingress_alone() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (setup, control) = scripted(Envelope::UFrameFields);
    let ingress = setup.carrier_ingress(&carrier).unwrap();
    let rt = setup.enter_runtime().unwrap();
    control.script([scripted_door_frame(&uri, 1)]);
    assert_eq!(
        rt.pump(),
        0,
        "a carrier takes frames under its forwarder's check"
    );
    let handed = rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
        .unwrap();
    assert_eq!(handed, 1, "the frame was still queued");
}

// ------------------------------------------------------- tokens, transports, wake

#[test]
fn a_token_on_another_transport_is_refused() {
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let (a, _control_a) = scripted(Envelope::UFrameFields);
    let (b, _control_b) = scripted(Envelope::UFrameFields);
    let ingress = a.carrier_ingress(&carrier).unwrap();
    let egress = a.carrier_egress(&carrier).unwrap();
    let a_rt = a.enter_runtime().unwrap();
    let b_rt = b.enter_runtime().unwrap();
    assert!(matches!(
        b_rt.receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue),
        Err(ReceiveError::UnknownRoute)
    ));
    // A send on a foreign route is refused and counted, never a panic.
    let (ingress_setup, ingress_control) = scripted(Envelope::UFrameFields);
    let other_ingress = ingress_setup.carrier_ingress(&carrier).unwrap();
    let in_rt = ingress_setup.enter_runtime().unwrap();
    ingress_control.script([scripted_door_frame(&uri, 1)]);
    let mut refused = None;
    in_rt
        .receive(other_ingress, &mut |frame: CarriedFrame<'_>| {
            refused = Some(b_rt.send(egress, &frame));
            Flow::Continue
        })
        .unwrap();
    assert!(matches!(refused, Some(Err(SendError::Rejected { .. }))));
    assert_eq!(a_rt.counters(egress.id()).refused, 1);
}

#[test]
fn enter_runtime_all_returns_every_runtime_or_the_first_refusal() {
    let bus = InMemoryBus::new(8, 4096);
    let setups: Vec<TransportSetup> = vec![handoff(bus, 1), handoff(bus, 2), handoff(bus, 3)];
    let runtimes: Vec<TransportRuntime> = enter_runtime_all(setups).unwrap();
    assert_eq!(runtimes.len(), 3);

    // A waiver no route uses is refused, and the error comes back unchanged.
    let waiver: &'static RuntimeWaiver = runtime_waiver!(
        id: "UPW-test-unused",
        permit: "UPW-test-unused",
        reason: "a test of refusal",
        placement: "test",
        author: "author",
        approved_by: "reviewer",
        date: "2026-11-09",
        evidence: "none",
        review_trigger: "none",
    );
    let bad = TransportSetup::handoff_in(&memory(), bus.participant(4))
        .unwrap()
        .with_waiver(waiver)
        .unwrap();
    let refused = enter_runtime_all::<Vec<TransportRuntime>, _>(vec![bad]).unwrap_err();
    assert!(
        matches!(refused, StartupError::WaiverRefused { .. }),
        "{refused}"
    );
}

#[test]
fn every_handle_is_inert_and_copy() {
    fn copyable<T: Copy>() {}
    copyable::<IngressRoute>();
    copyable::<EgressRoute>();
    assert!(!std::mem::needs_drop::<IngressRoute>());
    assert!(!std::mem::needs_drop::<EgressRoute>());
}

#[test]
fn a_readable_binding_wakes_a_waiter_through_the_shared_signal() {
    static WAKE: Signal = Signal::new();
    let bus = LendingBus::new();
    let uri = topic();
    let carried = door_wires();
    let carrier = route(&uri, &carried);
    let upstream = lending(bus, 1);
    let carrier_in = lending(bus, 2);
    carrier_in.signal_on(&WAKE);
    let publisher = Publisher::<DoorState>::on(&upstream, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .unwrap();
    let _ingress = carrier_in.carrier_ingress(&carrier).unwrap();
    let up = upstream.enter_runtime().unwrap();
    let _in_rt = carrier_in.enter_runtime().unwrap();
    let token = WAKE.token();
    publisher
        .activate(&up)
        .unwrap()
        .publish(&door(1, 0))
        .unwrap();
    assert_eq!(
        WAKE.wait(token, Duration::from_secs(5)),
        SignalWait::Notified
    );
}

#[test]
fn a_silent_ingress_trips_its_deadline_monitor() {
    let clock = TestClock::leak(1_000, 1_000_000);
    let uri = topic();
    let carried = door_wires();
    let carrier = CarrierRoute {
        qos: Qos::DEFAULT.with_deadline(Duration::from_millis(5)),
        ..route(&uri, &carried)
    };
    let (binding, _control) = ScriptedHandoff::new(Envelope::UFrameFields);
    let setup = TransportSetup::handoff_on(&memory(), clock, binding).unwrap();
    let ingress = setup.carrier_ingress(&carrier).unwrap();
    let rt = setup.enter_runtime().unwrap();
    clock.advance_ns(6_000_000);
    rt.service();
    assert_eq!(rt.counters(ingress.id()).deadline_missed, 1);
}

// --------------------------------------------------------- concurrency, for Miri

/// Two threads share one lending ingress while a publisher publishes. Every
/// frame arrives exactly once and no slot is lost.
#[test]
fn two_workers_share_a_lending_ingress_without_loss_or_duplication() {
    let rounds: u32 = if cfg!(miri) { 12 } else { 400 };
    let carrier = lending_carrier(Qos::DEFAULT.with_depth(64), 4, LendingOptions::default());
    let seen = std::sync::Mutex::new(vec![0_u32; rounds as usize]);
    let done = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        let in_rt = carrier.in_rt;
        let ingress = carrier.ingress;
        let seen = &seen;
        let done = &done;
        let worker = move || {
            let mut idle = 0_u32;
            loop {
                let handed = in_rt
                    .receive(ingress, &mut |frame: CarriedFrame<'_>| {
                        let bytes = frame.payload().as_contiguous().unwrap();
                        let id = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                        seen.lock().unwrap()[id as usize] += 1;
                        Flow::Continue
                    })
                    .unwrap();
                if handed == 0 {
                    if done.load(std::sync::atomic::Ordering::SeqCst) {
                        idle += 1;
                        if idle > 16 {
                            break;
                        }
                    }
                    std::thread::yield_now();
                } else {
                    idle = 0;
                }
            }
        };
        scope.spawn(worker);
        scope.spawn(worker);
        for id in 0..rounds {
            while carrier.publisher.publish(&door(id, 0)).is_err() {
                std::thread::yield_now();
            }
        }
        done.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let seen = seen.into_inner().unwrap();
    assert!(
        seen.iter().all(|count| *count == 1),
        "every frame exactly once: {seen:?}"
    );
    assert_eq!(
        u64::from(carrier.in_rt.counters(carrier.ingress.id()).carried),
        u64::from(rounds)
    );
}

/// Producers push retained frames while a consumer takes them. Counts balance and
/// every slot comes back.
#[test]
fn concurrent_pushes_keep_counts_and_slots() {
    let rounds: u32 = if cfg!(miri) { 8 } else { 300 };
    let options = LendingOptions {
        push: true,
        slots: 64,
        ..LendingOptions::default()
    };
    let bus = LendingBus::new();
    let uri = topic();
    let carried = door_wires();
    let carrier = CarrierRoute {
        qos: Qos::DEFAULT
            .with_depth(4)
            .with_overflow(Overflow::DropOldest),
        slots: 2,
        ..route(&uri, &carried)
    };
    let upstream_one = lending_with(bus, 1, options);
    let upstream_two = lending_with(bus, 2, options);
    let carrier_in = lending_with(bus, 3, options);
    let provider = entity("door", 1);
    let one = Publisher::<DoorState>::on(&upstream_one, &provider, 0x8001)
        .wire(DoorWire)
        .slots(32)
        .build()
        .unwrap();
    let two = Publisher::<DoorState>::on(&upstream_two, &provider, 0x8001)
        .wire(DoorWire)
        .slots(32)
        .build()
        .unwrap();
    let ingress = carrier_in.carrier_ingress(&carrier).unwrap();
    let up_one = upstream_one.enter_runtime().unwrap();
    let up_two = upstream_two.enter_runtime().unwrap();
    let in_rt = carrier_in.enter_runtime().unwrap();
    let one = one.activate(&up_one).unwrap();
    let two = two.activate(&up_two).unwrap();
    let done = std::sync::atomic::AtomicBool::new(false);
    let handed_total = std::sync::atomic::AtomicU64::new(0);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for id in 0..rounds {
                while one.publish(&door(id, 0)).is_err() {
                    std::thread::yield_now();
                }
            }
        });
        scope.spawn(|| {
            for id in 0..rounds {
                while two.publish(&door(id, 0)).is_err() {
                    std::thread::yield_now();
                }
            }
        });
        scope.spawn(|| {
            let mut idle = 0_u32;
            loop {
                let handed = in_rt
                    .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
                    .unwrap();
                handed_total.fetch_add(
                    u64::try_from(handed).unwrap(),
                    std::sync::atomic::Ordering::SeqCst,
                );
                if handed == 0 {
                    if done.load(std::sync::atomic::Ordering::SeqCst) {
                        idle += 1;
                        if idle > 16 {
                            break;
                        }
                    }
                    std::thread::yield_now();
                } else {
                    idle = 0;
                }
            }
        });
        // Publishers finish first; give the consumer the signal to stop.
        std::thread::sleep(if cfg!(miri) {
            Duration::from_millis(20)
        } else {
            Duration::from_millis(200)
        });
        done.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    // Frames the consumer had not taken when it stopped are still queued: take them.
    let mut drained = 0_u64;
    loop {
        let handed = in_rt
            .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
            .unwrap();
        if handed == 0 {
            break;
        }
        drained += u64::try_from(handed).unwrap();
    }
    let counters = in_rt.counters(ingress.id());
    let total = u64::from(rounds) * 2;
    let handed = handed_total.into_inner() + drained;
    assert_eq!(
        u64::from(counters.carried) + u64::from(counters.dropped_oldest) + u64::from(counters.refused_contention),
        total,
        "every push is carried or counted: {counters:?}"
    );
    assert_eq!(u64::from(counters.carried), handed);
    // Every slot came back: the inbox still takes a full depth.
    for id in 0..4_u32 {
        one.publish(&door(id, 0)).unwrap();
    }
    let handed = in_rt
        .receive(ingress, &mut |_frame: CarriedFrame<'_>| Flow::Continue)
        .unwrap();
    assert_eq!(handed, 4);
}
