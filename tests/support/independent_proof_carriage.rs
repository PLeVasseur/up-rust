//! One program's scripted view of a shared in-memory medium.
//!
//! Exact per-attachment attach/detach faults, send refusal and admission
//! refusal for setup and cleanup controls. The wrapped medium is shared by
//! independently deployed programs, like a network; this wrapper is one
//! program's local carriage. Test-only: not a production transport, a network
//! or physical evidence.
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::sync::Notify;
use up_rust::{
    OwnedAttachment, PayloadIdentity, UCode, UOwnedFrame, UOwnedListener, UOwnedTransport,
    UOwnedTransportImpl, UStatus, UUri, UWireError, WireIdentity,
};

/// Exact injected carriage status, comparable after SDK propagation.
pub(crate) fn injected(message: &str) -> UStatus {
    UStatus::fail_with_code(UCode::Unavailable, message)
}

/// Attachment ids start at 1 and count every attach attempt on this carriage.
pub(crate) struct ScriptedCarriage {
    medium: Arc<dyn UOwnedTransport>,
    next: AtomicUsize,
    failed_attach: AtomicUsize,
    held_attach: AtomicUsize,
    pub(crate) attach_entered: Notify,
    failing_sends: AtomicBool,
    refusing_admission: AtomicBool,
    failed_detach: Arc<AtomicUsize>,
    detach_log: Arc<Mutex<Vec<usize>>>,
    live: Arc<AtomicUsize>,
    admissions: AtomicUsize,
    sends: AtomicUsize,
}

impl ScriptedCarriage {
    pub(crate) fn new(medium: Arc<dyn UOwnedTransport>) -> Self {
        Self {
            medium,
            next: AtomicUsize::new(0),
            failed_attach: AtomicUsize::new(0),
            held_attach: AtomicUsize::new(0),
            attach_entered: Notify::new(),
            failing_sends: AtomicBool::new(false),
            refusing_admission: AtomicBool::new(false),
            failed_detach: Arc::new(AtomicUsize::new(0)),
            detach_log: Arc::new(Mutex::new(Vec::new())),
            live: Arc::new(AtomicUsize::new(0)),
            admissions: AtomicUsize::new(0),
            sends: AtomicUsize::new(0),
        }
    }
    /// The attach attempt with this id fails before any registration exists.
    pub(crate) fn fail_attach(&self, id: usize) {
        self.failed_attach.store(id, Ordering::SeqCst);
    }
    /// The attach attempt with this id never completes; callers cancel it.
    pub(crate) fn hold_attach(&self, id: usize) {
        self.held_attach.store(id, Ordering::SeqCst);
    }
    /// Detaching the attachment with this id fails and stays retryable.
    pub(crate) fn fail_detach(&self, id: usize) {
        self.failed_detach.store(id, Ordering::SeqCst);
    }
    pub(crate) fn clear_detach_failure(&self) {
        self.failed_detach.store(0, Ordering::SeqCst);
    }
    pub(crate) fn fail_sends(&self, failing: bool) {
        self.failing_sends.store(failing, Ordering::SeqCst);
    }
    pub(crate) fn refuse_admission(&self, refusing: bool) {
        self.refusing_admission.store(refusing, Ordering::SeqCst);
    }
    pub(crate) fn attach_calls(&self) -> usize {
        self.next.load(Ordering::SeqCst)
    }
    /// Registrations currently present on the shared medium for this program.
    pub(crate) fn live(&self) -> usize {
        self.live.load(Ordering::SeqCst)
    }
    /// Exact attachment ids in detach-attempt order.
    pub(crate) fn detach_log(&self) -> Vec<usize> {
        self.detach_log.lock().unwrap().clone()
    }
    /// Every observable effect: (admission checks, attach attempts, sends).
    pub(crate) fn effects(&self) -> (usize, usize, usize) {
        (
            self.admissions.load(Ordering::SeqCst),
            self.attach_calls(),
            self.sends.load(Ordering::SeqCst),
        )
    }
}

#[async_trait::async_trait]
impl UOwnedTransportImpl for ScriptedCarriage {
    fn admit_payload_contract(
        &self,
        identity: PayloadIdentity,
        family: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        self.admissions.fetch_add(1, Ordering::SeqCst);
        if self.refusing_admission.load(Ordering::SeqCst) {
            return Err(UWireError::invalid_payload("scripted admission refusal"));
        }
        self.medium.admit_payload_contract(identity, family)
    }
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        if self.failing_sends.load(Ordering::SeqCst) {
            return Err(injected("send refused"));
        }
        self.medium.send_owned(frame).await
    }
    async fn attach_validated_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<OwnedAttachment, UStatus> {
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        if self.failed_attach.load(Ordering::SeqCst) == id {
            return Err(injected(&format!("attach#{id}")));
        }
        if self.held_attach.load(Ordering::SeqCst) == id {
            self.attach_entered.notify_one();
            std::future::pending::<()>().await;
        }
        let mut inner = self
            .medium
            .attach_owned_listener(source, sink, listener)
            .await?;
        let failed = Arc::clone(&self.failed_detach);
        let log = Arc::clone(&self.detach_log);
        let live = Arc::clone(&self.live);
        live.fetch_add(1, Ordering::SeqCst);
        let mut active = true;
        Ok(OwnedAttachment::new(move || {
            if !active {
                return Ok(());
            }
            log.lock().unwrap().push(id);
            if failed.load(Ordering::SeqCst) == id {
                return Err(injected(&format!("detach#{id}")));
            }
            inner.detach()?;
            active = false;
            live.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }))
    }
}
