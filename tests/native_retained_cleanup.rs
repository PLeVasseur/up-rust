//! Heap-backed fault fixtures for SDK lifecycle and generic Send contracts only.
//! These do not qualify middleware-native storage or per-message allocation.
#[allow(dead_code)]
#[path = "../examples/support/user_first_door.rs"]
mod door;
#[allow(dead_code)]
#[path = "support/native_door_factories.rs"]
mod native_door_factories;
#[path = "support/native_recovery3.rs"]
mod recovery3;

use door::{
    native::{start, NativeDoor, NativeDoorApp, NativeDoorSetup},
    registration, rpc_options, DoorOptions, DoorState, SetDoor,
};
use std::{
    cell::Cell,
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
        application::{
            native::{self as native_roles, Receive, Serve},
            Delivery, Handler, PublishOptions, RequestHandler as AppRequestHandler, ServerOptions,
        },
        typed::NativeConfig,
        zero_copy::{ReceiveConfig, ReceiverStopError, StableTransport},
        SubscriptionStatus,
    },
    core::usubscription::{MockUSubscription, USubscription},
    *,
};

struct Attachment {
    inner: <PoolCore as PreparedTransportCore>::Subscription,
    id: usize,
    local: Cell<usize>,
}

struct FaultCore {
    pool: Arc<PoolCore>,
    fail_detach: AtomicBool,
    fail_attach: AtomicUsize,
    hold_attach: AtomicUsize,
    attach_entered: Notify,
    next_id: AtomicUsize,
    attempts: Mutex<Vec<usize>>,
    deliveries: Arc<AtomicUsize>,
}

struct CountedSink {
    inner: Arc<dyn FrameSink<<PoolCore as PreparedTransportCore>::Rx>>,
    deliveries: Arc<AtomicUsize>,
}

impl FrameSink<<PoolCore as PreparedTransportCore>::Rx> for CountedSink {
    fn push(&self, frame: <PoolCore as PreparedTransportCore>::Rx) {
        self.deliveries.fetch_add(1, Ordering::SeqCst);
        self.inner.push(frame);
    }

    fn close(&self) {
        self.inner.close();
    }
}

impl NativeLoanCore for FaultCore {}

impl PreparedTransportCore for FaultCore {
    type Route = <PoolCore as PreparedTransportCore>::Route;
    type Tx<'loan> = <PoolCore as PreparedTransportCore>::Tx<'loan>;
    type Rx = <PoolCore as PreparedTransportCore>::Rx;
    type Subscription = Attachment;

    fn prepare_route(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        limits: RouteLimits,
    ) -> Result<Self::Route, SetupError> {
        self.pool.prepare_route(source, sink, limits)
    }

    fn loan<'loan>(
        &'loan self,
        route: &'loan Self::Route,
        layout: TxFrameLayout,
    ) -> Result<Self::Tx<'loan>, LoanError> {
        self.pool.loan(route, layout)
    }

    fn publish<'loan>(
        &'loan self,
        route: &'loan Self::Route,
        frame: InitializedTx<Self::Tx<'loan>>,
    ) -> Result<(), SendError> {
        self.pool.publish(route, frame)
    }

    async fn attach(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        inbox: Arc<dyn FrameSink<Self::Rx>>,
    ) -> Result<Self::Subscription, SetupError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        if self.fail_attach.load(Ordering::SeqCst) == id {
            return Err(SetupError::Worker);
        }
        if self.hold_attach.load(Ordering::SeqCst) == id {
            self.attach_entered.notify_one();
            std::future::pending::<()>().await;
        }
        let inner = self
            .pool
            .attach(
                source,
                sink,
                Arc::new(CountedSink {
                    inner: inbox,
                    deliveries: self.deliveries.clone(),
                }),
            )
            .await?;
        Ok(Attachment {
            inner,
            id,
            local: Cell::new(0),
        })
    }

    fn detach(&self, attachment: &mut Self::Subscription) -> Result<(), SetupError> {
        self.attempts.lock().unwrap().push(attachment.id);
        attachment.local.set(attachment.local.get() + 1);
        if self.fail_detach.load(Ordering::SeqCst) {
            return Err(SetupError::Worker);
        }
        self.pool.detach(&mut attachment.inner)
    }
}

