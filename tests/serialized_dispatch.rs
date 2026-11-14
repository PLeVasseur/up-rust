//! Dispatch allocation controls, not middleware or zero-copy acceptance.
use std::{
    cell::Cell,
    future::Future,
    marker::PhantomPinned,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Wake, Waker},
};
use up_rust::prelude::*;
use up_rust::{
    heap_guard::{observe, HeapCounts, HeapGuard, Scope},
    inline_dispatch::InlineFuture,
    serialized_dispatch::{
        InlineOwnedCore, InlineOwnedTransport, OwnedMailbox, StaticOwnedCore, StaticOwnedTransport,
    },
    wire::{
        ProtobufWire, UWireMetadataCodec, UWireMetadataCodecFor, UWireMetadataContext,
        UWireMetadataError,
    },
    wire_transport::{PreparedOwnedFrame, UOwnedTransportCore, UWireTransport},
    *,
};

type Allocations = HeapCounts;

#[global_allocator]
static ALLOCATOR: HeapGuard = HeapGuard::new(std::alloc::System);

fn count<Output>(operation: impl FnOnce() -> Output) -> (Output, Allocations) {
    let (output, report) = observe(Scope::Enrolled, operation);
    if !report.is_clean() {
        eprintln!("heap observed: {report}");
    }
    (output, report.counts())
}

fn ready<Output>(future: impl Future<Output = Output>) -> Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("fixture expected immediate completion"),
    }
}

fn frame() -> UOwnedFrame {
    UOwnedFrame::new(
        UFrameMetadata::publish(UUri::try_from_parts("dispatch", 0x10001, 1, 0x8001).unwrap())
            .build()
            .unwrap(),
        None,
    )
    .unwrap()
}

#[derive(Default)]
struct Sink {
    calls: AtomicUsize,
}
impl StaticOwnedTransport for Sink {
    fn admit_payload_contract(
        &self,
        identity: PayloadIdentity,
        _: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        if matches!(identity, PayloadIdentity::Fixed(_)) {
            Ok(())
        } else {
            Err(UWireError::invalid_payload("serialized fixture only"))
        }
    }
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        drop(frame);
        Ok(())
    }
}
#[async_trait::async_trait]
impl UOwnedTransportImpl for Sink {
    fn admit_payload_contract(
        &self,
        identity: PayloadIdentity,
        family: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        StaticOwnedTransport::admit_payload_contract(self, identity, family)
    }
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        StaticOwnedTransport::send_validated_owned(self, frame).await
    }
}
impl StaticOwnedCore for Sink {
    async fn send_prepared_owned(&self, frame: PreparedOwnedFrame) -> Result<(), UStatus> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        drop(frame);
        Ok(())
    }
}
#[async_trait::async_trait]
impl UOwnedTransportCore for Sink {
    async fn send_prepared_owned(&self, frame: PreparedOwnedFrame) -> Result<(), UStatus> {
        StaticOwnedCore::send_prepared_owned(self, frame).await
    }
}

#[derive(Clone)]
struct EmptyWire;
impl UWireMetadataCodec for EmptyWire {
    const METADATA_LAYOUT_ID: WireIdentity = UFRAME_FIELDS_METADATA_LAYOUT_ID;
    fn encoded_frame_metadata_len(
        &self,
        _: UWireMetadataContext,
        _: &UFrameMetadata,
    ) -> Result<usize, UWireMetadataError> {
        Ok(0)
    }
    fn encode_frame_metadata_into(
        &self,
        _: UWireMetadataContext,
        _: &UFrameMetadata,
        _: &mut [u8],
    ) -> Result<usize, UWireMetadataError> {
        Ok(0)
    }
    fn decode_frame_metadata(
        &self,
        _: UWireMetadataContext,
        _: &[u8],
    ) -> Result<UFrameMetadata, UWireMetadataError> {
        panic!("send-only allocation fixture is not a wire format")
    }
}
impl UWireMetadataCodecFor<ProtobufWire> for EmptyWire {}

