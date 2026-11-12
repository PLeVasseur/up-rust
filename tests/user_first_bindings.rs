//! Source fixtures only: no middleware, zero-copy or allocation acceptance claim.
#[allow(dead_code)]
#[path = "../examples/support/user_first_door.rs"]
mod door;
#[allow(dead_code)]
#[path = "../examples/support/user_first_meter.rs"]
mod meter;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use up_rust::communication::application::{Receiver, RpcServer, Subscription};
use up_rust::{
    communication::{application::*, SubscriptionStatus},
    core::usubscription::{MockUSubscription, USubscription},
    *,
};

#[derive(Default)]
struct Carriage {
    bus: InMemoryOwnedTransport,
    frames: Mutex<Vec<UOwnedFrame>>,
    fail_detach: Arc<AtomicBool>,
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
        self.frames.lock().unwrap().push(frame.clone());
        self.bus.send_owned(frame).await
    }
    async fn attach_validated_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<OwnedAttachment, UStatus> {
        let mut inner = self
            .bus
            .attach_owned_listener(source, sink, listener)
            .await?;
        let fail = self.fail_detach.clone();
        let attached = self.attached.clone();
        attached.fetch_add(1, Ordering::AcqRel);
        let mut active = true;
        Ok(OwnedAttachment::new(move || {
            if fail.swap(false, Ordering::AcqRel) {
                return Err(UStatus::fail_with_code(
                    UCode::Unavailable,
                    "injected exact detach refusal",
                ));
            }
            inner.detach()?;
            if active {
                active = false;
                attached.fetch_sub(1, Ordering::AcqRel);
            }
            Ok(())
        }))
    }
}
fn address(device: bool) -> StaticUriProvider {
    let type_id = if device { 1 } else { 2 };
    StaticUriProvider::for_entity("meter", UEntityId::first_instance(type_id), 1).unwrap()
}
fn setup(transport: Arc<dyn UOwnedTransport>, device: bool) -> Deployment {
    let address = Arc::new(address(device));
    Deployment::new(
        transport,
        address,
        PayloadDecodeLimit::new(1024),
        door::registration(),
        32,
    )
    .unwrap()
}
fn options() -> OperationOptions {
    OperationOptions {
        publish: PublishOptions::default(),
        rpc: door::rpc_options(),
        server: ServerOptions { max_clients: 4 },
    }
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

#[derive(Clone, Copy)]
enum Construction {
    Declared,
    Tables,
    Static,
}
fn static_endpoint<K: PayloadCodec>(
    transport: Arc<Carriage>,
    device: bool,
) -> up_rust::communication::typed::Endpoint<
    up_rust::communication::typed::Serialized<Carriage, K>,
    StaticUriProvider,
> {
    use up_rust::communication::typed::{Endpoint, Serialized};
    Endpoint::new(
        Serialized::<_, K>::new(
            transport,
            PayloadDecodeLimit::new(1024),
            door::registration(),
        )
        .unwrap(),
        Arc::new(address(device)),
    )
}

#[tokio::test]
async fn second_domain_stores_every_role_and_selects_rpc_directions_independently() {
    for profile in [meter::Profile::Protobuf, meter::Profile::Mixed] {
        let transport = Arc::new(Carriage::default());
        let schema = meter::Meter::bind_profiles(
            setup(transport.clone(), true),
            setup(transport.clone(), false),
            profile,
            profile,
            options(),
        )
        .unwrap();
        let (events, notifications) = (Arc::default(), Arc::default());
        let mut setup = meter::MeterSetup::default();
        meter::start(
            &mut setup,
            &schema,
            service(),
            Arc::clone(&events),
            Arc::clone(&notifications),
        )
        .await
        .unwrap();
        let mut app = setup.finish().unwrap();
        app.subscription.activate().await.unwrap();
        app.publisher
            .publish(&meter::Reading { value: 17 })
            .await
            .unwrap();
        app.notifier
            .notify(&meter::Reading { value: 18 })
            .await
            .unwrap();
        let reply = app
            .client
            .invoke(&meter::SetLevel { value: 19 })
            .await
            .unwrap()
            .response()
            .await
            .unwrap();
        assert_eq!(reply.payload().unwrap().value, 19);
        assert_eq!(
            app.close(Duration::from_secs(1)).await,
            up_rust::Completion::Succeeded
        );
        assert_eq!(events.count.load(Ordering::Acquire), 1);
        assert_eq!(events.value.load(Ordering::Acquire), 17);
        assert_eq!(notifications.count.load(Ordering::Acquire), 1);
        assert_eq!(notifications.value.load(Ordering::Acquire), 18);
        assert_eq!(reply.payload().unwrap().value, 19);
        assert_eq!(transport.attached.load(Ordering::Acquire), 0);
        let frames = transport.frames.lock().unwrap();
        let request = frames
            .iter()
            .find(|frame| frame.metadata().kind() == UMessageType::Request)
            .unwrap();
        let response = frames
            .iter()
            .find(|frame| frame.metadata().kind() == UMessageType::Response)
            .unwrap();
        assert_eq!(
            request.metadata().payload_encoding(),
            Some(&PayloadEncoding::PROTOBUF)
        );
        let response_encoding = match profile {
            meter::Profile::Protobuf => PayloadEncoding::PROTOBUF,
            meter::Profile::Mixed => PayloadEncoding::JSON,
        };
        assert_eq!(
            response.metadata().payload_encoding(),
            Some(&response_encoding)
        );
    }
}

#[tokio::test]
async fn all_constructors_share_exact_detach_retry() {
    for construction in [
        Construction::Declared,
        Construction::Tables,
        Construction::Static,
    ] {
        let transport = Arc::new(Carriage::default());
        let device = setup(transport.clone(), true);
        let panel = setup(transport.clone(), false);
        let mut receiver = match construction {
            Construction::Declared => {
                let selected = BoundDeployment::new(panel, meter::Profile::Mixed);
                meter::CHANGED
                    .receiver(&selected, device.uri_provider(), meter::Observer::default())
                    .await
                    .unwrap()
            }
            Construction::Tables => panel
                .listen(
                    device
                        .uri_provider()
                        .get_resource_uri(meter::CHANGED.resource()),
                    meter::Observer::default(),
                    Decoder::from_schema_mapping::<meter::LevelJson>().unwrap(),
                )
                .await
                .unwrap(),
            Construction::Static => static_endpoint::<meter::LevelJson>(transport.clone(), false)
                .listen(
                    device
                        .uri_provider()
                        .get_resource_uri(meter::CHANGED.resource()),
                    meter::Observer::default(),
                )
                .await
                .unwrap(),
        };
        transport.fail_detach.store(true, Ordering::Release);
        assert!(receiver.stop().await.unwrap_err().detach.is_some());
        assert_eq!(transport.attached.load(Ordering::Acquire), 1);
        receiver.stop().await.unwrap();
        assert_eq!(transport.attached.load(Ordering::Acquire), 0);
    }
}

struct Hold {
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
impl Handler<meter::Reading> for Hold {
    async fn on_receive<D: Delivery<meter::Reading>>(&mut self, delivery: D) {
        self.entered.notify_one();
        self.release.notified().await;
        assert_eq!(delivery.payload().unwrap().value, 25);
    }
}
#[tokio::test]
async fn all_constructors_retain_cancelled_worker_drain() {
    for construction in [
        Construction::Declared,
        Construction::Tables,
        Construction::Static,
    ] {
        let transport = Arc::new(Carriage::default());
        let device = BoundDeployment::new(setup(transport.clone(), true), meter::Profile::Mixed);
        let panel = BoundDeployment::new(setup(transport.clone(), false), meter::Profile::Mixed);
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let handler = Hold {
            entered: entered.clone(),
            release: release.clone(),
        };
        let mut receiver = match construction {
            Construction::Declared => meter::CHANGED
                .receiver(&panel, device.address(), handler)
                .await
                .unwrap(),
            Construction::Tables => panel
                .deployment()
                .listen(
                    device.address().get_resource_uri(meter::CHANGED.resource()),
                    handler,
                    Decoder::from_schema_mapping::<meter::LevelJson>().unwrap(),
                )
                .await
                .unwrap(),
            Construction::Static => static_endpoint::<meter::LevelJson>(transport.clone(), false)
                .listen(
                    device.address().get_resource_uri(meter::CHANGED.resource()),
                    handler,
                )
                .await
                .unwrap(),
        };
        meter::CHANGED
            .notifier(
                &device,
                panel.address().get_source_uri(),
                PublishOptions::default(),
            )
            .unwrap()
            .notify(&meter::Reading { value: 25 })
            .await
            .unwrap();
        entered.notified().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(1), receiver.stop())
                .await
                .is_err()
        );
        assert_eq!(transport.attached.load(Ordering::Acquire), 0);
        release.notify_one();
        receiver.stop().await.unwrap();
    }
}

