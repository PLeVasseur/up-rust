//! Executable independent-role controls over an in-process loopback medium.
//!
//! Every program below constructs ONE local deployment and learns its peers
//! only as independently configured addresses. The shared
//! `InMemoryBus` carries the common bounded publisher scenario; the explicitly
//! legacy scenarios use `InMemoryOwnedTransport`, standing in
//! for a network; it is not a peer object, and these controls are not
//! physical, cross-process or production-transport evidence.
#[allow(dead_code)]
#[path = "../examples/support/independent_apps.rs"]
mod apps;
#[allow(dead_code)]
#[path = "support/independent_proof_carriage.rs"]
mod carriage;
#[allow(dead_code)]
#[path = "../examples/support/user_first_door.rs"]
mod door;
#[allow(dead_code)]
#[path = "../examples/support/independent_layers.rs"]
mod layers;
#[allow(dead_code)]
#[path = "../examples/support/user_first_meter.rs"]
mod meter;
#[allow(dead_code)]
#[path = "../examples/support/independent_schema.rs"]
mod schema;
#[allow(dead_code)]
#[path = "support/independent_proof.rs"]
mod support;

use carriage::{injected, ScriptedCarriage};
use std::{
    future::Future,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use support::{
    address, eventually, next, Alarm, Gate, Record, RecordingService, Request, Scale, Seen, BOUND,
};
use tokio::sync::mpsc;
use up_rust::{
    communication::application::{
        acquire, CallError, CleanupLeg, Delivery, Deployment, OperationOptions, LegacyPublisher,
        Receiver, RoleSetError, RpcClient, RpcServer, SerializedError, SerializedStopError,
        ServerOptions,
    },
    Completion, InMemoryOwnedTransport, LocalUriProvider, PayloadEncoding, UMessageType,
    UOwnedTransport, UUri, UWireError,
};

/// One program's local deployment; the carriage/medium is its only shared object.
fn local(medium: Arc<dyn UOwnedTransport>, authority: &str, service: u16) -> Deployment {
    Deployment::with_defaults(medium, Arc::new(address(authority, service)))
        .expect("bounded defaults")
}

/// The SDK exposes no `SerializedStopError` classifier (native stop errors have
/// `completion()`), so this composes the SDK's own `is_terminal` policy: a
/// failed detach or a nonterminal worker refusal stays retryable.
fn stop_completion(error: &SerializedStopError) -> Completion {
    if error.detach.is_some() || error.worker.is_some_and(|worker| !worker.is_terminal()) {
        Completion::Pending
    } else if error.worker.is_some() {
        Completion::Failed
    } else {
        Completion::Succeeded
    }
}

async fn optional<E>(stop: Option<impl Future<Output = Result<(), E>>>) -> Result<(), E> {
    match stop {
        Some(stop) => stop.await,
        None => Ok(()),
    }
}

#[tokio::test]
async fn serialized_publisher_and_subscriber_programs_exchange_decoded_events() {
    use up_rust::communication::application::PublisherDeployment;
    use up_rust::test_util::handoff::InMemoryBus;
    use up_rust::two_mode::{ClassicUri, StartupError, Subscriber, TransportSetup, TransportSetupExt};

    // Existing bounded medium; no adapter to the legacy Arc/UOwnedTransport.
    // This is a host fixture only; running it cannot qualify a physical/MCU binding.
    let medium = InMemoryBus::new(8, 512);
    let device = TransportSetup::handoff(medium.participant(1)).expect("publisher transport");
    let panel = TransportSetup::handoff(medium.participant(2)).expect("subscriber transport");
    let device_address = address("device", 1);
    let core_address = ClassicUri(&device_address);
    let topic = device_address.get_resource_uri(schema::TEMPERATURE_RESOURCE);
    let pressure_topic = device_address.get_resource_uri(schema::PRESSURE_RESOURCE);
    let prepared = {
        let json = PublisherDeployment::new(&device, &core_address, schema::Format::Json);
        assert!(matches!(apps::MultiplePublishers::prepare(&json),
                         Err(StartupError::InvalidConfig(_))), "no allocating JSON or protobuf fallback");
        let local = PublisherDeployment::new(&device, &core_address, schema::Format::Protobuf);
        apps::MultiplePublishers::prepare(&local).expect("publisher-only program")
    };
    // These are independent receiver routes in existing binding storage, not
    // a second SDK inbox. RX capacity is explicit; six is only the TX maximum.
    let temperatures = Subscriber::<schema::Reading>::on(&panel, topic.clone())
        .wire(schema::ReadingProtobuf).max_payload(64).build().expect("temperature route");
    let pressures = Subscriber::<schema::Reading>::on(&panel, pressure_topic.clone())
        .wire(schema::ReadingProtobuf).max_payload(64).build().expect("pressure route");
    let tx = device.enter_runtime().expect("publisher runtime");
    let rx = panel.enter_runtime().expect("subscriber runtime");
    let publishers = prepared.activate(&tx).expect("activate publishers");
    // Verify the host export is the same concrete common SDK handle.
    let _: &up_rust::communication::application::Publisher<schema::Reading> = &publishers.temperature;
    let temperatures = temperatures.activate(&rx).expect("activate temperature route");
    let pressures = pressures.activate(&rx).expect("activate pressure route");

    publishers.publish_readings(&[21, 22, 23], 101).await.expect("same real service body");
    for expected in [21, 22, 23] {
        let event = temperatures.try_recv().expect("receive temperature").expect("temperature present");
        let header = event.header();
        assert_eq!(event.value, expected);
        assert_eq!(header.kind, UMessageType::Publish);
        assert_eq!(header.source, topic);
        assert_eq!(header.sink, None);
        assert_eq!(header.payload_encoding, Some(PayloadEncoding::PROTOBUF));
        assert!(header.native_type_token.is_none());
        assert_eq!(event.payload_bytes(), Some([0x08, expected as u8].as_slice()));
    }
    assert!(temperatures.try_recv().expect("temperature drained").is_none(),
            "pressure is an independent resource");
    let event = pressures.try_recv().expect("receive pressure").expect("pressure present");
    assert_eq!(event.value, 101);
    assert_eq!(event.header().source, pressure_topic);
    assert_eq!(event.header().kind, UMessageType::Publish);
    assert_eq!(event.header().sink, None);
    assert_eq!(event.header().payload_encoding, Some(PayloadEncoding::PROTOBUF));
    assert!(event.header().native_type_token.is_none());
    drop(event);
    assert!(pressures.try_recv().expect("pressure drained").is_none());

    // Proto3 zero is a present empty payload, retaining its encoding identity.
    publishers.publish_readings(&[], 0).await.expect("empty temperature batch and zero pressure");
    assert!(temperatures.try_recv().expect("no extra temperature").is_none());
    let zero = pressures.try_recv().expect("receive zero").expect("zero present");
    assert_eq!(zero.value, 0);
    assert_eq!(zero.payload_bytes(), Some(&[][..]));
    assert_eq!(zero.header().payload_encoding, Some(PayloadEncoding::PROTOBUF));
}

#[tokio::test]
async fn legacy_serialized_publishers_preserve_json_and_subscription_lifecycle() {
    for (format, encoding) in [
        (schema::Format::Protobuf, PayloadEncoding::PROTOBUF),
        (schema::Format::Json, PayloadEncoding::JSON),
    ] {
        let medium = Arc::new(InMemoryOwnedTransport::default());
        // LegacyPublisher program: one local deployment, two publishers, no subscriber.
        let device = schema::bind_format(local(medium.clone(), "device", 1), format);
        // Preserve the original legacy codec/subscription coverage explicitly.
        let temperature: LegacyPublisher<schema::Reading> = schema::TEMPERATURE
            .publisher(&device, Default::default()).expect("legacy temperature");
        let pressure: LegacyPublisher<schema::Reading> = schema::PRESSURE
            .publisher(&device, Default::default()).expect("legacy pressure");
        // Subscriber program: its own deployment and only the publisher's ADDRESS.
        let panel = schema::bind_format(local(medium.clone(), "panel", 2), format);
        let device_address = address("device", 1);
        let service = Arc::new(RecordingService::default());
        let (seen_tx, mut seen) = mpsc::unbounded_channel();
        let mut subscriber = apps::SubscriberOnly::prepare(
            &panel,
            &device_address,
            service.clone(),
            Record(seen_tx),
        )
        .await
        .expect("subscriber-only program");
        subscriber.readings.activate().await.expect("activation");
        let topic = device_address.get_resource_uri(schema::TEMPERATURE.resource());
        assert_eq!(service.subscribed(), std::slice::from_ref(&topic));

        for value in [21, 22, 23] {
            temperature.publish(&schema::Reading { value }).await.expect("legacy temperature");
        }
        pressure.publish(&schema::Reading { value: 101 }).await.expect("legacy pressure");
        for expected in [21, 22, 23] {
            let event = next(&mut seen).await;
            assert_eq!(event.value, expected);
            assert_eq!(event.kind, UMessageType::Publish);
            assert_eq!(event.source, topic);
            assert_eq!(event.sink, None);
            assert_eq!(event.encoding, Some(encoding));
            assert!(!event.native_token);
        }
        // Unsubscribe drains accepted work, so nothing further can be pending.
        subscriber
            .readings
            .unsubscribe()
            .await
            .expect("subscription cleanup");
        assert_eq!(service.unsubscribed(), [topic]);
        assert!(
            seen.try_recv().is_err(),
            "pressure is an independent resource"
        );
    }
}

#[tokio::test]
async fn independently_selected_mismatched_profiles_reject_instead_of_misdecoding() {
    let medium = Arc::new(InMemoryOwnedTransport::default());
    let device = schema::bind_format(local(medium.clone(), "device", 1), schema::Format::Json);
    let publisher = apps::PublisherOnly::new(&device).expect("publisher-only program");
    let panel = schema::bind_format(local(medium.clone(), "panel", 2), schema::Format::Protobuf);
    let (seen_tx, mut seen) = mpsc::unbounded_channel();
    let mut subscriber = apps::SubscriberOnly::prepare(
        &panel,
        &address("device", 1),
        Arc::new(RecordingService::default()),
        Record(seen_tx),
    )
    .await
    .expect("subscriber-only program");
    subscriber.readings.activate().await.expect("activation");
    publisher
        .reading
        .publish(&schema::Reading { value: 7 })
        .await
        .expect("JSON publication");
    subscriber
        .readings
        .unsubscribe()
        .await
        .expect("drains the rejected frame");
    assert_eq!(
        subscriber.readings.stats().rejected.load(Ordering::Acquire),
        1
    );
    assert!(seen.try_recv().is_err(), "no silent cross-profile decode");
}

#[tokio::test]
async fn serialized_notifier_and_receiver_programs_exchange_decoded_notifications() {
    let medium = Arc::new(InMemoryOwnedTransport::default());
    let panel_address = address("panel", 2);
    // Notifier program: knows only the panel's address as its destination.
    let device = schema::bind(local(medium.clone(), "device", 1));
    let notifier = apps::NotifierOnly::new(&device, panel_address.get_source_uri())
        .expect("notifier-only program");
    // Receiver program: knows only the device's address as its source.
    let panel = schema::bind(local(medium.clone(), "panel", 2));
    let device_address = address("device", 1);
    let (seen_tx, mut seen) = mpsc::unbounded_channel();
    let mut receiver = apps::ReceiverOnly::prepare(&panel, &device_address, Record(seen_tx))
        .await
        .expect("receiver-only program");
    for value in [5, 6] {
        notifier
            .alarm
            .notify(&schema::Reading { value })
            .await
            .expect("repeated notification");
    }
    for expected in [5, 6] {
        let notice = next(&mut seen).await;
        assert_eq!(notice.value, expected);
        assert_eq!(notice.kind, UMessageType::Notification);
        assert_eq!(
            notice.source,
            device_address.get_resource_uri(schema::ALARM.resource())
        );
        assert_eq!(notice.sink, Some(panel_address.get_source_uri()));
        assert_eq!(notice.encoding, Some(PayloadEncoding::PROTOBUF));
    }
    receiver.alarm.stop().await.expect("receiver cleanup");
}

#[tokio::test]
async fn serialized_client_only_and_server_only_programs_correlate_repeated_requests() {
    let medium = Arc::new(InMemoryOwnedTransport::default());
    // Server program: one deployment, one server, no client knowledge.
    let device = schema::bind(local(medium.clone(), "device", 1));
    let (requests_tx, mut requests) = mpsc::unbounded_channel();
    let mut server = apps::ServerOnly::prepare(
        &device,
        Scale {
            factor: 10,
            seen: requests_tx,
        },
    )
    .await
    .expect("server-only program");
    // Client program: its own deployment and only the server's ADDRESS.
    let panel = schema::bind(local(medium.clone(), "panel", 2));
    let device_address = address("device", 1);
    let client = apps::ClientOnly::prepare(&panel, &device_address)
        .await
        .expect("client-only program");
    client
        .read
        .set_token("panel-credential")
        .expect("bounded credential");

    // Two concurrently pending calls, awaited out of order.
    let first = client
        .read
        .invoke(&schema::Query { slot: 1 })
        .await
        .expect("first call");
    let second = client
        .read
        .invoke(&schema::Query { slot: 2 })
        .await
        .expect("second call");
    assert_eq!(client.read.in_flight(), 2);
    let second = second.response().await.expect("second response");
    let first = first.response().await.expect("first response");
    assert_eq!(client.read.in_flight(), 0);

    let (one, two) = {
        let a = next(&mut requests).await;
        let b = next(&mut requests).await;
        if a.slot == 1 {
            (a, b)
        } else {
            (b, a)
        }
    };
    let method = device_address.get_resource_uri(schema::READ.resource());
    let reply_to = address("panel", 2).get_source_uri();
    for (response, request, expected) in [(&first, &one, 10), (&second, &two, 20)] {
        assert_eq!(response.payload().expect("owned response").value, expected);
        let context = response.context().expect("response context");
        assert_eq!(context.kind, UMessageType::Response);
        assert_eq!(context.source, method);
        assert_eq!(context.sink, Some(reply_to.clone()));
        assert_eq!(context.request_id, Some(request.id.clone()));
        assert_eq!(request.source, reply_to);
        assert_eq!(request.sink, Some(method.clone()));
        assert_eq!(request.token.as_deref(), Some("panel-credential"));
    }
    assert_ne!(one.id, two.id);

    // A third, sequential operation on the same stored client.
    let third = client
        .read
        .invoke(&schema::Query { slot: 3 })
        .await
        .expect("third call")
        .response()
        .await
        .expect("third response");
    assert_eq!(third.payload().expect("owned response").value, 30);
    assert_eq!(next(&mut requests).await.slot, 3);

    client.read.close().expect("client cleanup");
    server.read.stop().await.expect("server cleanup");
    // Owned responses remain valid after both programs shut down.
    assert_eq!(first.payload().expect("owned response").value, 10);
}

/// An arbitrary gateway subset in caller-retained optional slots: two
/// receivers and two clients of the same kinds, a publisher, and a late
/// server. Only `acquire` and `CleanupLeg` are used; no engine is introduced.
#[derive(Default)]
struct GatewaySetup {
    relay: Option<LegacyPublisher<schema::Reading>>,
    alarms_a: Option<Receiver>,
    alarms_b: Option<Receiver>,
    read_a: Option<RpcClient<schema::Query, schema::Reading>>,
    read_b: Option<RpcClient<schema::Query, schema::Reading>>,
    backup: Option<RpcServer>,
    cleanup: GatewayCleanup,
}

#[derive(Default)]
struct GatewayCleanup {
    alarms_a: CleanupLeg<SerializedStopError>,
    alarms_b: CleanupLeg<SerializedStopError>,
    read_a: CleanupLeg<SerializedError>,
    read_b: CleanupLeg<SerializedError>,
    backup: CleanupLeg<SerializedStopError>,
}

impl GatewayCleanup {
    fn completion(&self) -> Completion {
        let outcomes = [
            self.alarms_a.completion,
            self.alarms_b.completion,
            self.read_a.completion,
            self.read_b.completion,
            self.backup.completion,
        ];
        if outcomes.contains(&Completion::Pending) {
            Completion::Pending
        } else if outcomes.contains(&Completion::Failed) {
            Completion::Failed
        } else {
            Completion::Succeeded
        }
    }
}

impl GatewaySetup {
    async fn fill(
        &mut self,
        local: &schema::LocalDeployment,
        device_a: &dyn LocalUriProvider,
        device_b: &dyn LocalUriProvider,
        alarms: &mpsc::UnboundedSender<Seen>,
        gate: &Arc<Gate>,
        requests: &mpsc::UnboundedSender<Request>,
    ) -> Result<(), SerializedError> {
        let options = OperationOptions::default();
        if self.relay.is_none() {
            self.relay = Some(schema::TEMPERATURE.publisher(local, options.publish)?);
        }
        acquire(
            &mut self.alarms_a,
            schema::ALARM.receiver(
                local,
                device_a,
                Alarm {
                    seen: alarms.clone(),
                    hold: None,
                },
            ),
        )
        .await?;
        acquire(
            &mut self.alarms_b,
            schema::ALARM.receiver(
                local,
                device_b,
                Alarm {
                    seen: alarms.clone(),
                    hold: Some(Arc::clone(gate)),
                },
            ),
        )
        .await?;
        acquire(
            &mut self.read_a,
            schema::READ.client(local, device_a, options.rpc),
        )
        .await?;
        acquire(
            &mut self.read_b,
            schema::READ.client(local, device_b, options.rpc),
        )
        .await?;
        acquire(
            &mut self.backup,
            schema::READ_BACKUP.server(
                local,
                Scale {
                    factor: 1,
                    seen: requests.clone(),
                },
                |value| schema::Reading { value },
                options.server,
            ),
        )
        .await?;
        Ok(())
    }

    /// Closes every local admission first, then bounds each drain separately.
    fn close(&mut self, deadline: Duration) -> impl Future<Output = Completion> + '_ {
        let Self {
            alarms_a,
            alarms_b,
            read_a,
            read_b,
            backup,
            cleanup,
            ..
        } = self;
        cleanup.alarms_a.begin();
        cleanup.alarms_b.begin();
        cleanup.read_a.begin();
        cleanup.read_b.begin();
        cleanup.backup.begin();
        if let Some(owner) = alarms_a.as_ref() {
            owner.close_admission();
        }
        if let Some(owner) = alarms_b.as_ref() {
            owner.close_admission();
        }
        if let Some(owner) = backup.as_ref() {
            owner.close_admission();
        }
        // Exact detach attempts happen here, in slot order, before any await.
        let stop_a = alarms_a.as_mut().map(Receiver::stop);
        let stop_b = alarms_b.as_mut().map(Receiver::stop);
        cleanup
            .read_a
            .record(read_a.as_ref().map_or(Ok(()), RpcClient::close), |_| {
                Completion::Pending
            });
        cleanup
            .read_b
            .record(read_b.as_ref().map_or(Ok(()), RpcClient::close), |_| {
                Completion::Pending
            });
        let stop_backup = backup.as_mut().map(RpcServer::stop);
        async move {
            tokio::join!(
                cleanup
                    .alarms_a
                    .run(deadline, optional(stop_a), stop_completion),
                cleanup
                    .alarms_b
                    .run(deadline, optional(stop_b), stop_completion),
                cleanup
                    .backup
                    .run(deadline, optional(stop_backup), stop_completion),
            );
            cleanup.completion()
        }
    }
}