fn setup() -> (Arc<FaultCore>, NativeDoor<FaultCore>) {
    let pool = Arc::new(
        PoolCore::new(PoolCoreConfig {
            slots: 16,
            slot_bytes: 4096,
            alignment: 8,
            max_routes: 16,
            max_registrations: 8,
        })
        .unwrap(),
    );
    let core = Arc::new(FaultCore {
        pool,
        fail_detach: AtomicBool::new(false),
        fail_attach: AtomicUsize::new(0),
        hold_attach: AtomicUsize::new(0),
        attach_entered: Notify::new(),
        next_id: AtomicUsize::new(1),
        attempts: Mutex::new(Vec::new()),
        deliveries: Arc::new(AtomicUsize::new(0)),
    });
    let profile = Arc::new(
        NativeProfile::new(
            "retained-cleanup-fixture",
            1,
            PayloadEncoding::private_use(0xF100),
            [
                DoorState::native_representation(),
                SetDoor::native_representation(),
            ],
        )
        .unwrap(),
    );
    let agreement = NativeProfileAgreement::new(profile.clone(), &profile).unwrap();
    let transport: Arc<StableTransport<FaultCore>> =
        Arc::new(UWireTransport::with_native_agreement(
            core.clone(),
            StableContainerWire,
            UFrameFieldsMetadataCodec,
            agreement,
        ));
    let config = NativeConfig {
        route: RouteConfig::default(),
        receive: ReceiveConfig {
            registration: registration(),
            header: HeaderLimits { max_token_len: 32 },
        },
    };
    let mut rpc = rpc_options();
    rpc.ttl = Duration::from_secs(30);
    let options = DoorOptions {
        publish: PublishOptions::default(),
        rpc,
        server: ServerOptions { max_clients: 4 },
    };
    (core, NativeDoor::bind(transport, config, options).unwrap())
}

fn service() -> Arc<dyn USubscription> {
    let mut service = MockUSubscription::new();
    service
        .expect_subscribe()
        .times(1)
        .returning(|_, _, _| Ok(SubscriptionStatus::Subscribed));
    service.expect_unsubscribe().times(1).returning(|_| Ok(()));
    Arc::new(service)
}

struct Control {
    entered: Notify,
    release: Notify,
    completed: AtomicUsize,
    panic_after_release: bool,
}

impl Control {
    fn new(panic_after_release: bool) -> Arc<Self> {
        Arc::new(Self {
            entered: Notify::new(),
            release: Notify::new(),
            completed: AtomicUsize::new(0),
            panic_after_release,
        })
    }
}

struct Waiting(Arc<Control>);

impl Handler<DoorState> for Waiting {
    async fn on_receive<Owner: Delivery<DoorState>>(&mut self, owner: Owner) {
        let address = owner.payload().unwrap() as *const DoorState as usize;
        self.0.entered.notify_one();
        self.0.release.notified().await;
        assert_eq!(
            address,
            owner.payload().unwrap() as *const DoorState as usize
        );
        assert!(owner.payload().unwrap().open);
        self.0.completed.fetch_add(1, Ordering::SeqCst);
        assert!(
            !self.0.panic_after_release,
            "injected native receiver failure"
        );
    }
}

impl AppRequestHandler<SetDoor> for Waiting {
    type Reply = bool;

    async fn handle<Owner: Delivery<SetDoor>>(&mut self, owner: Owner) -> Result<bool, UCode> {
        let address = owner.payload().unwrap() as *const SetDoor as usize;
        self.0.entered.notify_one();
        self.0.release.notified().await;
        assert_eq!(address, owner.payload().unwrap() as *const SetDoor as usize);
        let open = owner.payload().unwrap().open;
        self.0.completed.fetch_add(1, Ordering::SeqCst);
        assert!(
            !self.0.panic_after_release,
            "injected native server failure"
        );
        Ok(open)
    }
}

async fn poll_once_pending(future: impl Future) {
    let mut future = std::pin::pin!(future);
    std::future::poll_fn(|context| {
        assert!(future.as_mut().poll(context).is_pending());
        Poll::Ready(())
    })
    .await;
}

const CLOSE_DEADLINE: Duration = Duration::from_secs(10);

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(10), future)
        .await
        .expect("fixture stalled")
}

