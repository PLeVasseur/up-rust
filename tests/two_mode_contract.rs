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

//! The merged two-mode API on the in-memory bindings (design 9.4).
//!
//! The union of the `ws-owned` and `ws-zero-copy` contract suites, over one API,
//! plus the proofs design section 9.4 lists as new. Heap windows live in
//! `two_mode_no_heap.rs`; these are behaviour and byte-level tests.

#![allow(clippy::indexing_slicing)]

use std::cell::Cell;
use std::fs;
use std::mem::{needs_drop, MaybeUninit};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use bytes::Bytes;
use up_rust::frame::envelope::NativeUFrameEnvelope;
use up_rust::prelude::UuidExt as _;
use up_rust::startup::LEAKED_HEAP_REGIONS;
use up_rust::test_util::handoff::{InMemoryBus, InMemoryOptions};
use up_rust::test_util::lending::{LendingBus, LendingOptions};
use up_rust::test_util::wires::{
    AllocatingWire, DoorState, DoorWire, LittleDoorWire, VariableDoorWire, DOOR_ENCODING,
    LITTLE_DOOR_ENCODING,
};
use up_rust::test_util::{EnvelopeHarness, TestClock};
use up_rust::two_mode::*;
use up_rust::{
    HeaderLimits, NativeProfile, NativeProfileAgreement, PayloadEncoding, RxHeader,
    StablePayloadExt, StaticUriProvider, TxHeaderParts, UCode, UEntityId, UFrameEnvelope,
    UFrameFieldsMetadataCodec, UFrameHeaderError, UFrameMetadata, UMessageType, UOwnedFrame,
    UPriority, UProtocolNativeWire, UUri, UWireMetadataCodec, UWireMetadataContext,
    NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID, UPROTOCOL_NATIVE_WIRE_ID, UUID,
};
use up_rust_core::frame::header::{header_fields_len, parse_field_block, write_field_block_uninit};
use up_rust_core::transport::inbound::{PeerAdmission, PeerTable};
use up_rust_core::transport::slots::SlotPool;
use up_rust_core::wire::{
    render_selected_prefix, selected_prefix_len, FORMAT_VERSION,
    STABLE_CONTAINER_PAYLOAD_FAMILY_ID, STABLE_CONTAINER_WIRE_ID, UFRAME_FIELDS_METADATA_LAYOUT_ID,
};

/// The time-to-live of every RPC request here that runs on the wall clock: long
/// enough for an interpreter. The client's 1 s default expired a request in
/// flight under Miri with Tree Borrows.
const TTL: Duration = Duration::from_secs(30);
const METHOD: u16 = 0x0001;

fn entity(authority: &str, type_id: u16) -> StaticUriProvider {
    StaticUriProvider::for_entity(authority, UEntityId::first_instance(type_id), 1).expect("uri")
}

fn memory() -> StartupMemory {
    StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 64 * 1024).expect("memory")
}

fn handoff(bus: InMemoryBus, peer: u64) -> TransportSetup {
    TransportSetup::handoff_in(&memory(), bus.participant(peer)).expect("setup")
}

fn handoff_with(bus: InMemoryBus, peer: u64, options: InMemoryOptions) -> TransportSetup {
    TransportSetup::handoff_in(&memory(), bus.participant(peer).with_options(options))
        .expect("setup")
}

fn handoff_on(bus: InMemoryBus, peer: u64, clock: &'static TestClock) -> TransportSetup {
    TransportSetup::handoff_on(&memory(), clock, bus.participant(peer)).expect("setup")
}

fn lending(bus: LendingBus, peer: u64) -> TransportSetup {
    TransportSetup::lending_in(&memory(), bus.participant(peer)).expect("setup")
}

fn lending_with(bus: LendingBus, peer: u64, options: LendingOptions) -> TransportSetup {
    TransportSetup::lending_in(&memory(), bus.participant(peer).with_options(options))
        .expect("setup")
}

fn lending_on(bus: LendingBus, peer: u64, clock: &'static TestClock) -> TransportSetup {
    TransportSetup::lending_on(&memory(), clock, bus.participant(peer)).expect("setup")
}

fn topic() -> UUri {
    entity("door", 1).get_resource_uri(0x8001)
}

fn door(id: u32, open: u8) -> DoorState {
    DoorState { id, open }
}

fn publisher_on(setup: &TransportSetup) -> PublisherSetup<DoorState> {
    Publisher::<DoorState>::on(setup, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .expect("publisher")
}

fn subscriber_on(setup: &TransportSetup, qos: Qos) -> SubscriberSetup<DoorState> {
    Subscriber::<DoorState>::on(setup, topic())
        .wire(DoorWire)
        .qos(qos)
        .build()
        .expect("subscriber")
}

struct Pair {
    publisher: Publisher<DoorState>,
    subscriber: Subscriber<DoorState>,
    tx: TransportRuntime,
    rx: TransportRuntime,
}

fn activate(a: TransportSetup, b: TransportSetup, qos: Qos) -> Pair {
    let publisher = publisher_on(&a);
    let subscriber = subscriber_on(&b, qos);
    let tx = a.enter_runtime().expect("runtime");
    let rx = b.enter_runtime().expect("runtime");
    Pair {
        publisher: publisher.activate(&tx).expect("activate"),
        subscriber: subscriber.activate(&rx).expect("activate"),
        tx,
        rx,
    }
}

fn lending_pair(qos: Qos) -> Pair {
    let bus = LendingBus::new();
    activate(lending(bus, 1), lending(bus, 2), qos)
}

fn handoff_pair(frames: usize, qos: Qos) -> (Pair, InMemoryBus) {
    let bus = InMemoryBus::new(frames, 4096);
    (activate(handoff(bus, 1), handoff(bus, 2), qos), bus)
}

/// One real frame, through the real send path, taken back off the bus as bytes.
fn captured_door_frame(id: u32, open: u8) -> (Vec<u8>, Vec<u8>) {
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let b = handoff(bus, 2);
    let publisher = publisher_on(&a);
    let _subscriber = subscriber_on(&b, Qos::DEFAULT);
    let tx = a.enter_runtime().expect("runtime");
    publisher
        .activate(&tx)
        .expect("activate")
        .publish(&door(id, open))
        .expect("publish");
    let (metadata, payload) = bus.take_raw(2).expect("a queued frame");
    (metadata, payload.expect("a payload"))
}

// ---------------------------------------------------------------- golden bytes

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/wire-metadata")
}

fn golden_files(suffix: &str) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(golden_dir())
        .expect("golden directory")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.to_string_lossy().ends_with(suffix))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no golden files for {suffix}");
    files
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(&path).expect("golden file");
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                hex::decode(text.trim()).expect("hex"),
            )
        })
        .collect()
}

fn limits() -> HeaderLimits {
    HeaderLimits {
        max_token_len: 4096,
    }
}

fn native_context() -> UWireMetadataContext {
    UWireMetadataContext::from_wire::<UProtocolNativeWire, UFrameFieldsMetadataCodec>()
}

fn harness(envelope: Envelope) -> EnvelopeHarness {
    EnvelopeHarness::new(
        envelope,
        UPROTOCOL_NATIVE_WIRE_ID,
        NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID,
        WirePayloadIdentity::serialized(PayloadEncoding::RAW),
    )
}

fn assert_header_matches(header: &RxHeader<'_>, metadata: &UFrameMetadata) {
    assert_eq!(header.kind, metadata.kind());
    assert_eq!(&header.id, metadata.id());
    assert_eq!(header.reqid.as_ref(), metadata.reqid());
    assert_eq!(&header.source, metadata.source());
    assert_eq!(header.sink.as_ref(), metadata.sink());
    assert_eq!(header.ttl, metadata.ttl());
    assert_eq!(header.priority, metadata.priority());
    assert_eq!(header.comm_status, metadata.comm_status());
    assert_eq!(header.permission_level, metadata.permission_level());
    assert_eq!(header.token(), metadata.token());
    assert_eq!(header.traceparent(), metadata.traceparent());
    assert_eq!(
        header.payload_encoding,
        metadata.payload_encoding().copied()
    );
    assert_eq!(header.native_type_token, metadata.native_type_token());
}

/// `UFrameFieldsMetadataCodec` golden vectors, byte for byte, through the
/// heap-free writer and borrowed parser.
#[test]
fn uframe_fields_envelope_matches_every_golden_vector() {
    let harness = harness(Envelope::UFrameFields);
    for (name, bytes) in golden_files(".uframe-fields.hex") {
        let metadata = UFrameFieldsMetadataCodec
            .decode_frame_metadata(native_context(), &bytes)
            .expect("stored bytes decode");
        let parts = TxHeaderParts::from(&metadata);
        let (head_len, encoded) = harness.encode(&parts, None).expect("encode");
        assert_eq!(encoded, bytes, "bytes for {name}");
        assert_eq!(head_len, bytes.len());
        // A vector holds metadata only; supply the payload its encoding promises.
        let body = if metadata.payload_encoding().is_some() {
            PayloadBytes::one(&[])
        } else {
            PayloadBytes::Absent
        };
        let (header, payload) = harness.parse(&bytes, body, &limits()).expect("parse");
        assert_eq!(payload.is_present(), metadata.payload_encoding().is_some());
        assert_header_matches(&header, &metadata);
    }
}

fn differential_metadata(seed: usize) -> UFrameMetadata {
    let topic = UUri::try_from_wire_parts("sensor", 0x0001_1234, 1, 0x8001).unwrap();
    let method = UUri::try_from_wire_parts("service", 0x0001_4567, 2, 0x42).unwrap();
    let reply = UUri::try_from_wire_parts("client", 0x0001_6789, 3, 0).unwrap();
    let mut builder = match seed % 4 {
        0 => UFrameMetadata::publish(topic),
        1 => UFrameMetadata::notification(topic, reply),
        2 => UFrameMetadata::request(method, reply, Duration::from_millis(250)),
        _ => UFrameMetadata::response(method, reply, UUID::build()),
    };
    builder = builder.with_id(UUID::build());
    if seed & 4 != 0 {
        builder = builder.with_priority(UPriority::CS5);
    }
    if seed & 8 != 0 {
        builder = builder.with_ttl(Duration::from_millis(999));
    }
    if seed & 16 != 0 {
        builder = builder.with_comm_status(UCode::Unavailable);
    }
    if seed & 32 != 0 {
        builder = builder.with_permission_level(seed as u32);
    }
    if seed & 64 != 0 {
        builder = builder.with_token(if seed & 512 == 0 { "private-token" } else { "" });
    }
    if seed & 128 != 0 {
        builder =
            builder.with_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01");
    }
    if seed & 256 != 0 {
        builder = builder.with_payload_encoding(PayloadEncoding::PROTOBUF);
    }
    builder.build().expect("metadata")
}

fn frame_for(seed: usize, metadata: UFrameMetadata) -> UOwnedFrame {
    match metadata.payload_encoding() {
        Some(_) if seed & 1024 == 0 => {
            UOwnedFrame::with_payload(metadata, Bytes::from_static(b"payload bytes")).unwrap()
        }
        Some(_) => UOwnedFrame::with_payload(metadata, Bytes::new()).unwrap(),
        None => UOwnedFrame::without_payload(metadata).unwrap(),
    }
}

#[test]
fn native_whole_frame_matches_the_reference_encoder() {
    let harness = harness(Envelope::NativeWholeFrame);
    let seeds = if cfg!(miri) { 16 } else { 2048 };
    for seed in 0..seeds {
        let frame = frame_for(seed, differential_metadata(seed));
        let expected = NativeUFrameEnvelope::serialize_frame(&frame).expect("reference encode");
        let payload = frame.has_payload().then(|| frame.payload_bytes());
        let (_, encoded) = harness
            .encode(&TxHeaderParts::from(frame.metadata()), payload)
            .expect("heap-free encode");
        assert_eq!(encoded, &expected[..], "seed {seed}");
        let (header, parsed) = harness
            .parse(&[], PayloadBytes::one(&encoded), &limits())
            .expect("parse");
        assert_header_matches(&header, frame.metadata());
        match payload {
            Some(bytes) => assert_eq!(parsed.as_contiguous(), Some(bytes)),
            None => assert!(!parsed.is_present()),
        }
    }
}

/// The heap-free parser must never accept what the reference rejects, and must
/// agree on every field when both accept. It may be stricter in the named ways:
/// a declared length past the end of the input, and a traceparent over the
/// runtime capacity of 63 bytes.
#[test]
fn native_parser_agrees_with_the_reference_on_corrupted_input() {
    let harness = harness(Envelope::NativeWholeFrame);
    let (mut accepted, mut rejected, mut stricter) = (0_usize, 0_usize, 0_usize);
    let seeds = if cfg!(miri) { 8 } else { 512 };
    for seed in 0..seeds {
        let frame = frame_for(seed, differential_metadata(seed));
        let original = NativeUFrameEnvelope::serialize_frame(&frame).expect("encode");
        for mutation in 0..64 {
            let mut bytes = original.to_vec();
            let offset = (seed * 13 + mutation * 7) % bytes.len();
            bytes[offset] ^= 1 << ((seed + mutation) % 8);
            let reference = NativeUFrameEnvelope::deserialize_frame(&bytes);
            let parsed = harness.parse(&[], PayloadBytes::one(&bytes), &limits());
            match (parsed, reference) {
                (Ok((header, payload)), Ok(reference)) => {
                    accepted += 1;
                    assert_header_matches(&header, reference.metadata());
                    assert_eq!(payload.as_contiguous(), reference.payload().map(|b| &b[..]));
                }
                (Err(_), Err(_)) => rejected += 1,
                (Ok(_), Err(error)) => panic!(
                    "seed {seed}, mutation {mutation}: parser accepted what the reference \
                     rejects ({error}) on {bytes:02x?}"
                ),
                (Err(error), Ok(_)) => {
                    assert!(
                        matches!(
                            error,
                            UFrameHeaderError::Truncated
                                | UFrameHeaderError::Malformed("traceparent capacity")
                        ),
                        "seed {seed}, mutation {mutation}: parser refused {error:?} what the \
                         reference accepts, on {bytes:02x?}"
                    );
                    stricter += 1;
                }
            }
        }
    }
    println!("accepted={accepted} rejected={rejected} stricter={stricter}");
    assert!(accepted > 0 && rejected > 0, "{accepted} {rejected}");
}

struct Upf1Envelope;