#[tokio::test(start_paused = true)]
async fn serialized_repeated_roles_survive_failed_or_cancelled_later_acquisition() {
    for cancel in [false, true] {
        let medium = Arc::new(InMemoryOwnedTransport::default());
        let gateway_address = address("gateway", 3);
        let peer_a = address("device-a", 1);
        let peer_b = address("device-b", 2);
        // Two independently deployed devices; each knows only the gateway address.
        let device_a = schema::bind(local(medium.clone(), "device-a", 1));
        let device_b = schema::bind(local(medium.clone(), "device-b", 2));
        let (requests_tx, mut requests) = mpsc::unbounded_channel();
        let mut server_a = apps::ServerOnly::prepare(
            &device_a,
            Scale {
                factor: 10,
                seen: requests_tx.clone(),
            },
        )
        .await
        .expect("device A server-only program");
        let mut server_b = apps::ServerOnly::prepare(
            &device_b,
            Scale {
                factor: 20,
                seen: requests_tx.clone(),
            },
        )
        .await
        .expect("device B server-only program");
        let alarm_a = apps::NotifierOnly::new(&device_a, gateway_address.get_source_uri())
            .expect("device A notifier-only program");
        let alarm_b = apps::NotifierOnly::new(&device_b, gateway_address.get_source_uri())
            .expect("device B notifier-only program");

        // Gateway program: scripted local carriage over the same medium.
        let carriage = Arc::new(ScriptedCarriage::new(medium.clone()));
        let gateway = schema::bind(local(carriage.clone(), "gateway", 3));
        if cancel {
            carriage.hold_attach(5);
        } else {
            carriage.fail_attach(5);
        }
        let (alarms_tx, mut alarms) = mpsc::unbounded_channel();
        let gate = Arc::new(Gate::default());
        let mut setup = GatewaySetup::default();
        let original = if cancel {
            tokio::select! {
                result = setup.fill(&gateway, &peer_a, &peer_b, &alarms_tx, &gate, &requests_tx) => {
                    panic!("gateway setup unexpectedly completed: {result:?}")
                }
                () = carriage.attach_entered.notified() => None,
            }
        } else {
            Some(
                setup
                    .fill(&gateway, &peer_a, &peer_b, &alarms_tx, &gate, &requests_tx)
                    .await
                    .expect_err("late server acquisition refused"),
            )
        };
        assert!(setup.relay.is_some() && setup.alarms_a.is_some() && setup.alarms_b.is_some());
        assert!(setup.read_a.is_some() && setup.read_b.is_some() && setup.backup.is_none());
        assert_eq!((carriage.attach_calls(), carriage.live()), (5, 4));

        // Every retained owner still operates on the exact attachment it acquired.
        alarm_a
            .alarm
            .notify(&schema::Reading { value: 1 })
            .await
            .expect("device A alarm");
        let notice = next(&mut alarms).await;
        assert_eq!(notice.value, 1);
        assert_eq!(
            notice.source,
            peer_a.get_resource_uri(schema::ALARM.resource())
        );
        alarm_b
            .alarm
            .notify(&schema::Reading { value: 2 })
            .await
            .expect("device B alarm");
        let notice = next(&mut alarms).await;
        assert_eq!(notice.value, 2);
        assert_eq!(
            notice.source,
            peer_b.get_resource_uri(schema::ALARM.resource())
        );
        // Device B's receiver now retains its delivery inside the handler.
        gate.entered.notified().await;
        let response_a = setup
            .read_a
            .as_ref()
            .expect("retained client A")
            .invoke(&schema::Query { slot: 3 })
            .await
            .expect("call A")
            .response()
            .await
            .expect("response A");
        let response_b = setup
            .read_b
            .as_ref()
            .expect("retained client B")
            .invoke(&schema::Query { slot: 3 })
            .await
            .expect("call B")
            .response()
            .await
            .expect("response B");
        assert_eq!(response_a.payload().expect("owned").value, 30);
        assert_eq!(response_b.payload().expect("owned").value, 60);
        assert_eq!(
            response_a.context().expect("context").source,
            peer_a.get_resource_uri(schema::READ.resource())
        );
        assert_eq!(
            response_b.context().expect("context").source,
            peer_b.get_resource_uri(schema::READ.resource())
        );
        let served = [next(&mut requests).await, next(&mut requests).await];
        assert!(
            served
                .iter()
                .all(|request| request.slot == 3
                    && request.source == gateway_address.get_source_uri())
        );

        // Attempt 1: the earliest owner's exact detach fails and device B's
        // drain is held; the other owners still complete independently.
        carriage.fail_detach(1);
        assert_eq!(
            setup.close(Duration::from_millis(200)).await,
            Completion::Pending
        );
        assert_eq!(carriage.detach_log(), [1, 2, 3, 4]);
        assert_eq!(carriage.live(), 1);
        {
            let cleanup = &setup.cleanup;
            assert_eq!(cleanup.alarms_a.completion, Completion::Pending);
            let first = cleanup
                .alarms_a
                .first_error
                .as_ref()
                .expect("first detach error retained");
            assert_eq!(first.attempt, 1);
            assert_eq!(first.error.detach, Some(injected("detach#1")));
            assert!(cleanup.alarms_b.timed_out && cleanup.alarms_b.current.is_none());
            assert_eq!(cleanup.alarms_b.completion, Completion::Pending);
            assert_eq!(cleanup.read_a.completion, Completion::Succeeded);
            assert_eq!(cleanup.read_b.completion, Completion::Succeeded);
            assert_eq!(cleanup.backup.completion, Completion::Succeeded);
        }
        let owner_a = setup.alarms_a.as_ref().expect("exact owner retained");
        let owner_first = owner_a
            .stop_progress()
            .detach
            .first
            .as_ref()
            .expect("owner-side first detach error");
        assert_eq!(
            (owner_first.attempt, owner_first.error.clone()),
            (1, injected("detach#1"))
        );

        // Attempt 2 retries the same owners; only the failed attachment detaches.
        carriage.clear_detach_failure();
        gate.release.notify_one();
        assert_eq!(setup.close(BOUND).await, Completion::Succeeded);
        assert_eq!(carriage.detach_log(), [1, 2, 3, 4, 1]);
        assert_eq!(carriage.live(), 0);
        let first = setup
            .cleanup
            .alarms_a
            .first_error
            .as_ref()
            .expect("first error survives success");
        assert_eq!(
            (first.attempt, first.error.detach.clone()),
            (1, Some(injected("detach#1")))
        );
        assert!(setup
            .alarms_a
            .as_ref()
            .expect("owner")
            .stop_progress()
            .detach
            .latest
            .is_none());
        if !cancel {
            assert!(matches!(
                &original,
                Some(SerializedError::Transport(status)) if *status == injected("attach#5")
            ));
        }
        assert!(original.is_some() != cancel);
        // Owned responses outlive the gateway's cleanup.
        assert_eq!(response_a.payload().expect("owned").value, 30);
        server_a.read.stop().await.expect("device A cleanup");
        server_b.read.stop().await.expect("device B cleanup");
    }
}