#[tokio::test]
async fn receiver_failed_exact_detach_then_cancelled_drain_retries_same_owner() {
    let (core, binding) = setup();
    let control = Control::new(false);
    let mut receiver = binding
        .changed_receiver(Waiting(control.clone()))
        .await
        .unwrap();
    let notifier = binding.changed_notifier().unwrap();
    notifier
        .notify_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    bounded(control.entered.notified()).await;
    core.fail_detach.store(true, Ordering::SeqCst);
    poll_once_pending(Receive::stop(&mut receiver)).await;
    assert_eq!(
        receiver
            .stop_progress()
            .detach
            .first
            .as_ref()
            .unwrap()
            .error,
        SetupError::Worker
    );
    assert_eq!(
        receiver
            .stop_progress()
            .detach
            .first
            .as_ref()
            .unwrap()
            .attempt,
        1
    );
    assert_eq!(receiver.stop_progress().completion(), Completion::Pending);
    assert_eq!(core.attempts.lock().unwrap().as_slice(), &[1]);
    assert_eq!(control.completed.load(Ordering::SeqCst), 0);
    core.fail_detach.store(false, Ordering::SeqCst);
    control.release.notify_one();
    bounded(Receive::stop(&mut receiver)).await.unwrap();
    Receive::stop(&mut receiver).await.unwrap();
    assert_eq!(receiver.stop_progress().completion(), Completion::Succeeded);
    assert_eq!(
        receiver
            .stop_progress()
            .detach
            .first
            .as_ref()
            .unwrap()
            .error,
        SetupError::Worker
    );
    assert!(receiver.stop_progress().detach.latest.is_none());
    assert_eq!(core.attempts.lock().unwrap().as_slice(), &[1, 1]);
    assert_eq!(control.completed.load(Ordering::SeqCst), 1);
    assert_eq!(core.pool.free_slots(), 16);
}