struct PanicController(Arc<tokio::sync::Notify>);
impl up_rust::communication::application::RequestHandler<meter::SetLevel> for PanicController {
    type Reply = u32;
    async fn handle<D: Delivery<meter::SetLevel>>(&mut self, _: D) -> Result<u32, UCode> {
        self.0.notify_one();
        panic!("injected terminal worker failure")
    }
}

#[tokio::test]
async fn all_clients_share_pending_cancellation_and_retained_close() {
    for construction in [
        Construction::Declared,
        Construction::Tables,
        Construction::Static,
    ] {
        let transport = Arc::new(Carriage::default());
        let device = BoundDeployment::new(setup(transport.clone(), true), meter::Profile::Protobuf);
        let panel = BoundDeployment::new(setup(transport.clone(), false), meter::Profile::Protobuf);
        let client = match construction {
            Construction::Declared => meter::SET
                .client(&panel, device.address(), options().rpc)
                .await
                .unwrap(),
            Construction::Tables => panel
                .deployment()
                .rpc_client(
                    device.address().get_resource_uri(meter::SET.resource()),
                    options().rpc,
                    Encoder::from_schema_mapping::<meter::LevelProtobuf>().unwrap(),
                    Decoder::from_schema_mapping::<meter::LevelProtobuf>().unwrap(),
                )
                .await
                .unwrap(),
            Construction::Static => {
                static_endpoint::<meter::LevelProtobuf>(transport.clone(), false)
                    .rpc_client::<meter::SetLevel, meter::Reading>(
                        device.address().get_resource_uri(meter::SET.resource()),
                        options().rpc,
                    )
                    .await
                    .unwrap()
            }
        };
        let pending = client.invoke(&meter::SetLevel { value: 29 }).await.unwrap();
        assert_eq!(client.in_flight(), 1);
        drop(pending);
        assert_eq!(client.in_flight(), 0);
        transport.fail_detach.store(true, Ordering::Release);
        assert!(client.close().is_err());
        assert_eq!(transport.attached.load(Ordering::Acquire), 1);
        assert!(matches!(
            client.invoke(&meter::SetLevel { value: 30 }).await,
            Err(up_rust::communication::application::CallError::Closed)
        ));
        client.close().unwrap();
        assert_eq!(transport.attached.load(Ordering::Acquire), 0);
    }
}
#[tokio::test]
async fn all_server_constructors_keep_terminal_worker_failure_sticky() {
    for construction in [
        Construction::Declared,
        Construction::Tables,
        Construction::Static,
    ] {
        let transport = Arc::new(Carriage::default());
        let device = BoundDeployment::new(setup(transport.clone(), true), meter::Profile::Protobuf);
        let panel = BoundDeployment::new(setup(transport.clone(), false), meter::Profile::Protobuf);
        let entered = Arc::new(tokio::sync::Notify::new());
        let handler = PanicController(entered.clone());
        let mut server = match construction {
            Construction::Declared => meter::SET
                .server(
                    &device,
                    handler,
                    |value| meter::Reading { value },
                    options().server,
                )
                .await
                .unwrap(),
            Construction::Tables => device
                .deployment()
                .rpc_server(
                    meter::SET.resource(),
                    handler,
                    |value| meter::Reading { value },
                    options().server,
                    Decoder::from_schema_mapping::<meter::LevelProtobuf>().unwrap(),
                    Encoder::from_schema_mapping::<meter::LevelProtobuf>().unwrap(),
                )
                .await
                .unwrap(),
            Construction::Static => {
                static_endpoint::<meter::LevelProtobuf>(transport.clone(), true)
                    .rpc_server::<meter::SetLevel, meter::Reading, _, _>(
                        meter::SET.resource(),
                        handler,
                        |value| meter::Reading { value },
                        options().server,
                    )
                    .await
                    .unwrap()
            }
        };
        let client = meter::SET
            .client(&panel, device.address(), options().rpc)
            .await
            .unwrap();
        server
            .prepare_client(&panel.address().get_source_uri())
            .unwrap();
        let pending = client.invoke(&meter::SetLevel { value: 27 }).await.unwrap();
        entered.notified().await;
        assert!(server.stop().await.unwrap_err().worker.is_some());
        assert!(server.stop().await.unwrap_err().worker.is_some());
        drop(pending);
        client.close().unwrap();
        assert_eq!(transport.attached.load(Ordering::Acquire), 0);
    }
}