/// A server setup with a post-acquisition configuration step.
#[derive(Default)]
struct KnownClientsSetup {
    server: Option<RpcServer>,
    cleanup: CleanupLeg<SerializedStopError>,
}

impl KnownClientsSetup {
    async fn fill(
        &mut self,
        local: &schema::LocalDeployment,
        known: &[UUri],
        handler: Scale,
    ) -> Result<(), SerializedError> {
        acquire(
            &mut self.server,
            schema::READ.server(
                local,
                handler,
                |value| schema::Reading { value },
                ServerOptions { max_clients: 1 },
            ),
        )
        .await?;
        let server = self.server.as_ref().expect("acquired above");
        for client in known {
            server.prepare_client(client)?;
        }
        Ok(())
    }
}

#[tokio::test]
async fn serialized_server_prepare_client_refusal_after_acquisition_keeps_exact_owner() {
    let medium = Arc::new(InMemoryOwnedTransport::default());
    let carriage = Arc::new(ScriptedCarriage::new(medium.clone()));
    let device = schema::bind(local(carriage.clone(), "device", 1));
    let panel_a = address("panel-a", 2);
    let panel_b = address("panel-b", 3);
    let (requests_tx, mut requests) = mpsc::unbounded_channel();
    let mut setup = KnownClientsSetup::default();
    let refused = setup
        .fill(
            &device,
            &[panel_a.get_source_uri(), panel_b.get_source_uri()],
            Scale {
                factor: 3,
                seen: requests_tx,
            },
        )
        .await
        .expect_err("second known client exceeds the configured capacity");
    assert!(matches!(
        refused,
        SerializedError::Config("reply client capacity exceeded")
    ));
    assert!(setup.server.is_some());
    assert_eq!((carriage.attach_calls(), carriage.live()), (1, 1));
    // An address that is not a reply destination is refused by the same owner.
    let invalid = setup
        .server
        .as_ref()
        .expect("retained server")
        .prepare_client(&panel_a.get_resource_uri(0x8001))
        .expect_err("topic is not a reply destination");
    assert!(matches!(
        invalid,
        SerializedError::Config("RPC reply destination required")
    ));

    // The retained exact server still serves its prepared client ...
    let client_a = apps::ClientOnly::prepare(
        &schema::bind(local(medium.clone(), "panel-a", 2)),
        &address("device", 1),
    )
    .await
    .expect("client-only program A");
    let reply = client_a
        .read
        .invoke(&schema::Query { slot: 5 })
        .await
        .expect("call")
        .response()
        .await
        .expect("prepared client served");
    assert_eq!(reply.payload().expect("owned").value, 15);
    assert_eq!(next(&mut requests).await.slot, 5);
    // ... and genuinely refuses the client beyond its bounded capacity.
    let client_b = apps::ClientOnly::prepare(
        &schema::bind(local(medium.clone(), "panel-b", 3)),
        &address("device", 1),
    )
    .await
    .expect("client-only program B");
    let pending = client_b
        .read
        .invoke(&schema::Query { slot: 6 })
        .await
        .expect("request submitted");
    let server = setup.server.as_ref().expect("retained server");
    eventually(|| server.stats().refused.load(Ordering::Acquire) == 1).await;
    drop(pending);
    assert_eq!(client_b.read.in_flight(), 0);

    // Exact, retryable cleanup of the retained owner.
    carriage.fail_detach(1);
    setup.cleanup.begin();
    setup
        .cleanup
        .run(
            BOUND,
            optional(setup.server.as_mut().map(RpcServer::stop)),
            stop_completion,
        )
        .await;
    assert_eq!(setup.cleanup.completion, Completion::Pending);
    carriage.clear_detach_failure();
    setup.cleanup.begin();
    setup
        .cleanup
        .run(
            BOUND,
            optional(setup.server.as_mut().map(RpcServer::stop)),
            stop_completion,
        )
        .await;
    assert_eq!(setup.cleanup.completion, Completion::Succeeded);
    let first = setup
        .cleanup
        .first_error
        .as_ref()
        .expect("first error kept");
    assert_eq!(
        (first.attempt, first.error.detach.clone()),
        (1, Some(injected("detach#1")))
    );
    assert_eq!(carriage.detach_log(), [1, 1]);
    assert_eq!(carriage.live(), 0);
    client_a.read.close().expect("client A cleanup");
    client_b.read.close().expect("client B cleanup");
}

