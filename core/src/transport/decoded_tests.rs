// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Ownership regressions. These deliberately invoke safe panicking callbacks;
//! they test containment, not production panic/resource qualification.
extern crate std;

#[cfg(test)]
use crate::test_support::FixtureResult as _;
use super::*;
use crate::{
    communication::{two_mode::{RpcClient, Subscriber}, LocalUriProvider},
    memory::StartupRegion,
    transport::{
        inbound::{PeerTable, RxCommon, TypedWire},
        rpc::CallTable,
        tests::{NullHandoff, TestUri, CLOCK},
        Envelope, LifetimePolicy, Qos, RouteDirection, RouteId, RouteName, RouteView, SetupCheck,
        TransportSetup, TransportView,
    },
    uattributes::{PayloadEncoding, UMessageType},
    wire::{
        EncodedSize, PayloadWriter, Ready, RuntimeHeap, WireIdentity, WireInfo, WirePayloadIdentity,
    },
};
use core::{
    any::TypeId,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};
use std::panic::{catch_unwind, AssertUnwindSafe};

struct Counts {
    made: AtomicUsize,
    dropped: AtomicUsize,
}
impl Counts {
    const fn new() -> Self {
        Self {
            made: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
        }
    }
}
struct Owned {
    counts: &'static Counts,
    panic_drop: bool,
}
impl Drop for Owned {
    fn drop(&mut self) {
        self.counts.dropped.fetch_add(1, SeqCst);
        assert!(!self.panic_drop, "intentional destructor unwind");
    }
}
struct OwnedWire {
    counts: &'static Counts,
    panic_init_at: usize,
    panic_drop: bool,
}
impl Wire<Owned> for OwnedWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "owned-count",
            WireIdentity::new("test.owned", 0),
            WireIdentity::new("test.owned.family", 0),
            WirePayloadIdentity::serialized(PayloadEncoding::RAW),
            RuntimeHeap::Allocates("owned test value"),
            RuntimeHeap::Allocates("owned test value"),
        )
        .max_encoded_len(1)
        .exact()
    }
    fn encoded_size(&self, _: &Owned) -> EncodedSize {
        EncodedSize::Exact(1)
    }
    fn encode(&self, _: &Owned, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        out.put(&[0])
    }
    fn init_slot<'s>(&self, slot: StartupSlot<'s, Owned>) -> Ready<'s, Owned> {
        let ordinal = self.counts.made.load(SeqCst);
        assert_ne!(
            ordinal, self.panic_init_at,
            "intentional initializer unwind"
        );
        self.counts.made.fetch_add(1, SeqCst);
        slot.write(Owned {
            counts: self.counts,
            panic_drop: self.panic_drop,
        })
    }
    fn decode(&self, _: PayloadEncoding, _: &[u8], _: &mut Owned) -> Result<(), WireError> {
        Ok(())
    }
}

struct RefuseReceive;
impl SetupCheck for RefuseReceive {
    fn route(&self, _: &TransportView<'_>, route: &RouteView) -> Result<(), StartupError> {
        if route.report.route.direction == RouteDirection::Receive {
            Err(StartupError::InvalidConfig(
                "late receive admission refusal",
            ))
        } else {
            Ok(())
        }
    }
    fn transport(&self, _: &TransportView<'_>) -> Result<(), StartupError> {
        Ok(())
    }
}
static REFUSE_RECEIVE: RefuseReceive = RefuseReceive;

#[test]
fn owned_seeds_are_cleaned_after_late_subscriber_policy_refusal() {
    static REGION: StartupRegion<{ 1 << 18 }> = StartupRegion::new();
    static COUNTS: Counts = Counts::new();
    let memory = REGION.claim().require_fixture();
    let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff)
        .require_fixture()
        .strict(&REFUSE_RECEIVE);
    let result = Subscriber::<Owned>::on(&setup, TestUri(1).get_resource_uri(0x8001))
        .wire(OwnedWire {
            counts: &COUNTS,
            panic_init_at: usize::MAX,
            panic_drop: false,
        })
        .build();
    assert!(matches!(
        result,
        Err(StartupError::InvalidConfig(
            "late receive admission refusal"
        ))
    ));
    assert!(COUNTS.made.load(SeqCst) > 0);
    assert_eq!(COUNTS.made.load(SeqCst), COUNTS.dropped.load(SeqCst));
}

