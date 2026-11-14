//! Deterministic source controls; no execution or physical/native credit.
#[allow(dead_code)]
#[path = "../examples/support/user_first_door.rs"]
mod door;
#[allow(dead_code)]
#[path = "support/door_factories.rs"]
mod door_factories;

use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::Poll,
    time::Duration,
};
use tokio::sync::Notify;
use up_rust::{
    communication::{
        application::{self, Delivery, Handler},
        SubscriptionStatus,
    },
    core::usubscription::{ResetReason, SubscriptionInfo, USubscription},
    *,
};

fn error(message: &str) -> UStatus {
    UStatus::fail_with_code(UCode::Unavailable, message)
}
const BUDGET: Duration = Duration::from_secs(1);

#[derive(Default)]
struct Carriage {
    bus: InMemoryOwnedTransport,
    next: AtomicUsize,
    failed_attach: AtomicUsize,
    held_attach: AtomicUsize,
    attach_entered: Notify,
    attach_release: Notify,
    failed_detach: Arc<AtomicUsize>,
    attempts: Arc<Mutex<Vec<usize>>>,
    attached: Arc<AtomicUsize>,
}
#[async_trait::async_trait]
impl UOwnedTransportImpl for Carriage {
    fn admit_payload_contract(
        &self,
        identity: PayloadIdentity,
        family: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        UOwnedTransport::admit_payload_contract(&self.bus, identity, family)
    }
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.bus.send_owned(frame).await
    }
    async fn attach_validated_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<OwnedAttachment, UStatus> {
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        if self.failed_attach.load(Ordering::SeqCst) == id {
            return Err(error(&format!("attach#{id}")));
        }
        if self.held_attach.load(Ordering::SeqCst) == id {
            self.attach_entered.notify_one();
            self.attach_release.notified().await;
        }
        let mut inner = self
            .bus
            .attach_owned_listener(source, sink, listener)
            .await?;
        let failed = self.failed_detach.clone();
        let attempts = self.attempts.clone();
        let attached = self.attached.clone();
        attached.fetch_add(1, Ordering::SeqCst);
        let mut active = true;
        Ok(OwnedAttachment::new(move || {
            if !active {
                return Ok(());
            }
            attempts.lock().unwrap().push(id);
            if failed.load(Ordering::SeqCst) == id {
                return Err(error(&format!("detach#{id}")));
            }
            inner.detach()?;
            active = false;
            attached.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }))
    }
}

#[derive(Default)]
struct Service {
    remote: AtomicBool,
    activation_error: AtomicBool,
    removal_error: AtomicBool,
    hold_removal: AtomicBool,
    hold_query: AtomicBool,
    removal_entered: Notify,
    removal_release: Notify,
    query_entered: Notify,
    query_release: Notify,
}
#[async_trait::async_trait]
impl USubscription for Service {
    async fn subscribe(
        &self,
        _: &UUri,
        _: Option<u64>,
        _: Option<u32>,
    ) -> Result<SubscriptionStatus, UStatus> {
        self.remote.store(true, Ordering::SeqCst);
        if self.activation_error.load(Ordering::SeqCst) {
            Err(error("activation-original"))
        } else {
            Ok(SubscriptionStatus::Subscribed)
        }
    }
    async fn unsubscribe(&self, _: &UUri) -> Result<(), UStatus> {
        if self.hold_removal.load(Ordering::SeqCst) {
            self.removal_entered.notify_one();
            self.removal_release.notified().await;
        }
        self.remote.store(false, Ordering::SeqCst);
        if self.removal_error.swap(false, Ordering::SeqCst) {
            Err(error("unsubscribe-original"))
        } else {
            Ok(())
        }
    }
    async fn fetch_subscribers(&self, _: &UUri) -> Result<Vec<UUri>, UStatus> {
        if self.hold_query.load(Ordering::SeqCst) {
            self.query_entered.notify_one();
            self.query_release.notified().await;
        }
        Ok(if self.remote.load(Ordering::SeqCst) {
            vec![door::uris().1.get_source_uri()]
        } else {
            Vec::new()
        })
    }
    async fn fetch_subscriptions_by_topic(
        &self,
        _: &UUri,
    ) -> Result<Vec<SubscriptionInfo>, UStatus> {
        unreachable!()
    }
    async fn fetch_subscriptions_by_subscriber(
        &self,
        _: &UUri,
    ) -> Result<Vec<SubscriptionInfo>, UStatus> {
        unreachable!()
    }
    async fn register_for_notifications(&self, _: &UUri) -> Result<(), UStatus> {
        unreachable!()
    }
    async fn unregister_for_notifications(&self, _: &UUri) -> Result<(), UStatus> {
        unreachable!()
    }
    async fn reset(&self, _: ResetReason, _: Option<String>) -> Result<(), UStatus> {
        unreachable!()
    }
}