#[tokio::test]
async fn door_setup_token_refusal_after_client_acquisition_is_cleanup_only() {
    let carriage = Arc::new(ScriptedCarriage::new(Arc::new(
        InMemoryOwnedTransport::default(),
    )));
    let (device, panel) = door::uris();
    let mut rpc = door::rpc_options();
    // Shorter than the Door credential: refusal happens after acquisition.
    rpc.max_token_len = 4;
    let options = OperationOptions {
        rpc,
        ..OperationOptions::default()
    };
    let binding = door::Door::bind(
        Deployment::with_defaults(carriage.clone(), device).expect("device deployment"),
        Deployment::with_defaults(carriage.clone(), panel).expect("panel deployment"),
        door::Encoding::Protobuf,
        options,
    )
    .expect("door binding");
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut setup = door::roles::Setup::default();
    let original = door::start(
        &mut setup,
        &binding,
        Arc::new(RecordingService::default()),
        observed.clone(),
    )
    .await
    .expect_err("credential refused after client acquisition");
    assert!(matches!(
        &original,
        door::AppError::Setup(SerializedError::Config("token capacity exceeded"))
    ));
    let slots = setup.slots();
    assert!(slots.state.is_held() && slots.changed.is_held() && slots.set.is_held());
    assert!(!slots.server.is_held() && !slots.notifications.is_held());
    assert!(!slots.subscription.is_held());
    assert_eq!((carriage.attach_calls(), carriage.live()), (1, 1));

    // The interrupted setup is cleanup-only: a refill creates no new effects.
    let effects = carriage.effects();
    let again = door::start(
        &mut setup,
        &binding,
        Arc::new(RecordingService::default()),
        observed,
    )
    .await
    .expect_err("cleanup-only setup");
    assert!(matches!(
        again,
        door::AppError::RoleSet(RoleSetError::AlreadyStarted)
    ));
    assert_eq!(carriage.effects(), effects);
    assert!(setup.finish().is_err());

    carriage.fail_detach(1);
    assert_eq!(setup.close(BOUND).await, Completion::Pending);
    let first = setup
        .cleanup
        .set
        .first_error
        .as_ref()
        .expect("first client detach error");
    assert_eq!(first.attempt, 1);
    assert!(matches!(
        &*first.error,
        SerializedError::Transport(status) if *status == injected("detach#1")
    ));
    carriage.clear_detach_failure();
    assert_eq!(setup.close(BOUND).await, Completion::Succeeded);
    assert_eq!(carriage.detach_log(), [1, 1]);
    assert_eq!(carriage.live(), 0);
    assert_eq!(
        setup
            .cleanup
            .set
            .first_error
            .as_ref()
            .expect("first error survives success")
            .attempt,
        1
    );
    assert!(matches!(
        &original,
        door::AppError::Setup(SerializedError::Config("token capacity exceeded"))
    ));
}