#[tokio::test]
async fn cleanup_failure_retains_app_original_operation_and_authoritative_observation() {
    let remote = Arc::new(AtomicBool::new(false));
    let mut service = MockUSubscription::new();
    let subscribed = remote.clone();
    service
        .expect_subscribe()
        .times(1)
        .returning(move |_, _, _| {
            subscribed.store(true, Ordering::Release);
            Err(UStatus::fail_with_code(
                UCode::Unavailable,
                "activation acknowledgement lost",
            ))
        });
    let removed = remote.clone();
    service.expect_unsubscribe().times(1).returning(move |_| {
        removed.store(false, Ordering::Release);
        Err(UStatus::fail_with_code(
            UCode::Unavailable,
            "removal acknowledgement lost",
        ))
    });
    service
        .expect_fetch_subscribers()
        .times(1)
        .returning(move |_| {
            assert!(!remote.load(Ordering::Acquire));
            Ok(Vec::new())
        });
    let service = Arc::new(service);
    let schema = door::deployment(Arc::new(Carriage::default()), door::Encoding::Protobuf).unwrap();
    let mut setup = door::roles::Setup::default();
    door::start(
        &mut setup,
        &schema,
        service.clone(),
        Arc::new(Mutex::new(Vec::new())),
    )
    .await
    .unwrap();
    let app = setup.finish().unwrap();
    let mut retained = door::Recovery::new(app);
    retained.run().await;
    assert!(matches!(
        &retained.operation,
        Some(door::AppError::Activate(
            SubscriptionActivationError::Service(_)
        ))
    ));
    assert_eq!(
        retained.app.close_bounded(1, Duration::from_secs(1)).await,
        Completion::Pending
    );
    assert_eq!(
        retained.app.subscription.phase(),
        SubscriptionPhase::Closing
    );
    retained
        .app
        .subscription
        .observe_remote(Duration::from_secs(1))
        .await;
    assert_eq!(
        retained.app.close_bounded(1, Duration::from_secs(1)).await,
        Completion::Succeeded
    );
    assert_eq!(retained.app.subscription.phase(), SubscriptionPhase::Closed);
    assert!(retained.operation.is_some());
    assert_eq!(retained.app.cleanup.subscription.attempt, 2);
    assert!(retained.app.cleanup.subscription.first_error.is_some());
    assert!(matches!(
        retained.app.cleanup.subscription.current,
        Some(Ok(()))
    ));
    assert!(matches!(
        retained.app.subscription.remote_observations().observed,
        Some(SubscriptionStatus::Unsubscribed)
    ));
}

