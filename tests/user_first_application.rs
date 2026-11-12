//! Owning semantic fixtures. Pool/loopback checks do not qualify middleware-native
//! zero-copy, physical retention, network copies, or allocation acceptance.
#[allow(dead_code)]
#[path = "../examples/support/user_first_door.rs"]
mod door;
#[allow(dead_code)]
#[path = "support/door_factories.rs"]
mod door_factories;
use door::schema::{DoorJson, DoorProtobuf};
use door::*;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use up_rust::{
    communication::{application::*, SubscriptionStatus},
    core::usubscription::{MockUSubscription, USubscription},
    *,
};

fn service() -> Arc<dyn USubscription> {
    let mut service = MockUSubscription::new();
    service
        .expect_subscribe()
        .times(1)
        .returning(|_, _, _| Ok(SubscriptionStatus::Subscribed));
    service.expect_unsubscribe().times(1).returning(|_| Ok(()));
    Arc::new(service)
}

#[derive(Default)]
struct Tracking {
    bus: InMemoryOwnedTransport,
    attached: Arc<AtomicUsize>,
    sent: Mutex<Vec<UOwnedFrame>>,
    fail: AtomicBool,
    fail_detach: Arc<AtomicBool>,
    suspend: AtomicBool,
    broaden: bool,
}
#[async_trait::async_trait]
impl UOwnedTransportImpl for Tracking {
    fn admit_payload_contract(
        &self,
        identity: PayloadIdentity,
        family: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        UOwnedTransport::admit_payload_contract(&self.bus, identity, family)
    }
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.sent.lock().unwrap().push(frame.clone());
        if self.fail.load(Ordering::Relaxed) {
            return Err(UStatus::fail_with_code(
                UCode::Unavailable,
                "injected send refusal",
            ));
        }
        if self.suspend.load(Ordering::Relaxed) {
            std::future::pending::<()>().await;
        }
        self.bus.send_owned(frame).await
    }
    async fn attach_validated_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<OwnedAttachment, UStatus> {
        let source = if self.broaden {
            UUri::any()
        } else {
            source.clone()
        };
        let sink = if self.broaden {
            sink.map(|_| UUri::any())
        } else {
            sink.cloned()
        };
        let mut inner = self
            .bus
            .attach_owned_listener(&source, sink.as_ref(), listener)
            .await?;
        let attached = self.attached.clone();
        let fail_detach = self.fail_detach.clone();
        attached.fetch_add(1, Ordering::Relaxed);
        let mut active = true;
        Ok(OwnedAttachment::new(move || {
            if fail_detach.swap(false, Ordering::AcqRel) {
                return Err(UStatus::fail_with_code(
                    UCode::Unavailable,
                    "injected detach refusal",
                ));
            }
            inner.detach()?;
            if active {
                attached.fetch_sub(1, Ordering::Relaxed);
                active = false;
            }
            Ok(())
        }))
    }
}
fn response(request: &UOwnedFrame, open: bool) -> UOwnedFrame {
    let m = request.metadata();
    let metadata = UFrameMetadata::response(
        m.sink().unwrap().clone(),
        m.source().clone(),
        m.id().clone(),
    )
    .with_payload_encoding(PayloadEncoding::JSON)
    .build()
    .unwrap();
    UOwnedFrame::with_payload(
        metadata,
        DoorJson::encode_payload_owned(&DoorState { open }).unwrap(),
    )
    .unwrap()
}

async fn all_roles(transport: Arc<dyn UOwnedTransport>, encoding: Encoding) {
    let schema = deployment(transport, encoding).unwrap();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut setup = roles::Setup::default();
    start(&mut setup, &schema, service(), observed.clone())
        .await
        .unwrap();
    let mut app = setup.finish().expect("all requested roles acquired");
    assert_eq!(app.subscription.phase(), SubscriptionPhase::Prepared);
    app.activate().await.unwrap();
    let response = app.exercise().await.unwrap();
    let close = app.close(std::time::Duration::from_secs(1)).await;
    assert_eq!(close, Completion::Succeeded);
    assert!(response.payload().unwrap().open);
    assert!(response.context().unwrap().request_id.is_some());
    assert_eq!(observed.lock().unwrap().len(), 2);
    assert_eq!(app.subscription.phase(), SubscriptionPhase::Closed);
}