impl CustomEnvelope for Upf1Envelope {
    fn name(&self) -> &'static str {
        "uPF1"
    }

    fn max_head_len(&self, max_token_len: usize) -> usize {
        9 + 640 + max_token_len
    }

    fn head_len(
        &self,
        parts: &TxHeaderParts<'_>,
        _payload_len: Option<usize>,
    ) -> Result<usize, UFrameHeaderError> {
        Ok(9 + header_fields_len(parts)?)
    }

    fn write_head(
        &self,
        parts: &TxHeaderParts<'_>,
        payload_len: Option<usize>,
        out: &mut [u8],
    ) -> Result<usize, UFrameHeaderError> {
        let fields = header_fields_len(parts)?;
        let total = 9 + fields;
        let head = out
            .get_mut(..total)
            .ok_or(UFrameHeaderError::OutputTooShort)?;
        let (fixed, rest) = head.split_at_mut(9);
        fixed[..4].copy_from_slice(b"uPF1");
        fixed[4] = u8::from(payload_len.is_some());
        fixed[5..9].copy_from_slice(&(fields as u32).to_be_bytes());
        // SAFETY: MaybeUninit<u8> has u8's layout and `rest` is only written with bytes.
        let uninit = unsafe { &mut *(std::ptr::from_mut(rest) as *mut [MaybeUninit<u8>]) };
        write_field_block_uninit(parts, uninit)?;
        Ok(total)
    }

    fn parse<'a>(
        &self,
        metadata: &'a [u8],
        payload: PayloadBytes<'a>,
        limits: &HeaderLimits,
    ) -> Result<(RxHeader<'a>, PayloadBytes<'a>), UFrameHeaderError> {
        let frame = if metadata.is_empty() {
            payload
                .as_contiguous()
                .ok_or(UFrameHeaderError::Truncated)?
        } else {
            metadata
        };
        let fixed = frame.get(..9).ok_or(UFrameHeaderError::Truncated)?;
        if &fixed[..4] != b"uPF1" {
            return Err(UFrameHeaderError::WrongPrefix);
        }
        let fields = u32::from_be_bytes([fixed[5], fixed[6], fixed[7], fixed[8]]) as usize;
        let end = 9 + fields;
        let header = parse_field_block(
            frame.get(9..end).ok_or(UFrameHeaderError::Truncated)?,
            limits,
        )?;
        let body = if metadata.is_empty() {
            frame
                .get(end..)
                .map_or(PayloadBytes::Absent, PayloadBytes::one)
        } else {
            payload
        };
        Ok((header, body))
    }
}

static UPF1: Upf1Envelope = Upf1Envelope;

#[test]
fn a_transport_defined_envelope_writes_the_same_bytes_as_its_profile() {
    let metadata = differential_metadata(256);
    let parts = TxHeaderParts::from(&metadata);
    let fields = up_rust::frame::codec::encode_frame_metadata_fields(&metadata).unwrap();
    let (_, encoded) = harness(Envelope::Custom(&UPF1))
        .encode(&parts, Some(b"tail"))
        .unwrap();
    let mut expected = b"uPF1\x01".to_vec();
    expected.extend_from_slice(&(fields.len() as u32).to_be_bytes());
    expected.extend_from_slice(&fields);
    expected.extend_from_slice(b"tail");
    assert_eq!(encoded, expected);
}

/// The selected-wire prefix is the same bytes core renders and the classic SDK renders.
#[test]
fn the_selected_wire_prefix_matches_the_classic_prefix() {
    let wires = [
        (UPROTOCOL_NATIVE_WIRE_ID, NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID),
        (
            WireIdentity::new("test.door-wire", 0x8A01),
            WireIdentity::new("test.door-family", 0x8A02),
        ),
        (
            WireIdentity::new("example.vendor/wire", 0),
            WireIdentity::new("example.vendor/family", 0),
        ),
    ];
    for (wire, family) in wires {
        let context = UWireMetadataContext::new_exact(
            wire,
            family,
            UFRAME_FIELDS_METADATA_LAYOUT_ID,
            FORMAT_VERSION,
        );
        let classic = UFrameFieldsMetadataCodec
            .native_prefix(context)
            .expect("classic prefix");
        let len = selected_prefix_len(wire, family).expect("core prefix length");
        let mut out = vec![MaybeUninit::uninit(); len];
        let written = render_selected_prefix(wire, family, &mut out).expect("core prefix");
        assert_eq!(written, len);
        let rendered: Vec<u8> = out
            .iter()
            // SAFETY: `render_selected_prefix` initialized the first `written` bytes.
            .map(|byte| unsafe { byte.assume_init() })
            .collect();
        assert_eq!(rendered, classic, "prefix for {wire:?}, {family:?}");
    }
}

// ------------------------------------------------------------------ the modes

#[test]
fn runtime_handles_have_no_drop_glue() {
    assert!(!needs_drop::<TransportRuntime>());
    assert!(!needs_drop::<Publisher<DoorState>>());
    assert!(!needs_drop::<Notifier<DoorState>>());
    assert!(!needs_drop::<Subscriber<DoorState>>());
    assert!(!needs_drop::<NotificationListener<DoorState>>());
    assert!(!needs_drop::<RpcClient<DoorState, DoorState>>());
    assert!(!needs_drop::<RpcServer<DoorState, DoorState>>());
    assert!(!needs_drop::<Publisher<NativeDoor, InPlace>>());
    assert!(!needs_drop::<Subscriber<NativeDoor, InPlace>>());
}

#[test]
#[allow(dropping_copy_types)]
fn runtime_handles_are_inert_to_drop() {
    let pair = lending_pair(Qos::DEFAULT);
    drop(pair.publisher);
    drop(pair.subscriber);
    // The handles are `Copy`: dropping one does nothing to the routes.
    pair.publisher.publish(&door(1, 1)).unwrap();
    assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, 1);
    let (_tx, _rx) = (pair.tx, pair.rx);
}

#[test]
fn transports_chosen_at_run_time_can_sit_in_a_list() {
    let (lend, hand) = (LendingBus::new(), InMemoryBus::new(4, 512));
    let setups: Vec<TransportSetup> = vec![lending(lend, 1), handoff(hand, 2), lending(lend, 3)];
    let runtimes: Vec<TransportRuntime> = setups
        .into_iter()
        .map(TransportSetup::enter_runtime)
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(runtimes.len(), 3);
    assert_eq!(runtimes[1].binding_name(), "in-memory-handoff");
    assert!(runtimes.iter().all(|runtime| runtime.routes().is_empty()));
}

#[test]
fn a_role_cannot_be_activated_on_another_transport() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let publisher = publisher_on(&a);
    let other = b.enter_runtime().unwrap();
    let _keep = a.enter_runtime().unwrap();
    assert!(matches!(
        publisher.activate(&other),
        Err(StartupError::WrongTransport)
    ));
}

#[test]
fn lending_and_handoff_carry_the_same_typed_traffic() {
    let lend = lending_pair(Qos::DEFAULT);
    lend.publisher.publish(&door(7, 1)).unwrap();
    let received = lend.subscriber.try_recv().unwrap().unwrap();
    assert_eq!((received.id, received.open), (7, 1));
    drop(received);
    let (hand, _bus) = handoff_pair(8, Qos::DEFAULT);
    hand.publisher.publish(&door(9, 0)).unwrap();
    assert_eq!(hand.subscriber.try_recv().unwrap().unwrap().id, 9);
    assert!(hand.subscriber.try_recv().unwrap().is_none());
}

#[test]
fn a_received_value_stays_valid_while_later_traffic_flows() {
    let pair = lending_pair(Qos::DEFAULT);
    pair.publisher.publish(&door(1, 1)).unwrap();
    let held = pair.subscriber.try_recv().unwrap().unwrap();
    pair.publisher.publish(&door(2, 2)).unwrap();
    let next = pair.subscriber.try_recv().unwrap().unwrap();
    assert_eq!((held.id, next.id), (1, 2));
}

#[test]
fn a_type_with_no_drop_glue_passes_the_runtime_check() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let _sub = subscriber_on(&a, Qos::DEFAULT);
    assert!(a.enter_runtime().is_ok());
}

// ---------------------------------------------------------- capability refusals

#[test]
fn a_wire_that_allocates_is_refused_at_enter_runtime_naming_the_route() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let _publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(AllocatingWire)
        .build()
        .unwrap();
    let refused = a.enter_runtime().unwrap_err();
    assert!(matches!(
        refused,
        StartupError::RouteNeedsHeapAtRuntime { .. }
    ));
    assert!(refused.to_string().contains("Publish"), "{refused}");
    assert!(refused.to_string().contains("door"), "{refused}");
}

#[derive(Clone, Debug, Default, PartialEq)]
struct OwnsHeap(String);

#[derive(Clone, Copy, Debug, Default)]
struct OwnsHeapWire;

impl Wire<OwnsHeap> for OwnsHeapWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "owns-heap",
            WireIdentity::new("test.owns-heap", 0x8B01),
            WireIdentity::new("test.owns-heap-family", 0x8B02),
            WirePayloadIdentity::serialized(PayloadEncoding::private_use(0xF101)),
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
        .max_encoded_len(8)
    }

    fn encode(&self, _value: &OwnsHeap, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        out.put(b"x")
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, OwnsHeap>) -> Ready<'s, OwnsHeap> {
        slot.write(OwnsHeap::default())
    }

    fn decode(
        &self,
        _encoding: PayloadEncoding,
        _bytes: &[u8],
        _into: &mut OwnsHeap,
    ) -> Result<(), WireError> {
        Ok(())
    }
}

#[test]
fn a_decoded_type_that_owns_heap_is_refused_at_enter_runtime() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let _sub = Subscriber::<OwnsHeap>::on(&a, topic())
        .wire(OwnsHeapWire)
        .build()
        .unwrap();
    assert!(matches!(
        a.enter_runtime(),
        Err(StartupError::RouteNeedsHeapAtRuntime { .. })
    ));
}

#[derive(Clone, Copy, Debug, Default)]
struct UnboundedWire;

impl Wire<u32> for UnboundedWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "unbounded",
            WireIdentity::new("test.unbounded", 0x8B03),
            WireIdentity::new("test.unbounded-family", 0x8B04),
            WirePayloadIdentity::serialized(PayloadEncoding::private_use(0xF102)),
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
    }

    fn encode(&self, _value: &u32, _out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        Ok(())
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, u32>) -> Ready<'s, u32> {
        slot.write(0)
    }

    fn decode(
        &self,
        _encoding: PayloadEncoding,
        _bytes: &[u8],
        _into: &mut u32,
    ) -> Result<(), WireError> {
        Ok(())
    }
}

#[test]
fn a_wire_with_no_bound_needs_max_payload() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    assert!(matches!(
        Publisher::<u32>::on(&a, &entity("door", 1), 1)
            .wire(UnboundedWire)
            .build(),
        Err(StartupError::PayloadUnbounded { .. })
    ));
    assert!(Publisher::<u32>::on(&a, &entity("door", 1), 2)
        .wire(UnboundedWire)
        .max_payload(16)
        .build()
        .is_ok());
}

fn permit() -> &'static RuntimePermit {
    static PERMIT: RuntimePermit = RuntimePermit {
        id: "UPW-test-1",
        rule: "runtime.no_heap_acquire",
        binding: "in-memory-lending",
        operations: &[HeapOperation::PeerChange],
        bound: "per connection change",
    };
    &PERMIT
}

fn waiver() -> &'static RuntimeWaiver {
    runtime_waiver! {
        id: "WAIVER-test-1",
        permit: "UPW-test-1",
        reason: "accepted for a QM partition",
        placement: "QM process",
        author: "integrator",
        approved_by: "safety-manager",
        date: "2026-11-09",
        evidence: "two_mode_contract",
        review_trigger: "middleware version change",
    }
}

fn permitted_lending() -> LendingOptions {
    LendingOptions {
        middleware_permit: Some(permit()),
        ..LendingOptions::default()
    }
}

#[test]
fn middleware_that_allocates_needs_a_named_waiver_and_it_is_recorded() {
    let bus = LendingBus::new();
    let a = lending_with(bus, 1, permitted_lending());
    let _p = publisher_on(&a);
    assert!(matches!(
        a.enter_runtime(),
        Err(StartupError::MiddlewareHeapNeedsWaiver {
            permit: "UPW-test-1",
            ..
        })
    ));

    let bus = LendingBus::new();
    let a = lending_with(bus, 1, permitted_lending())
        .with_waiver(waiver())
        .unwrap();
    let _p = publisher_on(&a);
    let runtime = a.enter_runtime().unwrap();
    let recorded = runtime.waivers();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].id, "WAIVER-test-1");
    assert!(recorded[0].location.contains("two_mode_contract"));
}

#[test]
fn a_waiver_must_be_complete_and_independently_approved() {
    // The author approves their own waiver.
    let self_approved: &'static RuntimeWaiver = Box::leak(Box::new(RuntimeWaiver {
        approved_by: "integrator",
        ..*waiver()
    }));
    let bus = LendingBus::new();
    let a = lending_with(bus, 1, permitted_lending())
        .with_waiver(self_approved)
        .unwrap();
    let _p = publisher_on(&a);
    assert!(matches!(
        a.enter_runtime(),
        Err(StartupError::WaiverRefused { .. })
    ));

    // A record with a blank field.
    let incomplete: &'static RuntimeWaiver = Box::leak(Box::new(RuntimeWaiver {
        evidence: "",
        ..*waiver()
    }));
    let bus = LendingBus::new();
    let a = lending_with(bus, 1, permitted_lending())
        .with_waiver(incomplete)
        .unwrap();
    let _p = publisher_on(&a);
    assert!(matches!(
        a.enter_runtime(),
        Err(StartupError::WaiverRefused { .. })
    ));

    // A waiver with no route declaring its permit does not sit around unused.
    let bus = LendingBus::new();
    let a = lending(bus, 1).with_waiver(waiver()).unwrap();
    assert!(matches!(
        a.enter_runtime(),
        Err(StartupError::WaiverRefused { .. })
    ));

    // The same waiver twice.
    let bus = LendingBus::new();
    let a = lending(bus, 1).with_waiver(waiver()).unwrap();
    assert!(a.with_waiver(waiver()).is_err());
}

#[derive(Clone, Copy, Debug, Default)]
struct ClaimsMiddlewareWire;

impl Wire<u32> for ClaimsMiddlewareWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "claims-middleware",
            WireIdentity::new("test.claims-middleware", 0x8B07),
            WireIdentity::new("test.claims-middleware-family", 0x8B08),
            WirePayloadIdentity::serialized(PayloadEncoding::private_use(0xF103)),
            RuntimeHeap::Middleware(permit()),
            RuntimeHeap::None,
        )
        .max_encoded_len(4)
    }

    fn encode(&self, _value: &u32, _out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        Ok(())
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, u32>) -> Ready<'s, u32> {
        slot.write(0)
    }

    fn decode(
        &self,
        _encoding: PayloadEncoding,
        _bytes: &[u8],
        _into: &mut u32,
    ) -> Result<(), WireError> {
        Ok(())
    }
}

/// `Middleware` is only ever a binding's claim. A waiver cannot admit a wire.
#[test]
fn a_wire_cannot_claim_a_middleware_heap_class() {
    let bus = LendingBus::new();
    let a = lending(bus, 1).with_waiver(waiver()).unwrap();
    let _p = Publisher::<u32>::on(&a, &entity("door", 1), 1)
        .wire(ClaimsMiddlewareWire)
        .build()
        .unwrap();
    assert!(a.enter_runtime().is_err());
}