#[tokio::test]
async fn failed_independent_query_never_assumes_remote_removal() {
    let mut service = MockUSubscription::new();
    service
        .expect_subscribe()
        .times(1)
        .returning(|_, _, _| Ok(SubscriptionStatus::Subscribed));
    let remaining_failures = AtomicUsize::new(2);
    service.expect_unsubscribe().times(3).returning(move |_| {
        if remaining_failures.fetch_sub(1, Ordering::AcqRel) > 0 {
            Err(UStatus::fail_with_code(UCode::Unavailable, "still unknown"))
        } else {
            Ok(())
        }
    });
    service.expect_fetch_subscribers().times(1).returning(|_| {
        Err(UStatus::fail_with_code(
            UCode::Unavailable,
            "query unavailable",
        ))
    });
    let service = Arc::new(service);
    let schema = door::deployment(Arc::new(Carriage::default()), door::Encoding::Protobuf).unwrap();
    let mut setup = door::roles::Setup::default();
    door::start(
        &mut setup,
        &schema,
        service.clone(),
        Arc::new(Mutex::new(Vec::new())),
    )
    .await
    .unwrap();
    let app = setup.finish().unwrap();
    let mut retained = door::Recovery::new(app);
    retained.app.activate().await.unwrap();
    assert_eq!(
        retained.app.close_bounded(1, Duration::from_secs(1)).await,
        Completion::Pending
    );
    retained
        .app
        .subscription
        .observe_remote(Duration::from_secs(1))
        .await;
    assert!(retained
        .app
        .subscription
        .remote_observations()
        .failures
        .first
        .is_some());
    assert_eq!(
        retained.app.close_bounded(1, Duration::from_secs(1)).await,
        Completion::Pending
    );
    assert_eq!(
        retained.app.subscription.phase(),
        SubscriptionPhase::Closing
    );
    assert!(retained.app.subscription.close_outcome().service.is_some());
    assert_eq!(
        retained.app.close_bounded(1, Duration::from_secs(1)).await,
        Completion::Succeeded
    );
    assert_eq!(retained.app.subscription.phase(), SubscriptionPhase::Closed);
}

