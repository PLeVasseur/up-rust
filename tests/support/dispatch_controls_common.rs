//! Shared fixtures for the serialized-dispatch R2/R3/R4 runtime controls.
//!
//! Every destructor here records observations instead of asserting, so an
//! unexpected failure cannot turn into a panic during unwinding. Only the
//! explicitly armed `Plan::PanicInGuardDrop` destructor panics, and never while
//! the thread is already panicking.

use super::shapes::{fixed_id, protobuf_context, topic};
use bytes::Bytes;
use std::{
    any::Any,
    future::Future,
    panic::{catch_unwind, AssertUnwindSafe},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use up_rust::{
    heap_guard::{observe, HeapGuard, Scope},
    serialized_dispatch::{
        InlineOwnedCore, InlineOwnedTransport, StaticOwnedCore, StaticOwnedTransport,
    },
    wire::{
        ProtobufWire, UFrameFieldsMetadataCodec, UWireMetadataCodec, UWireMetadataCodecFor,
        UWireMetadataContext, UWireMetadataError,
    },
    wire_transport::{PreparedOwnedFrame, UWireTransport},
    OwnedWireTransport, PayloadEncoding, UFrameMetadata, UOwnedFrame, UOwnedTransport, UStatus,
    WireIdentity, UFRAME_FIELDS_METADATA_LAYOUT_ID,
};

// ---- calling-thread heap observation (same approach as tests/serialized_dispatch.rs) ----

/// Heap operations observed on the calling thread inside `count`.
pub use up_rust::heap_guard::HeapCounts as Allocations;

#[global_allocator]
static ALLOCATOR: HeapGuard = HeapGuard::new(std::alloc::System);

/// Runs `operation` with calling-thread heap observation enabled.
pub fn count<Output>(operation: impl FnOnce() -> Output) -> (Output, Allocations) {
    let (output, report) = observe(Scope::Enrolled, operation);
    if !report.is_clean() {
        eprintln!("heap observed: {report}");
    }
    (output, report.counts())
}

// ---- panic observation ----

/// Exact refusal implemented by `InlineFuture::poll` (src/inline_dispatch.rs).
pub const REPOLL_REFUSAL: &str = "inline future polled after completion or panic";
/// Panic raised by `Resume` on its second poll under `Plan::PanicOnResume`.
pub const RESUME_PANIC: &str = "dispatch-controls resume panic";
/// Panic raised by `BorrowGuard::drop` under `Plan::PanicInGuardDrop`.
pub const GUARD_DROP_PANIC: &str = "dispatch-controls guard destructor panic";

/// Text of a panic payload (`&'static str` or `String`).
pub fn panic_text(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&'static str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        String::from("<non-string panic payload>")
    }
}

/// `None` when `result` completed; otherwise the caught panic's text.
pub fn panic_message<T>(result: std::thread::Result<T>) -> Option<String> {
    result.err().map(|payload| panic_text(&*payload))
}

/// Shared inspection of a pinned erased owner through its public `Debug`.
pub fn owner_debug<T: std::fmt::Debug>(owner: Pin<&T>) -> String {
    format!("{:?}", owner.get_ref())
}

/// Two further polls of a retained owner must each panic with exactly
/// `REPOLL_REFUSAL`; callers check that the hidden future was not re-entered.
pub fn assert_repoll_refused<F: Future>(mut future: Pin<&mut F>, context: &mut Context<'_>) {
    for _attempt in 0..2 {
        let refused = catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(context)));
        assert_eq!(panic_message(refused).as_deref(), Some(REPOLL_REFUSAL));
    }
}

// ---- observation counters ----

/// Shared counters for one control. Codec, implementation and destructor
/// observations are recorded independently of the erased future's storage.
#[derive(Default)]
pub struct Counts {
    encodes: AtomicUsize,
    decodes: AtomicUsize,
    entries: AtomicUsize,
    polls: AtomicUsize,
    completions: AtomicUsize,
    guard_drops: AtomicUsize,
    state_drops: AtomicUsize,
    payload_drops: AtomicUsize,
    guard_seen: AtomicUsize,
    state_seen: AtomicUsize,
}

/// Exact point-in-time copy of `Counts`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// `encode_frame_metadata` calls (setup probe plus lazy per-send encoding).
    pub encodes: usize,
    /// `decode_frame_metadata` calls (provisioned-context setup probe only).
    pub decodes: usize,
    /// Calls into the static implementation's send method (core entry).
    pub entries: usize,
    /// Polls of the implementation's inner suspension future.
    pub polls: usize,
    /// Implementation bodies that resumed and returned `Ok`.
    pub completions: usize,
    /// `BorrowGuard` destructor runs.
    pub guard_drops: usize,
    /// `LocalState` destructor runs.
    pub state_drops: usize,
    /// Destructions of the frame payload owner (exactly one per frame).
    pub payload_drops: usize,
    /// Value the guard destructor read through its retained `&mut` borrow.
    pub guard_seen: usize,
    /// Value the async-local state held when its own destructor ran.
    pub state_seen: usize,
}

