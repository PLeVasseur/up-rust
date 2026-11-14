//! Shared R4 layout shapes for serialized-dispatch controls.
//!
//! Consumers: the runtime target `serialized_dispatch_controls` (module
//! `shapes`) and every standalone `tests/ui/dispatch_controls_*.rs` file. Each
//! consumer instantiates a subset, so unused items are expected per consumer.
//! A negative UI file differs from `dispatch_controls_positive.rs` only by the
//! generic argument named in its `main`; imports, adapters and drivers are this
//! one module. Live state is used after its suspension point, so it is stored in
//! the generated future and cannot be optimized out of its layout.
#![allow(dead_code)]

use std::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, Waker},
};
use up_rust::{
    inline_dispatch::InlineFuture,
    serialized_dispatch::{
        InlineOwnedCore, InlineOwnedTransport, StaticOwnedCore, StaticOwnedTransport,
    },
    wire::{ProtobufWire, UFrameFieldsMetadataCodec, UWireMetadataCodec, FORMAT_VERSION},
    wire_transport::{PreparedOwnedFrame, UWireTransport},
    OwnedWireContext, OwnedWireDeclaration, OwnedWireTransport, UCode, UFrameMetadata, UOwnedFrame,
    UOwnedTransport, UStatus, UUri, PROTOBUF_PAYLOAD_FAMILY_ID, PROTOBUF_WIRE_ID,
    UFRAME_FIELDS_METADATA_LAYOUT_ID, UUID,
};

/// Admitted live bytes for a direct implementation future. The public inline
/// capacity is 4096; the dispatch block also carries the frame twice (upvar
/// slot and implementation future), so this keeps a wide margin for every
/// feature-dependent `UOwnedFrame` layout.
pub const DIRECT_FILL: usize = 2048;
/// Admitted live bytes for a core future. The core inline capacity is 2048 and
/// `PreparedOwnedFrame` is carried twice; this leaves a wide margin even when
/// `zero-copy-transport` stores URI authorities inline.
pub const CORE_FILL: usize = 512;
const FILL_BYTE: u8 = 0xA5;

/// State that must remain live and unchanged across one suspension point.
pub trait LiveState: Send + 'static {
    /// Creates the state before suspension.
    fn make() -> Self;
    /// Checks contents (and, where relevant, address alignment) after resumption.
    fn intact(&self) -> bool;
}

/// `N` live bytes with alignment 1.
pub struct Fill<const N: usize>([u8; N]);
impl<const N: usize> LiveState for Fill<N> {
    fn make() -> Self {
        Self([FILL_BYTE; N])
    }
    fn intact(&self) -> bool {
        self.0.iter().all(|byte| *byte == FILL_BYTE)
    }
}

/// Live state at exactly the maximum supported alignment (`FUTURE_ALIGNMENT`).
#[repr(align(16))]
pub struct Align16([u8; 16]);
impl LiveState for Align16 {
    fn make() -> Self {
        Self([FILL_BYTE; 16])
    }
    fn intact(&self) -> bool {
        std::ptr::from_ref(self).is_aligned() && self.0.iter().all(|byte| *byte == FILL_BYTE)
    }
}

/// Live state above the maximum supported alignment; negative controls only.
#[repr(align(32))]
pub struct Align32([u8; 32]);
impl LiveState for Align32 {
    fn make() -> Self {
        Self([FILL_BYTE; 32])
    }
    fn intact(&self) -> bool {
        std::ptr::from_ref(self).is_aligned() && self.0.iter().all(|byte| *byte == FILL_BYTE)
    }
}

/// Returns `Pending` once (after waking), then `Ready`.
#[derive(Default)]
pub struct YieldOnce {
    yielded: bool,
}
impl Future for YieldOnce {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            context.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Holds `S` across one real suspension point, then checks it. An `async move`
/// block (not an `async fn`) so the captured frame is stored once, not also as
/// a moved parameter local; this keeps the admitted margins wide. The layout is
/// what this fixture measures, so `clippy::manual_async_fn` is allowed here.
#[allow(clippy::manual_async_fn)]
fn hold<S: LiveState, F: Send>(frame: F) -> impl Future<Output = Result<(), UStatus>> + Send {
    async move {
        let state = S::make();
        YieldOnce::default().await;
        let intact = state.intact();
        drop(state);
        drop(frame);
        if intact {
            Ok(())
        } else {
            Err(UStatus::fail_with_code(
                UCode::Internal,
                "live state changed across its suspension point",
            ))
        }
    }
}

/// Static semantic implementation reached through `InlineOwnedTransport`.
pub struct Direct<S>(PhantomData<fn() -> S>);
impl<S> Default for Direct<S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}
impl<S: LiveState> StaticOwnedTransport for Direct<S> {
    fn send_validated_owned(
        &self,
        frame: UOwnedFrame,
    ) -> impl Future<Output = Result<(), UStatus>> + Send + '_ {
        hold::<S, UOwnedFrame>(frame)
    }
}