#[cfg(feature = "up-l2-zero-copy")]
fn native_stored_application_is_send_when_core_contract_supports_it<C: NativeLoanCore + 'static>(
    app: meter::native::MeterApp<C>,
) -> impl std::future::Future<Output = meter::native::MeterApp<C>> + Send {
    // The explicit `+ Send` return bound is the contract under test.
    close_native_meter(app)
}

#[cfg(feature = "up-l2-zero-copy")]
async fn close_native_meter<C: NativeLoanCore + 'static>(
    mut app: meter::native::MeterApp<C>,
) -> meter::native::MeterApp<C> {
    let _report = app.close(Duration::from_secs(2)).await;
    app
}

#[cfg(feature = "up-l2-zero-copy")]
#[path = "support/native_fixture.rs"]
mod native_fixture;

struct SelfStoppingReceiver {
    owner: Arc<tokio::sync::Mutex<Option<Receiver>>>,
    refused: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
impl Handler<meter::Reading> for SelfStoppingReceiver {
    async fn on_receive<D: Delivery<meter::Reading>>(&mut self, delivery: D) {
        let mut owner = self.owner.lock().await;
        let report = owner.as_mut().unwrap().stop().await.unwrap_err();
        assert!(matches!(
            report.worker,
            Some(RegistrationStopError::CalledFromWorker)
        ));
        drop(owner);
        self.refused.notify_one();
        self.release.notified().await;
        assert_eq!(delivery.payload().unwrap().value, 31);
    }
}
#[tokio::test]
async fn all_receivers_retain_join_after_nonterminal_same_worker_refusal() {
    for construction in [
        Construction::Declared,
        Construction::Tables,
        Construction::Static,
    ] {
        let transport = Arc::new(Carriage::default());
        let device = BoundDeployment::new(setup(transport.clone(), true), meter::Profile::Mixed);
        let panel = BoundDeployment::new(setup(transport.clone(), false), meter::Profile::Mixed);
        let owner = Arc::new(tokio::sync::Mutex::new(None));
        let refused = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let handler = SelfStoppingReceiver {
            owner: owner.clone(),
            refused: refused.clone(),
            release: release.clone(),
        };
        let receiver = match construction {
            Construction::Declared => meter::CHANGED
                .receiver(&panel, device.address(), handler)
                .await
                .unwrap(),
            Construction::Tables => panel
                .deployment()
                .listen(
                    device.address().get_resource_uri(meter::CHANGED.resource()),
                    handler,
                    Decoder::from_schema_mapping::<meter::LevelJson>().unwrap(),
                )
                .await
                .unwrap(),
            Construction::Static => static_endpoint::<meter::LevelJson>(transport.clone(), false)
                .listen(
                    device.address().get_resource_uri(meter::CHANGED.resource()),
                    handler,
                )
                .await
                .unwrap(),
        };
        *owner.lock().await = Some(receiver);
        meter::CHANGED
            .notifier(
                &device,
                panel.address().get_source_uri(),
                PublishOptions::default(),
            )
            .unwrap()
            .notify(&meter::Reading { value: 31 })
            .await
            .unwrap();
        refused.notified().await;
        let mut retained = owner.lock().await;
        let receiver = retained.as_mut().unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(1), receiver.stop())
                .await
                .is_err()
        );
        release.notify_one();
        receiver.stop().await.unwrap();
        assert_eq!(transport.attached.load(Ordering::Acquire), 0);
    }
}

