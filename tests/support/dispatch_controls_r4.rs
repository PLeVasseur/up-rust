//! R4: executable layout positives.
//!
//! The adapter positives instantiate the same `shapes` compositions whose
//! oversize/over-aligned variants are the standalone compile-fail controls in
//! `tests/ui/dispatch_controls_*.rs`: static implementation futures that keep
//! large or 16-byte-aligned state live across a real suspension, reached
//! through public dyn `send_owned` via `InlineOwnedTransport`, and via
//! `InlineOwnedCore` under both selected-wire adapters. Container positives
//! cover zero capacity (ZST, including a ZST at alignment 16) and an exactly
//! full capacity at exactly the supported alignment. Allocation windows cover
//! send creation, every poll and drop on the calling thread.

use super::common::{self, count, Allocations};
use super::shapes::{
    self, Align16, AlignedExact, Core, Fill, LiveState, Zst, ZstAlign16, CORE_FILL, DIRECT_FILL,
    EXACT_BYTES,
};
use std::{
    future::Future,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, Waker},
};
use up_rust::{
    inline_dispatch::{
        CoreSendFuture, InlineFuture, SendFuture, CORE_FUTURE_BYTES, FUTURE_ALIGNMENT,
        SEND_FUTURE_BYTES,
    },
    serialized_dispatch::{StaticOwnedCore, StaticOwnedTransport},
    wire::UFrameFieldsMetadataCodec,
    wire_transport::PreparedOwnedFrame,
    UOwnedTransport,
};

/// Size and alignment of a concrete (pre-erasure) implementation future.
fn layout_of<T>(value: &T) -> (usize, usize) {
    (std::mem::size_of_val(value), std::mem::align_of_val(value))
}

/// Direct composition: records the concrete future's layout, then drives a
/// public dyn send through its suspension with no allocation at all.
fn direct_positive<S: LiveState>() -> (usize, usize) {
    let transport = shapes::direct::<S>();
    let concrete = StaticOwnedTransport::send_validated_owned(transport.inner(), shapes::frame());
    let layout = layout_of(&concrete);
    drop(concrete);
    let public: &dyn UOwnedTransport = &transport;
    let input = shapes::frame();
    let (result, allocations) = count(|| shapes::poll_twice(public, input));
    result.expect("admitted live state survives its suspension");
    assert_eq!(
        allocations,
        Allocations::default(),
        "no heap fallback for an admitted layout"
    );
    layout
}

/// Concrete core future layout for live state `S`.
fn core_layout<S: LiveState>() -> (usize, usize) {
    let core = Core::<S>::default();
    let prepared = PreparedOwnedFrame::from_context(
        shapes::frame(),
        &UFrameFieldsMetadataCodec,
        shapes::context().metadata_context(),
    )
    .expect("prepared control frame");
    let concrete = StaticOwnedCore::send_prepared_owned(&core, prepared);
    let layout = layout_of(&concrete);
    drop(concrete);
    layout
}

/// Wire composition: the whole dyn send allocates exactly what the real
/// metadata codec allocates for the same frame, measured separately.
fn wire_positive(public: &dyn UOwnedTransport) {
    let control = shapes::frame();
    let context = shapes::context().metadata_context();
    let ((), codec_cost) = count(|| {
        drop(
            PreparedOwnedFrame::from_context(control, &UFrameFieldsMetadataCodec, context)
                .expect("codec control"),
        );
    });
    let input = shapes::frame();
    let (result, total) = count(|| shapes::poll_twice(public, input));
    result.expect("admitted core live state survives its suspension");
    assert!(
        codec_cost.allocations > 0,
        "real metadata encoding is counted"
    );
    assert_eq!(
        total, codec_cost,
        "nested inline dispatch adds nothing to metadata encoding"
    );
}

#[test]
fn direct_large_live_state_is_admitted_without_allocation() {
    let (size, align) = direct_positive::<Fill<{ DIRECT_FILL }>>();
    assert!(
        size >= DIRECT_FILL,
        "the concrete future stores its live state"
    );
    eprintln!("R4 direct Fill<{DIRECT_FILL}> concrete future: size={size}, align={align}; public capacity={SEND_FUTURE_BYTES}");
}

#[test]
fn direct_maximum_alignment_live_state_is_admitted_without_allocation() {
    let (size, align) = direct_positive::<Align16>();
    assert_eq!(
        align, FUTURE_ALIGNMENT,
        "the concrete future is 16-byte aligned"
    );
    eprintln!("R4 direct Align16 concrete future: size={size}, align={align}");
}

#[test]
fn preset_core_large_live_state_is_admitted() {
    let (size, align) = core_layout::<Fill<{ CORE_FILL }>>();
    assert!(
        size >= CORE_FILL,
        "the concrete core future stores its live state"
    );
    wire_positive(&shapes::preset::<Fill<{ CORE_FILL }>>());
    eprintln!("R4 core Fill<{CORE_FILL}> concrete future: size={size}, align={align}; core capacity={CORE_FUTURE_BYTES}");
}

#[test]
fn preset_core_maximum_alignment_live_state_is_admitted() {
    let (size, align) = core_layout::<Align16>();
    assert_eq!(
        align, FUTURE_ALIGNMENT,
        "the concrete core future is 16-byte aligned"
    );
    wire_positive(&shapes::preset::<Align16>());
    eprintln!("R4 core Align16 concrete future: size={size}, align={align}");
}