#[test]
fn a_route_is_refused_for_capability_only() {
    let provider = entity("door", 1);
    // An envelope the transport does not carry.
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    assert!(matches!(
        Publisher::<DoorState>::on(&a, &provider, 1)
            .wire(DoorWire)
            .envelope(Envelope::NativeWholeFrame)
            .build(),
        Err(StartupError::EnvelopeNotAccepted { .. })
    ));
    // A second wire on a transport with one encoding per route.
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff_with(
        bus,
        1,
        InMemoryOptions {
            per_message_encoding: false,
            ..InMemoryOptions::default()
        },
    );
    assert!(matches!(
        Publisher::<DoorState>::on(&a, &provider, 1)
            .wire(DoorWire)
            .also_wire(LittleDoorWire)
            .build(),
        Err(StartupError::SingleEncodingOnly { .. })
    ));
    // A wire type given twice: `publish_using` could not choose.
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    assert!(matches!(
        Publisher::<DoorState>::on(&a, &provider, 1)
            .wire(DoorWire)
            .also_wire(DoorWire)
            .build(),
        Err(StartupError::DuplicateWireType { .. })
    ));
    assert!(matches!(
        Subscriber::<DoorState>::on(&a, topic())
            .wire(DoorWire)
            .also_wire(DoorWire)
            .build(),
        Err(StartupError::DuplicateWireType { .. })
    ));
}

#[test]
fn a_binding_that_cannot_honor_qos_refuses_the_route_at_setup() {
    let provider = entity("door", 1);
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let refuse = |qos: Qos, resource: u16| {
        Publisher::<DoorState>::on(&a, &provider, resource)
            .wire(DoorWire)
            .qos(qos)
            .build()
            .unwrap_err()
    };
    assert!(matches!(
        refuse(
            Qos::DEFAULT.with_overflow(Overflow::Wait {
                timeout: Duration::from_millis(5)
            }),
            1
        ),
        StartupError::QosNotHonored {
            setting: QosSetting::Overflow,
            ..
        }
    ));
    assert!(matches!(
        refuse(Qos::DEFAULT.with_depth(0), 2),
        StartupError::QosNotHonored {
            setting: QosSetting::Depth,
            requested: 0,
            ..
        }
    ));
    assert!(matches!(
        refuse(Qos::DEFAULT.with_priority(UPriority::CS5), 3),
        StartupError::QosNotHonored {
            setting: QosSetting::Priority,
            ..
        }
    ));
}

#[test]
fn a_role_can_require_lending_and_a_copying_transport_refuses_it_at_setup() {
    let provider = entity("door", 1);
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    assert!(matches!(
        Publisher::<DoorState>::on(&a, &provider, 1)
            .wire(DoorWire)
            .require_lending()
            .build(),
        Err(StartupError::LendingRequired { .. })
    ));
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let _p = Publisher::<DoorState>::on(&a, &provider, 1)
        .wire(DoorWire)
        .require_lending()
        .build()
        .unwrap();
    assert_eq!(
        a.enter_runtime().unwrap().routes()[0].send_mode,
        Some(SendMode::Lend)
    );
}

#[test]
fn the_route_report_says_what_each_route_got() {
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let _p = publisher_on(&a);
    let _s = subscriber_on(&a, Qos::DEFAULT);
    let runtime = a.enter_runtime().unwrap();
    let reports = runtime.routes();
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0].send_mode, Some(SendMode::SlotCopy));
    assert_eq!(reports[1].send_mode, None);
    assert_eq!(reports[0].qos, Qos::DEFAULT);
    assert_eq!(reports[0].envelope, Envelope::UFrameFields);
    assert!(reports[0].route.to_string().contains("Publish"));
}

#[test]
fn counters_report_what_a_route_carried() {
    let pair = lending_pair(Qos::DEFAULT);
    pair.publisher.publish(&door(1, 1)).unwrap();
    let _ = pair.subscriber.try_recv().unwrap().unwrap();
    assert_eq!(pair.publisher.counters().carried, 1);
    assert_eq!(pair.subscriber.counters().carried, 1);
    assert_eq!(pair.tx.counters(pair.publisher.route_id()).carried, 1);
    assert_eq!(pair.rx.counters(pair.subscriber.route_id()).carried, 1);
}

fn two_wire_pair() -> (
    Publisher<DoorState>,
    Subscriber<DoorState>,
    TransportRuntime,
    TransportRuntime,
) {
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let b = handoff(bus, 2);
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .also_wire(LittleDoorWire)
        .wire(DoorWire)
        .build()
        .unwrap();
    let subscriber = Subscriber::<DoorState>::on(&b, topic())
        .wire(DoorWire)
        .also_wire(LittleDoorWire)
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    (
        publisher.activate(&tx).unwrap(),
        subscriber.activate(&rx).unwrap(),
        tx,
        rx,
    )
}

/// `.wire` is the route's default however `.also_wire` was ordered, `publish_using`
/// names one admitted wire, and anything else is refused.
#[test]
fn most_specific_wire_wins_for_one_message() {
    let (publisher, subscriber, _tx, _rx) = two_wire_pair();
    let value = door(0x0102_0304, 1);

    publisher.publish(&value).unwrap();
    let default = subscriber.try_recv().unwrap().unwrap();
    assert_eq!(default.header().payload_encoding, Some(DOOR_ENCODING));
    assert_eq!(default.payload_bytes(), Some(&[1, 2, 3, 4, 1][..]));
    assert_eq!(default.id, 0x0102_0304);
    drop(default);

    publisher.publish_using::<LittleDoorWire>(&value).unwrap();
    let little = subscriber.try_recv().unwrap().unwrap();
    assert_eq!(little.header().payload_encoding, Some(LITTLE_DOOR_ENCODING));
    assert_eq!(little.payload_bytes(), Some(&[4, 3, 2, 1, 1][..]));
    assert_eq!(little.id, 0x0102_0304);
    drop(little);

    assert!(matches!(
        publisher.publish_using::<VariableDoorWire>(&value),
        Err(SendError::WireNotAdmitted)
    ));
}

#[test]
fn a_message_can_choose_a_wire_from_the_closed_set() {
    let (publisher, subscriber, _tx, _rx) = two_wire_pair();
    publisher
        .publish_encoded(LITTLE_DOOR_ENCODING, PayloadBytes::one(&[9, 0, 0, 0, 1]))
        .unwrap();
    let received = subscriber.try_recv().unwrap().unwrap();
    assert_eq!(received.id, 9);
    assert_eq!(
        received.header().payload_encoding,
        Some(LITTLE_DOOR_ENCODING)
    );
    drop(received);
    let unknown = PayloadEncoding::private_use(0xF999);
    assert!(matches!(
        publisher.publish_encoded(unknown, PayloadBytes::one(&[])),
        Err(SendError::EncodingNotAdmitted(code)) if code == unknown.id()
    ));
}

#[test]
fn default_wire_and_role_wire_are_the_same_mechanism() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let wired = a.with_wire(DoorWire);
    let defaulted = Publisher::<DoorState>::on(&wired, &entity("door", 1), 0x8001)
        .build()
        .unwrap();
    let overridden = Publisher::<DoorState>::on(&wired, &entity("door", 1), 0x8002)
        .wire(LittleDoorWire)
        .build()
        .unwrap();
    let subscriber = Subscriber::<DoorState>::on(&b, UUri::any())
        .wire(DoorWire)
        .also_wire(LittleDoorWire)
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let defaulted = defaulted.activate(&tx).unwrap();
    let overridden = overridden.activate(&tx).unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    defaulted.publish(&door(1, 0)).unwrap();
    overridden.publish(&door(2, 0)).unwrap();
    let first = subscriber.try_recv().unwrap().unwrap();
    let second = subscriber.try_recv().unwrap().unwrap();
    assert_eq!(first.header().payload_encoding, Some(DOOR_ENCODING));
    assert_eq!(second.header().payload_encoding, Some(LITTLE_DOOR_ENCODING));
}

// -------------------------------------------------------------------- overflow

#[test]
fn inbox_overflow_drop_oldest_evicts_queued_frames_and_counts() {
    let (pair, _bus) = handoff_pair(
        32,
        Qos::DEFAULT
            .with_depth(3)
            .with_overflow(Overflow::DropOldest),
    );
    for id in 1..=8 {
        pair.publisher.publish(&door(id, 0)).unwrap();
    }
    // A plain `try_recv` reader: the pull binding's queue is the route's queue,
    // applies its policy and reports each discard.
    let mut got = Vec::new();
    while let Some(frame) = pair.subscriber.try_recv().unwrap() {
        got.push(frame.id);
    }
    assert_eq!(got, vec![6, 7, 8], "no double eviction under DropOldest");
    let counters = pair.subscriber.counters();
    assert_eq!(counters.dropped_oldest, 5);
    assert_eq!(counters.carried, 8, "all were accepted before eviction");
}

#[test]
fn inbox_overflow_drop_newest_keeps_queued_frames_and_counts() {
    let (pair, _bus) = handoff_pair(
        32,
        Qos::DEFAULT
            .with_depth(2)
            .with_overflow(Overflow::DropNewest),
    );
    for id in 1..=5 {
        pair.publisher.publish(&door(id, 0)).unwrap();
    }
    // A plain `try_recv` reader, as above.
    assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, 1);
    assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, 2);
    assert!(pair.subscriber.try_recv().unwrap().is_none());
    let counters = pair.subscriber.counters();
    assert_eq!(counters.dropped_newest, 3);
    assert_eq!(counters.carried, 2);
}

/// On a lending transport the middleware's queue is the queue.
#[test]
fn a_lending_route_keeps_the_newest_depth_in_the_middleware_queue() {
    let pair = lending_pair(
        Qos::DEFAULT
            .with_depth(2)
            .with_overflow(Overflow::DropOldest),
    );
    for id in 1..=5 {
        pair.publisher.publish(&door(id, 0)).unwrap();
    }
    assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, 4);
    assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, 5);
    assert!(pair.subscriber.try_recv().unwrap().is_none());
}

#[test]
fn drop_oldest_never_evicts_a_frame_the_application_holds() {
    let (pair, _bus) = handoff_pair(32, Qos::DEFAULT);
    pair.publisher.publish(&door(1, 0)).unwrap();
    let held = pair.subscriber.try_recv().unwrap().unwrap();
    for id in 2..40 {
        pair.publisher.publish(&door(id, 0)).unwrap();
        let _ = pair.subscriber.try_recv().unwrap();
    }
    assert_eq!(held.id, 1);
}

#[test]
fn held_slots_are_bounded_and_refused_clearly() {
    let (pair, _bus) = handoff_pair(32, Qos::DEFAULT);
    let mut held = Vec::new();
    for id in 0..4 {
        pair.publisher.publish(&door(id, 0)).unwrap();
        held.push(pair.subscriber.try_recv().unwrap().unwrap());
    }
    pair.publisher.publish(&door(99, 0)).unwrap();
    assert!(matches!(
        pair.subscriber.try_recv(),
        Err(ReceiveError::MaxHeldExceeded)
    ));
    drop(held.pop());
    assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, 99);
}

struct Reentrant {
    publisher: OnceLock<Publisher<DoorState>>,
    outcome: OnceLock<Result<SendReport, SendError>>,
}

struct ReentrantBinding {
    state: &'static Reentrant,
}

struct ReentrantRunning {
    state: &'static Reentrant,
}

fn simple_capabilities(native_loans: bool) -> Capabilities {
    Capabilities::new(Envelope::UFrameFields, &[Envelope::UFrameFields], 4096)
        .with_per_message_encoding(true)
        .with_truncate_payload(true)
        .with_declared_clients(false)
        .with_chunked_receive(true)
        .with_native_loans(native_loans)
}

fn simple_qos() -> QosSupport {
    QosSupport::new()
        .with_reliable(true)
        .with_drop_oldest(true)
        .with_drop_newest(true)
        .with_wait(false)
        .with_max_depth(64)
        .with_max_history(0)
        .with_priority_levels(0)
}

impl BindingSetup for ReentrantBinding {
    type Running = ReentrantRunning;

    fn name(&self) -> &'static str {
        "re-entrant"
    }

    fn capabilities(&self) -> Capabilities {
        simple_capabilities(false)
    }

    fn qos_support(&self, _kind: RouteKind) -> QosSupport {
        simple_qos()
    }

    fn prepare_route(
        &mut self,
        _route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        Ok(PreparedRoute::new(BindingRoute(0), RuntimeHeap::None))
    }

    fn into_running(self, _port: RuntimePort) -> Result<ReentrantRunning, BindingRefusal> {
        Ok(ReentrantRunning { state: self.state })
    }
}

impl HandoffRuntime for ReentrantRunning {
    fn receive_bounded(
        &self, route: up_rust::two_mode::BindingRoute, budget: &up_rust::two_mode::ReceiveBudget,
        sink: &mut dyn FnMut(up_rust::two_mode::InboundFrame<'_>) -> up_rust::two_mode::Flow,
    ) -> usize {
        // Explicitly selected sink-visible adapter; no hidden filtering loop.
        if !budget.try_take() { return 0; }
        self.receive(route, &mut |frame| {
            let flow = sink(frame);
            if matches!(flow, up_rust::two_mode::Flow::Stop) || !budget.try_take() {
                up_rust::two_mode::Flow::Stop
            } else { up_rust::two_mode::Flow::Continue }
        })
    }

    fn send(&self, _route: BindingRoute, _frame: OutgoingFrame<'_>) -> Result<up_rust_core::transport::SendReport, up_rust_core::transport::HandoffFailure> {
        // A second send while the first still holds the route's only slot.
        if let Some(publisher) = self.state.publisher.get() {
            let _ = self.state.outcome.set(publisher.publish(&door(2, 0)));
        }
        Ok(up_rust_core::transport::SendReport::unknown())
    }
}

#[test]
fn slot_overflow_refuse_returns_at_once() {
    let state: &'static Reentrant = Box::leak(Box::new(Reentrant {
        publisher: OnceLock::new(),
        outcome: OnceLock::new(),
    }));
    let a = TransportSetup::handoff_in(&memory(), ReentrantBinding { state }).unwrap();
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .slots(1)
        .build()
        .unwrap();
    let runtime = a.enter_runtime().unwrap();
    let publisher = publisher.activate(&runtime).unwrap();
    let _ = state.publisher.set(publisher);
    publisher.publish(&door(1, 0)).unwrap();
    assert!(matches!(
        state.outcome.get(),
        Some(Err(SendError::SlotsExhausted))
    ));
    assert_eq!(publisher.counters().slots_exhausted, 1);
}

#[test]
fn a_lending_route_reports_an_exhausted_pool() {
    let bus = LendingBus::new();
    let a = lending_with(
        bus,
        1,
        LendingOptions {
            slots: 1,
            ..LendingOptions::default()
        },
    );
    let b = lending(bus, 2);
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .slots(1)
        .build()
        .unwrap();
    let subscriber = subscriber_on(&b, Qos::DEFAULT);
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let publisher = publisher.activate(&tx).unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    publisher.publish(&door(1, 0)).unwrap();
    assert!(matches!(
        publisher.publish(&door(2, 0)),
        Err(SendError::Loan(LoanError::PoolExhausted))
    ));
    assert_eq!(publisher.counters().slots_exhausted, 1);
    drop(subscriber.try_recv().unwrap());
    publisher.publish(&door(3, 0)).unwrap();
}