impl Counts {
    /// Loads every counter.
    pub fn snapshot(&self) -> Snapshot {
        let load = |value: &AtomicUsize| value.load(Ordering::SeqCst);
        Snapshot {
            encodes: load(&self.encodes),
            decodes: load(&self.decodes),
            entries: load(&self.entries),
            polls: load(&self.polls),
            completions: load(&self.completions),
            guard_drops: load(&self.guard_drops),
            state_drops: load(&self.state_drops),
            payload_drops: load(&self.payload_drops),
            guard_seen: load(&self.guard_seen),
            state_seen: load(&self.state_seen),
        }
    }
}

fn bump(value: &AtomicUsize) {
    value.fetch_add(1, Ordering::SeqCst);
}

// ---- frames whose destruction is observable ----

struct PayloadOwner(Arc<Counts>);
impl AsRef<[u8]> for PayloadOwner {
    fn as_ref(&self) -> &[u8] {
        b"dispatch-controls payload"
    }
}
impl Drop for PayloadOwner {
    fn drop(&mut self) {
        bump(&self.0.payload_drops);
    }
}

/// Validated publish frame (fixed id, RAW payload) whose payload owner records
/// its single destruction in `counts`.
pub fn frame_with_probe(counts: &Arc<Counts>) -> UOwnedFrame {
    let metadata = UFrameMetadata::publish(topic())
        .with_id(fixed_id())
        .with_payload_encoding(PayloadEncoding::RAW)
        .build()
        .expect("valid fixture metadata");
    UOwnedFrame::with_payload(
        metadata,
        Bytes::from_owner(PayloadOwner(Arc::clone(counts))),
    )
    .expect("payload presence matches its encoding")
}

// ---- counting metadata codec around the real field-block codec ----

/// Delegates to `UFrameFieldsMetadataCodec`, counting whole-frame encodes and
/// decodes. It really round trips, so the provisioned-context adapter's setup
/// probe succeeds; that probe's one encode and one decode are setup costs.
#[derive(Clone)]
pub struct CountingCodec(Arc<Counts>);
impl UWireMetadataCodec for CountingCodec {
    const METADATA_LAYOUT_ID: WireIdentity = UFRAME_FIELDS_METADATA_LAYOUT_ID;
    fn encode_frame_metadata(
        &self,
        context: UWireMetadataContext,
        metadata: &UFrameMetadata,
    ) -> Result<Vec<u8>, UWireMetadataError> {
        bump(&self.0.encodes);
        UFrameFieldsMetadataCodec.encode_frame_metadata(context, metadata)
    }
    fn encoded_frame_metadata_len(
        &self,
        context: UWireMetadataContext,
        metadata: &UFrameMetadata,
    ) -> Result<usize, UWireMetadataError> {
        UFrameFieldsMetadataCodec.encoded_frame_metadata_len(context, metadata)
    }
    fn encode_frame_metadata_into(
        &self,
        context: UWireMetadataContext,
        metadata: &UFrameMetadata,
        out: &mut [u8],
    ) -> Result<usize, UWireMetadataError> {
        UFrameFieldsMetadataCodec.encode_frame_metadata_into(context, metadata, out)
    }
    fn decode_frame_metadata(
        &self,
        context: UWireMetadataContext,
        src: &[u8],
    ) -> Result<UFrameMetadata, UWireMetadataError> {
        bump(&self.0.decodes);
        UFrameFieldsMetadataCodec.decode_frame_metadata(context, src)
    }
}
impl UWireMetadataCodecFor<ProtobufWire> for CountingCodec {}

// ---- self-borrowing implementation future (R2, R3) ----

/// Initial value of the async-local checksum.
pub const SEED: usize = 40;
/// Written through the retained borrow before suspension.
pub const BEFORE_SUSPEND: usize = 1;
/// Written through the retained borrow after resumption.
pub const AFTER_RESUME: usize = 1;
/// Written through the retained borrow by the guard's destructor.
pub const GUARD_DROP_MARK: usize = 100;
/// Guard observation when destroyed while suspended (cancellation or unwind).
pub const SUSPENDED_GUARD_VALUE: usize = SEED + BEFORE_SUSPEND;
/// State observation when destroyed while suspended.
pub const SUSPENDED_STATE_VALUE: usize = SUSPENDED_GUARD_VALUE + GUARD_DROP_MARK;
/// Guard observation after completion.
pub const COMPLETED_GUARD_VALUE: usize = SEED + BEFORE_SUSPEND + AFTER_RESUME;
/// State observation after completion.
pub const COMPLETED_STATE_VALUE: usize = COMPLETED_GUARD_VALUE + GUARD_DROP_MARK;

/// Behavior of the self-borrowing implementation after its first suspension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    /// Resume and return `Ok(())`.
    Complete,
    /// Panic inside the inner future's second poll, with the borrow live.
    PanicOnResume,
    /// Complete normally, but the guard destructor panics (when not unwinding).
    PanicInGuardDrop,
}