#[test]
fn missing_schema_choice_is_an_explicit_setup_error() {
    assert!(matches!(
        Encoding::named("infer-from-bytes"),
        Err(SerializedError::Config(_))
    ));
}

struct Refusing;
#[async_trait::async_trait]
impl UOwnedTransportImpl for Refusing {
    fn admit_payload_contract(
        &self,
        _: PayloadIdentity,
        _: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        Err(UWireError::invalid_payload(
            "payload/envelope declaration not admitted",
        ))
    }
    async fn send_validated_owned(&self, _: UOwnedFrame) -> Result<(), UStatus> {
        panic!("setup refusal must precede send")
    }
}
#[test]
fn deployment_refuses_incompatible_context_before_any_role_side_effect() {
    assert!(deployment(Arc::new(Refusing), Encoding::Json).is_err());
}

struct SendOnly;
#[async_trait::async_trait]
impl UOwnedTransportImpl for SendOnly {
    fn admit_payload_contract(
        &self,
        identity: PayloadIdentity,
        family: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        UOwnedTransport::admit_payload_contract(
            &InMemoryOwnedTransport::default(),
            identity,
            family,
        )
    }
    async fn send_validated_owned(&self, _: UOwnedFrame) -> Result<(), UStatus> {
        Ok(())
    }
}

#[cfg(feature = "up-l2-zero-copy")]
#[path = "support/native_fixture.rs"]
mod native_fixture;

#[cfg(feature = "up-l2-zero-copy")]
#[allow(dead_code)]
#[path = "support/native_door_factories.rs"]
mod native_door_factories;

#[cfg(feature = "up-l2-zero-copy")]
mod native_binding {
    use super::*;
    use door::native::{NativeDoor, NativeDoorSetup};
    use up_rust::communication::{
        typed::NativeConfig,
        zero_copy::{ReceiveConfig, StableTransport},
    };

    fn profile() -> NativeProfile {
        NativeProfile::new(
            "vehicle",
            1,
            PayloadEncoding::private_use(0xF100),
            [
                DoorState::native_representation(),
                SetDoor::native_representation(),
            ],
        )
        .unwrap()
    }
    fn setup() -> (
        Arc<PoolCore>,
        Arc<StableTransport<native_fixture::FixtureCore>>,
    ) {
        let core = Arc::new(
            PoolCore::new(PoolCoreConfig {
                slots: 16,
                slot_bytes: 4096,
                alignment: 8,
                max_routes: 16,
                max_registrations: 8,
            })
            .unwrap(),
        );
        let agreement = NativeProfileAgreement::new(Arc::new(profile()), &profile()).unwrap();
        let transport = Arc::new(UWireTransport::with_native_agreement(
            Arc::new(native_fixture::FixtureCore(core.clone())),
            StableContainerWire,
            UFrameFieldsMetadataCodec,
            agreement,
        ));
        (core, transport)
    }
    fn config() -> NativeConfig {
        NativeConfig {
            route: RouteConfig::default(),
            receive: ReceiveConfig {
                registration: registration(),
                header: HeaderLimits { max_token_len: 32 },
            },
        }
    }
    fn options() -> DoorOptions {
        DoorOptions {
            publish: PublishOptions::default(),
            rpc: rpc_options(),
            server: ServerOptions { max_clients: 4 },
        }
    }

    #[tokio::test]
    async fn actual_binding_creates_and_stores_all_roles_and_retains_rx_after_shutdown() {
        let (core, transport) = setup();
        let binding = NativeDoor::bind(transport, config(), options()).unwrap();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let mut setup = NativeDoorSetup::default();
        door::native::start(&mut setup, &binding, service(), observed.clone())
            .await
            .unwrap();
        let mut app = setup.finish().expect("all requested native roles acquired");
        app.activate().await.unwrap();
        let response = app.exercise().await.unwrap();
        assert_eq!(
            app.close(std::time::Duration::from_secs(1)).await,
            Completion::Succeeded
        );
        assert!(response.payload().unwrap().open);
        assert!(response.context().unwrap().native_type_token.is_some());
        assert!(response.context().unwrap().request_id.is_some());
        assert_eq!(observed.lock().unwrap().len(), 2);
        drop(response);
        assert_eq!(core.free_slots(), 16);
    }