// ------------------------------------------------------------------- envelopes

#[test]
fn the_in_memory_bus_carries_every_built_in_envelope() {
    for envelope in [Envelope::UFrameFields, Envelope::NativeWholeFrame] {
        let bus = InMemoryBus::new(8, 4096);
        let options = InMemoryOptions {
            envelope,
            ..InMemoryOptions::default()
        };
        let pair = activate(
            handoff_with(bus, 1, options),
            handoff_with(bus, 2, options),
            Qos::DEFAULT,
        );
        pair.publisher.publish(&door(42, 1)).unwrap();
        assert_eq!(
            pair.subscriber.try_recv().unwrap().unwrap().id,
            42,
            "{envelope:?}"
        );
    }
}

#[test]
fn an_unknown_encoding_is_refused_and_counted() {
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let b = handoff(bus, 2);
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(LittleDoorWire)
        .build()
        .unwrap();
    let subscriber = subscriber_on(&b, Qos::DEFAULT);
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let publisher = publisher.activate(&tx).unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    publisher.publish(&door(42, 1)).unwrap();
    assert!(subscriber.try_recv().unwrap().is_none());
    assert_eq!(subscriber.counters().refused, 1);
}

#[test]
fn malformed_frames_are_refused_not_panicked() {
    let (pair, bus) = handoff_pair(8, Qos::DEFAULT);
    bus.inject(2, &[1, 2, 3], Some(&[4]));
    bus.inject(2, b"", None);
    pair.publisher.publish(&door(7, 1)).unwrap();
    assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, 7);
    assert_eq!(pair.subscriber.counters().refused, 2);
}

// --------------------------------------------------------------- variable size

/// A loose size bound: the wire reports only that a value fits in the route's maximum.
#[derive(Clone, Copy, Debug, Default)]
struct UnknownSizeWire;

impl Wire<DoorState> for UnknownSizeWire {
    fn describe(&self) -> WireInfo {
        WireInfo::wrapping(
            &VariableDoorWire.describe(),
            "unknown-size-wire",
            WireIdentity::new("test.unknown-size", 0x8A06),
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
    }

    fn encode(&self, value: &DoorState, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        VariableDoorWire.encode(value, out)
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, DoorState>) -> Ready<'s, DoorState> {
        Wire::init_slot(&VariableDoorWire, slot)
    }

    fn decode(
        &self,
        encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut DoorState,
    ) -> Result<(), WireError> {
        VariableDoorWire.decode(encoding, bytes, into)
    }
}

fn variable_pair<W>(wire: W, bus: LendingBus) -> (Publisher<DoorState>, Subscriber<DoorState>)
where
    W: Wire<DoorState> + Clone + Copy,
{
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(wire)
        .build()
        .unwrap();
    let subscriber = Subscriber::<DoorState>::on(&b, topic())
        .wire(wire)
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    (
        publisher.activate(&tx).unwrap(),
        subscriber.activate(&rx).unwrap(),
    )
}

/// A loose bound means the loan is longer than the payload; only what the wire
/// used is published, and the receiver decodes exactly that.
#[test]
fn a_variable_size_wire_on_a_lending_route_publishes_only_what_it_used() {
    let bus = LendingBus::new();
    let (publisher, subscriber) = variable_pair(VariableDoorWire, bus);
    publisher.publish(&door(7, 1)).unwrap();
    let (_, payload) = bus.take_raw(2).expect("a frame");
    assert_eq!(
        payload.as_deref(),
        Some(&b"\x01\x017"[..]),
        "three bytes used, of 12"
    );
    publisher.publish(&door(7, 1)).unwrap();
    let frame = subscriber.try_recv().unwrap().unwrap();
    assert_eq!((frame.id, frame.open), (7, 1));

    let bus = LendingBus::new();
    let (publisher, subscriber) = variable_pair(UnknownSizeWire, bus);
    publisher.publish(&door(4321, 0)).unwrap();
    let (_, payload) = bus.take_raw(2).expect("a frame");
    assert_eq!(payload.as_deref(), Some(&b"\x00\x044321"[..]));
    publisher.publish(&door(4321, 0)).unwrap();
    assert_eq!(subscriber.try_recv().unwrap().unwrap().id, 4321);
}

#[test]
fn a_binding_that_cannot_truncate_refuses_variable_wires_at_setup() {
    let bus = LendingBus::new();
    let a = lending_with(
        bus,
        1,
        LendingOptions {
            truncate_payload: false,
            ..LendingOptions::default()
        },
    );
    let provider = entity("door", 1);
    assert!(matches!(
        Publisher::<DoorState>::on(&a, &provider, 1)
            .wire(VariableDoorWire)
            .build(),
        Err(StartupError::TruncationNotSupported { .. })
    ));
    assert!(Publisher::<DoorState>::on(&a, &provider, 2)
        .wire(DoorWire)
        .build()
        .is_ok());
}

// ------------------------------------------------------------------ the writer

#[derive(Clone, Copy, Debug, Default)]
struct SwappingWire;

impl Wire<u32> for SwappingWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "swapping",
            WireIdentity::new("test.swap", 0x8B05),
            WireIdentity::new("test.swap-family", 0x8B06),
            WirePayloadIdentity::serialized(PayloadEncoding::private_use(0xF104)),
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
        .max_encoded_len(8)
    }

    fn encode(&self, _value: &u32, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        // Safe code can overwrite the writer through `&mut`; the SDK must notice.
        let elsewhere: &'static mut [MaybeUninit<u8>] =
            Box::leak(vec![MaybeUninit::uninit(); 8].into_boxed_slice());
        *out = PayloadWriter::new(elsewhere);
        out.put(&[1, 2, 3, 4])
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, u32>) -> Ready<'s, u32> {
        slot.write(0)
    }

    fn decode(
        &self,
        _encoding: PayloadEncoding,
        _bytes: &[u8],
        _into: &mut u32,
    ) -> Result<(), WireError> {
        Ok(())
    }
}

#[test]
// [utest->req~ssr-unsafe-wire-untrusted~1] method: test
fn a_wire_that_replaces_its_writer_is_refused() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let publisher = Publisher::<u32>::on(&a, &entity("door", 1), 0x8001)
        .wire(SwappingWire)
        .build()
        .unwrap();
    let runtime = a.enter_runtime().unwrap();
    assert!(matches!(
        publisher.activate(&runtime).unwrap().publish(&1),
        Err(SendError::Wire(WireError::WriterReplaced))
    ));

    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let publisher = Publisher::<u32>::on(&a, &entity("door", 1), 0x8001)
        .wire(SwappingWire)
        .build()
        .unwrap();
    let runtime = a.enter_runtime().unwrap();
    assert!(matches!(
        publisher.activate(&runtime).unwrap().publish(&1),
        Err(SendError::Wire(WireError::WriterReplaced))
    ));
}

// ---------------------------------------------------------------- wire wrappers

#[derive(Clone, Copy, Debug, Default)]
struct BufferingWrapper<W>(W);

impl<W: Wire<DoorState>> Wire<DoorState> for BufferingWrapper<W> {
    fn describe(&self) -> WireInfo {
        WireInfo::wrapping(
            &self.0.describe(),
            "buffering-wrapper",
            WireIdentity::new("test.buffering", 0x8B09),
            RuntimeHeap::Allocates("the wrapper buffers on the heap"),
            RuntimeHeap::None,
        )
    }

    fn encoded_size(&self, value: &DoorState) -> EncodedSize {
        self.0.encoded_size(value)
    }

    fn encode(&self, value: &DoorState, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        self.0.encode(value, out)
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, DoorState>) -> Ready<'s, DoorState> {
        self.0.init_slot(slot)
    }

    fn decode(
        &self,
        encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut DoorState,
    ) -> Result<(), WireError> {
        self.0.decode(encoding, bytes, into)
    }
}

#[test]
fn a_wire_wrapper_states_its_own_heap_class() {
    let info = BufferingWrapper(DoorWire).describe();
    assert!(matches!(info.encode_heap(), RuntimeHeap::Allocates(_)));
    assert!(matches!(info.decode_heap(), RuntimeHeap::None));
    assert_eq!(info.wire_id.compact_id(), 0x8B09);
    assert_eq!(info.identity, DoorWire.describe().identity);
    // The inner wire's heap-free claim does not carry over to the wrapper.
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let _ = Publisher::<DoorState>::on(&a, &entity("door", 1), 1)
        .wire(BufferingWrapper(DoorWire))
        .build()
        .unwrap();
    assert!(matches!(
        a.enter_runtime(),
        Err(StartupError::RouteNeedsHeapAtRuntime { .. })
    ));
}

// -------------------------------------------------- deadlines, TTL and clocks

fn ttl_pair(
    clock: &'static TestClock,
    ttl: Duration,
) -> (
    Publisher<DoorState>,
    Subscriber<DoorState>,
    TransportRuntime,
) {
    let bus = LendingBus::new();
    let a = lending_on(bus, 1, clock);
    let b = lending_on(bus, 2, clock);
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .ttl(ttl)
        .build()
        .unwrap();
    let subscriber = subscriber_on(&b, Qos::DEFAULT);
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    (
        publisher.activate(&tx).unwrap(),
        subscriber.activate(&rx).unwrap(),
        rx,
    )
}

#[test]
fn an_expired_message_is_dropped_at_try_recv_and_counted() {
    let clock = TestClock::leak(1_000, 1_700_000_000_000);
    let (publisher, subscriber, _rx) = ttl_pair(clock, Duration::from_millis(100));
    publisher.publish(&door(1, 0)).unwrap();
    clock.advance_wall_ms(99);
    assert_eq!(subscriber.try_recv().unwrap().unwrap().id, 1, "still fresh");
    publisher.publish(&door(2, 0)).unwrap();
    clock.advance_wall_ms(200);
    assert!(subscriber.try_recv().unwrap().is_none());
    assert_eq!(subscriber.counters().expired, 1);
}

/// A TTL'd frame sitting ahead of a fresh one must not hide the fresh one.
#[test]
fn an_expired_frame_does_not_hide_the_next_frame() {
    let clock = TestClock::leak(1_000, 1_700_000_000_000);
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff_on(bus, 1, clock);
    let b = handoff_on(bus, 2, clock);
    let short = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .ttl(Duration::from_millis(10))
        .build()
        .unwrap();
    let long = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .ttl(Duration::from_secs(10))
        .build()
        .unwrap();
    let subscriber = subscriber_on(&b, Qos::DEFAULT);
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let (short, long) = (short.activate(&tx).unwrap(), long.activate(&tx).unwrap());
    let subscriber = subscriber.activate(&rx).unwrap();
    short.publish(&door(1, 0)).unwrap();
    long.publish(&door(2, 0)).unwrap();
    clock.advance_wall_ms(1_000);
    assert_eq!(subscriber.try_recv().unwrap().unwrap().id, 2);
    assert_eq!(subscriber.counters().expired, 1);
}

#[test]
fn when_the_clock_cannot_be_read_a_ttl_frame_is_refused_and_counted() {
    let clock = TestClock::leak(1_000, 1_700_000_000_000);
    let (publisher, subscriber, _rx) = ttl_pair(clock, Duration::from_millis(100));
    publisher.publish(&door(1, 0)).unwrap();
    clock.break_wall_clock();
    assert!(
        subscriber.try_recv().unwrap().is_none(),
        "an unknown expiry is not processed"
    );
    assert_eq!(subscriber.counters().clock_unavailable, 1);
    clock.repair_wall_clock();
    publisher.publish(&door(2, 0)).unwrap();
    assert_eq!(subscriber.try_recv().unwrap().unwrap().id, 2);
}

#[test]
fn enter_runtime_refuses_a_platform_with_no_wall_clock() {
    let clock = TestClock::leak(1_000, 1_700_000_000_000);
    clock.break_wall_clock();
    let a = lending_on(LendingBus::new(), 1, clock);
    assert!(matches!(
        a.enter_runtime(),
        Err(StartupError::InvalidConfig(_))
    ));
}

#[test]
fn a_silent_publisher_trips_the_deadline_monitor_once_per_gap() {
    let clock = TestClock::leak(1_000, 1_700_000_000_000);
    let bus = LendingBus::new();
    let a = lending_on(bus, 1, clock);
    let b = lending_on(bus, 2, clock);
    let publisher = publisher_on(&a);
    let subscriber = subscriber_on(&b, Qos::DEFAULT.with_deadline(Duration::from_millis(10)));
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let publisher = publisher.activate(&tx).unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    clock.advance_ns(20_000_000);
    assert!(subscriber.try_recv().unwrap().is_none());
    assert!(subscriber.try_recv().unwrap().is_none());
    assert_eq!(subscriber.counters().deadline_missed, 1, "one per gap");
    publisher.publish(&door(1, 0)).unwrap();
    let _ = subscriber.try_recv().unwrap();
    clock.advance_ns(20_000_000);
    assert_eq!(rx.service(), 1, "the service tick counts an unpolled route");
    assert_eq!(subscriber.counters().deadline_missed, 2);
}

// ------------------------------------------------------------------------ RPC

struct Rpc {
    client: RpcClient<DoorState, DoorState>,
    server: RpcServer<DoorState, DoorState>,
    bus: InMemoryBus,
    client_rt: TransportRuntime,
    server_rt: TransportRuntime,
}

fn rpc_with(
    a: TransportSetup,
    b: TransportSetup,
    bus: InMemoryBus,
    clients: &[UUri],
    ttl: Duration,
) -> Rpc {
    let method = entity("door", 2).get_resource_uri(METHOD);
    let client = RpcClient::<DoorState, DoorState>::on(&a, &entity("door", 1), method)
        .wire(DoorWire)
        .ttl(ttl)
        .max_in_flight(2)
        .build()
        .unwrap();
    let server = RpcServer::<DoorState, DoorState>::on(&b, &entity("door", 2), METHOD)
        .wire(DoorWire)
        .clients(clients)
        .build()
        .unwrap();
    let client_rt = a.enter_runtime().unwrap();
    let server_rt = b.enter_runtime().unwrap();
    Rpc {
        client: client.activate(&client_rt).unwrap(),
        server: server.activate(&server_rt).unwrap(),
        bus,
        client_rt,
        server_rt,
    }
}

fn rpc(clients: &[UUri], ttl: Duration) -> Rpc {
    let bus = InMemoryBus::new(8, 4096);
    rpc_with(handoff(bus, 1), handoff(bus, 2), bus, clients, ttl)
}

fn rpc_clocked(clock: &'static TestClock, ttl: Duration) -> Rpc {
    let bus = InMemoryBus::new(16, 4096);
    rpc_with(
        handoff_on(bus, 1, clock),
        handoff_on(bus, 2, clock),
        bus,
        &[],
        ttl,
    )
}

