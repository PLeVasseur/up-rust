//! Heap-backed native fault witness with exact per-attachment faults.
//!
//! WITNESS LABEL — FIXTURE, NOT PHYSICAL PROOF: `FaultCore` wraps the SDK's
//! heap-backed `PoolCore` (test-util) and is an explicit test-only
//! `NativeLoanCore`, like `native_fixture::FixtureCore`. It exercises SDK
//! ownership, partial-setup and cleanup mechanics only. It is not installed
//! middleware and confers no physical zero-copy, loan-lifetime or allocation
//! credit.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::sync::Notify;
use up_rust::{
    FrameSink, InitializedTx, LoanError, NativeLoanCore, PoolCore, PreparedTransportCore,
    RouteLimits, SendError, SetupError, TxFrameLayout, UUri,
};

/// Injected registration refusal (attachment is never created).
pub(crate) const ATTACH_REFUSED: SetupError = SetupError::Worker;
/// Injected exact detach refusal; the SDK owner must retain it for retry.
pub(crate) const DETACH_REFUSED: SetupError =
    SetupError::InvalidConfig("injected exact detach refusal");

pub(crate) struct FaultAttachment {
    inner: <PoolCore as PreparedTransportCore>::Subscription,
    id: usize,
}

/// Attachment ids start at 1 and count every attach attempt on this core.
pub(crate) struct FaultCore {
    pub(crate) pool: Arc<PoolCore>,
    next: AtomicUsize,
    failed_attach: AtomicUsize,
    held_attach: AtomicUsize,
    pub(crate) attach_entered: Notify,
    failed_detach: AtomicUsize,
    detach_log: Mutex<Vec<usize>>,
    live: AtomicUsize,
}

impl FaultCore {
    pub(crate) fn new(pool: Arc<PoolCore>) -> Self {
        Self {
            pool,
            next: AtomicUsize::new(0),
            failed_attach: AtomicUsize::new(0),
            held_attach: AtomicUsize::new(0),
            attach_entered: Notify::new(),
            failed_detach: AtomicUsize::new(0),
            detach_log: Mutex::new(Vec::new()),
            live: AtomicUsize::new(0),
        }
    }
    pub(crate) fn fail_attach(&self, id: usize) {
        self.failed_attach.store(id, Ordering::SeqCst);
    }
    pub(crate) fn hold_attach(&self, id: usize) {
        self.held_attach.store(id, Ordering::SeqCst);
    }
    pub(crate) fn fail_detach(&self, id: usize) {
        self.failed_detach.store(id, Ordering::SeqCst);
    }
    pub(crate) fn clear_detach_failure(&self) {
        self.failed_detach.store(0, Ordering::SeqCst);
    }
    pub(crate) fn attach_calls(&self) -> usize {
        self.next.load(Ordering::SeqCst)
    }
    /// Registrations currently present in the shared pool for this program.
    pub(crate) fn live(&self) -> usize {
        self.live.load(Ordering::SeqCst)
    }
    /// Exact attachment ids in detach-attempt order.
    pub(crate) fn detach_log(&self) -> Vec<usize> {
        self.detach_log.lock().unwrap().clone()
    }
}

impl NativeLoanCore for FaultCore {}

impl PreparedTransportCore for FaultCore {
    type Route = <PoolCore as PreparedTransportCore>::Route;
    type Tx<'a>
        = <PoolCore as PreparedTransportCore>::Tx<'a>
    where
        Self: 'a;
    type Rx = <PoolCore as PreparedTransportCore>::Rx;
    type Subscription = FaultAttachment;

    fn prepare_route(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        limits: RouteLimits,
    ) -> Result<Self::Route, SetupError> {
        self.pool.prepare_route(source, sink, limits)
    }

    fn loan<'a>(
        &'a self,
        route: &'a Self::Route,
        layout: TxFrameLayout,
    ) -> Result<Self::Tx<'a>, LoanError> {
        self.pool.loan(route, layout)
    }

    fn publish<'a>(
        &'a self,
        route: &'a Self::Route,
        frame: InitializedTx<Self::Tx<'a>>,
    ) -> Result<(), SendError> {
        self.pool.publish(route, frame)
    }

    async fn attach(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        inbox: Arc<dyn FrameSink<Self::Rx>>,
    ) -> Result<Self::Subscription, SetupError> {
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        if self.failed_attach.load(Ordering::SeqCst) == id {
            return Err(ATTACH_REFUSED);
        }
        if self.held_attach.load(Ordering::SeqCst) == id {
            self.attach_entered.notify_one();
            std::future::pending::<()>().await;
        }
        let inner = self.pool.attach(source, sink, inbox).await?;
        self.live.fetch_add(1, Ordering::SeqCst);
        Ok(FaultAttachment { inner, id })
    }

    fn detach(&self, attachment: &mut Self::Subscription) -> Result<(), SetupError> {
        self.detach_log.lock().unwrap().push(attachment.id);
        if self.failed_detach.load(Ordering::SeqCst) == attachment.id {
            return Err(DETACH_REFUSED);
        }
        self.pool.detach(&mut attachment.inner)?;
        self.live.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
}