    #[tokio::test]
    async fn native_pending_drop_and_client_close_conserve_slots() {
        let (core, transport) = setup();
        let binding = NativeDoor::bind(transport, config(), options()).unwrap();
        let client = binding.set_client().await.unwrap();
        let pending = client
            .invoke_in_place(|loan| loan.into_initializer().open(true).finish())
            .unwrap();
        assert_eq!(client.in_flight(), 1);
        drop(pending);
        assert_eq!(client.in_flight(), 0);
        let pending = client
            .invoke_in_place(|loan| loan.into_initializer().open(false).finish())
            .unwrap();
        client.close().unwrap();
        assert!(pending.response().await.is_err());
        assert_eq!(client.in_flight(), 0);
        drop(client);
        assert_eq!(core.free_slots(), 16);
    }
}
#[tokio::test]
async fn missing_scoped_receive_refuses_without_touching_remote_service() {
    let schema = deployment(Arc::new(SendOnly), Encoding::Json).unwrap();
    let service = Arc::new(MockUSubscription::new());
    let result = schema
        .state_subscription(service, DoorObserver(Arc::new(Mutex::new(Vec::new()))))
        .await;
    assert!(matches!(result, Err(SerializedError::Transport(_))));
}

#[tokio::test]
async fn ordinary_heap_domain_values_do_not_require_a_native_profile() {
    use protobuf::well_known_types::wrappers::StringValue;
    struct Capture(Arc<Mutex<Vec<String>>>);
    impl Handler<StringValue> for Capture {
        async fn on_receive<Owner: Delivery<StringValue>>(&mut self, delivery: Owner) {
            tokio::task::yield_now().await;
            self.0
                .lock()
                .unwrap()
                .push(delivery.payload().unwrap().value.clone());
        }
    }
    let transport = Arc::new(InMemoryOwnedTransport::default());
    let (device, panel) = uris();
    let sender = Deployment::new(
        transport.clone(),
        device.clone(),
        PayloadDecodeLimit::new(64),
        registration(),
        32,
    )
    .unwrap();
    let receiver = Deployment::new(
        transport,
        panel,
        PayloadDecodeLimit::new(64),
        registration(),
        32,
    )
    .unwrap();
    let values = Arc::new(Mutex::new(Vec::new()));
    let mut ticket = receiver
        .subscriber(service())
        .subscription(
            device.get_resource_uri(0x8001),
            Capture(values.clone()),
            Decoder::from_schema_mapping::<ProtobufPayload>().unwrap(),
        )
        .await
        .unwrap();
    ticket.activate().await.unwrap();
    let publisher: up_rust::communication::application::Publisher<StringValue> = sender
        .publisher(
            0x8001,
            PublishOptions::default(),
            Encoder::from_schema_mapping::<ProtobufPayload>().unwrap(),
        )
        .unwrap();
    let mut value = StringValue::new();
    value.value = "door open".into();
    publisher.publish(&value).await.unwrap();
    ticket.unsubscribe().await.unwrap();
    assert_eq!(*values.lock().unwrap(), ["door open"]);
}

#[tokio::test]
async fn stored_five_roles_independent_codec_envelope_and_carriage() {
    use up_rust::frame::envelope::{NativeUFrameEnvelope, ProtobufUMessageFrame};
    for encoding in [Encoding::Protobuf, Encoding::Json] {
        all_roles(
            Arc::new(EnvelopeLoop::<ProtobufUMessageFrame>::default()),
            encoding,
        )
        .await;
        all_roles(
            Arc::new(EnvelopeLoop::<NativeUFrameEnvelope>::default()),
            encoding,
        )
        .await;
        all_roles(Arc::new(Tracking::default()), encoding).await;
    }
}