async fn cancel_after_poll(future: impl Future) {
    let mut future = std::pin::pin!(future);
    std::future::poll_fn(|context| {
        assert!(future.as_mut().poll(context).is_pending());
        Poll::Ready(())
    })
    .await;
}

struct Hold {
    entered: Arc<Notify>,
    release: Arc<Notify>,
    panic: bool,
}
impl application::RequestHandler<door::SetDoor> for Hold {
    type Reply = bool;
    async fn handle<D: Delivery<door::SetDoor>>(&mut self, delivery: D) -> Result<bool, UCode> {
        self.entered.notify_one();
        self.release.notified().await;
        assert!(!self.panic, "server-worker-original");
        Ok(delivery.payload().map_err(|_| UCode::InvalidArgument)?.open)
    }
}
impl Handler<door::DoorState> for Hold {
    async fn on_receive<D: Delivery<door::DoorState>>(&mut self, delivery: D) {
        assert!(delivery.payload().unwrap().open);
        self.entered.notify_one();
        self.release.notified().await;
        assert!(!self.panic, "worker-original");
        assert!(delivery.payload().unwrap().open);
    }
}

#[tokio::test(start_paused = true)]
async fn serialized_server_detach_failure_survives_cancelled_drain_and_successful_retry() {
    let transport = Arc::new(Carriage::default());
    let schema = door::deployment(transport.clone(), door::Encoding::Protobuf).unwrap();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let mut server = schema
        .set_server(Hold {
            entered: entered.clone(),
            release: release.clone(),
            panic: false,
        })
        .await
        .unwrap();
    server.prepare_client(&schema.panel_address()).unwrap();
    let client = schema.set_client().await.unwrap();
    client.set_token("door-control").unwrap();
    let pending = client.invoke(&door::SetDoor { open: true }).await.unwrap();
    entered.notified().await;
    transport.failed_detach.store(1, Ordering::SeqCst);
    cancel_after_poll(server.stop()).await;
    assert_eq!(
        server.stop_progress().detach.first.as_ref().unwrap().error,
        error("detach#1")
    );
    assert_eq!(
        server
            .stop_progress()
            .detach
            .first
            .as_ref()
            .unwrap()
            .attempt,
        1
    );
    assert_eq!(server.stop_progress().completion(), Completion::Pending);
    transport.failed_detach.store(0, Ordering::SeqCst);
    release.notify_one();
    server.stop().await.unwrap();
    assert_eq!(server.stop_progress().completion(), Completion::Succeeded);
    assert_eq!(
        server
            .stop_progress()
            .detach
            .first
            .as_ref()
            .unwrap()
            .attempt,
        1
    );
    assert!(pending.response().await.unwrap().payload().unwrap().open);
    client.close().unwrap();
    assert_eq!(transport.attempts.lock().unwrap().as_slice(), &[1, 1, 2]);
}