#[test]
fn owned_seeds_are_cleaned_after_late_rpc_policy_refusal() {
    static REGION: StartupRegion<{ 1 << 18 }> = StartupRegion::new();
    static COUNTS: Counts = Counts::new();
    let memory = REGION.claim().require_fixture();
    let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff)
        .require_fixture()
        .strict(&REFUSE_RECEIVE);
    let result = RpcClient::<Owned, Owned>::on(&setup, &TestUri(1), TestUri(2).get_resource_uri(1))
        .wire(OwnedWire {
            counts: &COUNTS,
            panic_init_at: usize::MAX,
            panic_drop: false,
        })
        .max_in_flight(2)
        .build();
    assert!(matches!(
        result,
        Err(StartupError::InvalidConfig(
            "late receive admission refusal"
        ))
    ));
    assert_eq!(COUNTS.made.load(SeqCst), 2);
    assert_eq!(COUNTS.dropped.load(SeqCst), 2);
}

#[test]
fn owned_seeds_survive_until_response_owner_allocation_finishes() {
    static REGION: StartupRegion<16384> = StartupRegion::new();
    static COUNTS: Counts = Counts::new();
    static WIRE: OwnedWire = OwnedWire {
        counts: &COUNTS,
        panic_init_at: usize::MAX,
        panic_drop: false,
    };
    let memory = REGION.claim().require_fixture();
    let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).require_fixture();
    let source = TestUri(2).get_resource_uri(1);
    let sink = TestUri(1).get_source_uri();
    let name = memory
        .place(RouteName {
            direction: RouteDirection::Receive,
            kind: Some(UMessageType::Response),
            source: source.clone(),
            sink: Some(sink.clone()),
        })
        .require_fixture();
    let common = RxCommon::new(
        name,
        RouteId(0),
        super::super::BindingRoute(0),
        Some(UMessageType::Response),
        source,
        Some(sink),
        &[],
        Envelope::NativeWholeFrame,
        &[],
        crate::frame::header::HeaderLimits { max_token_len: 0 },
        PeerTable::new_in(&memory, 1).require_fixture(),
        Qos::DEFAULT,
        RuntimeHeap::None,
        &[],
        setup.shared(),
    );
    let table = CallTable::<Owned>::prepare(
        &memory,
        2,
        1,
        false,
        &[],
        Some(&WIRE),
        LifetimePolicy::default(),
    )
    .require_fixture();
    assert_eq!(COUNTS.dropped.load(SeqCst), 0);
    // Exhaust only this 16 KiB test region after table preparation, forcing the
    // later ClientResponses placement to fail with all completed seeds owned.
    while memory.bytes(16).is_ok() {}
    while memory.bytes(1).is_ok() {}
    assert!(table.with_responses(&memory, common).is_err());
    assert_eq!(COUNTS.made.load(SeqCst), 2);
    assert_eq!(COUNTS.dropped.load(SeqCst), 2);
}

#[test]
fn completed_seeds_are_cleaned_when_a_later_initializer_unwinds() {
    static REGION: StartupRegion<16384> = StartupRegion::new();
    static COUNTS: Counts = Counts::new();
    static WIRE: OwnedWire = OwnedWire {
        counts: &COUNTS,
        panic_init_at: 2,
        panic_drop: false,
    };
    let memory = REGION.claim().require_fixture();
    assert!(catch_unwind(AssertUnwindSafe(|| initialize(&memory, &WIRE, 4))).is_err());
    assert_eq!(COUNTS.made.load(SeqCst), 2);
    assert_eq!(COUNTS.dropped.load(SeqCst), 2);
}

#[test]
fn owned_clear_and_reinitialize_tracks_exactly_one_live_value() {
    static REGION: StartupRegion<16384> = StartupRegion::new();
    static COUNTS: Counts = Counts::new();
    static WIRE: OwnedWire = OwnedWire {
        counts: &COUNTS,
        panic_init_at: usize::MAX,
        panic_drop: false,
    };
    let memory = REGION.claim().require_fixture();
    let guard = initialize(&memory, &WIRE, 1).require_fixture();
    let slot = guard.slots().first().require_fixture();
    // SAFETY: this unpublished test owns the sole value obligation. No borrower,
    // index pool, route, or callback can access it; every temporary ends before
    // the next transition. Guard retains cleanup until the final drop below.
    unsafe {
        assert!(slot.get().is_some());
        slot.clear();
        assert!(slot.get().is_none());
        slot.clear();
        slot.get_or_init(&WIRE).require_fixture();
        assert!(slot.get().is_some());
    }
    assert_eq!(COUNTS.made.load(SeqCst), 2);
    assert_eq!(COUNTS.dropped.load(SeqCst), 1);
    drop(guard);
    assert_eq!(COUNTS.dropped.load(SeqCst), 2);
}