#[test]
fn configured_core_large_live_state_is_admitted() {
    wire_positive(
        &shapes::configured::<Fill<{ CORE_FILL }>>().expect("provisioned-context adapter"),
    );
}

#[test]
fn configured_core_maximum_alignment_live_state_is_admitted() {
    wire_positive(&shapes::configured::<Align16>().expect("provisioned-context adapter"));
}

/// Zero-capacity lifecycle: unpolled drop, `Pending` → `Ready` → drop,
/// refused repoll after `Ready`, and cancellation after `Pending`. Each hidden
/// destructor runs exactly once; counted windows allocate nothing.
fn zero_capacity_lifecycle<F>(make: fn() -> F, polls: &AtomicUsize, drops: &AtomicUsize)
where
    F: Future<Output = ()> + Send + 'static,
{
    let observed = || (polls.load(Ordering::SeqCst), drops.load(Ordering::SeqCst));
    assert_eq!(observed(), (0, 0));
    let ((), allocations) = count(|| {
        drop(InlineFuture::<(), 0>::new(make()));
        let mut owner = std::pin::pin!(InlineFuture::<(), 0>::new(make()));
        let mut context = Context::from_waker(Waker::noop());
        assert!(owner.as_mut().poll(&mut context).is_pending());
        assert!(owner.as_mut().poll(&mut context).is_ready());
    });
    assert_eq!(allocations, Allocations::default());
    assert_eq!(observed(), (2, 2));
    {
        let mut owner = std::pin::pin!(InlineFuture::<(), 0>::new(make()));
        let mut context = Context::from_waker(Waker::noop());
        assert!(owner.as_mut().poll(&mut context).is_pending());
        assert_eq!(
            common::owner_debug(owner.as_ref()),
            "InlineFuture { capacity: 0, pollable: true, .. }"
        );
        assert!(owner.as_mut().poll(&mut context).is_ready());
        common::assert_repoll_refused(owner.as_mut(), &mut context);
        assert_eq!(observed(), (4, 2));
    }
    assert_eq!(observed(), (4, 3));
    let ((), allocations) = count(|| {
        let mut owner = std::pin::pin!(InlineFuture::<(), 0>::new(make()));
        assert!(owner
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
    });
    assert_eq!(allocations, Allocations::default());
    assert_eq!(observed(), (5, 4));
}

#[test]
fn zero_capacity_zst_polls_and_drops() {
    assert_eq!(std::mem::size_of::<Zst>(), 0);
    assert_eq!(
        std::mem::align_of::<InlineFuture<'static, (), 0>>(),
        FUTURE_ALIGNMENT
    );
    zero_capacity_lifecycle(|| Zst, &shapes::ZST_POLLS, &shapes::ZST_DROPS);
}

#[test]
fn zero_capacity_zst_at_maximum_alignment_polls_and_drops() {
    assert_eq!(std::mem::size_of::<ZstAlign16>(), 0);
    assert_eq!(std::mem::align_of::<ZstAlign16>(), FUTURE_ALIGNMENT);
    zero_capacity_lifecycle(|| ZstAlign16, &shapes::ZST16_POLLS, &shapes::ZST16_DROPS);
}

#[test]
fn exact_capacity_at_maximum_alignment_polls_and_drops() {
    assert_eq!(EXACT_BYTES, std::mem::size_of::<AlignedExact>());
    assert_eq!(std::mem::align_of::<AlignedExact>(), FUTURE_ALIGNMENT);
    let drops = || shapes::EXACT_DROPS.load(Ordering::SeqCst);
    let (polls, allocations) = count(|| {
        let mut owner = std::pin::pin!(InlineFuture::<bool, { EXACT_BYTES }>::new(
            AlignedExact::default()
        ));
        let mut context = Context::from_waker(Waker::noop());
        let first = owner.as_mut().poll(&mut context);
        let second = owner.as_mut().poll(&mut context);
        (first, second)
    });
    assert!(
        matches!(polls, (Poll::Pending, Poll::Ready(true))),
        "pinned address stayed 16-byte aligned and contents survived"
    );
    assert_eq!(allocations, Allocations::default());
    assert_eq!(drops(), 1);
    drop(InlineFuture::<bool, { EXACT_BYTES }>::new(
        AlignedExact::default(),
    ));
    assert_eq!(drops(), 2, "an unpolled exact-capacity owner drops once");
}

#[test]
fn erased_owner_layouts_use_the_supported_alignment() {
    assert_eq!(
        std::mem::align_of::<SendFuture<'static>>(),
        FUTURE_ALIGNMENT
    );
    assert_eq!(
        std::mem::align_of::<CoreSendFuture<'static>>(),
        FUTURE_ALIGNMENT
    );
    let public = std::mem::size_of::<SendFuture<'static>>();
    let core = std::mem::size_of::<CoreSendFuture<'static>>();
    let empty = std::mem::size_of::<InlineFuture<'static, (), 0>>();
    assert!(public >= SEND_FUTURE_BYTES);
    assert!(core >= CORE_FUTURE_BYTES);
    assert!(empty < core);
    eprintln!("R4 target owner bytes: public={public}, core={core}, zero-capacity={empty}, alignment={FUTURE_ALIGNMENT}");
}