#[tokio::test(start_paused = true)]
async fn combined_original_errors_survive_timeout_and_outer_cancellation_then_retry() {
    for outer_cancel in [false, true] {
        let transport = Arc::new(Carriage::default());
        let service = Arc::new(Service::default());
        service.activation_error.store(true, Ordering::SeqCst);
        service.removal_error.store(true, Ordering::SeqCst);
        let schema = door::deployment(transport.clone(), door::Encoding::Protobuf).unwrap();
        let mut setup = door::roles::Setup::default();
        door::start(&mut setup, &schema, service.clone(), Default::default())
            .await
            .unwrap();
        let mut app = setup.finish().unwrap();
        app.notifications.stop().await.unwrap();
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        app.notifications = schema
            .changed_receiver(Hold {
                entered: entered.clone(),
                release: release.clone(),
                panic: false,
            })
            .await
            .unwrap();
        let receiver_id = transport.next.load(Ordering::SeqCst);
        app.changed
            .notify(&door::DoorState { open: true })
            .await
            .unwrap();
        entered.notified().await;
        transport.failed_detach.store(receiver_id, Ordering::SeqCst);
        let mut retained = door::Recovery::new(app);
        retained.run().await;
        if outer_cancel {
            cancel_after_poll(retained.app.close_bounded(1, BUDGET)).await;
        } else {
            assert_eq!(
                retained.app.close_bounded(1, BUDGET).await,
                Completion::Pending
            );
        }
        let first = retained
            .app
            .notifications
            .stop_progress()
            .detach
            .first
            .as_ref()
            .unwrap();
        assert_eq!(first.attempt, 1);
        assert_eq!(first.error, error(&format!("detach#{receiver_id}")));
        let remote = retained
            .app
            .subscription
            .service_history()
            .first
            .as_ref()
            .unwrap();
        assert_eq!(remote.attempt, 1);
        assert_eq!(remote.error, error("unsubscribe-original"));
        assert!(matches!(retained.app.cleanup.set.current, Some(Ok(()))));
        assert_eq!(retained.app.cleanup.notifications.attempt, 1);
        assert!(retained.app.cleanup.notifications.current.is_none());
        assert_eq!(retained.app.cleanup.notifications.timed_out, !outer_cancel);
        retained.app.subscription.observe_remote(BUDGET).await;
        release.notify_one();
        transport.failed_detach.store(0, Ordering::SeqCst);
        assert_eq!(
            retained.app.close_bounded(1, BUDGET).await,
            Completion::Succeeded
        );
        assert_eq!(retained.app.cleanup.completion(), Completion::Succeeded);
        assert!(
            matches!(&retained.operation, Some(door::AppError::Activate(application::SubscriptionActivationError::Service(original))) if *original == error("activation-original"))
        );
        assert_eq!(
            retained
                .app
                .subscription
                .service_history()
                .first
                .as_ref()
                .unwrap()
                .error,
            error("unsubscribe-original")
        );
        let progress = retained.app.notifications.stop_progress();
        assert_eq!(progress.attempt, 2);
        assert_eq!(progress.detach.first.as_ref().unwrap().attempt, 1);
        assert!(progress.detach.latest.is_none());
        assert_eq!(transport.attached.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test(start_paused = true)]
async fn hung_remote_removal_and_query_do_not_starve_other_owners() {
    let transport = Arc::new(Carriage::default());
    let service = Arc::new(Service::default());
    service.hold_removal.store(true, Ordering::SeqCst);
    service.hold_query.store(true, Ordering::SeqCst);
    let schema = door::deployment(transport.clone(), door::Encoding::Json).unwrap();
    let mut setup = door::roles::Setup::default();
    door::start(&mut setup, &schema, service.clone(), Default::default())
        .await
        .unwrap();
    let mut retained = door::Recovery::new(setup.finish().unwrap());
    retained.app.activate().await.unwrap();
    assert_eq!(
        retained.app.close_bounded(1, BUDGET).await,
        Completion::Pending
    );
    assert_eq!(
        retained.app.cleanup.notifications.completion,
        Completion::Succeeded
    );
    assert_eq!(
        retained.app.cleanup.server.completion,
        Completion::Succeeded
    );
    assert_eq!(retained.app.cleanup.set.completion, Completion::Succeeded);
    assert!(matches!(
        retained.app.set.invoke(&door::SetDoor { open: true }).await,
        Err(application::CallError::Closed)
    ));
    assert!(retained.app.cleanup.subscription.timed_out);
    assert_eq!(transport.attached.load(Ordering::SeqCst), 0);
    retained.app.subscription.observe_remote(BUDGET).await;
    let observations = retained.app.subscription.remote_observations();
    assert_eq!(observations.timeouts, 1);
    assert!(observations.observed.is_none());
    assert_eq!(
        retained.app.subscription.phase(),
        application::SubscriptionPhase::Closing
    );
    service.hold_removal.store(false, Ordering::SeqCst);
    assert_eq!(
        retained.app.close_bounded(1, BUDGET).await,
        Completion::Succeeded
    );
    assert_eq!(retained.app.cleanup.completion(), Completion::Succeeded);
}

#[tokio::test(start_paused = true)]
async fn failed_and_cancelled_partial_setup_keep_the_exact_failed_attachment() {
    for cancel in [false, true] {
        let transport = Arc::new(Carriage::default());
        if cancel {
            transport.held_attach.store(2, Ordering::SeqCst);
        } else {
            transport.failed_attach.store(2, Ordering::SeqCst);
        }
        let schema = door::deployment(transport.clone(), door::Encoding::Protobuf).unwrap();
        let mut setup = door::roles::Setup::default();
        let mut original = None;
        if cancel {
            tokio::select! {
                result = door::start(&mut setup, &schema, Arc::new(Service::default()), Default::default()) => panic!("setup unexpectedly completed: {result:?}"),
                () = transport.attach_entered.notified() => {},
            }
        } else {
            original = Some(
                door::start(
                    &mut setup,
                    &schema,
                    Arc::new(Service::default()),
                    Default::default(),
                )
                .await
                .unwrap_err(),
            );
        }
        assert!(setup.slots().set.is_held());
        assert!(!setup.slots().server.is_held());
        assert_eq!(transport.attached.load(Ordering::SeqCst), 1);
        let other = Arc::new(Carriage::default());
        let other_schema = door::deployment(other.clone(), door::Encoding::Protobuf).unwrap();
        assert!(matches!(
            door::start(
                &mut setup,
                &other_schema,
                Arc::new(Service::default()),
                Default::default()
            )
            .await,
            Err(door::AppError::RoleSet(
                application::RoleSetError::AlreadyStarted
            ))
        ));
        assert_eq!(other.attached.load(Ordering::SeqCst), 0);

        transport.failed_detach.store(1, Ordering::SeqCst);
        assert_eq!(setup.close(BUDGET).await, Completion::Pending);
        assert!(setup.slots().set.is_held());
        let first = setup.cleanup.set.first_error.as_ref().unwrap();
        assert_eq!(first.attempt, 1);
        assert!(
            matches!(first.error.as_ref(), application::SerializedError::Transport(status) if *status == error("detach#1"))
        );
        transport.failed_detach.store(0, Ordering::SeqCst);
        assert_eq!(setup.close(BUDGET).await, Completion::Succeeded);
        assert_eq!(transport.attempts.lock().unwrap().as_slice(), &[1, 1]);
        assert_eq!(transport.attached.load(Ordering::SeqCst), 0);
        if !cancel {
            assert!(
                matches!(original, Some(door::AppError::Setup(application::SerializedError::Transport(status))) if status == error("attach#2"))
            );
        }
        assert!(setup.cleanup.set.first_error.is_some());
    }
}

#[tokio::test(start_paused = true)]
async fn panic_is_finite_terminal_failed_not_success_or_endless_retry() {
    let transport = Arc::new(Carriage::default());
    let schema = door::deployment(transport.clone(), door::Encoding::Protobuf).unwrap();
    let service = Arc::new(Service::default());
    service.activation_error.store(true, Ordering::SeqCst);
    let mut setup = door::roles::Setup::default();
    door::start(&mut setup, &schema, service, Default::default())
        .await
        .unwrap();
    let mut app = setup.finish().unwrap();
    app.notifications.stop().await.unwrap();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    app.notifications = schema
        .changed_receiver(Hold {
            entered: entered.clone(),
            release: release.clone(),
            panic: true,
        })
        .await
        .unwrap();
    app.changed
        .notify(&door::DoorState { open: true })
        .await
        .unwrap();
    entered.notified().await;
    release.notify_one();
    let mut retained = door::Recovery::new(app);
    retained.run().await;
    assert_eq!(
        retained.app.close_bounded(8, BUDGET).await,
        Completion::Failed
    );
    assert_eq!(retained.app.cleanup.notifications.attempt, 1);
    assert_eq!(retained.app.cleanup.completion(), Completion::Failed);
    assert_eq!(
        retained
            .app
            .notifications
            .stop_progress()
            .worker
            .first
            .as_ref()
            .unwrap()
            .error,
        RegistrationStopError::HandlerPanicked
    );
    assert!(retained.operation.is_some());
    assert_eq!(transport.attached.load(Ordering::SeqCst), 0);
    assert_eq!(
        retained.app.close_bounded(8, BUDGET).await,
        Completion::Failed
    );
    assert_eq!(retained.app.cleanup.completion(), Completion::Failed);
}

#[test]
fn bounded_history_never_promotes_called_from_worker_to_terminal() {
    let mut progress = StopProgress::<UStatus>::default();
    progress.begin(Some(error("first-detach")));
    progress.finish(Some(RegistrationStopError::CalledFromWorker));
    assert_eq!(progress.completion(), Completion::Pending);
    progress.begin(None);
    progress.finish(Some(RegistrationStopError::HandlerPanicked));
    assert_eq!(progress.completion(), Completion::Failed);
    assert_eq!(progress.worker.first.as_ref().unwrap().attempt, 2);
    for _attempt in 0..100 {
        progress.begin(None);
        progress.finish(Some(RegistrationStopError::HandlerPanicked));
    }
    assert_eq!(
        progress.detach.first.as_ref().unwrap().error,
        error("first-detach")
    );
    assert_eq!(progress.worker.first.as_ref().unwrap().attempt, 2);
    assert_eq!(progress.worker.failures, 101);
}