fn meter_binding(carriage: &Arc<ScriptedCarriage>, profile: meter::Profile) -> meter::Meter {
    meter::Meter::bind(
        Deployment::with_defaults(carriage.clone(), Arc::new(address("meter", 1)))
            .expect("device deployment"),
        Deployment::with_defaults(carriage.clone(), Arc::new(address("meter", 2)))
            .expect("panel deployment"),
        profile,
        OperationOptions::default(),
    )
    .expect("meter binding")
}

#[tokio::test]
async fn meter_setup_refuses_changed_deployment_and_keeps_interrupted_owners() {
    for cancel in [false, true] {
        let carriage_a = Arc::new(ScriptedCarriage::new(Arc::new(
            InMemoryOwnedTransport::default(),
        )));
        let carriage_b = Arc::new(ScriptedCarriage::new(Arc::new(
            InMemoryOwnedTransport::default(),
        )));
        let meter_a = meter_binding(&carriage_a, meter::Profile::Protobuf);
        // Same Rust binding type; different carriage AND response profile.
        let meter_b = meter_binding(&carriage_b, meter::Profile::Mixed);
        if cancel {
            carriage_a.hold_attach(2);
        } else {
            carriage_a.fail_attach(2);
        }
        let mut setup = meter::MeterSetup::default();
        let original = if cancel {
            tokio::select! {
                result = meter::start(&mut setup, &meter_a, Arc::new(RecordingService::default()), Arc::default(), Arc::default()) => {
                    panic!("meter setup unexpectedly completed: {result:?}")
                }
                () = carriage_a.attach_entered.notified() => None,
            }
        } else {
            Some(
                meter::start(
                    &mut setup,
                    &meter_a,
                    Arc::new(RecordingService::default()),
                    Arc::default(),
                    Arc::default(),
                )
                .await
                .expect_err("server attachment refused"),
            )
        };
        let slots = setup.slots();
        assert!(slots.publisher.is_held() && slots.notifier.is_held() && slots.client.is_held());
        assert!(!slots.server.is_held() && !slots.receiver.is_held());
        assert!(!slots.subscription.is_held());
        assert_eq!((carriage_a.attach_calls(), carriage_a.live()), (2, 1));

        let before_b = carriage_b.effects();
        let refused = meter::start(
            &mut setup,
            &meter_b,
            Arc::new(RecordingService::default()),
            Arc::default(),
            Arc::default(),
        )
        .await
        .expect_err("changed deployment refused");
        assert!(matches!(refused, SerializedError::Config(_)));
        assert_eq!(
            carriage_b.effects(),
            before_b,
            "no B admission, attach or send"
        );
        assert_eq!(carriage_a.attach_calls(), 2);

        carriage_a.fail_detach(1);
        assert_eq!(setup.close(BOUND).await, Completion::Pending);
        {
            let first = setup
                .cleanup
                .client
                .first_error
                .as_ref()
                .expect("retained A client detach error");
            assert_eq!(first.attempt, 1);
            assert!(matches!(
                &*first.error,
                SerializedError::Transport(status) if *status == injected("detach#1")
            ));
        }
        carriage_a.clear_detach_failure();
        assert_eq!(setup.close(BOUND).await, Completion::Succeeded);
        assert_eq!(carriage_a.detach_log(), [1, 1]);
        assert_eq!(carriage_a.live(), 0);
        assert_eq!(
            setup
                .cleanup
                .client
                .first_error
                .as_ref()
                .expect("first error survives success")
                .attempt,
            1
        );
        if !cancel {
            assert!(matches!(
                &original,
                Some(SerializedError::Transport(status)) if *status == injected("attach#2")
            ));
        }
        assert!(original.is_some() != cancel);
        assert_eq!(carriage_b.effects(), before_b);
    }
}

#[tokio::test]
async fn counted_decorator_delegates_exact_attachments_and_inner_errors() {
    let medium = Arc::new(InMemoryOwnedTransport::default());
    // Implementer audience: a decorator over ANOTHER carriage, never an engine.
    let inner = Arc::new(ScriptedCarriage::new(medium.clone()));
    let counted = Arc::new(layers::CountedCarriage {
        inner: inner.clone(),
        completed_sends: AtomicUsize::new(0),
    });
    let panel = schema::bind(local(counted.clone(), "panel", 2));
    let device = schema::bind(local(medium.clone(), "device", 1));
    let device_address = address("device", 1);
    let publisher = apps::PublisherOnly::new(&device).expect("publisher-only program");

    // Attach failure: the inner carriage's exact status reaches the caller.
    inner.fail_attach(1);
    let (seen_tx, mut seen) = mpsc::unbounded_channel();
    let Err(refused) = apps::SubscriberOnly::prepare(
        &panel,
        &device_address,
        Arc::new(RecordingService::default()),
        Record(seen_tx.clone()),
    )
    .await
    else {
        panic!("inner attach refusal expected");
    };
    assert!(matches!(
        &refused,
        SerializedError::Transport(status) if *status == injected("attach#1")
    ));
    assert_eq!(inner.live(), 0);

    // Successful attach: the decorator returns the inner carriage's exact owner.
    let mut subscriber = apps::SubscriberOnly::prepare(
        &panel,
        &device_address,
        Arc::new(RecordingService::default()),
        Record(seen_tx),
    )
    .await
    .expect("decorated subscriber");
    assert_eq!(inner.live(), 1);
    subscriber.readings.activate().await.expect("activation");
    publisher
        .reading
        .publish(&schema::Reading { value: 9 })
        .await
        .expect("publication");
    assert_eq!(next(&mut seen).await.value, 9);

    // Detach failure is the inner hook's exact error and keeps the registration.
    inner.fail_detach(2);
    let close = subscriber
        .readings
        .unsubscribe()
        .await
        .expect_err("inner detach refusal");
    assert_eq!(close.detach, Some(injected("detach#2")));
    assert!(close.service.is_none() && close.worker.is_none());
    assert_eq!(inner.live(), 1);
    inner.clear_detach_failure();
    subscriber
        .readings
        .unsubscribe()
        .await
        .expect("retried exact detach");
    assert_eq!(inner.live(), 0);
    assert_eq!(inner.detach_log(), [2, 2]);

    // Send failure is delegated unchanged and is not counted as completed.
    let (requests_tx, mut requests) = mpsc::unbounded_channel();
    let mut server = apps::ServerOnly::prepare(
        &device,
        Scale {
            factor: 2,
            seen: requests_tx,
        },
    )
    .await
    .expect("server-only program");
    let client = apps::ClientOnly::prepare(&panel, &device_address)
        .await
        .expect("decorated client");
    inner.fail_sends(true);
    let failed = client
        .read
        .invoke(&schema::Query { slot: 4 })
        .await
        .expect_err("inner send refusal");
    assert!(matches!(
        &failed,
        CallError::Submit(SerializedError::Transport(status)) if *status == injected("send refused")
    ));
    assert_eq!(counted.completed_sends.load(Ordering::Acquire), 0);
    assert_eq!(client.read.in_flight(), 0);
    inner.fail_sends(false);
    let reply = client
        .read
        .invoke(&schema::Query { slot: 4 })
        .await
        .expect("delegated send")
        .response()
        .await
        .expect("correlated reply");
    assert_eq!(reply.payload().expect("owned").value, 8);
    assert_eq!(counted.completed_sends.load(Ordering::Acquire), 1);
    // Only the delegated request reached the server; the refused one never did.
    assert_eq!(next(&mut requests).await.slot, 4);
    assert!(requests.try_recv().is_err());

    // Admission refusal is the inner carriage's exact payload error.
    inner.refuse_admission(true);
    let Err(admission) = apps::PublisherOnly::new(&panel) else {
        panic!("inner admission refusal expected");
    };
    assert!(matches!(
        &admission,
        SerializedError::Payload(error) if *error == UWireError::invalid_payload("scripted admission refusal")
    ));
    inner.refuse_admission(false);

    client.read.close().expect("client cleanup");
    assert_eq!(inner.live(), 0);
    server.read.stop().await.expect("server cleanup");
}