/// Async-local state that owns the frame and the borrowed checksum.
struct LocalState<'a, F> {
    counts: &'a Counts,
    checksum: usize,
    _frame: F,
}
impl<F> Drop for LocalState<'_, F> {
    fn drop(&mut self) {
        self.counts
            .state_seen
            .store(self.checksum, Ordering::SeqCst);
        bump(&self.counts.state_drops);
    }
}

/// Holds a real `&mut` borrow into `LocalState` across suspension and
/// dereferences it (read and write) in its destructor.
struct BorrowGuard<'a> {
    counts: &'a Counts,
    plan: Plan,
    slot: &'a mut usize,
}
impl Drop for BorrowGuard<'_> {
    fn drop(&mut self) {
        let seen = *self.slot;
        self.counts.guard_seen.store(seen, Ordering::SeqCst);
        *self.slot = seen + GUARD_DROP_MARK;
        bump(&self.counts.guard_drops);
        if self.plan == Plan::PanicInGuardDrop && !std::thread::panicking() {
            panic!("{GUARD_DROP_PANIC}");
        }
    }
}

/// Inner suspension future holding a second `&mut` borrow into the enclosing
/// future's own state; it writes through that borrow on every poll.
struct Resume<'a> {
    counts: &'a Counts,
    plan: Plan,
    resumes: &'a mut usize,
}
impl Future for Resume<'_> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        bump(&this.counts.polls);
        *this.resumes += 1;
        if *this.resumes == 1 {
            context.waker().wake_by_ref();
            Poll::Pending
        } else if this.plan == Plan::PanicOnResume {
            panic!("{RESUME_PANIC}");
        } else {
            Poll::Ready(())
        }
    }
}

async fn self_borrowing_send<F: Send>(
    counts: &Counts,
    plan: Plan,
    frame: F,
) -> Result<(), UStatus> {
    let mut state = LocalState {
        counts,
        checksum: SEED,
        _frame: frame,
    };
    let guard = BorrowGuard {
        counts,
        plan,
        slot: &mut state.checksum,
    };
    *guard.slot += BEFORE_SUSPEND;
    let mut resumes = 0_usize;
    Resume {
        counts,
        plan,
        resumes: &mut resumes,
    }
    .await;
    *guard.slot += AFTER_RESUME;
    bump(&counts.completions);
    Ok(())
}

/// Static semantic and core implementation whose send future borrows itself.
pub struct SelfBorrowing {
    counts: Arc<Counts>,
    plan: Plan,
}
impl StaticOwnedTransport for SelfBorrowing {
    fn send_validated_owned(
        &self,
        frame: UOwnedFrame,
    ) -> impl Future<Output = Result<(), UStatus>> + Send + '_ {
        bump(&self.counts.entries);
        self_borrowing_send(&self.counts, self.plan, frame)
    }
}
impl StaticOwnedCore for SelfBorrowing {
    fn send_prepared_owned(
        &self,
        frame: PreparedOwnedFrame,
    ) -> impl Future<Output = Result<(), UStatus>> + Send + '_ {
        bump(&self.counts.entries);
        self_borrowing_send(&self.counts, self.plan, frame)
    }
}

/// Public dyn adapter composition under test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    /// `InlineOwnedTransport` over the static semantic implementation.
    Direct,
    /// Preset `UWireTransport` over `InlineOwnedCore` (wire_transport/owned.rs).
    Preset,
    /// `OwnedWireTransport` over `InlineOwnedCore` (wire_transport/configured_owned.rs).
    Configured,
}
impl Path {
    /// Exact codec cost of constructing the adapter.
    pub fn construction(self) -> Snapshot {
        match self {
            Path::Direct | Path::Preset => Snapshot::default(),
            Path::Configured => Snapshot {
                encodes: 1,
                decodes: 1,
                ..Snapshot::default()
            },
        }
    }
    /// Metadata encodings performed by the first poll of one send.
    pub fn encodes_per_send(self) -> usize {
        match self {
            Path::Direct => 0,
            Path::Preset | Path::Configured => 1,
        }
    }
}

/// Builds the composition on the stack and lends it as the public dyn boundary.
pub fn with_public<R>(
    path: Path,
    counts: &Arc<Counts>,
    plan: Plan,
    body: impl FnOnce(&dyn UOwnedTransport) -> R,
) -> R {
    let implementation = SelfBorrowing {
        counts: Arc::clone(counts),
        plan,
    };
    let codec = CountingCodec(Arc::clone(counts));
    match path {
        Path::Direct => body(&InlineOwnedTransport::new(implementation)),
        Path::Preset => body(&UWireTransport::new(
            InlineOwnedCore::new(implementation),
            ProtobufWire,
            codec,
        )),
        Path::Configured => {
            let transport = OwnedWireTransport::new(
                InlineOwnedCore::new(implementation),
                codec,
                protobuf_context::<CountingCodec>(),
            )
            .expect("explicitly provisioned protobuf context round trips");
            body(&transport)
        }
    }
}