#[derive(Clone)]
struct RefusingWire(Arc<AtomicUsize>);
impl UWireMetadataCodec for RefusingWire {
    const METADATA_LAYOUT_ID: WireIdentity = UFRAME_FIELDS_METADATA_LAYOUT_ID;
    fn encoded_frame_metadata_len(
        &self,
        _: UWireMetadataContext,
        _: &UFrameMetadata,
    ) -> Result<usize, UWireMetadataError> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Err(UWireMetadataError::FrameMetadata(
            "intentional encoding refusal".into(),
        ))
    }
    fn encode_frame_metadata_into(
        &self,
        _: UWireMetadataContext,
        _: &UFrameMetadata,
        _: &mut [u8],
    ) -> Result<usize, UWireMetadataError> {
        panic!("length refusal must precede encoding")
    }
    fn decode_frame_metadata(
        &self,
        _: UWireMetadataContext,
        _: &[u8],
    ) -> Result<UFrameMetadata, UWireMetadataError> {
        panic!("send-only refusal fixture")
    }
}
impl UWireMetadataCodecFor<ProtobufWire> for RefusingWire {}

#[test]
fn wire_encoding_stays_lazy_and_refuses_before_core_effects() {
    let encodes = Arc::new(AtomicUsize::new(0));
    let transport = UWireTransport::new(
        InlineOwnedCore::new(Sink::default()),
        ProtobufWire,
        RefusingWire(encodes.clone()),
    );
    let public: &dyn UOwnedTransport = &transport;
    drop(public.send_owned(frame()));
    assert_eq!(encodes.load(Ordering::Relaxed), 0);
    assert!(ready(public.send_owned(frame())).is_err());
    assert_eq!(encodes.load(Ordering::Relaxed), 1);
    assert_eq!(transport.core().inner().calls.load(Ordering::Relaxed), 0);
    assert!(public
        .admit_payload_contract(PayloadIdentity::Fixed(PayloadEncoding::PROTOBUF), None)
        .is_err());
}

#[test]
fn public_dyn_creation_poll_drop_has_no_dispatch_allocation() {
    let transport: Arc<dyn UOwnedTransport> = Arc::new(InlineOwnedTransport::new(Sink::default()));
    let frame = frame();
    let (result, counts) = count(|| ready(transport.send_owned(frame)));
    result.unwrap();
    assert_eq!(counts, Allocations::default());
}

#[test]
fn selected_wire_dyn_creation_poll_drop_has_no_dispatch_allocation() {
    let transport: Arc<dyn UOwnedTransport> = Arc::new(UWireTransport::new(
        InlineOwnedCore::new(Sink::default()),
        ProtobufWire,
        EmptyWire,
    ));
    let frame = frame();
    let (result, counts) = count(|| ready(transport.send_owned(frame)));
    result.unwrap();
    assert_eq!(counts, Allocations::default());
}

/// The stub above isolates dispatch. This is the same send with the real
/// field-block metadata codec, the path a selected-wire owned transport runs.
#[test]
fn selected_wire_dyn_send_with_the_real_metadata_codec_is_heap_free() {
    let transport: Arc<dyn UOwnedTransport> = Arc::new(UWireTransport::new(
        InlineOwnedCore::new(Sink::default()),
        ProtobufWire,
        UFrameFieldsMetadataCodec,
    ));
    let frame = frame();
    let (result, counts) = count(|| ready(transport.send_owned(frame)));
    result.unwrap();
    assert_eq!(
        counts,
        Allocations::default(),
        "the real metadata codec on the owned path"
    );
}

#[test]
fn explicit_legacy_paths_still_allocate_one_send_future() {
    let direct: Arc<dyn UOwnedTransport> = Arc::new(Sink::default());
    let wire: Arc<dyn UOwnedTransport> = Arc::new(UWireTransport::new(
        Sink::default(),
        ProtobufWire,
        EmptyWire,
    ));
    for transport in [direct, wire] {
        let frame = frame();
        let (result, counts) = count(|| ready(transport.send_owned(frame)));
        result.unwrap();
        assert_eq!(counts.allocations, 1);
        assert_eq!(counts.reallocations, 0);
    }
}