#[tokio::test]
async fn reusable_l2_protocol_and_direct_l1_reach_an_independent_subscriber() {
    let medium = Arc::new(InMemoryOwnedTransport::default());
    let inner = Arc::new(ScriptedCarriage::new(medium.clone()));
    let counted = Arc::new(layers::CountedCarriage {
        inner: inner.clone(),
        completed_sends: AtomicUsize::new(0),
    });
    let device_address = address("device", 1);
    // L2 implementation audience: a protocol built only from L1 contracts.
    let protocol = layers::PublicationProtocol::for_schema::<schema::ReadingProtobuf>(
        counted.clone(),
        &device_address,
        &schema::TEMPERATURE,
    )
    .expect("protocol admission");
    // L2 application audience: an ordinary, independently deployed subscriber.
    let panel = schema::bind(local(medium.clone(), "panel", 2));
    let (seen_tx, mut seen) = mpsc::unbounded_channel();
    let mut subscriber = apps::SubscriberOnly::prepare(
        &panel,
        &device_address,
        Arc::new(RecordingService::default()),
        Record(seen_tx),
    )
    .await
    .expect("subscriber-only program");
    subscriber.readings.activate().await.expect("activation");
    let topic = device_address.get_resource_uri(schema::TEMPERATURE.resource());

    protocol
        .publish(&schema::Reading { value: 77 })
        .await
        .expect("protocol publish");
    let event = next(&mut seen).await;
    assert_eq!(event.value, 77);
    assert_eq!(event.kind, UMessageType::Publish);
    assert_eq!(event.source, topic);
    assert_eq!(event.encoding, Some(PayloadEncoding::PROTOBUF));
    assert_eq!(counted.completed_sends.load(Ordering::Acquire), 1);

    // The protocol surfaces the carriage's exact refusal; nothing is counted.
    inner.fail_sends(true);
    let error = protocol
        .publish(&schema::Reading { value: 78 })
        .await
        .expect_err("carriage refusal");
    assert!(matches!(
        &error,
        layers::LayerError::Transport(status) if *status == injected("send refused")
    ));
    assert_eq!(counted.completed_sends.load(Ordering::Acquire), 1);
    inner.fail_sends(false);

    // Direct L1 audience: hand-built metadata, decoded by the same subscriber.
    let carriage: &layers::CountedCarriage = &counted;
    layers::direct_l1_publish(carriage, &device_address, &schema::Reading { value: 79 })
        .await
        .expect("direct L1 publish");
    let event = next(&mut seen).await;
    assert_eq!((event.value, event.source), (79, topic));
    assert_eq!(counted.completed_sends.load(Ordering::Acquire), 2);
    subscriber
        .readings
        .unsubscribe()
        .await
        .expect("subscription cleanup");
    assert!(
        seen.try_recv().is_err(),
        "the refused publish was never delivered"
    );
}

#[tokio::test]
async fn common_service_cancellation_wrong_transport_and_busy_stop() {
    use up_rust::communication::application::{PublishError, PublisherDeployment};
    use up_rust::test_util::handoff::InMemoryBus;
    use up_rust::two_mode::{ClassicUri, SendError, StartupError, Subscriber, TransportSetup, TransportSetupExt};
    let bus = InMemoryBus::with_medium(8, 512, 1);
    let setup = TransportSetup::handoff(bus.participant(1)).unwrap();
    let receiver = TransportSetup::handoff(bus.participant(2)).unwrap();
    let other = TransportSetup::handoff(bus.participant(3)).unwrap();
    let address = address("device", 1);
    let core_address = ClassicUri(&address);
    let deployment = PublisherDeployment::new(&setup, &core_address, schema::Format::Protobuf);
    let wrong = apps::MultiplePublishers::prepare(&deployment).unwrap();
    let prepared = apps::MultiplePublishers::prepare(&deployment).unwrap();
    let temperatures = Subscriber::<schema::Reading>::on(&receiver, address.get_resource_uri(schema::TEMPERATURE_RESOURCE))
        .wire(schema::ReadingProtobuf).max_payload(64).build().unwrap();
    let pressures = Subscriber::<schema::Reading>::on(&receiver, address.get_resource_uri(schema::PRESSURE_RESOURCE))
        .wire(schema::ReadingProtobuf).max_payload(64).build().unwrap();
    let tx = setup.enter_runtime().unwrap();
    let rx = receiver.enter_runtime().unwrap();
    let wrong_runtime = other.enter_runtime().unwrap();
    assert!(matches!(wrong.activate(&wrong_runtime), Err(StartupError::WrongTransport)));
    let app = prepared.activate(&tx).unwrap();
    let temperatures = temperatures.activate(&rx).unwrap();
    let pressures = pressures.activate(&rx).unwrap();
    drop(app.publish_readings(&[1, 2], 3));
    assert!(temperatures.try_recv().unwrap().is_none());
    assert!(pressures.try_recv().unwrap().is_none());
    // First poll completes each synchronous send until medium capacity refuses.
    assert_eq!(app.publish_readings(&[21, 22, 23], 101).await, Err(PublishError::Send(SendError::Busy)));
    let held = temperatures.try_recv().unwrap().unwrap();
    assert_eq!(held.value, 21);
    drop(held);
    assert!(temperatures.try_recv().unwrap().is_none());
    assert!(pressures.try_recv().unwrap().is_none());
    // Failed/cancelled operations retain no handoff send slot; later calls work.
    app.publish_readings(&[], 101).await.unwrap();
    assert_eq!(pressures.try_recv().unwrap().unwrap().value, 101);
}

#[tokio::test]
async fn common_service_preserves_partial_discard_report_without_retry() {
    use up_rust::communication::application::{PublishError, PublisherDeployment};
    use up_rust::test_util::lending::LendingBus;
    use up_rust::two_mode::{ClassicUri, Qos, SendReport, Subscriber, TransportSetup, TransportSetupExt};
    let bus = LendingBus::new();
    let setup = TransportSetup::lending(bus.participant(1)).unwrap();
    let first = TransportSetup::lending(bus.participant(2)).unwrap();
    let second = TransportSetup::lending(bus.participant(3)).unwrap();
    let address = address("device", 1);
    let core_address = ClassicUri(&address);
    let deployment = PublisherDeployment::new(&setup, &core_address, schema::Format::Protobuf);
    let prepared = apps::MultiplePublishers::prepare(&deployment).unwrap();
    let topic = address.get_resource_uri(schema::TEMPERATURE_RESOURCE);
    let one = Subscriber::<schema::Reading>::on(&first, topic.clone()).wire(schema::ReadingProtobuf)
        .max_payload(64).qos(Qos { depth: 1, ..Qos::DEFAULT }).build().unwrap();
    let two = Subscriber::<schema::Reading>::on(&second, topic).wire(schema::ReadingProtobuf)
        .max_payload(64).qos(Qos { depth: 1, ..Qos::DEFAULT }).build().unwrap();
    let pressure = Subscriber::<schema::Reading>::on(&first, address.get_resource_uri(schema::PRESSURE_RESOURCE))
        .wire(schema::ReadingProtobuf).max_payload(64).build().unwrap();
    let tx = setup.enter_runtime().unwrap(); let first = first.enter_runtime().unwrap(); let second = second.enter_runtime().unwrap();
    let app = prepared.activate(&tx).unwrap(); let one = one.activate(&first).unwrap(); let two = two.activate(&second).unwrap();
    let pressure = pressure.activate(&first).unwrap();
    app.temperature.publish(&schema::Reading { value: 10 }).await.unwrap();
    drop(two.try_recv().unwrap().unwrap()); // one stays full; two has room.
    let mut expected = SendReport::default();
    expected.delivered = 2; expected.dropped_full = 1;
    assert_eq!(app.publish_readings(&[21, 22], 101).await, Err(PublishError::Dropped(expected)));
    assert_eq!(one.try_recv().unwrap().unwrap().value, 21);
    assert_eq!(two.try_recv().unwrap().unwrap().value, 21);
    assert!(one.try_recv().unwrap().is_none()); assert!(two.try_recv().unwrap().is_none());
    assert!(pressure.try_recv().unwrap().is_none());
    // The lending TX owner was returned on the error path; no retry was hidden.
    expected.dropped_full = 0;
    assert_eq!(app.temperature.publish_report(&schema::Reading { value: 31 }).await.unwrap(), expected);
}