#[tokio::test]
async fn request_and_response_have_independent_admitted_mappings() {
    let transport = Arc::new(Tracking::default());
    let (device, panel) = uris();
    let device_setup = Deployment::new(
        transport.clone(),
        device.clone(),
        PayloadDecodeLimit::new(1024),
        registration(),
        32,
    )
    .unwrap();
    let panel_setup = Deployment::new(
        transport.clone(),
        panel,
        PayloadDecodeLimit::new(1024),
        registration(),
        32,
    )
    .unwrap();
    let mut server = device_setup
        .rpc_server(
            0x42,
            DoorController,
            |open| DoorState { open },
            ServerOptions { max_clients: 2 },
            Decoder::<SetDoor>::from_schema_mapping::<DoorProtobuf>().unwrap(),
            Encoder::<DoorState>::from_schema_mapping::<DoorJson>().unwrap(),
        )
        .await
        .unwrap();
    let client: up_rust::communication::application::RpcClient<SetDoor, DoorState> = panel_setup
        .rpc_client(
            device.get_resource_uri(0x42),
            rpc_options(),
            Encoder::from_schema_mapping::<DoorProtobuf>().unwrap(),
            Decoder::from_schema_mapping::<DoorJson>().unwrap(),
        )
        .await
        .unwrap();
    client.set_token("door-control").unwrap();
    let response = client
        .invoke(&SetDoor { open: true })
        .await
        .unwrap()
        .response()
        .await
        .unwrap();
    assert!(response.payload().unwrap().open);
    server.stop().await.unwrap();
    client.close().unwrap();
    let frames = transport.sent.lock().unwrap();
    assert_eq!(
        frames.first().unwrap().metadata().payload_encoding(),
        Some(&PayloadEncoding::PROTOBUF)
    );
    assert_eq!(
        frames.last().unwrap().metadata().payload_encoding(),
        Some(&PayloadEncoding::JSON)
    );
}