fn answer(server: &RpcServer<DoorState, DoorState>, delta: u32) {
    let request = server.try_recv().unwrap().expect("a request");
    let id = request.id;
    request.respond(&door(id + delta, 1)).unwrap();
}

#[test]
fn a_call_completes_with_the_servers_response() {
    let rpc = rpc(&[], TTL);
    let mut call = rpc.client.invoke(&door(41, 0)).unwrap();
    answer(&rpc.server, 1);
    {
        let reply = call.try_response().unwrap().expect("a response");
        assert_eq!(reply.id, 42);
    }
    assert!(matches!(call.try_response(), Err(CallError::AlreadyTaken)));
}

#[test]
fn every_call_slot_is_reserved_before_send_and_refused_when_exhausted() {
    let rpc = rpc(&[], TTL);
    let first = rpc.client.invoke(&door(1, 0)).unwrap();
    let second = rpc.client.invoke(&door(2, 0)).unwrap();
    assert!(matches!(
        rpc.client.invoke(&door(3, 0)),
        Err(CallError::SlotsExhausted)
    ));
    drop(first);
    assert!(rpc.client.invoke(&door(4, 0)).is_ok());
    drop(second);
}

#[test]
fn a_vanished_server_completes_the_call_by_deadline() {
    let clock = TestClock::leak(1_000, 1_700_000_000_000);
    let rpc = rpc_clocked(clock, Duration::from_millis(10));
    let mut call = rpc.client.invoke(&door(1, 0)).unwrap();
    assert!(call.try_response().unwrap().is_none());
    clock.advance_ns(20_000_000);
    assert_eq!(rpc.client_rt.service(), 1, "the tick completes the call");
    assert!(matches!(call.try_response(), Err(CallError::TimedOut)));
    drop(call);
    assert!(rpc.client.invoke(&door(2, 0)).is_ok(), "the slot came back");
}

#[test]
fn polling_alone_also_enforces_the_deadline() {
    let clock = TestClock::leak(1_000, 1_700_000_000_000);
    let rpc = rpc_clocked(clock, Duration::from_millis(1));
    let mut call = rpc.client.invoke(&door(1, 0)).unwrap();
    clock.advance_ns(5_000_000);
    assert!(matches!(call.try_response(), Err(CallError::TimedOut)));
}

#[test]
fn a_late_response_matches_no_call() {
    let rpc = rpc(&[], TTL);
    let call = rpc.client.invoke(&door(1, 0)).unwrap();
    let request = rpc.server.try_recv().unwrap().unwrap();
    drop(call);
    request.respond(&door(2, 0)).unwrap();
    assert_eq!(rpc.client_rt.pump(), 1);
    assert_eq!(rpc.client.response_counters().refused, 1);
}

#[test]
fn a_response_must_match_the_full_key_to_complete_a_call() {
    let rpc = rpc(&[], TTL);
    let mut call = rpc.client.invoke(&door(5, 1)).unwrap();
    let request_id = {
        let request = rpc.server.try_recv().unwrap().unwrap();
        request.received().header().id.clone()
    };
    let method = entity("door", 2).get_resource_uri(METHOD);
    let reply_to = entity("door", 1).get_source_uri();
    let info = DoorWire.describe();
    let harness = EnvelopeHarness::new(
        Envelope::UFrameFields,
        info.wire_id,
        info.payload_family,
        info.identity,
    );
    let respond = |reqid: &UUID, source: &UUri, sink: &UUri| {
        let id = UUID::build();
        let parts = TxHeaderParts {
            kind: UMessageType::Response,
            priority: Some(UPriority::CS4),
            id: &id,
            reqid: Some(reqid),
            ttl: None,
            comm_status: None,
            permission_level: None,
            source,
            sink: Some(sink),
            token: None,
            traceparent: None,
            payload_encoding: Some(DOOR_ENCODING),
            native_type_token: None,
        };
        let (head_len, frame) = harness
            .encode(&parts, Some(&[0, 0, 0, 50, 0]))
            .expect("encode");
        let (metadata, payload) = frame.split_at(head_len);
        rpc.bus.inject(1, metadata, Some(payload));
    };

    let stranger = entity("door", 2).get_resource_uri(0x0002);
    respond(&request_id, &stranger, &reply_to);
    assert!(call.try_response().unwrap().is_none(), "wrong source");

    let other_client = entity("other", 3).get_source_uri();
    respond(&request_id, &method, &other_client);
    assert!(call.try_response().unwrap().is_none(), "wrong sink");

    respond(&UUID::build(), &method, &reply_to);
    assert!(call.try_response().unwrap().is_none(), "wrong request ID");
    assert_eq!(rpc.client.response_counters().refused, 3);

    respond(&request_id, &method, &reply_to);
    assert_eq!(call.try_response().unwrap().expect("response").id, 50);
}

#[test]
fn a_server_failure_status_reaches_the_caller() {
    let rpc = rpc(&[], TTL);
    let mut call = rpc.client.invoke(&door(1, 0)).unwrap();
    rpc.server
        .try_recv()
        .unwrap()
        .unwrap()
        .respond_failure(UCode::PermissionDenied)
        .unwrap();
    assert!(matches!(
        call.try_response(),
        Err(CallError::Rejected(UCode::PermissionDenied))
    ));
}

#[test]
fn a_restarted_server_fails_outstanding_calls_and_stale_responses_are_refused() {
    let rpc = rpc(&[], TTL);
    let mut first = rpc.client.invoke(&door(1, 0)).unwrap();
    answer(&rpc.server, 1);
    assert!(first.try_response().unwrap().is_some(), "peer seen once");
    drop(first);

    let mut outstanding = rpc.client.invoke(&door(2, 0)).unwrap();
    let _ = rpc.server.try_recv().unwrap().unwrap();
    let restarted = rpc.bus.restart(2);
    rpc.bus.report_peer_restarted(
        1,
        PeerRef {
            peer: 2,
            incarnation: restarted,
        },
    );
    assert!(matches!(
        outstanding.try_response(),
        Err(CallError::PeerRestarted)
    ));
    drop(outstanding);

    // A response still in transit from the earlier incarnation completes nothing.
    let mut call = rpc.client.invoke(&door(3, 0)).unwrap();
    let request = rpc.server.try_recv().unwrap().unwrap();
    rpc.bus.set_incarnation(2, restarted - 1);
    request.respond(&door(30, 0)).unwrap();
    assert!(call.try_response().unwrap().is_none());
    assert_eq!(rpc.client.response_counters().stale_peer, 1);
    rpc.bus.set_incarnation(2, restarted);
    drop(call);

    // A server reported gone fails what is outstanding.
    let mut lost = rpc.client.invoke(&door(4, 0)).unwrap();
    rpc.bus.report_peer_lost(
        1,
        PeerRef {
            peer: 2,
            incarnation: restarted,
        },
    );
    assert!(matches!(lost.try_response(), Err(CallError::PeerLost)));
}

/// A response racing the deadline completes the call exactly once, and its slot returns.
#[test]
fn a_response_racing_its_deadline_completes_the_call_exactly_once() {
    let rounds = if cfg!(miri) { 16 } else { 2000 };
    let clock = TestClock::leak(1_000, 1_700_000_000_000);
    let rpc = rpc_clocked(clock, Duration::from_millis(1));
    for _ in 0..rounds {
        let mut call = rpc.client.invoke(&door(1, 0)).unwrap();
        let request = rpc.server.try_recv().unwrap().unwrap();
        let client_rt = rpc.client_rt;
        std::thread::scope(|scope| {
            scope.spawn(move || {
                let _ = request.respond(&door(2, 0));
                client_rt.pump();
            });
            scope.spawn(|| {
                clock.advance_ns(2_000_000);
                client_rt.service();
            });
        });
        client_rt.pump();
        // Exactly one outcome, and it does not change.
        let first = match call.try_response() {
            Ok(Some(reply)) => {
                assert_eq!(reply.id, 2);
                true
            }
            Err(CallError::TimedOut) => false,
            other => panic!("unexpected {other:?}"),
        };
        let second = call.try_response();
        if first {
            assert!(matches!(second, Err(CallError::AlreadyTaken)));
        } else {
            assert!(matches!(second, Err(CallError::TimedOut)));
        }
        drop(call);
    }
    // Every slot came back: a full table can still be reserved.
    let _a = rpc.client.invoke(&door(1, 0)).unwrap();
    let _b = rpc.client.invoke(&door(2, 0)).unwrap();
    let _ = rpc.server_rt;
}

#[test]
fn a_response_lease_outlives_later_traffic() {
    let rpc = rpc(&[], TTL);
    let mut call = rpc.client.invoke(&door(1, 0)).unwrap();
    answer(&rpc.server, 1);
    let reply = call.try_response().unwrap().unwrap();
    assert_eq!(reply.id, 2);
    let mut second = rpc.client.invoke(&door(10, 0)).unwrap();
    answer(&rpc.server, 1);
    let reply2 = second.try_response().unwrap().unwrap();
    assert_eq!(reply2.id, 11);
    assert_eq!(reply.id, 2, "the first reply stays valid");
}

#[test]
fn a_server_on_a_declaring_binding_is_refused_without_clients() {
    let bus = LendingBus::new();
    let a = lending_with(
        bus,
        1,
        LendingOptions {
            declared_clients: true,
            ..LendingOptions::default()
        },
    );
    let refused = RpcServer::<DoorState, DoorState>::on(&a, &entity("door", 1), METHOD)
        .wire(DoorWire)
        .build()
        .unwrap_err();
    assert!(matches!(refused, StartupError::ClientsRequired { .. }));
    assert!(refused.to_string().contains("clients"), "{refused}");
}

#[test]
fn an_undeclared_client_is_refused_and_counted() {
    let elsewhere = entity("client", 9).get_source_uri();
    let rpc = rpc(std::slice::from_ref(&elsewhere), TTL);
    let _call = rpc.client.invoke(&door(1, 0)).unwrap();
    assert!(rpc.server.try_recv().unwrap().is_none());
    assert_eq!(rpc.server.counters().unknown_client, 1);
}

#[test]
fn a_declared_client_gets_its_response() {
    let known = entity("door", 1).get_source_uri();
    let rpc = rpc(std::slice::from_ref(&known), TTL);
    let mut call = rpc.client.invoke(&door(1, 0)).unwrap();
    answer(&rpc.server, 1);
    assert_eq!(call.try_response().unwrap().expect("response").id, 2);
}

// ---------------------------------------------------------- delivery paths

struct EarlyDeliver {
    inbound: Option<InboundRoute>,
    metadata: Vec<u8>,
    payload: Vec<u8>,
}

struct EarlyRunning;

impl BindingSetup for EarlyDeliver {
    type Running = EarlyRunning;

    fn name(&self) -> &'static str {
        "early-deliver"
    }

    fn capabilities(&self) -> Capabilities {
        simple_capabilities(false)
    }

    fn qos_support(&self, _kind: RouteKind) -> QosSupport {
        simple_qos()
    }

    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        self.inbound = route.inbound;
        Ok(PreparedRoute::new(BindingRoute(0), RuntimeHeap::None))
    }

    fn into_running(self, port: RuntimePort) -> Result<EarlyRunning, BindingRefusal> {
        // The running value does not exist yet; `deliver` must not need it.
        port.deliver(
            self.inbound.ok_or(BindingRefusal("no route"))?,
            InboundFrame {
                metadata: &self.metadata,
                payload: PayloadBytes::one(&self.payload),
                peer: PeerRef::UNKNOWN,
                reply: ReplyToken::default(),
            },
        )
        .map_err(|_| BindingRefusal("delivery during into_running failed"))?;
        Ok(EarlyRunning)
    }
}

impl HandoffRuntime for EarlyRunning {
    fn receive_bounded(
        &self, route: up_rust::two_mode::BindingRoute, budget: &up_rust::two_mode::ReceiveBudget,
        sink: &mut dyn FnMut(up_rust::two_mode::InboundFrame<'_>) -> up_rust::two_mode::Flow,
    ) -> usize {
        // Explicitly selected sink-visible adapter; no hidden filtering loop.
        if !budget.try_take() { return 0; }
        self.receive(route, &mut |frame| {
            let flow = sink(frame);
            if matches!(flow, up_rust::two_mode::Flow::Stop) || !budget.try_take() {
                up_rust::two_mode::Flow::Stop
            } else { up_rust::two_mode::Flow::Continue }
        })
    }

    fn send(&self, _route: BindingRoute, _frame: OutgoingFrame<'_>) -> Result<up_rust_core::transport::SendReport, up_rust_core::transport::HandoffFailure> {
        Ok(up_rust_core::transport::SendReport::unknown())
    }
}

#[test]
fn a_binding_can_deliver_during_into_running() {
    let (metadata, payload) = captured_door_frame(7, 1);
    let a = TransportSetup::handoff_in(
        &memory(),
        EarlyDeliver {
            inbound: None,
            metadata,
            payload,
        },
    )
    .unwrap();
    let subscriber = subscriber_on(&a, Qos::DEFAULT);
    let runtime = a.enter_runtime().unwrap();
    let subscriber = subscriber.activate(&runtime).unwrap();
    let frame = subscriber.try_recv().unwrap().unwrap();
    assert_eq!((frame.id, frame.open), (7, 1));
}

#[test]
// [utest->req~ssr-unsafe-wire-untrusted~1] method: test
fn a_chunked_payload_is_copied_once_into_a_receive_slot() {
    let bus = InMemoryBus::new(8, 4096);
    let b = handoff(bus, 2);
    let subscriber = subscriber_on(&b, Qos::DEFAULT);
    let rx = b.enter_runtime().unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    let (metadata, payload) = captured_door_frame(9, 1);
    assert_eq!(payload, vec![0, 0, 0, 9, 1]);
    let chunks: [&[u8]; 2] = [&payload[..2], &payload[2..]];
    bus.deliver_direct(
        2,
        BindingRoute(0),
        InboundFrame {
            metadata: &metadata,
            payload: PayloadBytes::chunked(&chunks),
            peer: PeerRef {
                peer: 3,
                incarnation: 1,
            },
            reply: ReplyToken::default(),
        },
    )
    .unwrap();
    let received = subscriber.try_recv().unwrap().unwrap();
    assert_eq!(received.id, 9);
    let bytes = received.payload_bytes().unwrap();
    assert_eq!(bytes, &[0, 0, 0, 9, 1][..]);
    assert!(
        !payload.as_ptr_range().contains(&bytes.as_ptr()),
        "the slot holds its own copy"
    );
    assert_eq!(received.peer().incarnation, 1);
}

#[test]
fn received_is_shared_across_threads() {
    fn assert_traits<T: Send + Sync>() {}
    assert_traits::<Received<DoorState>>();
    let (pair, _bus) = handoff_pair(8, Qos::DEFAULT);
    pair.publisher.publish(&door(11, 1)).unwrap();
    let received = pair.subscriber.try_recv().unwrap().unwrap();
    std::thread::scope(|scope| {
        scope.spawn(|| assert_eq!(received.id, 11));
        scope.spawn(|| assert_eq!(received.open, 1));
        scope.spawn(|| assert!(received.payload_bytes().is_some()));
        scope.spawn(|| assert_eq!(received.header().kind, UMessageType::Publish));
    });
}

// --------------------------------------------------------------- notifications

#[test]
fn a_notifier_on_a_lending_transport_reaches_its_listener() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let dashboard = entity("dashboard", 2);
    let origin = entity("door", 1).get_resource_uri(0x8100);
    let notifier =
        Notifier::<DoorState>::on(&a, &entity("door", 1), 0x8100, dashboard.get_source_uri())
            .wire(DoorWire)
            .build()
            .unwrap();
    let listener = NotificationListener::<DoorState>::on(&b, origin, &dashboard)
        .wire(DoorWire)
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let notifier = notifier.activate(&tx).unwrap();
    let listener = listener.activate(&rx).unwrap();
    notifier.notify(&door(3, 1)).unwrap();
    let got = listener.try_recv().unwrap().unwrap();
    assert_eq!(got.id, 3);
    assert_eq!(got.header().kind, UMessageType::Notification);
    assert_eq!(
        got.header().sink.as_ref(),
        Some(&dashboard.get_source_uri())
    );
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Lamp(u8);

#[derive(Clone, Copy, Debug, Default)]
struct LampWire;

impl Wire<Lamp> for LampWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "lamp",
            WireIdentity::new("test.lamp", 0x8C03),
            WireIdentity::new("test.lamp-family", 0x8C04),
            WirePayloadIdentity::serialized(PayloadEncoding::private_use(0xF202)),
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
        .max_encoded_len(1)
        .exact()
    }

    fn encode(&self, value: &Lamp, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        out.put(&[value.0])
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, Lamp>) -> Ready<'s, Lamp> {
        slot.write(Lamp::default())
    }

    fn decode(
        &self,
        _encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut Lamp,
    ) -> Result<(), WireError> {
        let [value] = *bytes else {
            return Err(WireError::Malformed("one byte"));
        };
        into.0 = value;
        Ok(())
    }
}