#[tokio::test]
async fn common_publisher_keeps_exact_core_send_error() {
    use up_rust::communication::application::{PublishError, PublisherDeployment};
    use up_rust::test_util::handoff::{InMemoryBus, InMemoryOptions};
    use up_rust::two_mode::{ClassicUri, SendError, TransportSetup, TransportSetupExt};
    let bus = InMemoryBus::new(8, 512);
    let setup = TransportSetup::handoff(bus.participant(1).with_options(InMemoryOptions {
        fail_sends: Some((up_rust::UCode::Unavailable, "execution-package refusal")), ..Default::default()
    })).unwrap();
    let address = address("device", 1); let core_address = ClassicUri(&address);
    let deployment = PublisherDeployment::new(&setup, &core_address, schema::Format::Protobuf);
    let prepared = apps::MultiplePublishers::prepare(&deployment).unwrap();
    let runtime = setup.enter_runtime().unwrap(); let app = prepared.activate(&runtime).unwrap();
    let error = SendError::Rejected { code: up_rust::UCode::Unavailable, detail: "execution-package refusal" };
    assert_eq!(app.publish_readings(&[1, 2], 3).await, Err(PublishError::Send(error)));
    assert_eq!(app.temperature.publish_report(&schema::Reading { value: 1 }).await, Err(error));
}

#[test]
fn common_reading_wire_matches_pinned_protobuf_boundaries() {
    use protobuf::{Message, well_known_types::wrappers::UInt32Value};
    use std::mem::MaybeUninit;
    use up_rust_core::wire::{PayloadWriter, Wire};
    let wire = schema::ReadingProtobuf;
    for value in [0, 1, 0x7f, 0x80, 0x3fff, 0x4000, 0x1fffff, 0x200000, 0xfffffff, 0x10000000, u32::MAX] {
        let mut expected = UInt32Value::new(); expected.value = value;
        let expected = expected.write_to_bytes().unwrap();
        let mut bytes = [MaybeUninit::uninit(); 6]; let mut writer = PayloadWriter::new(&mut bytes);
        wire.encode(&schema::Reading { value }, &mut writer).unwrap();
        assert_eq!(writer.as_written(), expected, "canonical encode {value}");
        let mut decoded = schema::Reading { value: 77 };
        wire.decode(PayloadEncoding::PROTOBUF, &expected, &mut decoded).unwrap();
        assert_eq!(decoded.value, value);
        if !expected.is_empty() {
            let mut small = [MaybeUninit::uninit(); 6]; let mut writer = PayloadWriter::new(&mut small[..expected.len() - 1]);
            assert!(wire.encode(&schema::Reading { value }, &mut writer).is_err());
            assert!(writer.as_written().is_empty());
        }
    }
    let mut cases = vec![
        vec![], vec![8, 1, 8, 2], vec![16, 1, 8, 42], vec![10, 1, 0],
        vec![8, 0x80, 0], vec![8, 0xff, 0xff, 0xff, 0xff, 0x0f],
        vec![8, 0xff, 0xff, 0xff, 0xff, 0x10], vec![8, 0x80],
        vec![8, 0x80, 0x80, 0x80, 0x80, 0x80, 0],
        vec![18, 3, 1], vec![17, 0], vec![21, 0], vec![0], vec![15],
        vec![12], vec![11], vec![11, 20], vec![11, 19, 20],
        vec![16, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 1],
        vec![16, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 2],
    ];
    for depth in [100, 101, 102] {
        let mut group = vec![11; depth]; group.extend(std::iter::repeat_n(12, depth)); cases.push(group);
    }
    for bytes in cases {
        let expected = UInt32Value::parse_from_bytes(&bytes);
        let mut got = schema::Reading { value: 0x12345678 };
        let result = wire.decode(PayloadEncoding::PROTOBUF, &bytes, &mut got);
        assert_eq!(result.is_ok(), expected.is_ok(), "parse agreement {bytes:?}");
        if let Ok(expected) = expected { assert_eq!(got.value, expected.value, "decoded {bytes:?}"); }
        else { assert_eq!(got.value, 0x12345678, "refusal must leave destination intact"); }
    }
    let mut got = schema::Reading { value: 77 };
    assert!(wire.decode(PayloadEncoding::JSON, &[8, 42], &mut got).is_err());
    assert_eq!(got.value, 77);
}

#[global_allocator]
static CP_HEAP: up_rust::heap_guard::HeapGuard = up_rust::heap_guard::HeapGuard::new(std::alloc::System);

fn cp_ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())) {
        std::task::Poll::Ready(value) => value,
        std::task::Poll::Pending => panic!("common synchronous send unexpectedly suspended"),
    }
}
fn cp_runtime<T>(name: &str, operation: impl FnOnce() -> T) -> T {
    use up_rust::heap_guard::{HeapWindow,Scope};
    let window=HeapWindow::open_counts_only(Scope::Enrolled);
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation));
    let report=window.finish();
    println!("CP runtime {name}: {:?}; unwound={}",report.counts(),result.is_err());
    assert!(result.is_ok(),"runtime operation panicked");
    report.assert_no_heap(name);
    result.unwrap()
}

#[test]
fn cpr2_handoff_exact_zero_two_and_mixed_discards_without_runtime_heap() {
    use up_rust::communication::application::{PublisherDeployment,PublishError};
    use up_rust::test_util::handoff::InMemoryBus;
    use up_rust::two_mode::{ClassicUri,Qos,Overflow,SendReport,Subscriber,TransportSetup,TransportSetupExt};
    // Exact zero must not become one merely because the medium accepted send().
    let empty=InMemoryBus::new(8,512);let setup=TransportSetup::handoff(empty.participant(1)).unwrap();
    let source=address("device",1);let uri=ClassicUri(&source);
    let prepared=apps::MultiplePublishers::prepare(&PublisherDeployment::new(&setup,&uri,schema::Format::Protobuf)).unwrap();
    let tx=setup.enter_runtime().unwrap();let app=prepared.activate(&tx).unwrap();
    assert_eq!(cp_runtime("zero receivers",||cp_ready(app.temperature.publish_report(&schema::Reading{value:1}))).unwrap(),SendReport::exact(0,0));
    for overflow in [Overflow::DropNewest,Overflow::DropOldest] {
        let bus=InMemoryBus::new(8,512);let setup=TransportSetup::handoff(bus.participant(1)).unwrap();
        let a=TransportSetup::handoff(bus.participant(2)).unwrap();let b=TransportSetup::handoff(bus.participant(3)).unwrap();
        let prepared=apps::MultiplePublishers::prepare(&PublisherDeployment::new(&setup,&uri,schema::Format::Protobuf)).unwrap();
        let topic=source.get_resource_uri(schema::TEMPERATURE_RESOURCE);let qos=Qos{depth:1,overflow,..Qos::DEFAULT};
        let first=Subscriber::<schema::Reading>::on(&a,topic.clone()).wire(schema::ReadingProtobuf).max_payload(64).qos(qos).build().unwrap();
        let second=Subscriber::<schema::Reading>::on(&b,topic).wire(schema::ReadingProtobuf).max_payload(64).qos(qos).build().unwrap();
        let pressure=Subscriber::<schema::Reading>::on(&a,source.get_resource_uri(schema::PRESSURE_RESOURCE)).wire(schema::ReadingProtobuf).max_payload(64).build().unwrap();
        let tx=setup.enter_runtime().unwrap();let ar=a.enter_runtime().unwrap();let br=b.enter_runtime().unwrap();
        let app=prepared.activate(&tx).unwrap();let first=first.activate(&ar).unwrap();let second=second.activate(&br).unwrap();let pressure=pressure.activate(&ar).unwrap();
        assert_eq!(cp_runtime("two available receivers",||cp_ready(app.temperature.publish_report(&schema::Reading{value:10}))).unwrap(),SendReport::exact(2,0));
        drop(cp_runtime("drain second",||second.try_recv()).unwrap().unwrap());
        let expected=SendReport::exact(if overflow==Overflow::DropNewest {1}else{2},1);
        assert_eq!(cp_runtime("mixed full and available; service stops",||cp_ready(app.publish_readings(&[21,22],101))),Err(PublishError::Dropped(expected)));
        assert_eq!(first.try_recv().unwrap().unwrap().value,if overflow==Overflow::DropNewest{10}else{21});
        assert_eq!(second.try_recv().unwrap().unwrap().value,21);
        assert!(first.try_recv().unwrap().is_none());assert!(second.try_recv().unwrap().is_none());assert!(pressure.try_recv().unwrap().is_none());
        // The full-only case must report observed loss and never claim acceptance.
        cp_ready(app.temperature.publish(&schema::Reading{value:31})).unwrap();
        let report=cp_runtime("both full",||cp_ready(app.temperature.publish_report(&schema::Reading{value:32}))).unwrap();
        assert_eq!(report,SendReport::exact(if overflow==Overflow::DropNewest{0}else{2},2));
    }
}