struct SelfStoppingSubscription {
    owner: Arc<tokio::sync::Mutex<Option<Subscription>>>,
    refused: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
impl Handler<meter::Reading> for SelfStoppingSubscription {
    async fn on_receive<D: Delivery<meter::Reading>>(&mut self, _: D) {
        let mut owner = self.owner.lock().await;
        let report = owner.as_mut().unwrap().unsubscribe().await.unwrap_err();
        assert!(matches!(
            report.worker,
            Some(RegistrationStopError::CalledFromWorker)
        ));
        drop(owner);
        self.refused.notify_one();
        self.release.notified().await;
    }
}
#[tokio::test]
async fn subscription_refusal_is_observable_but_not_cached_as_terminal() {
    let transport = Arc::new(Carriage::default());
    let device = BoundDeployment::new(setup(transport.clone(), true), meter::Profile::Mixed);
    let panel = BoundDeployment::new(setup(transport.clone(), false), meter::Profile::Mixed);
    let owner = Arc::new(tokio::sync::Mutex::new(None));
    let refused = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let handler = SelfStoppingSubscription {
        owner: owner.clone(),
        refused: refused.clone(),
        release: release.clone(),
    };
    let mut ticket = meter::READING
        .subscription(&panel, device.address(), service(), handler)
        .await
        .unwrap();
    ticket.activate().await.unwrap();
    *owner.lock().await = Some(ticket);
    meter::READING
        .publisher(&device, PublishOptions::default())
        .unwrap()
        .publish(&meter::Reading { value: 32 })
        .await
        .unwrap();
    refused.notified().await;
    let mut retained = owner.lock().await;
    let ticket = retained.as_mut().unwrap();
    assert_eq!(ticket.phase(), SubscriptionPhase::Closing);
    assert!(matches!(
        ticket.close_outcome().worker,
        Some(RegistrationStopError::CalledFromWorker)
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(1), ticket.unsubscribe())
            .await
            .is_err()
    );
    release.notify_one();
    ticket.unsubscribe().await.unwrap();
    assert_eq!(ticket.phase(), SubscriptionPhase::Closed);
    assert!(ticket.close_outcome().worker.is_none());
    assert_eq!(transport.attached.load(Ordering::Acquire), 0);
}

struct SelfStoppingServer {
    owner: Arc<tokio::sync::Mutex<Option<RpcServer>>>,
    refused: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
impl up_rust::communication::application::RequestHandler<meter::SetLevel> for SelfStoppingServer {
    type Reply = u32;
    async fn handle<D: Delivery<meter::SetLevel>>(&mut self, delivery: D) -> Result<u32, UCode> {
        let mut owner = self.owner.lock().await;
        let report = owner.as_mut().unwrap().stop().await.unwrap_err();
        assert!(matches!(
            report.worker,
            Some(RegistrationStopError::CalledFromWorker)
        ));
        drop(owner);
        self.refused.notify_one();
        self.release.notified().await;
        Ok(delivery
            .payload()
            .map_err(|_| UCode::InvalidArgument)?
            .value)
    }
}
#[tokio::test]
async fn server_preserves_join_and_reply_after_same_worker_refusal() {
    let transport = Arc::new(Carriage::default());
    let device = BoundDeployment::new(setup(transport.clone(), true), meter::Profile::Mixed);
    let panel = BoundDeployment::new(setup(transport.clone(), false), meter::Profile::Mixed);
    let owner = Arc::new(tokio::sync::Mutex::new(None));
    let refused = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let handler = SelfStoppingServer {
        owner: owner.clone(),
        refused: refused.clone(),
        release: release.clone(),
    };
    let server = meter::SET
        .server(
            &device,
            handler,
            |value| meter::Reading { value },
            options().server,
        )
        .await
        .unwrap();
    server
        .prepare_client(&panel.address().get_source_uri())
        .unwrap();
    *owner.lock().await = Some(server);
    let client = meter::SET
        .client(&panel, device.address(), options().rpc)
        .await
        .unwrap();
    let pending = client.invoke(&meter::SetLevel { value: 33 }).await.unwrap();
    refused.notified().await;
    let mut retained = owner.lock().await;
    let server = retained.as_mut().unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(1), server.stop())
            .await
            .is_err()
    );
    release.notify_one();
    server.stop().await.unwrap();
    assert_eq!(
        pending.response().await.unwrap().payload().unwrap().value,
        33
    );
    client.close().unwrap();
    assert_eq!(transport.attached.load(Ordering::Acquire), 0);
}

#[cfg(feature = "up-l2-zero-copy")]
#[test]
fn native_stored_application_and_cleanup_future_have_send_contracts() {
    fn require_send<Value: Send>() {}
    require_send::<meter::native::MeterApp<native_fixture::FixtureCore>>();
    let _factory = native_stored_application_is_send_when_core_contract_supports_it::<
        native_fixture::FixtureCore,
    >;
}