#[tokio::test]
async fn receiver_and_server_detach_failure_remains_retryable() {
    let transport = Arc::new(Tracking::default());
    let schema = deployment(transport.clone(), Encoding::Json).unwrap();
    let mut receiver = schema
        .changed_receiver(DoorObserver(Arc::new(Mutex::new(Vec::new()))))
        .await
        .unwrap();
    transport.fail_detach.store(true, Ordering::Release);
    assert!(receiver.stop().await.unwrap_err().detach.is_some());
    assert_eq!(transport.attached.load(Ordering::Acquire), 1);
    receiver.stop().await.unwrap();
    assert_eq!(transport.attached.load(Ordering::Acquire), 0);
    let mut server = schema.set_server(DoorController).await.unwrap();
    transport.fail_detach.store(true, Ordering::Release);
    assert!(server.stop().await.unwrap_err().detach.is_some());
    server.stop().await.unwrap();
    assert_eq!(transport.attached.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn retry_does_not_erase_terminal_worker_failure() {
    struct Panicking(Arc<tokio::sync::Notify>);
    impl Handler<DoorState> for Panicking {
        async fn on_receive<Owner: Delivery<DoorState>>(&mut self, _: Owner) {
            self.0.notify_one();
            panic!("injected business handler failure");
        }
    }
    let schema = deployment(Arc::new(InMemoryOwnedTransport::default()), Encoding::Json).unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let mut receiver = schema
        .changed_receiver(Panicking(entered.clone()))
        .await
        .unwrap();
    schema
        .changed_notifier()
        .unwrap()
        .notify(&DoorState { open: true })
        .await
        .unwrap();
    entered.notified().await;
    assert!(receiver.stop().await.unwrap_err().worker.is_some());
    assert!(receiver.stop().await.unwrap_err().worker.is_some());
}

#[tokio::test]
async fn subscription_detach_retry_retains_remote_success_and_local_ticket() {
    let transport = Arc::new(Tracking::default());
    let schema = deployment(transport.clone(), Encoding::Json).unwrap();
    let mut ticket = schema
        .state_subscription(service(), DoorObserver(Arc::new(Mutex::new(Vec::new()))))
        .await
        .unwrap();
    ticket.activate().await.unwrap();
    transport.fail_detach.store(true, Ordering::Release);
    assert!(ticket.unsubscribe().await.unwrap_err().detach.is_some());
    assert_eq!(ticket.phase(), SubscriptionPhase::Closing);
    ticket.unsubscribe().await.unwrap();
    assert_eq!(ticket.phase(), SubscriptionPhase::Closed);
    assert_eq!(transport.attached.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn status_only_failure_is_not_decoded_as_a_domain_value() {
    let transport = Arc::new(Tracking::default());
    let schema = deployment(transport, Encoding::Json).unwrap();
    let mut server = schema.set_server(DoorController).await.unwrap();
    let client = schema.set_client().await.unwrap();
    let result = client
        .invoke(&SetDoor { open: true })
        .await
        .unwrap()
        .response()
        .await;
    assert!(matches!(
        result,
        Err(up_rust::communication::application::CallError::Status(
            UCode::PermissionDenied
        ))
    ));
    assert_eq!(client.in_flight(), 0);
    server.stop().await.unwrap();
    client.close().unwrap();
}

#[tokio::test]
async fn client_close_and_timeout_release_borrowed_pending_slots() {
    let transport = Arc::new(Tracking::default());
    let (device, panel) = uris();
    let setup = Deployment::new(
        transport.clone(),
        panel,
        PayloadDecodeLimit::new(1024),
        registration(),
        32,
    )
    .unwrap();
    let client: up_rust::communication::application::RpcClient<SetDoor, DoorState> = setup
        .rpc_client(
            device.get_resource_uri(0x42),
            RpcOptions {
                ttl: std::time::Duration::from_millis(5),
                ..rpc_options()
            },
            Encoder::from_schema_mapping::<DoorJson>().unwrap(),
            Decoder::from_schema_mapping::<DoorJson>().unwrap(),
        )
        .await
        .unwrap();
    let pending = client.invoke(&SetDoor { open: true }).await.unwrap();
    assert!(matches!(
        pending.response().await,
        Err(up_rust::communication::application::CallError::TimedOut)
    ));
    assert_eq!(client.in_flight(), 0);
    let pending = client.invoke(&SetDoor { open: true }).await.unwrap();
    client.close().unwrap();
    assert!(matches!(
        pending.response().await,
        Err(up_rust::communication::application::CallError::Closed)
    ));
    assert_eq!(client.in_flight(), 0);
}

#[tokio::test]
async fn wrong_identity_and_full_key_do_not_consume_a_reserved_wait() {
    let transport = Arc::new(Tracking {
        broaden: true,
        ..Tracking::default()
    });
    let schema = deployment(transport.clone(), Encoding::Json).unwrap();
    let client = schema.set_client().await.unwrap();
    let pending = client.invoke(&SetDoor { open: true }).await.unwrap();
    let request = transport.sent.lock().unwrap().last().unwrap().clone();
    let metadata = request.metadata();
    let rejected_before = client.stats().rejected.load(Ordering::Acquire);
    let wrong_identity = UOwnedFrame::with_payload(
        UFrameMetadata::response(
            metadata.sink().unwrap().clone(),
            metadata.source().clone(),
            metadata.id().clone(),
        )
        .with_payload_encoding(PayloadEncoding::PROTOBUF)
        .build()
        .unwrap(),
        DoorProtobuf::encode_payload_owned(&DoorState { open: true }).unwrap(),
    )
    .unwrap();
    transport.bus.send_owned(wrong_identity).await.unwrap();
    let (device, _) = uris();
    let wrong_source = UOwnedFrame::with_payload(
        UFrameMetadata::response(
            device.get_resource_uri(0x43),
            metadata.source().clone(),
            metadata.id().clone(),
        )
        .with_payload_encoding(PayloadEncoding::JSON)
        .build()
        .unwrap(),
        DoorJson::encode_payload_owned(&DoorState { open: true }).unwrap(),
    )
    .unwrap();
    transport.bus.send_owned(wrong_source).await.unwrap();
    assert_eq!(client.in_flight(), 1);
    assert_eq!(
        client.stats().rejected.load(Ordering::Acquire),
        rejected_before + 2
    );
    transport
        .bus
        .send_owned(response(&request, true))
        .await
        .unwrap();
    assert!(pending.response().await.unwrap().payload().unwrap().open);
    client.close().unwrap();
}

#[tokio::test]
async fn pending_is_armed_before_send_reversed_responses_and_drop_release_slots() {
    let transport = Arc::new(Tracking::default());
    let schema = deployment(transport.clone(), Encoding::Json).unwrap();
    let client = schema.set_client().await.unwrap();
    client.set_token("door-control").unwrap();
    assert!(client.set_token(&"x".repeat(33)).is_err());
    let first = client.invoke(&SetDoor { open: false }).await.unwrap();
    let second = client.invoke(&SetDoor { open: true }).await.unwrap();
    assert_eq!(client.in_flight(), 2);
    let requests = transport.sent.lock().unwrap().clone();
    assert_eq!(
        requests.first().unwrap().metadata().token(),
        Some("door-control")
    );
    transport
        .bus
        .send_owned(response(requests.get(1).unwrap(), true))
        .await
        .unwrap();
    transport
        .bus
        .send_owned(response(requests.first().unwrap(), false))
        .await
        .unwrap();
    assert!(!first.response().await.unwrap().payload().unwrap().open);
    assert!(second.response().await.unwrap().payload().unwrap().open);
    assert_eq!(client.in_flight(), 0);
    let wait = client.invoke(&SetDoor { open: true }).await.unwrap();
    let last = transport.sent.lock().unwrap().last().unwrap().clone();
    drop(wait);
    assert_eq!(client.in_flight(), 0);
    transport
        .bus
        .send_owned(response(&last, true))
        .await
        .unwrap();
    assert_eq!(client.stats().unmatched.load(Ordering::Relaxed), 1);
    client.close().unwrap();
    assert_eq!(transport.attached.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn failed_or_cancelled_submission_releases_pending_and_drop_detaches() {
    let transport = Arc::new(Tracking::default());
    let schema = deployment(transport.clone(), Encoding::Json).unwrap();
    let client = schema.set_client().await.unwrap();
    transport.fail.store(true, Ordering::Relaxed);
    assert!(client.invoke(&SetDoor { open: true }).await.is_err());
    assert_eq!(client.in_flight(), 0);
    transport.fail.store(false, Ordering::Relaxed);
    transport.suspend.store(true, Ordering::Relaxed);
    assert!(tokio::time::timeout(
        std::time::Duration::from_millis(1),
        client.invoke(&SetDoor { open: true })
    )
    .await
    .is_err());
    assert_eq!(client.in_flight(), 0);
    drop(client);
    assert_eq!(transport.attached.load(Ordering::Relaxed), 0);
}

/// Demonstration-only carriage that exercises actual whole-frame envelope bytes.
/// It copies/allocates normally; it is never a native capability or physical proof.
#[derive(Debug)]
pub(crate) struct EnvelopeLoop<E> {
    bus: up_rust::InMemoryOwnedTransport,
    envelope: std::marker::PhantomData<fn() -> E>,
}
impl<E> Default for EnvelopeLoop<E> {
    fn default() -> Self {
        Self {
            bus: up_rust::InMemoryOwnedTransport::default(),
            envelope: std::marker::PhantomData,
        }
    }
}
#[async_trait::async_trait]
impl<E: up_rust::UFrameEnvelope> up_rust::UOwnedTransportImpl for EnvelopeLoop<E> {
    fn admit_payload_contract(
        &self,
        identity: up_rust::PayloadIdentity,
        family: Option<up_rust::WireIdentity>,
    ) -> Result<(), UWireError> {
        UOwnedTransport::admit_payload_contract(&self.bus, identity, family)
    }
    async fn send_validated_owned(
        &self,
        frame: up_rust::UOwnedFrame,
    ) -> Result<(), up_rust::UStatus> {
        let bytes = E::serialize_frame(&frame)
            .map_err(|e| up_rust::UStatus::fail_with_code(UCode::InvalidArgument, e.to_string()))?;
        let decoded = E::deserialize_frame(&bytes)
            .map_err(|e| up_rust::UStatus::fail_with_code(UCode::InvalidArgument, e.to_string()))?;
        self.bus.send_owned(decoded).await
    }
    async fn attach_validated_owned_listener(
        &self,
        source: &up_rust::UUri,
        sink: Option<&up_rust::UUri>,
        listener: Arc<dyn up_rust::UOwnedListener>,
    ) -> Result<up_rust::OwnedAttachment, up_rust::UStatus> {
        self.bus.attach_owned_listener(source, sink, listener).await
    }
}

struct InterruptedService {
    subscribed: AtomicBool,
    stall_close: AtomicBool,
}
#[async_trait::async_trait]
impl USubscription for InterruptedService {
    async fn subscribe(
        &self,
        _: &UUri,
        _: Option<u64>,
        _: Option<u32>,
    ) -> Result<SubscriptionStatus, UStatus> {
        self.subscribed.store(true, Ordering::Release);
        std::future::pending().await
    }
    async fn unsubscribe(&self, _: &UUri) -> Result<(), UStatus> {
        if self.stall_close.load(Ordering::Acquire) {
            std::future::pending::<()>().await;
        }
        self.subscribed.store(false, Ordering::Release);
        Ok(())
    }
    async fn fetch_subscriptions_by_topic(
        &self,
        _: &UUri,
    ) -> Result<Vec<up_rust::core::usubscription::SubscriptionInfo>, UStatus> {
        unreachable!()
    }
    async fn fetch_subscriptions_by_subscriber(
        &self,
        _: &UUri,
    ) -> Result<Vec<up_rust::core::usubscription::SubscriptionInfo>, UStatus> {
        unreachable!()
    }
    async fn register_for_notifications(&self, _: &UUri) -> Result<(), UStatus> {
        unreachable!()
    }
    async fn unregister_for_notifications(&self, _: &UUri) -> Result<(), UStatus> {
        unreachable!()
    }
    async fn fetch_subscribers(&self, _: &UUri) -> Result<Vec<UUri>, UStatus> {
        unreachable!()
    }
    async fn reset(
        &self,
        _: up_rust::core::usubscription::ResetReason,
        _: Option<String>,
    ) -> Result<(), UStatus> {
        unreachable!()
    }
}
#[tokio::test]
async fn subscription_ticket_survives_remote_cancellation_and_closes_before_remote_await() {
    let transport = Arc::new(Tracking::default());
    let schema = deployment(transport.clone(), Encoding::Json).unwrap();
    let remote = Arc::new(InterruptedService {
        subscribed: AtomicBool::new(false),
        stall_close: AtomicBool::new(true),
    });
    let mut ticket = schema
        .state_subscription(
            remote.clone(),
            DoorObserver(Arc::new(Mutex::new(Vec::new()))),
        )
        .await
        .unwrap();
    assert_eq!(ticket.phase(), SubscriptionPhase::Prepared);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(1), ticket.activate())
            .await
            .is_err()
    );
    assert!(remote.subscribed.load(Ordering::Acquire));
    assert_eq!(ticket.phase(), SubscriptionPhase::RemoteUnknown);
    assert!(matches!(
        ticket.activate().await,
        Err(SubscriptionActivationError::ReconciliationRequired)
    ));
    ticket.reconcile(SubscriptionStatus::Subscribed);
    let closing = ticket.unsubscribe();
    assert_eq!(transport.attached.load(Ordering::Relaxed), 0);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(1), closing)
            .await
            .is_err()
    );
    assert_eq!(ticket.phase(), SubscriptionPhase::Closing);
    remote.stall_close.store(false, Ordering::Release);
    ticket.unsubscribe().await.unwrap();
    assert_eq!(ticket.phase(), SubscriptionPhase::Closed);
    assert!(!remote.subscribed.load(Ordering::Acquire));
}