#[test]
fn cpr2_handoff_refusal_preserves_accepted_prefix_and_original_error() {
    use up_rust::communication::application::{PublisherDeployment,PublishError};
    use up_rust::test_util::handoff::InMemoryBus;
    use up_rust::two_mode::{ClassicUri,SendError,SendReport,Subscriber,TransportSetup,TransportSetupExt};
    let bus=InMemoryBus::with_medium(8,512,1);let setup=TransportSetup::handoff(bus.participant(1)).unwrap();
    let a=TransportSetup::handoff(bus.participant(2)).unwrap();let b=TransportSetup::handoff(bus.participant(3)).unwrap();
    let source=address("device",1);let uri=ClassicUri(&source);
    let prepared=apps::MultiplePublishers::prepare(&PublisherDeployment::new(&setup,&uri,schema::Format::Protobuf)).unwrap();
    let topic=source.get_resource_uri(schema::TEMPERATURE_RESOURCE);
    let first=Subscriber::<schema::Reading>::on(&a,topic.clone()).wire(schema::ReadingProtobuf).max_payload(64).build().unwrap();
    let second=Subscriber::<schema::Reading>::on(&b,topic).wire(schema::ReadingProtobuf).max_payload(64).build().unwrap();
    let tx=setup.enter_runtime().unwrap();let ar=a.enter_runtime().unwrap();let br=b.enter_runtime().unwrap();let app=prepared.activate(&tx).unwrap();
    let first=first.activate(&ar).unwrap();let second=second.activate(&br).unwrap();
    let Err(PublishError::Send(error))=cp_runtime("Busy after first acceptance",||cp_ready(app.publish_readings(&[21,22],101))) else{panic!("missing refusal")};
    assert_eq!(error.cause(),SendError::Busy);assert_eq!(error.code(),SendError::Busy.code());assert_eq!(error.delivery_report(),Some(SendReport::exact(1,0)));
    assert_eq!(first.try_recv().unwrap().unwrap().value,21);assert!(first.try_recv().unwrap().is_none());assert!(second.try_recv().unwrap().is_none());
    // Retry remains an explicit caller decision. The service did not send22/101.
}

#[test]
fn cpr2_heap_counter_positive_control_counts_allocations_and_frees() {
    use up_rust::heap_guard::{HeapWindow,Scope};
    let w=HeapWindow::open_counts_only(Scope::Enrolled);
    let value=std::hint::black_box(Box::new([7_u8;64]));drop(std::hint::black_box(value));
    let r=w.finish();println!("CP allocation/free control: {:?}",r.counts());
    assert!(r.counts().allocations>=1);assert!(r.counts().deallocations>=1);
}

#[test]
fn cpr2_unavailable_receiver_counts_are_explicit() {
    use up_rust::communication::application::PublisherDeployment;
    use up_rust::test_util::carrier::ScriptedHandoff;
    use up_rust::two_mode::{ClassicUri,Envelope,SendReport,TransportSetup,TransportSetupExt};
    let (binding, control)=ScriptedHandoff::new(Envelope::NativeWholeFrame);
    let setup=TransportSetup::handoff(binding).unwrap();let source=address("device",1);let uri=ClassicUri(&source);
    let prepared=apps::MultiplePublishers::prepare(&PublisherDeployment::new(&setup,&uri,schema::Format::Protobuf)).unwrap();
    let tx=setup.enter_runtime().unwrap();let app=prepared.activate(&tx).unwrap();
    assert_eq!(cp_runtime("unavailable receiver counts",||cp_ready(app.temperature.publish_report(&schema::Reading{value:1}))).unwrap(),SendReport::unknown());
    let _ = control; // Capturing a local send is not observing remote recipients.
}

/// Fixed stack storage keeps formatter checks inside the runtime heap window.
struct CpReportText { bytes: [u8; 256], len: usize }
impl CpReportText {
    fn new() -> Self { Self { bytes: [0; 256], len: 0 } }
    fn text(&self) -> &str { std::str::from_utf8(&self.bytes[..self.len]).unwrap() }
}
impl std::fmt::Write for CpReportText {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        let end = self.len.checked_add(value.len()).ok_or(std::fmt::Error)?;
        let out = self.bytes.get_mut(self.len..end).ok_or(std::fmt::Error)?;
        out.copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[test]
fn cpr2_formatter_distinguishes_exact_and_incomplete_counts_without_heap() {
    use std::fmt::Write as _;
    use up_rust::communication::application::PublishError;
    use up_rust::two_mode::SendReport;
    let mut exact = CpReportText::new();
    cp_runtime("exact report formatting", || write!(&mut exact, "{}", PublishError::Dropped(SendReport::exact(2, 1)))).unwrap();
    assert_eq!(exact.text(), "publication accepted by 2 routes; 1 queue-full discards");
    let mut partial = SendReport::exact(2, 1);
    partial.append(SendReport::unknown());
    let mut incomplete = CpReportText::new();
    cp_runtime("incomplete report formatting", || write!(&mut incomplete, "{}", PublishError::Dropped(partial))).unwrap();
    assert_eq!(incomplete.text(), "publication accepted by at least 2 routes; at least 1 queue-full discards (incomplete counts)");
}

#[test]
fn cpr2_report_overflow_preserves_lower_bounds_and_observed_discards() {
    use up_rust::two_mode::SendReport;
    let (accept, discard, combined, unknown) = cp_runtime("report overflow and unknown composition", || {
        let mut accept = SendReport::exact(u64::MAX, 1);
        accept.accepted();
        let mut discard = SendReport::exact(1, u64::MAX);
        discard.discarded_full();
        let mut combined = SendReport::exact(u64::MAX - 1, u64::MAX - 1);
        combined.append(SendReport::exact(2, 2));
        let mut unknown = SendReport::exact(4, 3);
        unknown.append(SendReport::unknown());
        unknown.accepted();
        unknown.discarded_full();
        (accept, discard, combined, unknown)
    });
    assert_eq!((accept.delivered, accept.dropped_full, accept.counts_exact), (u64::MAX, 1, false));
    assert_eq!((discard.delivered, discard.dropped_full, discard.counts_exact), (1, u64::MAX, false));
    assert_eq!((combined.delivered, combined.dropped_full, combined.counts_exact), (u64::MAX, u64::MAX, false));
    assert_eq!((unknown.delivered, unknown.dropped_full, unknown.counts_exact), (5, 4, false));
    assert_eq!(SendReport::default(), SendReport::exact(0, 0));
}