#[test]
fn destructor_unwind_does_not_authorize_a_second_drop() {
    static REGION: StartupRegion<16384> = StartupRegion::new();
    static COUNTS: Counts = Counts::new();
    static WIRE: OwnedWire = OwnedWire {
        counts: &COUNTS,
        panic_init_at: usize::MAX,
        panic_drop: true,
    };
    let memory = REGION.claim().require_fixture();
    let guard = initialize(&memory, &WIRE, 1).require_fixture();
    let slot = guard.slots().first().require_fixture();
    // SAFETY: the slot is exclusively owned and unpublished, with no live value
    // references. There is no index pool to return it to on unwind; the guard's
    // subsequent cleanup observes the cleared initialized flag.
    assert!(catch_unwind(AssertUnwindSafe(|| unsafe { slot.clear() })).is_err());
    drop(guard);
    assert_eq!(COUNTS.dropped.load(SeqCst), 1);
}

#[test]
fn forgotten_seed_owner_leaks_without_destroying_a_live_value() {
    static REGION: StartupRegion<16384> = StartupRegion::new();
    static COUNTS: Counts = Counts::new();
    static WIRE: OwnedWire = OwnedWire {
        counts: &COUNTS,
        panic_init_at: usize::MAX,
        panic_drop: false,
    };
    let memory = REGION.claim().require_fixture();
    let guard = initialize(&memory, &WIRE, 1).require_fixture();
    core::mem::forget(guard);
    assert_eq!(COUNTS.made.load(SeqCst), 1);
    assert_eq!(COUNTS.dropped.load(SeqCst), 0);
}

#[test]
fn serialized_owned_decode_is_admitted_but_native_or_mixed_decode_still_refuses() {
    static COUNTS: Counts = Counts::new();
    static WIRE: OwnedWire = OwnedWire {
        counts: &COUNTS,
        panic_init_at: usize::MAX,
        panic_drop: false,
    };
    let serialized = WIRE.describe();
    let mut native = serialized;
    native.identity.native_type_token = Some(crate::frame::native::NativeTypeToken::from_u32(7));
    let entry = |info| TypedWire {
        type_id: TypeId::of::<Owned>(),
        wire: &WIRE as &dyn Wire<Owned>,
        info,
    };
    let name = || RouteName {
        direction: RouteDirection::Receive,
        kind: Some(UMessageType::Response),
        source: TestUri(2).get_resource_uri(1),
        sink: Some(TestUri(1).get_source_uri()),
    };
    assert!(super::super::rpc::check_decoders(&[entry(serialized)], name).is_ok());
    assert!(matches!(
        super::super::rpc::check_decoders(&[entry(native)], name),
        Err(StartupError::RouteNeedsHeapAtRuntime { .. })
    ));
    assert!(matches!(
        super::super::rpc::check_decoders(&[entry(serialized), entry(native)], name),
        Err(StartupError::RouteNeedsHeapAtRuntime { .. })
    ));
}

#[repr(align(128))]
struct AlignedZst;
struct ZstWire;
impl Wire<AlignedZst> for ZstWire {
    fn describe(&self) -> WireInfo {
        crate::transport::tests::TestWire { fail_encode: false }.describe()
    }
    fn encode(&self, _: &AlignedZst, _: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        Ok(())
    }
    fn init_slot<'s>(&self, slot: StartupSlot<'s, AlignedZst>) -> Ready<'s, AlignedZst> {
        slot.write(AlignedZst)
    }
    fn decode(&self, _: PayloadEncoding, _: &[u8], _: &mut AlignedZst) -> Result<(), WireError> {
        Ok(())
    }
}

#[test]
fn empty_and_aligned_zero_sized_slots_preserve_validity() {
    static REGION: StartupRegion<16384> = StartupRegion::new();
    static WIRE: ZstWire = ZstWire;
    let memory = REGION.claim().require_fixture();
    let empty = initialize(&memory, &WIRE, 0).require_fixture();
    assert!(empty.slots().is_empty());
    let seeds = initialize(&memory, &WIRE, 3).require_fixture();
    for slot in seeds.slots() {
        // SAFETY: each logical ZST slot is unpublished and exclusively owned;
        // shared borrows end before clear, and clear leaves no-drop T initialized.
        unsafe {
            let value = slot.get().require_fixture();
            assert_eq!((core::ptr::from_ref(value) as usize) % 128, 0);
            slot.clear();
            assert!(slot.get().is_some());
        }
    }
}