/// Static encoded core reached through `InlineOwnedCore` and either wire adapter.
pub struct Core<S>(PhantomData<fn() -> S>);
impl<S> Default for Core<S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}
impl<S: LiveState> StaticOwnedCore for Core<S> {
    fn send_prepared_owned(
        &self,
        frame: PreparedOwnedFrame,
    ) -> impl Future<Output = Result<(), UStatus>> + Send + '_ {
        hold::<S, PreparedOwnedFrame>(frame)
    }
}

/// Fixed valid uProtocol v7 id, so fixture frames are deterministic and do not
/// read the realtime clock (only `OwnedWireTransport::new`'s own probe does).
pub fn fixed_id() -> UUID {
    UUID::from_u64_pair(0x018D_548E_A8E0_7000, 0x8000_0000_0000_0001)
        .expect("fixture id is a valid uProtocol v7 UUID")
}

/// Publish topic shared by all dispatch controls.
pub fn topic() -> UUri {
    UUri::try_from_parts("dispatch-controls", 0x0001_0001, 1, 0x8001).expect("valid topic URI")
}

/// Payload-free validated publish frame with a fixed id.
pub fn frame() -> UOwnedFrame {
    let metadata = UFrameMetadata::publish(topic())
        .with_id(fixed_id())
        .build()
        .expect("valid publish metadata");
    UOwnedFrame::new(metadata, None).expect("payload-free frame is valid")
}

/// Explicitly provisioned protobuf/field-block context. Passing the local
/// declaration as the peer is fixture-only agreement, not deployment proof.
pub fn protobuf_context<C: UWireMetadataCodec>() -> OwnedWireContext {
    let declaration = OwnedWireDeclaration {
        wire: PROTOBUF_WIRE_ID,
        payload_family: PROTOBUF_PAYLOAD_FAMILY_ID,
        metadata_layout: UFRAME_FIELDS_METADATA_LAYOUT_ID,
        format_version: FORMAT_VERSION,
    };
    OwnedWireContext::agree::<C>(declaration, &declaration)
        .expect("fixture declaration agrees with itself")
}

/// Context for the real field-block metadata codec.
pub fn context() -> OwnedWireContext {
    protobuf_context::<UFrameFieldsMetadataCodec>()
}

/// Public semantic adapter (`SEND_FUTURE_BYTES` admission).
pub fn direct<S: LiveState>() -> InlineOwnedTransport<Direct<S>> {
    InlineOwnedTransport::new(Direct::default())
}

/// Preset selected wire over an inline core (`CORE_FUTURE_BYTES` admission).
pub fn preset<S: LiveState>(
) -> UWireTransport<InlineOwnedCore<Core<S>>, ProtobufWire, UFrameFieldsMetadataCodec> {
    UWireTransport::new(
        InlineOwnedCore::new(Core::default()),
        ProtobufWire,
        UFrameFieldsMetadataCodec,
    )
}

/// Provisioned-context selected wire over an inline core. Construction encodes
/// and decodes one probe frame whose id comes from `UUID::build()`.
///
/// # Errors
/// Returns the adapter's context/codec admission refusal.
pub fn configured<S: LiveState>(
) -> Result<OwnedWireTransport<InlineOwnedCore<Core<S>>, UFrameFieldsMetadataCodec>, UStatus> {
    OwnedWireTransport::new(
        InlineOwnedCore::new(Core::default()),
        UFrameFieldsMetadataCodec,
        context(),
    )
}

/// Drives one public dyn send through exactly one suspension to completion.
///
/// # Errors
/// Returns the send result, or an internal status if it did not suspend once.
pub fn poll_twice(public: &dyn UOwnedTransport, frame: UOwnedFrame) -> Result<(), UStatus> {
    let mut send = std::pin::pin!(public.send_owned(frame));
    let mut context = Context::from_waker(Waker::noop());
    if send.as_mut().poll(&mut context).is_ready() {
        return Err(UStatus::fail_with_code(
            UCode::Internal,
            "send completed before its suspension point",
        ));
    }
    match send.as_mut().poll(&mut context) {
        Poll::Ready(result) => result,
        Poll::Pending => Err(UStatus::fail_with_code(
            UCode::Internal,
            "send did not resume after one wake",
        )),
    }
}