#[test]
fn in_tree_memory_send_dispatch_is_unboxed_without_listeners() {
    let transport: Arc<dyn UOwnedTransport> = Arc::new(InMemoryOwnedTransport::default());
    let frame = frame();
    let (result, counts) = count(|| ready(transport.send_owned(frame)));
    result.unwrap();
    assert_eq!(counts, Allocations::default());
}

#[test]
fn useful_bounded_mailbox_delivers_without_dispatch_or_queue_allocation() {
    let mailbox = Arc::new(InlineOwnedTransport::new(OwnedMailbox::new(1).unwrap()));
    let transport: Arc<dyn UOwnedTransport> = mailbox.clone();
    let sent = frame();
    let expected = sent.clone();
    let (result, counts) = count(|| ready(transport.send_owned(sent)));
    result.unwrap();
    assert_eq!(counts, Allocations::default());
    let full = ready(transport.send_owned(frame())).unwrap_err();
    assert_eq!(full.code(), UCode::ResourceExhausted);
    assert_eq!(mailbox.inner().try_take().unwrap(), Some(expected));
    assert!(mailbox.inner().try_take().unwrap().is_none());
    drop(transport.send_owned(frame()));
    assert!(
        mailbox.inner().try_take().unwrap().is_none(),
        "unpolled send must have no enqueue effects"
    );
    let next = frame();
    let (_, counts) = count(|| {
        ready(transport.send_owned(next)).unwrap();
        drop(mailbox.inner().try_take().unwrap());
    });
    assert_eq!(counts, Allocations::default());
    assert_eq!(
        OwnedMailbox::new(0).unwrap_err().code(),
        UCode::InvalidArgument
    );
}