wire_table!(
    /// One wire value for both message types of this test.
    #[derive(Clone, Copy, Debug, Default)]
    VehicleTable {
        /// Doors.
        door: DoorWire => DoorState,
        /// Lamps.
        lamp: LampWire => Lamp,
    }
);

#[test]
fn one_wire_table_value_serves_several_message_types() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let provider = entity("door", 1);
    let table = VehicleTable::default();
    let door_out = Publisher::<DoorState>::on(&a, &provider, 0x8001)
        .wire(table)
        .build()
        .unwrap();
    let lamp_out = Publisher::<Lamp>::on(&a, &provider, 0x8002)
        .wire(table)
        .build()
        .unwrap();
    let door_in = Subscriber::<DoorState>::on(&b, topic())
        .wire(table)
        .build()
        .unwrap();
    let lamp_in = Subscriber::<Lamp>::on(&b, provider.get_resource_uri(0x8002))
        .wire(table)
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    door_out
        .activate(&tx)
        .unwrap()
        .publish(&door(3, 1))
        .unwrap();
    lamp_out.activate(&tx).unwrap().publish(&Lamp(7)).unwrap();
    assert_eq!(
        door_in
            .activate(&rx)
            .unwrap()
            .try_recv()
            .unwrap()
            .unwrap()
            .id,
        3
    );
    assert_eq!(
        *lamp_in.activate(&rx).unwrap().try_recv().unwrap().unwrap(),
        Lamp(7)
    );
}

#[test]
fn the_heap_backed_constructors_need_no_size_up_front() {
    use up_rust::two_mode::TransportSetupExt as _;
    let lend = LendingBus::new();
    let hand = InMemoryBus::new(8, 512);
    let a = TransportSetup::lending(lend.participant(1)).unwrap();
    let b = TransportSetup::handoff(hand.participant(2)).unwrap();
    let publisher = publisher_on(&a);
    let subscriber = subscriber_on(&b, Qos::DEFAULT);
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    assert!(publisher.activate(&tx).is_ok());
    assert!(subscriber.activate(&rx).is_ok());
}

/// A retained response is held for the call, and goes back when it is decoded.
#[test]
// [utest->req~ssr-unsafe-wire-untrusted~1] method: test
fn a_response_on_a_lending_route_is_held_until_the_call_takes_it() {
    let bus = LendingBus::new();
    let options = LendingOptions {
        push: true,
        ..LendingOptions::default()
    };
    let a = lending_with(bus, 1, options);
    let b = lending_with(bus, 2, options);
    let known = entity("door", 1).get_source_uri();
    let method = entity("door", 2).get_resource_uri(METHOD);
    let client = RpcClient::<DoorState, DoorState>::on(&a, &entity("door", 1), method)
        .wire(DoorWire)
        .ttl(TTL)
        .build()
        .unwrap();
    let server = RpcServer::<DoorState, DoorState>::on(&b, &entity("door", 2), METHOD)
        .wire(DoorWire)
        .clients(std::slice::from_ref(&known))
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let client = client.activate(&tx).unwrap();
    let server = server.activate(&rx).unwrap();
    let mut call = client.invoke(&door(1, 0)).unwrap();
    let request = server.try_recv().unwrap().expect("a request");
    let id = request.id;
    let before = bus.frame_drops();
    request.respond(&door(id + 1, 0)).unwrap();
    assert_eq!(
        bus.frame_drops(),
        before,
        "the response is held for the call"
    );
    assert_eq!(call.try_response().unwrap().expect("a response").id, 2);
    assert_eq!(bus.frame_drops(), before + 1, "decoded, so it went back");
}

#[test]
fn typed_notification_topics_and_methods_are_accepted_too() {
    const LOW_BATTERY: NotificationTopic<DoorState> = NotificationTopic::new(0x8200);
    const SET_DOOR: Method<DoorState, DoorState> = Method::new(0x0005);
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let b = handoff(bus, 2);
    let dashboard = entity("dashboard", 3);
    let notifier = Notifier::on(
        &a,
        &entity("door", 1),
        LOW_BATTERY,
        dashboard.get_source_uri(),
    )
    .wire(DoorWire)
    .build()
    .unwrap();
    let listener = NotificationListener::<DoorState>::on(
        &b,
        entity("door", 1).get_resource_uri(0x8200),
        &dashboard,
    )
    .wire(DoorWire)
    .build()
    .unwrap();
    let method = entity("door", 2).get_resource_uri(0x0005);
    let client = RpcClient::<DoorState, DoorState>::on(&a, &entity("door", 1), method)
        .wire(DoorWire)
        .ttl(TTL)
        .build()
        .unwrap();
    let server = RpcServer::on(&b, &entity("door", 2), SET_DOOR)
        .wire(DoorWire)
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let notifier: Notifier<DoorState> = notifier.activate(&tx).unwrap();
    let listener = listener.activate(&rx).unwrap();
    let client = client.activate(&tx).unwrap();
    let server: RpcServer<DoorState, DoorState> = server.activate(&rx).unwrap();
    notifier.notify(&door(9, 0)).unwrap();
    assert_eq!(listener.try_recv().unwrap().unwrap().id, 9);
    let mut call = client.invoke(&door(1, 0)).unwrap();
    server
        .try_recv()
        .unwrap()
        .unwrap()
        .respond(&door(2, 0))
        .unwrap();
    assert_eq!(call.try_response().unwrap().unwrap().id, 2);
}

#[test]
fn typed_topics_are_optional_and_interchangeable_with_plain_ids() {
    const DOOR_STATE: PublishTopic<DoorState> = PublishTopic::new(0x8001);
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let publisher = Publisher::on(&a, &entity("door", 1), DOOR_STATE)
        .wire(DoorWire)
        .build()
        .unwrap();
    let subscriber = subscriber_on(&b, Qos::DEFAULT);
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let publisher: Publisher<DoorState> = publisher.activate(&tx).unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    publisher.publish(&door(21, 0)).unwrap();
    assert_eq!(subscriber.try_recv().unwrap().unwrap().id, 21);
}

// ----------------------------------------------------------------------- races

#[test]
fn concurrent_owners_never_share_a_slot() {
    let rounds = if cfg!(miri) { 40 } else { 20_000 };
    let pool: &'static SlotPool = Box::leak(Box::new(SlotPool::new_in(&memory(), 3, 8).unwrap()));
    std::thread::scope(|scope| {
        for thread in 0..4_u8 {
            scope.spawn(move || {
                let pattern = [thread + 1; 8];
                for _ in 0..rounds {
                    let Ok(mut guard) = pool.acquire() else {
                        std::thread::yield_now();
                        continue;
                    };
                    guard.bytes_mut().copy_from_slice(&pattern);
                    std::thread::yield_now();
                    assert_eq!(guard.bytes(), pattern, "another owner wrote this slot");
                }
            });
        }
    });
    let guards: Vec<_> = (0..3).map(|_| pool.acquire().unwrap()).collect();
    assert!(
        matches!(pool.acquire(), Err(up_rust::two_mode::PoolError::Full)),
        "every slot came back exactly once"
    );
    drop(guards);
}

#[test]
fn concurrent_deliveries_reach_the_subscriber_whole_and_once() {
    const THREADS: u32 = 4;
    const PER_THREAD: u32 = 16;
    let rounds = if cfg!(miri) { 2 } else { 64 };
    for _ in 0..rounds {
        let bus = LendingBus::new();
        let setups: Vec<_> = (0..THREADS)
            .map(|index| lending(bus, u64::from(index) + 1))
            .collect();
        let sink = lending(bus, 100);
        let subscriber = Subscriber::<DoorState>::on(&sink, UUri::any())
            .wire(DoorWire)
            .qos(Qos::DEFAULT.with_depth(128))
            .build()
            .unwrap();
        let publishers: Vec<_> = setups
            .iter()
            .enumerate()
            .map(|(index, setup)| {
                Publisher::<DoorState>::on(
                    setup,
                    &entity("door", index as u16 + 1),
                    0x8001 + index as u16,
                )
                .wire(DoorWire)
                .build()
                .unwrap()
            })
            .collect();
        let runtimes: Vec<_> = setups
            .into_iter()
            .map(TransportSetup::enter_runtime)
            .collect::<Result<_, _>>()
            .unwrap();
        let sink_runtime = sink.enter_runtime().unwrap();
        let subscriber = subscriber.activate(&sink_runtime).unwrap();
        let active: Vec<_> = publishers
            .into_iter()
            .zip(runtimes.iter())
            .map(|(publisher, runtime)| publisher.activate(runtime).unwrap())
            .collect();
        std::thread::scope(|scope| {
            for (index, publisher) in active.iter().enumerate() {
                let publisher = *publisher;
                scope.spawn(move || {
                    for offset in 0..PER_THREAD {
                        publisher
                            .publish(&door(index as u32 * 1000 + offset, 0))
                            .unwrap();
                    }
                });
            }
        });
        let mut got = Vec::new();
        while let Some(frame) = subscriber.try_recv().unwrap() {
            got.push(frame.id);
        }
        got.sort_unstable();
        got.dedup();
        assert_eq!(got.len(), (THREADS * PER_THREAD) as usize);
    }
}

/// Binding threads deliver into the inbox while the application drains it.
///
/// This is the race behind the two defects `ws-owned` found: a free-list slot
/// leak and a false queue-full on a stale read.
#[test]
fn drop_oldest_under_concurrent_delivery_keeps_counts_and_slots() {
    const THREADS: u32 = 3;
    const PER_THREAD: u32 = if cfg!(miri) { 24 } else { 4_000 };
    const DEPTH: u16 = 4;
    let (metadata, payload) = captured_door_frame(1, 0);
    let bus = InMemoryBus::new(8, 4096);
    let sink = handoff(bus, 100);
    let subscriber = Subscriber::<DoorState>::on(&sink, UUri::any())
        .wire(DoorWire)
        .max_held(2)
        .qos(
            Qos::DEFAULT
                .with_depth(DEPTH)
                .with_overflow(Overflow::DropOldest),
        )
        .build()
        .unwrap();
    let rx = sink.enter_runtime().unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    let frame = || InboundFrame {
        metadata: &metadata,
        payload: PayloadBytes::one(&payload),
        peer: PeerRef::UNKNOWN,
        reply: ReplyToken::default(),
    };
    let done = AtomicU32::new(0);
    let received = AtomicU64::new(0);
    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| {
                for _ in 0..PER_THREAD {
                    let _ = bus.deliver_direct(100, BindingRoute(0), frame());
                }
                done.fetch_add(1, Ordering::SeqCst);
            });
        }
        scope.spawn(|| loop {
            let finished = done.load(Ordering::SeqCst) == THREADS;
            match subscriber.try_recv() {
                Ok(Some(_)) => {
                    received.fetch_add(1, Ordering::SeqCst);
                }
                _ if finished => break,
                _ => std::thread::yield_now(),
            }
        });
    });
    let counters = subscriber.counters();
    let total = u64::from(THREADS) * u64::from(PER_THREAD);
    assert_eq!(
        u64::from(counters.carried) + u64::from(counters.refused_contention) + u64::from(counters.slots_exhausted),
        total,
        "every delivery was either accepted or refused: {counters:?}"
    );
    assert_eq!(
        received.load(Ordering::SeqCst) + u64::from(counters.dropped_oldest),
        u64::from(counters.carried),
        "every accepted frame was received or evicted, once: {counters:?}"
    );
    // Every slot came back: a full-depth burst is accepted and received, in order.
    for _ in 0..DEPTH {
        bus.deliver_direct(100, BindingRoute(0), frame()).unwrap();
    }
    for _ in 0..DEPTH {
        assert!(subscriber.try_recv().unwrap().is_some());
    }
    assert!(subscriber.try_recv().unwrap().is_none());
}

#[test]
fn concurrent_received_leases_are_each_exclusive() {
    let (pair, _bus) = handoff_pair(32, Qos::DEFAULT.with_depth(16));
    let total = if cfg!(miri) { 16 } else { 4_000 };
    let (publisher, subscriber) = (pair.publisher, pair.subscriber);
    let done = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for id in 0..total {
                while publisher.publish(&door(id, (id % 251) as u8)).is_err() {
                    std::thread::yield_now();
                }
            }
            done.store(true, Ordering::SeqCst);
        });
        scope.spawn(|| loop {
            // Under DropOldest some frames are evicted; each one seen is whole.
            let finished = done.load(Ordering::SeqCst);
            match subscriber.try_recv() {
                Ok(Some(frame)) => assert_eq!(u32::from(frame.open), frame.id % 251),
                _ if finished => break,
                _ => std::thread::yield_now(),
            }
        });
    });
}

