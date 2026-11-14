//! Owning semantic fixtures. Pool/loopback checks do not qualify middleware-native
//! zero-copy, physical retention, network copies, or allocation acceptance.
use up_rust::prelude::*;
#[allow(dead_code)]
#[path = "../examples/support/door.rs"]
mod door;
use door::*;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use up_rust::{
    communication::{typed::*, SubscriptionStatus},
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
fn serialized<T: UOwnedTransport + 'static>(transport: Arc<T>) -> Serialized<T, DoorJson> {
    Serialized::new(transport, PayloadDecodeLimit::new(1024), registration()).unwrap()
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

#[tokio::test]
async fn all_five_roles_same_business_logic_two_codecs_and_two_transport_types() {
    let mut a =
        serialized_flow::<_, DoorProtobuf>(Arc::new(InMemoryOwnedTransport::default()), service())
            .await
            .unwrap();
    let transport = Arc::new(Tracking::default());
    let mut b = serialized_flow::<_, DoorJson>(transport.clone(), service())
        .await
        .unwrap();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, [false, true]);
    assert_eq!(a, b);
    assert_eq!(transport.attached.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn pending_is_armed_before_send_reversed_responses_and_drop_release_slots() {
    let transport = Arc::new(Tracking::default());
    let (door, panel) = uris();
    let endpoint = Endpoint::new(serialized(transport.clone()), panel);
    let client = endpoint
        .rpc_client::<SetDoor, DoorState>(door.get_resource_uri(0x42), rpc_options())
        .await
        .unwrap();
    client.set_token("door-control").unwrap();
    assert!(client.set_token(&"x".repeat(33)).is_err());
    let a = client.invoke(&SetDoor { open: false }).await.unwrap();
    let b = client.invoke(&SetDoor { open: true }).await.unwrap();
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
    assert!(!a.response().await.unwrap().payload().unwrap().open);
    assert!(b.response().await.unwrap().payload().unwrap().open);
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
    let (door, panel) = uris();
    let endpoint = Endpoint::new(serialized(transport.clone()), panel);
    let client = endpoint
        .rpc_client::<SetDoor, DoorState>(door.get_resource_uri(0x42), rpc_options())
        .await
        .unwrap();
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

#[tokio::test]
async fn wrong_codec_response_cannot_consume_a_correlated_wait() {
    let transport = Arc::new(Tracking::default());
    let (door, panel) = uris();
    let endpoint = Endpoint::new(serialized(transport.clone()), panel);
    let client = endpoint
        .rpc_client::<SetDoor, DoorState>(door.get_resource_uri(0x42), rpc_options())
        .await
        .unwrap();
    let pending = client.invoke(&SetDoor { open: true }).await.unwrap();
    let request = transport.sent.lock().unwrap().first().unwrap().clone();
    let m = request.metadata();
    let wrong = UOwnedFrame::with_payload(
        UFrameMetadata::response(
            m.sink().unwrap().clone(),
            m.source().clone(),
            m.id().clone(),
        )
        .with_payload_encoding(PayloadEncoding::PROTOBUF)
        .build()
        .unwrap(),
        DoorProtobuf::encode_payload_owned(&DoorState { open: true }).unwrap(),
    )
    .unwrap();
    transport.bus.send_owned(wrong).await.unwrap();
    assert_eq!(client.in_flight(), 1);
    assert_eq!(client.stats().rejected.load(Ordering::Relaxed), 1);
    transport
        .bus
        .send_owned(response(&request, true))
        .await
        .unwrap();
    assert!(pending.response().await.unwrap().payload().unwrap().open);
}

#[tokio::test]
async fn exact_notification_address_and_codec_checks_survive_broad_physical_filters() {
    let transport = Arc::new(Tracking {
        broaden: true,
        ..Tracking::default()
    });
    let (door, panel) = uris();
    let endpoint = Endpoint::new(serialized(transport.clone()), panel.clone());
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut receiver = endpoint
        .listen::<DoorState, _>(
            door.get_resource_uri(0x8002),
            DoorObserver(observed.clone()),
        )
        .await
        .unwrap();
    let other = StaticUriProvider::new("vehicle", 0x0001_0003, 1).unwrap();
    for (source, sink, codec) in [
        (
            door.get_resource_uri(0x8002),
            other.get_source_uri(),
            PayloadEncoding::JSON,
        ),
        (
            door.get_resource_uri(0x8003),
            panel.get_source_uri(),
            PayloadEncoding::JSON,
        ),
        (
            door.get_resource_uri(0x8002),
            panel.get_source_uri(),
            PayloadEncoding::PROTOBUF,
        ),
    ] {
        let frame = UOwnedFrame::with_payload(
            UFrameMetadata::notification(source, sink)
                .with_payload_encoding(codec)
                .build()
                .unwrap(),
            bytes::Bytes::from_static(b"{\"open\":true}"),
        )
        .unwrap();
        transport.send_owned(frame).await.unwrap();
    }
    receiver.stop().await.unwrap();
    assert!(observed.lock().unwrap().is_empty());
    assert_eq!(transport.attached.load(Ordering::Relaxed), 0);
}

struct SendOnly;
#[async_trait::async_trait]
impl UOwnedTransportImpl for SendOnly {
    fn admit_payload_contract(
        &self,
        identity: PayloadIdentity,
        _family: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        if matches!(identity, PayloadIdentity::Fixed(_)) {
            Ok(())
        } else {
            Err(UWireError::invalid_payload("native refused"))
        }
    }
    async fn send_validated_owned(&self, _: UOwnedFrame) -> Result<(), UStatus> {
        Ok(())
    }
}
#[tokio::test]
async fn missing_scoped_receive_capability_refuses_before_service_mutation() {
    let (door, panel) = uris();
    let endpoint = Endpoint::new(serialized(Arc::new(SendOnly)), panel);
    let mut untouched = MockUSubscription::new();
    untouched.expect_subscribe().times(0);
    assert!(endpoint
        .subscriber(Arc::new(untouched))
        .subscription::<DoorState, _>(
            door.get_resource_uri(0x8001),
            DoorObserver(Arc::new(Mutex::new(Vec::new())))
        )
        .await
        .is_err());
    assert!(endpoint
        .rpc_client::<SetDoor, DoorState>(door.get_resource_uri(0x42), rpc_options())
        .await
        .is_err());
}

#[tokio::test]
async fn server_preserves_status_and_client_capacity_and_drop_detaches() {
    let transport = Arc::new(Tracking::default());
    let (door, panel) = uris();
    let server_ep = Endpoint::new(serialized(transport.clone()), door.clone());
    let client_ep = Endpoint::new(serialized(transport.clone()), panel.clone());
    let server = server_ep
        .rpc_server::<SetDoor, DoorState, _, _>(
            0x42,
            DoorController,
            |open| DoorState { open },
            ServerOptions { max_clients: 1 },
        )
        .await
        .unwrap();
    server.prepare_client(&panel.get_source_uri()).unwrap();
    assert!(server
        .prepare_client(
            &StaticUriProvider::new("vehicle", 0x0001_0003, 1)
                .unwrap()
                .get_source_uri()
        )
        .is_err());
    let client = client_ep
        .rpc_client::<SetDoor, DoorState>(door.get_resource_uri(0x42), rpc_options())
        .await
        .unwrap();
    let wait = client.invoke(&SetDoor { open: true }).await.unwrap();
    assert!(matches!(
        wait.response().await,
        Err(SerializedCallError::Status(UCode::PermissionDenied))
    ));
    drop(server);
    drop(client);
    assert_eq!(transport.attached.load(Ordering::Relaxed), 0);
}

#[test]
fn classic_serialized_projection_preserves_payload() {
    let (door, _) = uris();
    let state = DoorState { open: true };
    let bytes = DoorJson::encode_payload_owned(&state).unwrap();
    let frame = UOwnedFrame::with_payload(
        UFrameMetadata::publish(door.get_resource_uri(0x8001))
            .with_payload_encoding(PayloadEncoding::JSON)
            .build()
            .unwrap(),
        bytes,
    )
    .unwrap();
    let (metadata, payload) = frame.into_parts();
    let message =
        up_rust::frame::metadata::try_project_frame_to_umessage(metadata, payload).unwrap();
    let decoded: DoorState = DoorJson::decode_payload(&message.payload().unwrap()).unwrap();
    assert_eq!(decoded, state);
}

#[cfg(feature = "up-l2-zero-copy")]
#[path = "support/native_fixture.rs"]
mod fixture;
#[cfg(feature = "up-l2-zero-copy")]
mod native {
    use super::*;
    use up_rust::communication::zero_copy::{ReceiveConfig, StableTransport};
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
    fn config() -> NativeConfig {
        NativeConfig {
            route: RouteConfig::default(),
            receive: ReceiveConfig {
                registration: registration(),
                header: HeaderLimits { max_token_len: 32 },
            },
        }
    }
    fn setup() -> (
        Arc<PoolCore>,
        Arc<StableTransport<super::fixture::FixtureCore>>,
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
        // Independent construction is a local fixture only, not peer provisioning.
        let agreement = NativeProfileAgreement::new(Arc::new(profile()), &profile()).unwrap();
        let transport = Arc::new(UWireTransport::with_native_agreement(
            Arc::new(super::fixture::FixtureCore(core.clone())),
            StableContainerWire,
            UFrameFieldsMetadataCodec,
            agreement,
        ));
        (core, transport)
    }
    #[tokio::test]
    async fn all_roles_share_handlers_without_native_payload_staging() {
        let (core, transport) = setup();
        let mut values = native_flow(transport, service()).await.unwrap();
        values.sort_unstable();
        assert_eq!(values, [false, true]);
        assert_eq!(core.free_slots(), 16);
    }
    #[tokio::test]
    async fn synchronous_submit_wait_cancellation_and_response_storage_survives_shutdown() {
        let (core, transport) = setup();
        let (door, panel) = uris();
        let server_ep = Endpoint::new(
            Native::new(transport.clone(), config()).unwrap(),
            door.clone(),
        );
        let client_ep = Endpoint::new(Native::new(transport.clone(), config()).unwrap(), panel);
        let mut server = server_ep
            .rpc_server::<SetDoor, DoorState, _, _>(
                0x42,
                DoorController,
                |open, init| init.into_initializer().open(open).finish(),
                ServerOptions { max_clients: 1 },
            )
            .await
            .unwrap();
        let client = client_ep
            .rpc_client::<SetDoor, DoorState>(door.get_resource_uri(0x42), rpc_options())
            .await
            .unwrap();
        client.set_token("door-control").unwrap();
        let wait = client
            .invoke_in_place(|loan| loan.into_initializer().open(true).finish())
            .unwrap();
        assert_eq!(client.in_flight(), 1); // Submission happened without polling.
        drop(wait);
        assert_eq!(client.in_flight(), 0);
        let response = client
            .invoke_in_place(|loan| loan.into_initializer().open(true).finish())
            .unwrap()
            .response()
            .await
            .unwrap();
        server.stop().await.unwrap();
        client.close().unwrap();
        drop(client);
        drop(server_ep);
        drop(client_ep);
        drop(transport);
        assert!(response.payload().unwrap().open);
        assert!(core.free_slots() < 16);
        drop(response);
        assert_eq!(core.free_slots(), 16);
    }
    #[tokio::test]
    async fn missing_agreement_and_token_losing_projection_are_explicit_refusals() {
        let local = profile();
        let peer = NativeProfile::new(
            "wrong-domain",
            1,
            PayloadEncoding::private_use(0xF100),
            [
                DoorState::native_representation(),
                SetDoor::native_representation(),
            ],
        )
        .unwrap();
        assert!(NativeProfileAgreement::new(Arc::new(local), &peer).is_err());
        let identity = NativeProfileAgreement::new(Arc::new(profile()), &profile())
            .unwrap()
            .identity_for(&DoorState::native_representation())
            .unwrap();
        let (door, _) = uris();
        let metadata = UFrameMetadata::publish(door.get_resource_uri(0x8001))
            .with_native_payload_identity(identity)
            .build()
            .unwrap();
        let classic =
            ClassicCarriage::new(Arc::new(up_rust::local_transport::LocalTransport::default()));
        let frame =
            UOwnedFrame::with_payload(metadata.clone(), bytes::Bytes::from_static(&[1])).unwrap();
        assert!(classic.send_owned(frame).await.is_err());
        assert!(up_rust::frame::metadata::try_project_frame_to_umessage(
            metadata,
            Some(bytes::Bytes::from_static(&[1]))
        )
        .is_err());
    }
}

#[tokio::test]
async fn all_roles_two_real_payload_codecs_across_classic_and_frame_envelopes() {
    use up_rust::frame::envelope::{NativeUFrameEnvelope, ProtobufUMessageFrame};
    let a = serialized_flow::<_, DoorProtobuf>(
        Arc::new(EnvelopeLoop::<ProtobufUMessageFrame>::default()),
        service(),
    )
    .await
    .unwrap();
    let b = serialized_flow::<_, DoorJson>(
        Arc::new(EnvelopeLoop::<ProtobufUMessageFrame>::default()),
        service(),
    )
    .await
    .unwrap();
    let c = serialized_flow::<_, DoorProtobuf>(
        Arc::new(EnvelopeLoop::<NativeUFrameEnvelope>::default()),
        service(),
    )
    .await
    .unwrap();
    let d = serialized_flow::<_, DoorJson>(
        Arc::new(EnvelopeLoop::<NativeUFrameEnvelope>::default()),
        service(),
    )
    .await
    .unwrap();
    assert_eq!([a.len(), b.len(), c.len(), d.len()], [2; 4]);
}

#[tokio::test]
async fn serialized_heap_owning_values_require_no_native_profile() {
    use protobuf::well_known_types::wrappers::StringValue;
    struct Capture(Arc<Mutex<Vec<String>>>);
    impl Handler<StringValue> for Capture {
        async fn on_receive<D: Delivery<StringValue>>(&mut self, delivery: D) {
            tokio::task::yield_now().await;
            self.0
                .lock()
                .unwrap()
                .push(delivery.payload().unwrap().value.clone());
        }
    }
    let (door, panel) = uris();
    let setup = Serialized::<_, ProtobufPayload>::new(
        Arc::new(InMemoryOwnedTransport::default()),
        PayloadDecodeLimit::new(64),
        registration(),
    )
    .unwrap();
    let sender = Endpoint::new(setup.clone(), door.clone());
    let receiver = Endpoint::new(setup, panel);
    let values = Arc::new(Mutex::new(Vec::new()));
    let mut subscription = receiver
        .subscriber(service())
        .subscription::<StringValue, _>(door.get_resource_uri(0x8001), Capture(values.clone()))
        .await
        .unwrap();
    subscription.activate().await.unwrap();
    let mut value = StringValue::new();
    value.value = "door open".into();
    sender
        .publisher::<StringValue>(0x8001, PublishOptions::default())
        .unwrap()
        .publish(&value)
        .await
        .unwrap();
    subscription.unsubscribe().await.unwrap();
    assert_eq!(*values.lock().unwrap(), ["door open"]);
}

#[tokio::test]
async fn adoption_classic_carriage_uses_all_five_primary_roles_and_two_codecs() {
    let classic = Arc::new(ClassicCarriage::new(Arc::new(
        up_rust::local_transport::LocalTransport::default(),
    )));
    assert_eq!(
        serialized_flow::<_, DoorProtobuf>(classic.clone(), service())
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        serialized_flow::<_, DoorJson>(classic, service())
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn pending_timeout_duplicate_wrong_full_key_and_close_recover_slots() {
    let transport = Arc::new(Tracking {
        broaden: true,
        ..Tracking::default()
    });
    let (door, panel) = uris();
    let endpoint = Endpoint::new(serialized(transport.clone()), panel.clone());
    let client = endpoint
        .rpc_client::<SetDoor, DoorState>(
            door.get_resource_uri(0x42),
            RpcOptions {
                ttl: std::time::Duration::from_millis(5),
                ..rpc_options()
            },
        )
        .await
        .unwrap();
    let wait = client.invoke(&SetDoor { open: true }).await.unwrap();
    let request = transport.sent.lock().unwrap().last().unwrap().clone();
    let wrong = UOwnedFrame::with_payload(
        UFrameMetadata::response(
            door.get_resource_uri(0x43),
            panel.get_source_uri(),
            request.metadata().id().clone(),
        )
        .with_payload_encoding(PayloadEncoding::JSON)
        .build()
        .unwrap(),
        DoorJson::encode_payload_owned(&DoorState { open: true }).unwrap(),
    )
    .unwrap();
    transport.bus.send_owned(wrong).await.unwrap();
    // Broad physical filters deliver our outgoing request too; both it and the wrong source are rejected.
    assert_eq!(client.stats().rejected.load(Ordering::Relaxed), 2);
    assert!(matches!(
        wait.response().await,
        Err(SerializedCallError::TimedOut)
    ));
    assert_eq!(client.in_flight(), 0);
    let wait = client.invoke(&SetDoor { open: true }).await.unwrap();
    let request = transport.sent.lock().unwrap().last().unwrap().clone();
    transport
        .bus
        .send_owned(response(&request, true))
        .await
        .unwrap();
    transport
        .bus
        .send_owned(response(&request, true))
        .await
        .unwrap();
    assert_eq!(client.stats().unmatched.load(Ordering::Relaxed), 1);
    wait.response().await.unwrap();
    let wait = client.invoke(&SetDoor { open: true }).await.unwrap();
    client.close().unwrap();
    assert!(matches!(
        wait.response().await,
        Err(SerializedCallError::Closed)
    ));
    assert_eq!(client.in_flight(), 0);
    assert_eq!(transport.attached.load(Ordering::Relaxed), 0);
}

struct ReplyDuringSend {
    bus: InMemoryOwnedTransport,
}
#[async_trait::async_trait]
impl UOwnedTransportImpl for ReplyDuringSend {
    fn admit_payload_contract(
        &self,
        i: PayloadIdentity,
        f: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        UOwnedTransport::admit_payload_contract(&self.bus, i, f)
    }
    async fn send_validated_owned(&self, request: UOwnedFrame) -> Result<(), UStatus> {
        // Delivery completes before the original send future returns.
        self.bus.send_owned(response(&request, true)).await
    }
    async fn attach_validated_owned_listener(
        &self,
        s: &UUri,
        k: Option<&UUri>,
        l: Arc<dyn UOwnedListener>,
    ) -> Result<OwnedAttachment, UStatus> {
        self.bus.attach_owned_listener(s, k, l).await
    }
}
#[tokio::test]
async fn response_during_send_completion_is_not_lost() {
    let transport = Arc::new(ReplyDuringSend {
        bus: InMemoryOwnedTransport::default(),
    });
    let (door, panel) = uris();
    let endpoint = Endpoint::new(serialized(transport), panel);
    let client = endpoint
        .rpc_client::<SetDoor, DoorState>(door.get_resource_uri(0x42), rpc_options())
        .await
        .unwrap();
    assert!(
        client
            .invoke(&SetDoor { open: true })
            .await
            .unwrap()
            .response()
            .await
            .unwrap()
            .payload()
            .unwrap()
            .open
    );
    assert_eq!(client.in_flight(), 0);
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
    let (door, panel) = uris();
    let endpoint = Endpoint::new(serialized(transport.clone()), panel);
    let remote = Arc::new(InterruptedService {
        subscribed: AtomicBool::new(false),
        stall_close: AtomicBool::new(true),
    });
    let mut ticket = endpoint
        .subscriber(remote.clone())
        .subscription::<DoorState, _>(
            door.get_resource_uri(0x8001),
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
    // Here the fixture independently reports the actual remote subscription state.
    ticket.reconcile(SubscriptionStatus::Subscribed);
    let closing = ticket.unsubscribe();
    assert_eq!(transport.attached.load(Ordering::Relaxed), 0); // Before polling.
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

#[cfg(feature = "wire-implementer-api")]
#[path = "support/owned_context.rs"]
mod owned_context;

struct FrameCapture(Arc<Mutex<Vec<UOwnedFrame>>>);
#[async_trait::async_trait]
impl UOwnedListener for FrameCapture {
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        self.0.lock().unwrap().push(frame);
    }
}
#[tokio::test]
async fn classic_adapter_preserves_absent_empty_and_exact_listener_cleanup() {
    let carriage =
        ClassicCarriage::new(Arc::new(up_rust::local_transport::LocalTransport::default()));
    let (door, _) = uris();
    let source = door.get_resource_uri(0x8001);
    let one = Arc::new(Mutex::new(Vec::new()));
    let two = Arc::new(Mutex::new(Vec::new()));
    let mut a = carriage
        .attach_owned_listener(&source, None, Arc::new(FrameCapture(one.clone())))
        .await
        .unwrap();
    let b = carriage
        .attach_owned_listener(&source, None, Arc::new(FrameCapture(two.clone())))
        .await
        .unwrap();
    let absent =
        UOwnedFrame::without_payload(UFrameMetadata::publish(source.clone()).build().unwrap())
            .unwrap();
    let empty = UOwnedFrame::with_payload(
        UFrameMetadata::publish(source)
            .with_payload_encoding(PayloadEncoding::RAW)
            .build()
            .unwrap(),
        bytes::Bytes::new(),
    )
    .unwrap();
    carriage.send_owned(absent).await.unwrap();
    a.detach().unwrap();
    carriage.send_owned(empty.clone()).await.unwrap();
    drop(b);
    carriage.send_owned(empty).await.unwrap();
    assert_eq!(one.lock().unwrap().len(), 1);
    let frames = two.lock().unwrap();
    assert_eq!(frames.len(), 2);
    assert!(frames.first().unwrap().payload().is_none());
    assert!(frames.get(1).unwrap().payload().unwrap().is_empty());
}

#[tokio::test]
async fn failed_detach_ticket_retries_without_repeating_remote_unsubscribe() {
    let transport = Arc::new(Tracking::default());
    let (door, panel) = uris();
    let endpoint = Endpoint::new(serialized(transport.clone()), panel);
    let mut ticket = endpoint
        .subscriber(service())
        .subscription::<DoorState, _>(
            door.get_resource_uri(0x8001),
            DoorObserver(Arc::new(Mutex::new(Vec::new()))),
        )
        .await
        .unwrap();
    ticket.activate().await.unwrap();
    transport.fail_detach.store(true, Ordering::Release);
    let failure = ticket.unsubscribe().await.unwrap_err();
    assert!(failure.detach.is_some());
    assert!(failure.service.is_none());
    assert_eq!(ticket.phase(), SubscriptionPhase::Closing);
    assert_eq!(transport.attached.load(Ordering::Relaxed), 1);
    // The mock permits exactly one service unsubscribe, including both attempts.
    ticket.unsubscribe().await.unwrap();
    assert_eq!(ticket.phase(), SubscriptionPhase::Closed);
    assert_eq!(transport.attached.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn full_rpc_key_rejects_wrong_sink_kind_and_request_id_then_accepts_reply() {
    let transport = Arc::new(Tracking {
        broaden: true,
        ..Tracking::default()
    });
    let (door, panel) = uris();
    let endpoint = Endpoint::new(serialized(transport.clone()), panel.clone());
    let client = endpoint
        .rpc_client::<SetDoor, DoorState>(door.get_resource_uri(0x42), rpc_options())
        .await
        .unwrap();
    let wait = client.invoke(&SetDoor { open: true }).await.unwrap();
    let request = transport.sent.lock().unwrap().last().unwrap().clone();
    let other = StaticUriProvider::new("vehicle", 0x0001_0003, 1).unwrap();
    for metadata in [
        UFrameMetadata::response(
            door.get_resource_uri(0x42),
            other.get_source_uri(),
            request.metadata().id().clone(),
        )
        .with_payload_encoding(PayloadEncoding::JSON)
        .build()
        .unwrap(),
        UFrameMetadata::notification(door.get_resource_uri(0x8002), panel.get_source_uri())
            .with_payload_encoding(PayloadEncoding::JSON)
            .build()
            .unwrap(),
        UFrameMetadata::response(
            door.get_resource_uri(0x42),
            panel.get_source_uri(),
            UUID::build(),
        )
        .with_payload_encoding(PayloadEncoding::JSON)
        .build()
        .unwrap(),
    ] {
        transport
            .bus
            .send_owned(
                UOwnedFrame::with_payload(
                    metadata,
                    DoorJson::encode_payload_owned(&DoorState { open: false }).unwrap(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(client.in_flight(), 1);
    }
    // Includes our outgoing request echoed by deliberately broad physical filters.
    assert_eq!(client.stats().rejected.load(Ordering::Relaxed), 3);
    assert_eq!(client.stats().unmatched.load(Ordering::Relaxed), 1);
    transport
        .bus
        .send_owned(response(&request, true))
        .await
        .unwrap();
    let response = wait.response().await.unwrap();
    assert!(response.payload().unwrap().open);
    let context = response.context().unwrap();
    assert_eq!(context.kind, UMessageType::Response);
    assert_eq!(context.request_id.as_ref(), Some(request.metadata().id()));
    assert_eq!(context.payload_encoding, Some(PayloadEncoding::JSON));
    assert_eq!(context.sink, Some(panel.get_source_uri()));
    assert_eq!(client.in_flight(), 0);
}

#[tokio::test]
async fn reconciliation_to_unsubscribed_disables_business_delivery_without_remote_retry() {
    let transport = Arc::new(Tracking::default());
    let (door, panel) = uris();
    let endpoint = Endpoint::new(serialized(transport.clone()), panel);
    let mut remote = MockUSubscription::new();
    remote
        .expect_subscribe()
        .times(1)
        .returning(|_, _, _| Ok(SubscriptionStatus::Subscribed));
    remote.expect_unsubscribe().times(0);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut ticket = endpoint
        .subscriber(Arc::new(remote))
        .subscription::<DoorState, _>(
            door.get_resource_uri(0x8001),
            DoorObserver(observed.clone()),
        )
        .await
        .unwrap();
    ticket.activate().await.unwrap();
    // Simulates an independent service query that found remote removal.
    ticket.reconcile(SubscriptionStatus::Unsubscribed);
    let sender = Endpoint::new(serialized(transport), door);
    sender
        .publisher::<DoorState>(0x8001, PublishOptions::default())
        .unwrap()
        .publish(&DoorState { open: true })
        .await
        .unwrap();
    ticket.unsubscribe().await.unwrap();
    assert!(observed.lock().unwrap().is_empty());
    assert_eq!(ticket.stats().filtered.load(Ordering::Relaxed), 1);
}