struct Probe<'a> {
    drops: &'a AtomicUsize,
    polls: Cell<usize>,
    address: Cell<usize>,
    pending: bool,
    _pinned: PhantomPinned,
}
impl Future for Probe<'_> {
    type Output = Result<(), UStatus>;
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_ref().get_ref();
        let address = this as *const Self as usize;
        let previous = this.address.replace(address);
        assert!(previous == 0 || previous == address);
        let previous_poll = this.polls.replace(this.polls.get() + 1);
        if this.pending && previous_poll == 0 {
            context.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }
}
impl Drop for Probe<'_> {
    fn drop(&mut self) {
        let pinned_address = self.address.get();
        assert!(pinned_address == 0 || pinned_address == self as *const Self as usize);
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}
fn probe(drops: &AtomicUsize, pending: bool) -> Probe<'_> {
    Probe {
        drops,
        pending,
        polls: Cell::new(0),
        address: Cell::new(0),
        _pinned: PhantomPinned,
    }
}
struct WakeCount(AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn borrowed_not_sync_future_pending_wake_ready_and_exact_drop() {
    let drops = AtomicUsize::new(0);
    let wake = Arc::new(WakeCount(AtomicUsize::new(0)));
    let waker = Waker::from(wake.clone());
    let (_, counts) = count(|| {
        let erased = InlineFuture::<_, 128>::new(probe(&drops, true));
        fn require_send<T: Send>(_: &T) {}
        require_send(&erased);
        let mut pinned = std::pin::pin!(erased);
        let mut context = Context::from_waker(&waker);
        assert!(pinned.as_mut().poll(&mut context).is_pending());
        assert_eq!(wake.0.load(Ordering::Relaxed), 1);
        assert!(matches!(
            pinned.as_mut().poll(&mut context),
            Poll::Ready(Ok(()))
        ));
        assert_eq!(drops.load(Ordering::Relaxed), 0);
    });
    assert_eq!(counts, Allocations::default());
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[test]
fn self_borrow_across_pending_survives_inline_poll_and_drop_projection() {
    let drops = AtomicUsize::new(0);
    let (_, counts) = count(|| {
        let concrete = async {
            let mut local = [3_u8; 16];
            let borrowed = local.first_mut().unwrap();
            probe(&drops, true).await.unwrap();
            *borrowed += 4;
            assert_eq!(*borrowed, 7);
        };
        let mut future = std::pin::pin!(InlineFuture::<(), 256>::new(concrete));
        let mut context = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut context).is_pending());
        std::hint::black_box(future.as_ref().get_ref());
        assert!(future.as_mut().poll(&mut context).is_ready());
    });
    assert_eq!(counts, Allocations::default());
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[test]
fn cancellation_before_poll_and_after_pending_drops_exactly_once() {
    let drops = AtomicUsize::new(0);
    let (_, before) = count(|| drop(InlineFuture::<_, 128>::new(probe(&drops, true))));
    assert_eq!(before, Allocations::default());
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    let (_, after) = count(|| {
        let mut future = std::pin::pin!(InlineFuture::<_, 128>::new(probe(&drops, true)));
        assert!(future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
    });
    assert_eq!(after, Allocations::default());
    assert_eq!(drops.load(Ordering::Relaxed), 2);
    let (_, immediate) = count(|| ready(InlineFuture::<_, 128>::new(probe(&drops, false))));
    assert_eq!(immediate, Allocations::default());
    assert_eq!(drops.load(Ordering::Relaxed), 3);
}

struct PendingTransport {
    drops: AtomicUsize,
    entries: AtomicUsize,
}
impl StaticOwnedTransport for PendingTransport {
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.entries.fetch_add(1, Ordering::Relaxed);
        let result = probe(&self.drops, true).await;
        drop(frame);
        result
    }
}
impl StaticOwnedCore for PendingTransport {
    async fn send_prepared_owned(&self, frame: PreparedOwnedFrame) -> Result<(), UStatus> {
        self.entries.fetch_add(1, Ordering::Relaxed);
        let result = probe(&self.drops, true).await;
        drop(frame);
        result
    }
}
#[test]
fn pending_public_and_wire_sends_cancel_and_complete_without_dispatch_allocation() {
    let direct = InlineOwnedTransport::new(PendingTransport {
        drops: AtomicUsize::new(0),
        entries: AtomicUsize::new(0),
    });
    let wire = UWireTransport::new(
        InlineOwnedCore::new(PendingTransport {
            drops: AtomicUsize::new(0),
            entries: AtomicUsize::new(0),
        }),
        ProtobufWire,
        EmptyWire,
    );
    for transport in [
        &direct as &dyn UOwnedTransport,
        &wire as &dyn UOwnedTransport,
    ] {
        let before = frame();
        let (_, counts) = count(|| drop(transport.send_owned(before)));
        assert_eq!(counts, Allocations::default());
        let cancelled = frame();
        let (_, counts) = count(|| {
            let mut future = std::pin::pin!(transport.send_owned(cancelled));
            assert!(future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending());
        });
        assert_eq!(counts, Allocations::default());
        let completed = frame();
        let (_, counts) = count(|| {
            let mut future = std::pin::pin!(transport.send_owned(completed));
            let mut context = Context::from_waker(Waker::noop());
            assert!(future.as_mut().poll(&mut context).is_pending());
            assert!(matches!(
                future.as_mut().poll(&mut context),
                Poll::Ready(Ok(()))
            ));
        });
        assert_eq!(counts, Allocations::default());
    }
    assert_eq!(direct.inner().entries.load(Ordering::Relaxed), 2);
    assert_eq!(direct.inner().drops.load(Ordering::Relaxed), 2);
    assert_eq!(wire.core().inner().entries.load(Ordering::Relaxed), 2);
    assert_eq!(wire.core().inner().drops.load(Ordering::Relaxed), 2);
}

#[test]
fn moving_unpolled_erasure_across_threads_preserves_send_and_borrow() {
    let drops = AtomicUsize::new(0);
    let erased = InlineFuture::<_, 128>::new(probe(&drops, false));
    std::thread::scope(|scope| {
        scope.spawn(move || ready(erased).unwrap()).join().unwrap();
    });
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[test]
fn simultaneous_sends_have_independent_storage_and_cancellation() {
    let transport = InlineOwnedTransport::new(PendingTransport {
        drops: AtomicUsize::new(0),
        entries: AtomicUsize::new(0),
    });
    let first = frame();
    let second = frame();
    let (_, counts) = count(|| {
        let mut second = std::pin::pin!(transport.send_owned(second));
        {
            let mut first = std::pin::pin!(transport.send_owned(first));
            let mut context = Context::from_waker(Waker::noop());
            assert!(first.as_mut().poll(&mut context).is_pending());
            assert!(second.as_mut().poll(&mut context).is_pending());
        }
        assert_eq!(transport.inner().drops.load(Ordering::Relaxed), 1);
        assert!(matches!(
            second
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Ok(()))
        ));
    });
    assert_eq!(counts, Allocations::default());
    assert_eq!(transport.inner().drops.load(Ordering::Relaxed), 2);
}

#[test]
fn unwinding_poll_drops_erased_capture_once() {
    struct DropCount<'a>(&'a AtomicUsize);
    impl Drop for DropCount<'_> {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    let drops = AtomicUsize::new(0);
    let captured = DropCount(&drops);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ready(InlineFuture::<(), 128>::new(async move {
            let _guard = captured;
            panic!("intentional poll unwind");
        }));
    }));
    assert!(result.is_err());
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[path = "support/serialized_dispatch_application.rs"]
mod application;