#[test]
fn a_new_peer_seen_mid_claim_is_not_a_restart() {
    let table = PeerTable::new_in(&memory(), 2).unwrap();
    table.__claim_first_entry(7);
    let peer = |incarnation| PeerRef {
        peer: 7,
        incarnation,
    };
    assert_eq!(table.admit(peer(1)), Ok(PeerAdmission::Current));
    assert_eq!(table.admit(peer(2)), Ok(PeerAdmission::Restarted));
    assert_eq!(table.admit(peer(1)), Err(ReceiveError::StalePeer));
}

#[test]
fn stale_peer_frames_are_refused_and_counted() {
    let (pair, bus) = handoff_pair(8, Qos::DEFAULT);
    pair.publisher.publish(&door(1, 0)).unwrap();
    let _ = pair.subscriber.try_recv().unwrap().unwrap();
    bus.set_incarnation(1, 5);
    pair.publisher.publish(&door(2, 0)).unwrap();
    let _ = pair.subscriber.try_recv().unwrap().unwrap();
    bus.set_incarnation(1, 3);
    pair.publisher.publish(&door(3, 0)).unwrap();
    assert!(pair.subscriber.try_recv().unwrap().is_none());
    assert_eq!(pair.subscriber.counters().stale_peer, 1);
}

#[test]
fn a_killed_publisher_only_stops_its_own_frames() {
    let (pair, bus) = handoff_pair(8, Qos::DEFAULT);
    bus.kill(1);
    pair.publisher.publish(&door(1, 0)).unwrap();
    assert!(pair.subscriber.try_recv().unwrap().is_none());
    bus.restart(1);
    pair.publisher.publish(&door(2, 0)).unwrap();
    assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, 2);
}

// ----------------------------------------------------------------- native path

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, up_rust::StablePayload, up_rust::StablePayloadInit,
)]
#[repr(C)]
#[stable_payload(type_name = "uprotocol.test.NativeDoor")]
struct NativeDoor {
    id: u32,
    open: u32,
}

fn native_agreement() -> &'static NativeAgreement {
    let profile = NativeProfile::new(
        "test",
        1,
        PayloadEncoding::private_use(0xF777),
        [NativeDoor::native_representation()],
    )
    .unwrap();
    let agreement = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
    native_agreement_table(&agreement)
}

fn native_pair(
    bus: LendingBus,
) -> (
    Publisher<NativeDoor, InPlace>,
    Subscriber<NativeDoor, InPlace>,
) {
    let agreement = native_agreement();
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let publisher = Publisher::<NativeDoor>::on(&a, &entity("door", 1), 0x8001)
        .wire(NativeWire::new(agreement))
        .build()
        .unwrap();
    let subscriber = Subscriber::<NativeDoor>::on(&b, topic())
        .wire(NativeWire::new(agreement))
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    (
        publisher.activate(&tx).unwrap(),
        subscriber.activate(&rx).unwrap(),
    )
}

/// L9. A native route carries its one wire: an `also_wire` beside it is refused at
/// setup, where it can be named, and not by refusing its frames at run time.
#[test]
fn also_wire_on_a_native_role_is_refused_at_setup() {
    struct NativeDoorCopyWire;

    impl Wire<NativeDoor> for NativeDoorCopyWire {
        fn describe(&self) -> WireInfo {
            WireInfo::new(
                "native-door-copy",
                WireIdentity::new("test.native-door-copy", 0x8C01),
                WireIdentity::new("test.native-door-copy-family", 0x8C02),
                WirePayloadIdentity::serialized(PayloadEncoding::private_use(0xF778)),
                RuntimeHeap::None,
                RuntimeHeap::None,
            )
            .max_encoded_len(8)
            .exact()
        }
        fn encoded_size(&self, _: &NativeDoor) -> EncodedSize {
            EncodedSize::Exact(8)
        }
        fn encode(&self, value: &NativeDoor, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
            out.put(&value.id.to_le_bytes())?;
            out.put(&value.open.to_le_bytes())
        }
        fn init_slot<'s>(&self, slot: StartupSlot<'s, NativeDoor>) -> Ready<'s, NativeDoor> {
            slot.write(NativeDoor::default())
        }
        fn decode(
            &self,
            _: PayloadEncoding,
            bytes: &[u8],
            into: &mut NativeDoor,
        ) -> Result<(), WireError> {
            into.id = u32::from_le_bytes(bytes.try_into().map_err(|_| WireError::InvalidBits)?);
            Ok(())
        }
    }

    let a = lending(LendingBus::new(), 1);
    let subscriber = Subscriber::<NativeDoor>::on(&a, topic())
        .also_wire(NativeDoorCopyWire)
        .wire(NativeWire::new(native_agreement()))
        .build();
    assert!(
        matches!(subscriber, Err(StartupError::InvalidConfig(_))),
        "its frames would be refused at run time, so the route is refused at setup: \
         {subscriber:?}"
    );
    let publisher = Publisher::<NativeDoor>::on(&a, &entity("door", 1), 0x8002)
        .also_wire(NativeDoorCopyWire)
        .wire(NativeWire::new(native_agreement()))
        .build();
    assert!(
        matches!(publisher, Err(StartupError::InvalidConfig(_))),
        "the send side refuses it too: {publisher:?}"
    );
}

/// Every outstanding call can have a response waiting at once. The response route
/// used to take the default QoS depth of 8, and a lending transport evicted the rest.
#[test]
fn a_burst_of_responses_up_to_max_in_flight_is_held_for_the_calls() {
    let bus = LendingBus::new();
    let options = LendingOptions {
        slots: 32,
        ..LendingOptions::default()
    };
    let a = lending_with(bus, 1, options);
    let b = lending_with(bus, 2, options);
    let known = entity("door", 1).get_source_uri();
    let method = entity("door", 2).get_resource_uri(METHOD);
    let client = RpcClient::<DoorState, DoorState>::on(&a, &entity("door", 1), method)
        .wire(DoorWire)
        .max_in_flight(16)
        // Under Miri sixteen sends take longer than the default 1 s TTL.
        .ttl(Duration::from_secs(600))
        .build()
        .unwrap();
    let server = RpcServer::<DoorState, DoorState>::on(&b, &entity("door", 2), METHOD)
        .wire(DoorWire)
        .qos(Qos::DEFAULT.with_depth(16))
        .max_held(16)
        .slots(16)
        .clients(std::slice::from_ref(&known))
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let client = client.activate(&tx).unwrap();
    let server = server.activate(&rx).unwrap();
    let mut calls: Vec<_> = (0..16)
        .map(|id| client.invoke(&door(id, 0)).unwrap())
        .collect();
    // The server answers everything before the client looks at anything.
    for _ in 0..16 {
        let request = server.try_recv().unwrap().expect("a request");
        let id = request.id;
        request.respond(&door(id + 100, 0)).unwrap();
    }
    for (id, call) in calls.iter_mut().enumerate() {
        let reply = call.try_response().unwrap().expect("a response");
        assert_eq!(reply.id, id as u32 + 100);
    }
}

/// One wake serves several transports: the shape the Streamer's loop relies on.
#[test]
fn one_signal_serves_several_transports() {
    static WAKE: Signal = Signal::new();
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let b = handoff(bus, 2);
    let c = handoff(bus, 3);
    b.signal_on(&WAKE);
    c.signal_on(&WAKE);
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .build()
        .unwrap();
    let on_b = subscriber_on(&b, Qos::DEFAULT);
    let on_c = subscriber_on(&c, Qos::DEFAULT);
    let tx = a.enter_runtime().unwrap();
    let rx_b = b.enter_runtime().unwrap();
    let rx_c = c.enter_runtime().unwrap();
    let publisher = publisher.activate(&tx).unwrap();
    let on_b = on_b.activate(&rx_b).unwrap();
    let on_c = on_c.activate(&rx_c).unwrap();
    assert_eq!(
        rx_b.wait_token(),
        rx_c.wait_token(),
        "both transports hand out tokens of the one signal"
    );
    let token = WAKE.token();
    publisher.publish(&door(1, 0)).unwrap();
    assert_eq!(
        WAKE.wait(token, Duration::from_secs(5)),
        SignalWait::Notified
    );
    assert_eq!(on_b.try_recv().unwrap().expect("a frame").id, 1);
    assert_eq!(on_c.try_recv().unwrap().expect("a frame").id, 1);
}

type NativeClient = RpcClient<NativeDoor, NativeDoor, InPlace>;
type NativeServer = RpcServer<NativeDoor, NativeDoor, InPlace>;

fn native_rpc(bus: LendingBus, options: LendingOptions) -> (NativeClient, NativeServer) {
    let agreement = native_agreement();
    let a = lending_with(bus, 1, options);
    let b = lending_with(bus, 2, options);
    let known = entity("door", 1).get_source_uri();
    let method = entity("door", 2).get_resource_uri(METHOD);
    let client = RpcClient::<NativeDoor, NativeDoor>::on(&a, &entity("door", 1), method)
        .wire(NativeWire::new(agreement))
        .ttl(TTL)
        .build()
        .unwrap();
    let server = RpcServer::<NativeDoor, NativeDoor>::on(&b, &entity("door", 2), METHOD)
        .wire(NativeWire::new(agreement))
        .clients(std::slice::from_ref(&known))
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    (client.activate(&tx).unwrap(), server.activate(&rx).unwrap())
}

#[test]
fn a_native_call_is_built_in_the_loan_and_answered_in_place() {
    let (client, server) = native_rpc(LendingBus::new(), LendingOptions::default());
    let mut call = client
        .invoke_in_place(|init| init.into_initializer().id(3).open(1).finish())
        .unwrap();
    let request = server.try_recv().unwrap().expect("a request");
    let asked = *request.payload().unwrap();
    assert_eq!(asked, NativeDoor { id: 3, open: 1 });
    request
        .respond_in_place(|init| init.into_initializer().id(asked.id + 1).open(0).finish())
        .unwrap();
    let reply = call.try_response().unwrap().expect("a response");
    assert_eq!(*reply.payload().unwrap(), NativeDoor { id: 4, open: 0 });
}

#[test]
fn a_native_request_stays_in_transport_memory_until_it_is_answered() {
    let bus = LendingBus::new();
    let (client, server) = native_rpc(bus, LendingOptions::default());
    let _call = client
        .invoke_in_place(|init| init.into_initializer().id(7).open(1).finish())
        .unwrap();
    let request = server.try_recv().unwrap().expect("a request");
    let before = bus.frame_drops();
    assert_eq!(request.payload().unwrap().id, 7);
    assert_eq!(
        bus.frame_drops(),
        before,
        "the loan is held for the request"
    );
    request
        .respond_in_place(|init| init.into_initializer().id(8).open(0).finish())
        .unwrap();
    assert_eq!(bus.frame_drops(), before + 1, "and returned with it");
}

#[test]
fn a_native_response_stays_in_transport_memory_until_the_call_is_dropped() {
    let bus = LendingBus::new();
    let options = LendingOptions {
        push: true,
        ..LendingOptions::default()
    };
    let (client, server) = native_rpc(bus, options);
    let mut call = client
        .invoke_in_place(|init| init.into_initializer().id(1).open(0).finish())
        .unwrap();
    let request = server.try_recv().unwrap().expect("a request");
    request
        .respond_in_place(|init| init.into_initializer().id(2).open(1).finish())
        .unwrap();
    let before = bus.frame_drops();
    {
        let reply = call.try_response().unwrap().expect("a response");
        assert_eq!(*reply.payload().unwrap(), NativeDoor { id: 2, open: 1 });
    }
    assert_eq!(
        bus.frame_drops(),
        before,
        "the call keeps the transport's storage"
    );
    assert!(matches!(call.try_response(), Err(CallError::AlreadyTaken)));
    drop(call);
    assert_eq!(
        bus.frame_drops(),
        before + 1,
        "and it goes back with the call"
    );
}

#[test]
fn a_native_server_failure_reaches_the_caller_as_a_status() {
    let (client, server) = native_rpc(LendingBus::new(), LendingOptions::default());
    let mut call = client
        .invoke_in_place(|init| init.into_initializer().id(5).open(0).finish())
        .unwrap();
    let request = server.try_recv().unwrap().expect("a request");
    request.respond_failure(UCode::Unavailable).unwrap();
    assert!(matches!(
        call.try_response(),
        Err(CallError::Rejected(UCode::Unavailable))
    ));
}

#[test]
fn a_native_request_under_another_agreement_is_refused_and_counted() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let other = NativeProfile::new(
        "test",
        1,
        PayloadEncoding::private_use(0xF778),
        [NativeDoor::native_representation()],
    )
    .unwrap();
    let other = NativeProfileAgreement::new(Arc::new(other.clone()), &other).unwrap();
    let known = entity("door", 1).get_source_uri();
    let method = entity("door", 2).get_resource_uri(METHOD);
    let client = RpcClient::<NativeDoor, NativeDoor>::on(&a, &entity("door", 1), method)
        .wire(NativeWire::new(native_agreement_table(&other)))
        .ttl(TTL)
        .build()
        .unwrap();
    let server = RpcServer::<NativeDoor, NativeDoor>::on(&b, &entity("door", 2), METHOD)
        .wire(NativeWire::new(native_agreement()))
        .clients(std::slice::from_ref(&known))
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let client: NativeClient = client.activate(&tx).unwrap();
    let server: NativeServer = server.activate(&rx).unwrap();
    let _call = client
        .invoke_in_place(|init| init.into_initializer().id(1).open(0).finish())
        .unwrap();
    assert!(server.try_recv().unwrap().is_none());
    assert_eq!(server.counters().refused, 1);
}

#[test]
fn a_native_rpc_route_on_a_handoff_transport_is_refused() {
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let method = entity("door", 2).get_resource_uri(METHOD);
    let client = RpcClient::<NativeDoor, NativeDoor>::on(&a, &entity("door", 1), method)
        .wire(NativeWire::new(native_agreement()))
        .build();
    assert!(matches!(
        client,
        Err(StartupError::NativeNeedsLending { .. })
    ));
    let server = RpcServer::<NativeDoor, NativeDoor>::on(&a, &entity("door", 2), METHOD)
        .wire(NativeWire::new(native_agreement()))
        .build();
    assert!(matches!(
        server,
        Err(StartupError::NativeNeedsLending { .. })
    ));
}

#[test]
fn a_type_outside_the_agreement_is_refused_on_an_rpc_route_too() {
    #[derive(Clone, Copy, Debug, Default, up_rust::StablePayload, up_rust::StablePayloadInit)]
    #[repr(C)]
    #[stable_payload(type_name = "uprotocol.test.Unagreed")]
    struct Unagreed {
        value: u32,
    }
    let a = lending(LendingBus::new(), 1);
    let method = entity("door", 2).get_resource_uri(METHOD);
    let client = RpcClient::<Unagreed, NativeDoor>::on(&a, &entity("door", 1), method)
        .wire(NativeWire::new(native_agreement()))
        .build();
    assert!(
        matches!(client, Err(StartupError::InvalidConfig(_))),
        "a request type nobody agreed on"
    );
}