/// UI driver: direct adapter composition with live state `S`.
///
/// # Errors
/// Propagates the send result.
pub fn run_direct<S: LiveState>() -> Result<(), UStatus> {
    poll_twice(&direct::<S>(), frame())
}

/// UI driver: preset selected-wire composition with core live state `S`.
///
/// # Errors
/// Propagates the send result.
pub fn run_preset<S: LiveState>() -> Result<(), UStatus> {
    poll_twice(&preset::<S>(), frame())
}

/// UI driver: provisioned-context composition with core live state `S`.
///
/// # Errors
/// Propagates adapter construction or the send result.
pub fn run_configured<S: LiveState>() -> Result<(), UStatus> {
    poll_twice(&configured::<S>()?, frame())
}

/// Polls a raw `InlineFuture<_, BYTES>` to completion (at most two polls).
///
/// # Panics
/// Panics if the future has not completed after two polls.
pub fn run_container<const BYTES: usize, F: Future + Send>(future: F) -> F::Output {
    let mut owner = std::pin::pin!(InlineFuture::<F::Output, BYTES>::new(future));
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..2 {
        if let Poll::Ready(output) = owner.as_mut().poll(&mut context) {
            return output;
        }
    }
    panic!("container future did not complete within two polls");
}

fn pending_then_ready(polls: &AtomicUsize, context: &mut Context<'_>) -> Poll<()> {
    if polls.fetch_add(1, Ordering::SeqCst) & 1 == 0 {
        context.waker().wake_by_ref();
        Poll::Pending
    } else {
        Poll::Ready(())
    }
}

/// Polls observed for `Zst`; each runtime test owns a distinct static.
pub static ZST_POLLS: AtomicUsize = AtomicUsize::new(0);
/// Destructor runs observed for `Zst`.
pub static ZST_DROPS: AtomicUsize = AtomicUsize::new(0);
/// Polls observed for `ZstAlign16`.
pub static ZST16_POLLS: AtomicUsize = AtomicUsize::new(0);
/// Destructor runs observed for `ZstAlign16`.
pub static ZST16_DROPS: AtomicUsize = AtomicUsize::new(0);
/// Destructor runs observed for `AlignedExact`.
pub static EXACT_DROPS: AtomicUsize = AtomicUsize::new(0);

/// Zero-sized, alignment-1 future: even-numbered polls of its type are
/// `Pending`, odd-numbered polls are `Ready`.
pub struct Zst;
impl Future for Zst {
    type Output = ();
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        pending_then_ready(&ZST_POLLS, context)
    }
}
impl Drop for Zst {
    fn drop(&mut self) {
        ZST_DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

/// Zero-sized future at exactly the maximum supported alignment.
#[repr(align(16))]
pub struct ZstAlign16;
impl Future for ZstAlign16 {
    type Output = ();
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        pending_then_ready(&ZST16_POLLS, context)
    }
}
impl Drop for ZstAlign16 {
    fn drop(&mut self) {
        ZST16_DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

/// Zero-sized future above the maximum supported alignment; negative only.
#[repr(align(32))]
pub struct ZstAlign32;
impl Future for ZstAlign32 {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        Poll::Ready(())
    }
}

/// One-byte future: definitely larger than a zero-capacity container.
pub struct OneByte(pub u8);
impl Future for OneByte {
    type Output = u8;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<u8> {
        Poll::Ready(self.0)
    }
}

/// Exactly fills `InlineFuture<_, EXACT_BYTES>` at alignment 16; checks its
/// pinned address and contents on every poll.
#[repr(C, align(16))]
pub struct AlignedExact {
    bytes: [u8; 48],
    polls: u8,
}
/// Exact capacity for `AlignedExact` on the compiling target (64 bytes).
pub const EXACT_BYTES: usize = std::mem::size_of::<AlignedExact>();
impl Default for AlignedExact {
    fn default() -> Self {
        Self {
            bytes: [FILL_BYTE; 48],
            polls: 0,
        }
    }
}
impl Future for AlignedExact {
    type Output = bool;
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<bool> {
        let this = self.get_mut();
        let sound = std::ptr::from_ref(&*this).is_aligned()
            && this.bytes.iter().all(|byte| *byte == FILL_BYTE);
        this.polls += 1;
        if this.polls == 1 && sound {
            context.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(sound)
        }
    }
}
impl Drop for AlignedExact {
    fn drop(&mut self) {
        EXACT_DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
use up_rust::prelude::*;