#[tokio::test]
async fn receiver_worker_failure_survives_detach_failure_and_successful_retry() {
    let (core, binding) = setup();
    let control = Control::new(true);
    let mut receiver = binding
        .changed_receiver(Waiting(control.clone()))
        .await
        .unwrap();
    binding
        .changed_notifier()
        .unwrap()
        .notify_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    bounded(control.entered.notified()).await;
    core.fail_detach.store(true, Ordering::SeqCst);
    control.release.notify_one();
    assert_eq!(
        bounded(Receive::stop(&mut receiver)).await,
        Err(ReceiverStopError::Detach(SetupError::Worker))
    );
    core.fail_detach.store(false, Ordering::SeqCst);
    for _ in 0..2 {
        assert_eq!(
            bounded(Receive::stop(&mut receiver)).await,
            Err(ReceiverStopError::Worker(
                RegistrationStopError::HandlerPanicked
            ))
        );
    }
    assert_eq!(core.attempts.lock().unwrap().as_slice(), &[1, 1]);
    assert_eq!(control.completed.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn server_cancelled_drain_retains_join_and_request_then_retries() {
    let (core, binding) = setup();
    let control = Control::new(false);
    let mut server = binding.set_server(Waiting(control.clone())).await.unwrap();
    server.prepare_client(&binding.panel_address()).unwrap();
    let client = binding.set_client().await.unwrap();
    let pending = client
        .invoke_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    bounded(control.entered.notified()).await;
    core.fail_detach.store(true, Ordering::SeqCst);
    poll_once_pending(Serve::stop(&mut server)).await;
    assert_eq!(
        server.stop_progress().detach.first.as_ref().unwrap().error,
        SetupError::Worker
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
    assert_eq!(core.attempts.lock().unwrap().as_slice(), &[1]);
    assert_eq!(control.completed.load(Ordering::SeqCst), 0);
    core.fail_detach.store(false, Ordering::SeqCst);
    control.release.notify_one();
    bounded(Serve::stop(&mut server)).await.unwrap();
    Serve::stop(&mut server).await.unwrap();
    assert_eq!(server.stop_progress().completion(), Completion::Succeeded);
    assert_eq!(
        server.stop_progress().detach.first.as_ref().unwrap().error,
        SetupError::Worker
    );
    assert!(server.stop_progress().detach.latest.is_none());
    assert_eq!(core.attempts.lock().unwrap().as_slice(), &[1, 1]);
    let response = bounded(pending.response()).await.unwrap();
    assert!(response.payload().unwrap().open);
    client.close().unwrap();
    drop(server);
    drop(client);
    drop(binding);
    assert!(response.payload().unwrap().open);
    assert!(response.header().unwrap().reqid.is_some());
    drop(response);
    assert_eq!(core.pool.free_slots(), 16);
}

#[tokio::test]
async fn server_combined_failure_keeps_terminal_worker_error_after_exact_detach_retry() {
    let (core, binding) = setup();
    let control = Control::new(true);
    let mut server = binding.set_server(Waiting(control.clone())).await.unwrap();
    let client = binding.set_client().await.unwrap();
    let pending = client
        .invoke_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    bounded(control.entered.notified()).await;
    core.fail_detach.store(true, Ordering::SeqCst);
    control.release.notify_one();
    let first = bounded(Serve::stop(&mut server)).await.unwrap_err();
    assert_eq!(first.detach, Some(SetupError::Worker));
    assert_eq!(first.worker, Some(RegistrationStopError::HandlerPanicked));
    core.fail_detach.store(false, Ordering::SeqCst);
    for _ in 0..2 {
        let retry = bounded(Serve::stop(&mut server)).await.unwrap_err();
        assert_eq!(retry.detach, None);
        assert_eq!(retry.worker, first.worker);
    }
    assert_eq!(core.attempts.lock().unwrap().as_slice(), &[1, 1]);
    assert_eq!(control.completed.load(Ordering::SeqCst), 1);
    drop(pending);
    client.close().unwrap();
}

#[tokio::test]
async fn door_cleanup_failure_retains_app_and_rx_for_explicit_retry() {
    let (core, binding) = setup();
    let mut setup = NativeDoorSetup::default();
    start(
        &mut setup,
        &binding,
        service(),
        Arc::new(Mutex::new(Vec::new())),
    )
    .await
    .unwrap();
    let mut app = setup.finish().unwrap();
    app.activate().await.unwrap();
    let response = bounded(app.exercise()).await.unwrap();
    let address = response.payload().unwrap() as *const DoorState as usize;
    bounded(app.subscription.unsubscribe()).await.unwrap();
    core.fail_detach.store(true, Ordering::SeqCst);
    assert_eq!(
        bounded(app.close(CLOSE_DEADLINE)).await,
        Completion::Pending
    );
    assert!(app.cleanup.notifications.first_error.is_some());
    assert!(app.cleanup.server.first_error.is_some());
    assert!(app.cleanup.set.first_error.is_some());
    core.fail_detach.store(false, Ordering::SeqCst);
    assert_eq!(
        bounded(app.close(CLOSE_DEADLINE)).await,
        Completion::Succeeded
    );
    drop(app);
    drop(binding);
    assert_eq!(
        address,
        response.payload().unwrap() as *const DoorState as usize
    );
    assert!(response.payload().unwrap().open);
    drop(response);
    assert_eq!(core.pool.free_slots(), 16);
}

#[tokio::test]
async fn door_cancelled_cleanup_keeps_entire_app_and_unpolled_server_drain() {
    let (core, binding) = setup();
    let control = Control::new(false);
    let mut setup = NativeDoorSetup::default();
    start(
        &mut setup,
        &binding,
        service(),
        Arc::new(Mutex::new(Vec::new())),
    )
    .await
    .unwrap();
    let mut app = setup.finish().unwrap();
    Receive::stop(&mut app.notifications).await.unwrap();
    app.notifications = binding
        .changed_receiver(Waiting(control.clone()))
        .await
        .unwrap();
    app.activate().await.unwrap();
    app.subscription.unsubscribe().await.unwrap();
    app.changed
        .notify_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    bounded(control.entered.notified()).await;
    poll_once_pending(app.close(CLOSE_DEADLINE)).await;
    let detached = core.attempts.lock().unwrap().clone();
    assert_eq!(detached.len(), 5);
    assert_eq!(control.completed.load(Ordering::SeqCst), 0);
    control.release.notify_one();
    assert_eq!(
        bounded(app.close(CLOSE_DEADLINE)).await,
        Completion::Succeeded
    );
    assert_eq!(*core.attempts.lock().unwrap(), detached);
    assert_eq!(control.completed.load(Ordering::SeqCst), 1);
    assert_eq!(core.pool.free_slots(), 16);
}

struct RetainOutsideWorker {
    entered: Arc<Notify>,
    release: Arc<Notify>,
    task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl Handler<DoorState> for RetainOutsideWorker {
    async fn on_receive<Owner: Delivery<DoorState>>(&mut self, owner: Owner) {
        let entered = self.entered.clone();
        let release = self.release.clone();
        let task = tokio::spawn(async move {
            let address = owner.payload().unwrap() as *const DoorState as usize;
            entered.notify_one();
            release.notified().await;
            assert_eq!(
                address,
                owner.payload().unwrap() as *const DoorState as usize
            );
            assert!(owner.payload().unwrap().open);
            assert!(owner.context().unwrap().native_type_token.is_some());
        });
        *self.task.lock().unwrap() = Some(task);
    }
}

#[tokio::test]
async fn receiver_delivery_outlives_successful_shutdown_and_all_endpoint_owners() {
    let (core, binding) = setup();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let task = Arc::new(Mutex::new(None));
    let mut receiver = binding
        .changed_receiver(RetainOutsideWorker {
            entered: entered.clone(),
            release: release.clone(),
            task: task.clone(),
        })
        .await
        .unwrap();
    binding
        .changed_notifier()
        .unwrap()
        .notify_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    bounded(entered.notified()).await;
    bounded(Receive::stop(&mut receiver)).await.unwrap();
    drop(receiver);
    drop(binding);
    drop(core);
    release.notify_one();
    let task = task.lock().unwrap().take().unwrap();
    bounded(task).await.unwrap();
}

fn spawn_app<C>(binding: NativeDoor<C>) -> tokio::task::JoinHandle<NativeDoorApp<C>>
where
    C: NativeLoanCore,
{
    tokio::spawn(async move {
        let mut setup = NativeDoorSetup::default();
        start(
            &mut setup,
            &binding,
            service(),
            Arc::new(Mutex::new(Vec::new())),
        )
        .await
        .unwrap();
        let mut app = setup.finish().unwrap();
        app.activate().await.unwrap();
        let response = app.exercise().await.unwrap();
        assert!(response.payload().unwrap().open);
        app.subscription.unsubscribe().await.unwrap();
        assert_eq!(app.close(CLOSE_DEADLINE).await, Completion::Succeeded);
        drop(response);
        app
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generic_owning_application_future_is_spawnable_with_send_only_attachments() {
    let (core, binding) = setup();
    let app = bounded(spawn_app(binding)).await.unwrap();
    drop(app);
    assert_eq!(core.pool.free_slots(), 16);
}

#[tokio::test]
async fn concrete_stop_is_eager_retryable_and_failed_detach_rejects_new_delivery() {
    let (core, binding) = setup();
    let control = Control::new(false);
    let mut receiver = binding
        .changed_receiver(Waiting(control.clone()))
        .await
        .unwrap();
    let notifier = binding.changed_notifier().unwrap();
    notifier
        .notify_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    bounded(control.entered.notified()).await;
    assert_eq!(core.pool.free_slots(), 15);
    core.fail_detach.store(true, Ordering::SeqCst);
    drop(receiver.stop());
    assert_eq!(core.attempts.lock().unwrap().as_slice(), &[1]);
    notifier
        .notify_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    assert_eq!(core.deliveries.load(Ordering::SeqCst), 2);
    assert_eq!(core.pool.free_slots(), 15);
    core.fail_detach.store(false, Ordering::SeqCst);
    poll_once_pending(receiver.stop()).await;
    assert_eq!(core.attempts.lock().unwrap().as_slice(), &[1, 1]);
    control.release.notify_one();
    bounded(receiver.stop()).await.unwrap();
    bounded(receiver.stop()).await.unwrap();
    assert_eq!(control.completed.load(Ordering::SeqCst), 1);
    assert_eq!(core.attempts.lock().unwrap().as_slice(), &[1, 1]);
    assert_eq!(core.pool.free_slots(), 16);
}

struct RegistrationSelfStop {
    owner: Arc<tokio::sync::Mutex<Option<Registration<usize>>>>,
    control: Arc<Control>,
}

impl RegistrationHandler<usize> for RegistrationSelfStop {
    async fn on_receive(&mut self, item: usize) {
        self.control.completed.fetch_add(1, Ordering::SeqCst);
        if item == 1 {
            let mut owner = self.owner.lock().await.take().unwrap();
            assert_eq!(
                owner.stop_ref().await,
                Err(RegistrationStopError::CalledFromWorker)
            );
            assert_eq!(owner.queued(), 1);
            *self.owner.lock().await = Some(owner);
            self.control.entered.notify_one();
            self.control.release.notified().await;
        } else {
            panic!("queued registration handler failure after self-stop");
        }
    }
}

#[tokio::test]
async fn borrowed_registration_self_stop_retains_join_for_external_panic_observation() {
    let owner = Arc::new(tokio::sync::Mutex::new(None));
    let control = Control::new(false);
    let detached = Arc::new(AtomicUsize::new(0));
    let detach_count = detached.clone();
    let registration = Registration::new(
        registration(),
        RegistrationSelfStop {
            owner: owner.clone(),
            control: control.clone(),
        },
        move || {
            detach_count.fetch_add(1, Ordering::SeqCst);
        },
    )
    .unwrap();
    let inbox = registration.inbox();
    *owner.lock().await = Some(registration);
    inbox.push(1).unwrap();
    inbox.push(2).unwrap();
    bounded(control.entered.notified()).await;
    assert_eq!(inbox.push(3), Err(InboxPushError::Closed));
    let mut registration = owner.lock().await.take().unwrap();
    poll_once_pending(registration.stop_ref()).await;
    control.release.notify_one();
    assert_eq!(
        bounded(registration.stop_ref()).await,
        Err(RegistrationStopError::HandlerPanicked)
    );
    assert_eq!(control.completed.load(Ordering::SeqCst), 2);
    assert_eq!(detached.load(Ordering::SeqCst), 1);
}

type ConcreteReceiver = native_roles::Receiver<FaultCore>;
type ConcreteServer = native_roles::RpcServer<FaultCore, DoorState>;

struct ReceiverSelfStop {
    owner: Arc<tokio::sync::Mutex<Option<ConcreteReceiver>>>,
    control: Arc<Control>,
}

impl Handler<DoorState> for ReceiverSelfStop {
    async fn on_receive<Owner: Delivery<DoorState>>(&mut self, owner: Owner) {
        assert!(owner.payload().unwrap().open);
        if self.control.completed.fetch_add(1, Ordering::SeqCst) == 0 {
            let mut receiver = self.owner.lock().await.take().unwrap();
            assert_eq!(
                receiver.stop().await,
                Err(ReceiverStopError::Worker(
                    RegistrationStopError::CalledFromWorker
                ))
            );
            *self.owner.lock().await = Some(receiver);
            self.control.entered.notify_one();
            self.control.release.notified().await;
        } else {
            panic!("queued native notification failure after self-stop");
        }
    }
}

#[tokio::test]
async fn concrete_receiver_self_stop_does_not_cache_refusal_or_lose_queued_panic() {
    let (core, binding) = setup();
    let owner = Arc::new(tokio::sync::Mutex::new(None));
    let control = Control::new(false);
    let receiver = binding
        .changed_receiver(ReceiverSelfStop {
            owner: owner.clone(),
            control: control.clone(),
        })
        .await
        .unwrap();
    *owner.lock().await = Some(receiver);
    let notifier = binding.changed_notifier().unwrap();
    for _ in 0..2 {
        notifier
            .notify_in_place(|loan| loan.into_initializer().open(true).finish())
            .unwrap();
    }
    bounded(control.entered.notified()).await;
    let mut receiver = owner.lock().await.take().unwrap();
    poll_once_pending(receiver.stop()).await;
    control.release.notify_one();
    for _ in 0..2 {
        assert_eq!(
            bounded(receiver.stop()).await,
            Err(ReceiverStopError::Worker(
                RegistrationStopError::HandlerPanicked
            ))
        );
    }
    assert_eq!(control.completed.load(Ordering::SeqCst), 2);
    assert_eq!(core.attempts.lock().unwrap().as_slice(), &[1]);
    assert_eq!(core.pool.free_slots(), 16);
}

struct ServerSelfStop {
    owner: Arc<tokio::sync::Mutex<Option<ConcreteServer>>>,
    control: Arc<Control>,
}

impl AppRequestHandler<SetDoor> for ServerSelfStop {
    type Reply = bool;

    async fn handle<Owner: Delivery<SetDoor>>(&mut self, owner: Owner) -> Result<bool, UCode> {
        assert!(owner.payload().unwrap().open);
        if self.control.completed.fetch_add(1, Ordering::SeqCst) == 0 {
            let mut server = self.owner.lock().await.take().unwrap();
            let refusal = server.stop().await.unwrap_err();
            assert_eq!(refusal.detach, None);
            assert_eq!(
                refusal.worker,
                Some(RegistrationStopError::CalledFromWorker)
            );
            *self.owner.lock().await = Some(server);
            self.control.entered.notify_one();
            self.control.release.notified().await;
            Ok(true)
        } else {
            panic!("queued native request failure after self-stop");
        }
    }
}

#[tokio::test]
async fn concrete_server_self_stop_does_not_cache_refusal_or_lose_queued_panic() {
    let (core, binding) = setup();
    let owner = Arc::new(tokio::sync::Mutex::new(None));
    let control = Control::new(false);
    let server = binding
        .set_server(ServerSelfStop {
            owner: owner.clone(),
            control: control.clone(),
        })
        .await
        .unwrap();
    server.prepare_client(&binding.panel_address()).unwrap();
    *owner.lock().await = Some(server);
    let client = binding.set_client().await.unwrap();
    let first = client
        .invoke_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    let second = client
        .invoke_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    bounded(control.entered.notified()).await;
    let mut server = owner.lock().await.take().unwrap();
    poll_once_pending(server.stop()).await;
    control.release.notify_one();
    for _ in 0..2 {
        let failure = bounded(server.stop()).await.unwrap_err();
        assert_eq!(failure.detach, None);
        assert_eq!(failure.worker, Some(RegistrationStopError::HandlerPanicked));
    }
    assert_eq!(control.completed.load(Ordering::SeqCst), 2);
    assert_eq!(core.attempts.lock().unwrap().as_slice(), &[1]);
    drop((first, second));
    client.close().unwrap();
    assert_eq!(core.pool.free_slots(), 16);
}

#[tokio::test]
async fn unpolled_app_stop_fences_all_local_roles_and_retries_exact_attachment_ids() {
    let (core, binding) = setup();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut setup = NativeDoorSetup::default();
    start(&mut setup, &binding, service(), observed.clone())
        .await
        .unwrap();
    let mut app = setup.finish().unwrap();
    app.activate().await.unwrap();
    bounded(app.subscription.unsubscribe()).await.unwrap();
    assert_eq!(core.next_id.load(Ordering::SeqCst), 5);
    assert_eq!(core.attempts.lock().unwrap().as_slice(), &[4]);
    core.fail_detach.store(true, Ordering::SeqCst);
    drop(app.close(CLOSE_DEADLINE));
    assert_eq!(core.attempts.lock().unwrap().as_slice(), &[4, 1, 3, 2]);
    assert!(app
        .set
        .invoke_in_place(|loan| loan.into_initializer().open(true).finish())
        .is_err());
    app.changed
        .notify_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    let client = binding.set_client().await.unwrap();
    let pending = client
        .invoke_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    assert_eq!(core.deliveries.load(Ordering::SeqCst), 2);
    assert_eq!(core.pool.free_slots(), 16);
    core.fail_detach.store(false, Ordering::SeqCst);
    assert_eq!(
        bounded(app.close(CLOSE_DEADLINE)).await,
        Completion::Succeeded
    );
    assert_eq!(
        core.attempts.lock().unwrap().as_slice(),
        &[4, 1, 3, 2, 1, 3, 2]
    );
    assert!(observed.lock().unwrap().is_empty());
    assert_eq!(app.server.stats().responded.load(Ordering::SeqCst), 0);
    drop(pending);
    client.close().unwrap();
    assert_eq!(
        core.attempts.lock().unwrap().as_slice(),
        &[4, 1, 3, 2, 1, 3, 2, 5]
    );
    assert_eq!(core.pool.free_slots(), 16);
}