#[test]
fn publish_in_place_on_a_lending_transport_is_received_in_place() {
    let (publisher, subscriber) = native_pair(LendingBus::new());
    publisher
        .publish_in_place(|init| init.into_initializer().id(9).open(1).finish())
        .unwrap();
    let frame = subscriber.try_recv().unwrap().unwrap();
    assert_eq!(*frame.payload().unwrap(), NativeDoor { id: 9, open: 1 });
}

#[test]
fn a_retained_frame_stays_in_transport_memory_until_its_lease_is_dropped() {
    let bus = LendingBus::new();
    let (publisher, subscriber) = native_pair(bus);
    publisher
        .publish_in_place(|init| init.into_initializer().id(5).open(0).finish())
        .unwrap();
    let frame = subscriber.try_recv().unwrap().unwrap();
    assert_eq!(bus.frame_drops(), 0, "the loan is held for the lease");
    assert_eq!(frame.payload().unwrap().id, 5);
    drop(frame);
    assert_eq!(bus.frame_drops(), 1, "and returned when the lease drops");
}

#[test]
fn an_encoded_route_returns_the_frame_as_soon_as_it_is_decoded() {
    let bus = LendingBus::new();
    let pair = activate(lending(bus, 1), lending(bus, 2), Qos::DEFAULT);
    pair.publisher.publish(&door(5, 0)).unwrap();
    let frame = pair.subscriber.try_recv().unwrap().unwrap();
    assert_eq!(frame.id, 5);
    assert_eq!(bus.frame_drops(), 1, "the value lives in the startup slot");
    assert_eq!(frame.payload_bytes(), None, "and the bytes are gone");
}

/// The frame goes back at once, so the token must be kept by the slot itself.
#[test]
fn a_token_on_an_encoded_lending_route_survives_the_frame_being_returned() {
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .max_token_len(32)
        .allow_traceparent(true)
        .build()
        .unwrap();
    let subscriber = Subscriber::<DoorState>::on(&b, topic())
        .wire(DoorWire)
        .max_token_len(32)
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let publisher = publisher.activate(&tx).unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    let trace = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    publisher
        .publish_with(
            &MessageFields {
                token: Some("access-token"),
                traceparent: Some(trace),
                ..MessageFields::default()
            },
            &door(1, 0),
        )
        .unwrap();
    let frame = subscriber.try_recv().unwrap().unwrap();
    assert_eq!(bus.frame_drops(), 1, "the frame has gone back");
    let header = frame.header();
    assert_eq!(header.token(), Some("access-token"));
    assert_eq!(header.traceparent(), Some(trace));
}

#[test]
fn a_native_route_on_a_handoff_transport_is_refused() {
    let agreement = native_agreement();
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    assert!(matches!(
        Publisher::<NativeDoor>::on(&a, &entity("door", 1), 0x8001)
            .wire(NativeWire::new(agreement))
            .build(),
        Err(StartupError::NativeNeedsLending { .. })
    ));
}

#[test]
fn a_native_type_outside_the_agreement_is_refused_at_setup() {
    static NO_ENTRIES: NativeAgreement = NativeAgreement {
        domain: "test",
        version: 1,
        encoding: PayloadEncoding::private_use(0xF778),
        target_triple: "x86_64-unknown-linux-gnu",
        pointer_width: 64,
        little_endian: true,
        entries: &[],
    };
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    assert!(matches!(
        Publisher::<NativeDoor>::on(&a, &entity("door", 1), 0x8001)
            .wire(NativeWire::new(&NO_ENTRIES))
            .build(),
        Err(StartupError::InvalidConfig(_))
    ));
}

/// A layout is agreed for one target. A table generated for another target is
/// refused, by its pointer width and byte order or else by the digest over its triple.
#[test]
fn an_agreement_generated_for_another_target_is_refused_at_setup() {
    let good = native_agreement();
    let other_width: &'static NativeAgreement = Box::leak(Box::new(NativeAgreement {
        pointer_width: if cfg!(target_pointer_width = "64") {
            32
        } else {
            64
        },
        ..*good
    }));
    let other_triple: &'static NativeAgreement = Box::leak(Box::new(NativeAgreement {
        target_triple: "aarch64-foreign-target",
        ..*good
    }));
    for agreement in [other_width, other_triple] {
        let a = lending(LendingBus::new(), 1);
        assert!(matches!(
            Publisher::<NativeDoor>::on(&a, &entity("door", 1), 0x8001)
                .wire(NativeWire::new(agreement))
                .build(),
            Err(StartupError::InvalidConfig(_))
        ));
    }
    // The genuine table still resolves, so the refusals above are about the target.
    let a = lending(LendingBus::new(), 1);
    assert!(Publisher::<NativeDoor>::on(&a, &entity("door", 1), 0x8001)
        .wire(NativeWire::new(good))
        .build()
        .is_ok());
}

/// Tokens are a wire contract with the classic SDK, so they must not drift.
#[test]
fn the_native_token_is_byte_identical_to_the_classic_sdk() {
    use up_rust_core::payload::agreement::descriptor_digest;
    use up_rust_core::payload::sha256::sha256;
    let representation = NativeDoor::native_representation();
    let agreement = native_agreement();
    assert_eq!(
        descriptor_digest::<NativeDoor>(agreement),
        sha256(&representation.canonical_bytes()),
        "the digest core computes is the classic canonical representation's"
    );
    let identity = agreement.resolve::<NativeDoor>().expect("agreed");
    assert_eq!(identity.encoding, PayloadEncoding::private_use(0xF777));
    assert_eq!(
        identity.native_type_token,
        Some(representation.token()),
        "core's token equals the classic token"
    );
}

/// A wire frame on a native lending route carries the type's token in its header.
#[test]
fn a_native_frame_carries_the_negotiated_token() {
    let bus = LendingBus::new();
    let agreement = native_agreement();
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let publisher = Publisher::<NativeDoor>::on(&a, &entity("door", 1), 0x8001)
        .wire(NativeWire::new(agreement))
        .build()
        .unwrap();
    let _keep = Subscriber::<NativeDoor>::on(&b, topic())
        .wire(NativeWire::new(agreement))
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let publisher = publisher.activate(&tx).unwrap();
    publisher
        .publish_in_place(|init| init.into_initializer().id(1).open(2).finish())
        .unwrap();
    let (metadata, payload) = bus.take_raw(2).expect("frame");
    let payload = payload.expect("payload");
    assert_eq!(payload.len(), core::mem::size_of::<NativeDoor>());
    let identity = agreement.resolve::<NativeDoor>().unwrap();
    let harness = EnvelopeHarness::new(
        Envelope::UFrameFields,
        STABLE_CONTAINER_WIRE_ID,
        STABLE_CONTAINER_PAYLOAD_FAMILY_ID,
        identity,
    );
    let (header, _) = harness
        .parse(&metadata, PayloadBytes::one(&payload), &limits())
        .expect("parse");
    assert_eq!(header.native_type_token, identity.native_type_token);
    assert_eq!(header.payload_encoding, Some(identity.encoding));
}

/// An owner that is `Send` but not `Sync`, read only through pointers kept in the
/// slot. It holds its bytes inline, so a view taken before it reached its final
/// address would dangle: this also proves the SDK takes its views after the move.
struct CellOwner {
    metadata: [u8; 128],
    metadata_len: usize,
    payload: AlignedPayload,
    len: usize,
    touches: Cell<u32>,
    metadata_calls: &'static AtomicU32,
    dropped: &'static AtomicU32,
}

#[repr(align(8))]
struct AlignedPayload([u8; 64]);

impl Frame for CellOwner {
    fn encoded_metadata(&self) -> &[u8] {
        self.touches.set(self.touches.get() + 1);
        self.metadata_calls.fetch_add(1, Ordering::SeqCst);
        &self.metadata[..self.metadata_len]
    }

    fn payload(&self) -> Option<LoanedPayload<'_>> {
        Some(LoanedPayload::new(
            &self.payload.0[..self.len],
            PayloadLoanProvenance::OpaqueTransportLoan,
        ))
    }

    fn peer(&self) -> PeerRef {
        PeerRef::UNKNOWN
    }
}

impl Drop for CellOwner {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

struct CellShared {
    port: OnceLock<RuntimePort>,
    inbound: OnceLock<InboundRoute>,
}

struct CellBinding {
    shared: &'static CellShared,
}

struct CellRunning;

struct NoTx;

// SAFETY: this binding never lends: `loan` always refuses, so no view is ever made.
unsafe impl PreparedTxStorage for NoTx {
    fn metadata_uninit_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        &mut []
    }

    fn payload_uninit_mut(&mut self) -> Option<&mut [MaybeUninit<u8>]> {
        None
    }

    fn record_header(&mut self, _header: up_rust::WrittenHeader) -> Result<(), SendError> {
        Ok(())
    }
}

impl BindingSetup for CellBinding {
    type Running = CellRunning;

    fn name(&self) -> &'static str {
        "cell-owner"
    }

    fn capabilities(&self) -> Capabilities {
        simple_capabilities(true)
    }

    fn qos_support(&self, _kind: RouteKind) -> QosSupport {
        simple_qos()
    }

    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        if let Some(inbound) = route.inbound {
            let _ = self.shared.inbound.set(inbound);
        }
        Ok(PreparedRoute::new(BindingRoute(0), RuntimeHeap::None))
    }

    fn into_running(self, port: RuntimePort) -> Result<CellRunning, BindingRefusal> {
        let _ = self.shared.port.set(port);
        Ok(CellRunning)
    }
}

impl LendingRuntime for CellRunning {
    // Explicit single-attempt adapter, justified by this retained implementation.
    fn receive_bounded(&self, route: up_rust::two_mode::BindingRoute, budget: &up_rust::two_mode::ReceiveBudget) -> Option<Self::Rx> {
        if !budget.try_take() { return None; }
        self.receive(route)
    }

    type Tx<'a> = NoTx;
    type Rx = CellOwner;

    fn loan(&self, _route: BindingRoute, _layout: TxFrameLayout) -> Result<NoTx, LoanError> {
        Err(LoanError::InvalidRoute)
    }

    fn publish(
        &self,
        _route: BindingRoute,
        _frame: InitializedTx<NoTx>,
    ) -> Result<SendReport, SendError> {
        Err(SendError::Delivery)
    }

    fn receive(&self, _route: BindingRoute) -> Option<CellOwner> {
        None
    }
}

#[test]
fn an_inline_cell_owner_is_read_only_through_pointers_kept_in_slot_metadata() {
    static CALLS: AtomicU32 = AtomicU32::new(0);
    static DROPPED: AtomicU32 = AtomicU32::new(0);

    // A real native frame, through the real send path.
    let bus = LendingBus::new();
    let (publisher, _subscriber) = native_pair(bus);
    publisher
        .publish_in_place(|init| init.into_initializer().id(77).open(1).finish())
        .unwrap();
    let (metadata, payload) = bus.take_raw(2).expect("a frame");
    let payload = payload.expect("a payload");

    let shared: &'static CellShared = Box::leak(Box::new(CellShared {
        port: OnceLock::new(),
        inbound: OnceLock::new(),
    }));
    let a = TransportSetup::lending_in(&memory(), CellBinding { shared }).unwrap();
    let subscriber = Subscriber::<NativeDoor>::on(&a, topic())
        .wire(NativeWire::new(native_agreement()))
        .build()
        .unwrap();
    let runtime = a.enter_runtime().unwrap();
    let subscriber = subscriber.activate(&runtime).unwrap();

    let mut stored = AlignedPayload([0; 64]);
    stored.0[..payload.len()].copy_from_slice(&payload);
    let mut inline_metadata = [0_u8; 128];
    inline_metadata[..metadata.len()].copy_from_slice(&metadata);
    let owner = CellOwner {
        metadata: inline_metadata,
        metadata_len: metadata.len(),
        payload: stored,
        len: payload.len(),
        touches: Cell::new(0),
        metadata_calls: &CALLS,
        dropped: &DROPPED,
    };
    shared
        .port
        .get()
        .unwrap()
        .deliver_frame(*shared.inbound.get().unwrap(), owner)
        .unwrap();

    let lease = subscriber.try_recv().unwrap().unwrap();
    let calls = CALLS.load(Ordering::SeqCst);
    assert!(
        calls > 0,
        "delivery reads the owner while it holds it alone"
    );
    std::thread::scope(|scope| {
        let lease = &lease;
        scope.spawn(move || assert_eq!(lease.payload().unwrap().id, 77));
        scope.spawn(move || assert_eq!(lease.header().kind, UMessageType::Publish));
    });
    assert_eq!(
        CALLS.load(Ordering::SeqCst),
        calls,
        "a shared lease reads pointers, never the owner"
    );
    assert_eq!(DROPPED.load(Ordering::SeqCst), 0);
    drop(lease);
    assert_eq!(DROPPED.load(Ordering::SeqCst), 1);
}

// ------------------------------------------------------------- large values

#[repr(C)]
#[derive(ZeroSlot)]
struct Huge {
    bytes: [u8; 48 * 1024 * 1024],
    tag: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct HugeWire;

impl Wire<Huge> for HugeWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "huge",
            WireIdentity::new("test.huge", 0x8C01),
            WireIdentity::new("test.huge-family", 0x8C02),
            WirePayloadIdentity::serialized(PayloadEncoding::private_use(0xF201)),
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
        .max_encoded_len(4)
    }

    fn encode(&self, value: &Huge, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        out.put(&value.tag.to_be_bytes())
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, Huge>) -> Ready<'s, Huge> {
        slot.zeroed()
    }

    fn decode(
        &self,
        _encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut Huge,
    ) -> Result<(), WireError> {
        let raw: [u8; 4] = bytes.try_into().map_err(|_| WireError::Malformed("size"))?;
        into.tag = u32::from_be_bytes(raw);
        Ok(())
    }
}

/// Setup builds three 48 MB slots through a pointer, in a debug build, on a
/// test thread's 2 MB stack. A value crossing the stack would overflow it.
#[test]
#[cfg_attr(miri, ignore)]
fn a_48_mb_value_never_crosses_the_stack_during_setup() {
    let bus = LendingBus::new();
    let b = lending(bus, 2);
    let subscriber = Subscriber::<Huge>::on(&b, topic())
        .wire(HugeWire)
        .max_held(1)
        .qos(Qos::DEFAULT.with_depth(1))
        .build()
        .unwrap();
    let a = lending(bus, 1);
    let publisher = Publisher::<Huge>::on(&a, &entity("door", 1), 0x8001)
        .wire(HugeWire)
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let publisher = publisher.activate(&tx).unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    let value: Box<Huge> = {
        let mut boxed = Box::<Huge>::new_uninit();
        // SAFETY: all-zero bytes are a valid `Huge` (`ZeroSlot` derive).
        unsafe {
            boxed.as_mut_ptr().write_bytes(0, 1);
            boxed.assume_init()
        }
    };
    let mut value = value;
    value.tag = 0xABCD;
    publisher.publish(&value).unwrap();
    assert_eq!(subscriber.try_recv().unwrap().unwrap().tag, 0xABCD);
}