#[test]
fn record_inline_owner_layout() {
    use up_rust::inline_dispatch::{
        CoreSendFuture, SendFuture, CORE_FUTURE_BYTES, FUTURE_ALIGNMENT, SEND_FUTURE_BYTES,
    };
    let public = std::mem::size_of::<SendFuture<'static>>();
    let core = std::mem::size_of::<CoreSendFuture<'static>>();
    assert!(public >= SEND_FUTURE_BYTES);
    assert!(core >= CORE_FUTURE_BYTES);
    assert!(core < public);
    assert_eq!(
        std::mem::align_of::<SendFuture<'static>>(),
        FUTURE_ALIGNMENT
    );
    eprintln!("target inline owner bytes: public={public}, core={core}, alignment={FUTURE_ALIGNMENT}; caller frames are larger than pointer-sized boxed dispatch");
}

// Mixed send/receive implementers use plain `async fn` for every method. No macro
// is involved, so nothing can box the public send.
struct Mixed;
impl StaticOwnedTransport for Mixed {
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        drop(frame);
        Ok(())
    }
    async fn receive_validated_owned(
        &self,
        _source: &UUri,
        _sink: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        Err(UStatus::fail_with_code(
            UCode::Unimplemented,
            "mixed receive control",
        ))
    }
}
impl StaticOwnedCore for Mixed {
    async fn send_prepared_owned(&self, frame: PreparedOwnedFrame) -> Result<(), UStatus> {
        drop(frame);
        Ok(())
    }
    async fn receive_encoded_owned(
        &self,
        _source: &UUri,
        _sink: Option<&UUri>,
    ) -> Result<up_rust::wire_transport::EncodedOwnedFrame, UStatus> {
        Err(UStatus::fail_with_code(
            UCode::Unimplemented,
            "mixed core receive control",
        ))
    }
}
#[test]
fn mixed_send_receive_implementers_preserve_unboxed_public_sends() {
    let direct = InlineOwnedTransport::new(Mixed);
    let wire = UWireTransport::new(InlineOwnedCore::new(Mixed), ProtobufWire, EmptyWire);
    for public in [
        &direct as &dyn UOwnedTransport,
        &wire as &dyn UOwnedTransport,
    ] {
        let input = frame();
        let (result, allocations) = count(|| ready(public.send_owned(input)));
        result.unwrap();
        assert_eq!(allocations, Allocations::default());
    }
    let source = UUri::try_from_parts("dispatch", 0x10001, 1, 0x8001).unwrap();
    assert!(ready(StaticOwnedTransport::receive_validated_owned(
        &Mixed, &source, None
    ))
    .is_err());
    assert!(ready(StaticOwnedCore::receive_encoded_owned(
        &Mixed, &source, None
    ))
    .is_err());
}
