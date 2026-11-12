//! Native independent-role controls using the SDK-generated role names.
//!
//! WITNESS LABEL — FIXTURE, NOT PHYSICAL PROOF: every native core here wraps
//! the SDK's heap-backed `PoolCore`. `FixtureCore`
//! (tests/support/native_fixture.rs) and `FaultCore`
//! (tests/support/independent_fault_core.rs) are explicit test-only
//! `NativeLoanCore` witnesses. Each program builds its OWN wire adapter and
//! agreement over the shared pool and learns peers only as addresses. No
//! installed middleware, physical zero-copy, retained-RX lifetime or
//! allocation acceptance follows from these controls.
#[allow(dead_code)]
#[path = "../examples/support/independent_native.rs"]
mod apps;
#[allow(dead_code)]
#[path = "../examples/support/user_first_door.rs"]
mod door;
#[allow(dead_code)]
#[path = "support/independent_fault_core.rs"]
mod fault_core;
#[allow(dead_code)]
#[path = "../examples/support/user_first_meter.rs"]
mod meter;
#[path = "support/native_fixture.rs"]
mod native_fixture;
#[allow(dead_code)]
#[path = "../examples/support/independent_schema.rs"]
mod schema;
#[allow(dead_code)]
#[path = "support/independent_proof.rs"]
mod support;

use fault_core::{FaultCore, ATTACH_REFUSED, DETACH_REFUSED};
use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};
use support::{address, next, Alarm, Gate, Record, RecordingService, Request, Scale, Seen, BOUND};
use tokio::sync::mpsc;
use up_rust::{
    communication::{
        application::{
            acquire,
            native::{self, Receive, Serve},
            CleanupLeg, Delivery, OperationOptions, RoleSetError, ServerOptions,
        },
        typed::NativeConfig,
        zero_copy::{ReceiveConfig, ReceiverStopError},
    },
    Completion, HeaderLimits, LocalUriProvider, NativeProfile, NativeProfileAgreement,
    OverflowPolicy, PayloadEncoding, PoolCore, PoolCoreConfig, RegistrationConfig, RouteConfig,
    RpcServerStopError, SetupError, StableContainerWire, StablePayload, TokenError,
    UFrameFieldsMetadataCodec, UMessageType, UUri, UWireTransport,
};

up_rust::native_roles! {
    mod gateway {
        core: crate::fault_core::FaultCore;
        publisher Relay, relay: super::schema::TEMPERATURE => super::schema::Reading;
        receiver Alarms, alarms: super::schema::ALARM => super::schema::Reading;
        client Read, read, ReadResponse: super::schema::READ => super::schema::Query, super::schema::Reading;
        server Backup, backup: super::schema::READ_BACKUP => super::schema::Query, super::schema::Reading;
        reply u32 => |value, loan| loan.into_initializer().value(value).finish();
    }
}

const SLOTS: usize = 32;

/// The shared heap-backed medium; each program still builds its own adapter.
fn pool() -> Arc<PoolCore> {
    Arc::new(
        PoolCore::new(PoolCoreConfig {
            slots: SLOTS,
            slot_bytes: 4096,
            alignment: 8,
            max_routes: 32,
            max_registrations: 16,
        })
        .expect("heap-backed fixture pool"),
    )
}

fn config() -> NativeConfig {
    NativeConfig {
        route: RouteConfig::default(),
        receive: ReceiveConfig {
            registration: RegistrationConfig {
                capacity: 8,
                overflow: OverflowPolicy::DropNewest,
            },
            header: HeaderLimits { max_token_len: 32 },
        },
    }
}

fn independent_profile() -> NativeProfile {
    NativeProfile::new(
        "independent-proof",
        1,
        PayloadEncoding::private_use(0xF100),
        [
            schema::Reading::native_representation(),
            schema::Query::native_representation(),
        ],
    )
    .expect("independent native profile")
}

fn meter_profile(domain: &str) -> NativeProfile {
    NativeProfile::new(
        domain,
        1,
        PayloadEncoding::private_use(0xF100),
        [
            meter::Reading::native_representation(),
            meter::SetLevel::native_representation(),
        ],
    )
    .expect("meter native profile")
}

fn door_profile() -> NativeProfile {
    NativeProfile::new(
        "vehicle",
        1,
        PayloadEncoding::private_use(0xF100),
        [
            door::DoorState::native_representation(),
            door::SetDoor::native_representation(),
        ],
    )
    .expect("door native profile")
}

/// Local declaration agreed with an independently provisioned peer copy.
fn agreement(local: NativeProfile) -> NativeProfileAgreement {
    let peer = local.clone();
    NativeProfileAgreement::new(Arc::new(local), &peer).expect("native agreement")
}

/// One FixtureCore program: its own adapter and agreement over the shared pool.
fn fixture_local(
    pool: &Arc<PoolCore>,
    authority: &str,
    service: u16,
) -> apps::selected::Deployment {
    let transport = Arc::new(UWireTransport::with_native_agreement(
        Arc::new(native_fixture::FixtureCore(Arc::clone(pool))),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        agreement(independent_profile()),
    ));
    apps::selected::Deployment::new(transport, config(), Arc::new(address(authority, service)))
        .expect("fixture deployment")
}

/// One FaultCore program with the given native declaration.
fn fault_local(
    core: &Arc<FaultCore>,
    profile: NativeProfile,
    authority: &str,
    service: u16,
) -> native::Deployment<FaultCore> {
    let transport = Arc::new(UWireTransport::with_native_agreement(
        Arc::clone(core),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        agreement(profile),
    ));
    native::Deployment::new(transport, config(), Arc::new(address(authority, service)))
        .expect("fault deployment")
}

async fn optional<E>(stop: Option<impl Future<Output = Result<(), E>>>) -> Result<(), E> {
    match stop {
        Some(stop) => stop.await,
        None => Ok(()),
    }
}

#[tokio::test]
async fn generated_native_publisher_and_subscriber_run_as_independent_programs() {
    let pool = pool();
    // Publisher program: generated names, one local deployment, no subscriber.
    let device = fixture_local(&pool, "device", 1);
    let publishers = apps::MultiplePublishers::new(&device).expect("publisher-only program");
    // Subscriber program: its own adapter and agreement, the publisher's ADDRESS.
    let panel = fixture_local(&pool, "panel", 2);
    let device_address = address("device", 1);
    let service = Arc::new(RecordingService::default());
    let (seen_tx, mut seen) = mpsc::unbounded_channel();
    let mut subscriber =
        apps::SubscriberOnly::prepare(&panel, &device_address, service.clone(), Record(seen_tx))
            .await
            .expect("subscriber-only program");
    subscriber.readings.activate().await.expect("activation");
    let topic = device_address.get_resource_uri(schema::TEMPERATURE.resource());
    assert_eq!(service.subscribed(), std::slice::from_ref(&topic));

    for value in [31, 32, 33] {
        publishers
            .temperature
            .publish_in_place(|loan| loan.into_initializer().value(value).finish())
            .expect("synchronous in-place publication");
    }
    publishers
        .pressure
        .publish_in_place(|loan| loan.into_initializer().value(900).finish())
        .expect("second generated publisher");
    for expected in [31, 32, 33] {
        let event = next(&mut seen).await;
        assert_eq!(event.value, expected);
        assert_eq!(event.kind, UMessageType::Publish);
        assert_eq!(event.source, topic);
        assert_eq!(event.sink, None);
        assert!(
            event.native_token,
            "native representation identity travelled"
        );
    }
    subscriber
        .readings
        .unsubscribe()
        .await
        .expect("native subscription cleanup");
    assert_eq!(service.unsubscribed(), [topic]);
    assert!(
        seen.try_recv().is_err(),
        "pressure is an independent resource"
    );
    drop(subscriber);
    drop(publishers);
    assert_eq!(pool.free_slots(), SLOTS);
}

#[tokio::test]
async fn generated_native_notifier_and_receiver_run_as_independent_programs() {
    let pool = pool();
    let panel_address = address("panel", 2);
    // Notifier program: the destination is routing data, not a receiver.
    let device = fixture_local(&pool, "device", 1);
    let notifier = apps::selected::alarm_sender(&device, panel_address.get_source_uri())
        .expect("notifier-only program");
    // Receiver program: the source is routing data, not a notifier.
    let panel = fixture_local(&pool, "panel", 2);
    let device_address = address("device", 1);
    let (seen_tx, mut seen) = mpsc::unbounded_channel();
    let mut receiver = apps::selected::alarm_receiver(&panel, &device_address, Record(seen_tx))
        .await
        .expect("receiver-only program");
    for value in [41, 42] {
        notifier
            .notify_in_place(|loan| loan.into_initializer().value(value).finish())
            .expect("synchronous in-place notification");
    }
    for expected in [41, 42] {
        let notice = next(&mut seen).await;
        assert_eq!(notice.value, expected);
        assert_eq!(notice.kind, UMessageType::Notification);
        assert_eq!(
            notice.source,
            device_address.get_resource_uri(schema::ALARM.resource())
        );
        assert_eq!(notice.sink, Some(panel_address.get_source_uri()));
        assert!(notice.native_token);
    }
    Receive::stop(&mut receiver)
        .await
        .expect("native receiver cleanup");
    drop(receiver);
    drop(notifier);
    assert_eq!(pool.free_slots(), SLOTS);
}

#[tokio::test]
async fn generated_native_client_and_server_correlate_and_retain_responses() {
    let pool = pool();
    // Server program: generated server name and its in-place reply initializer.
    let device = fixture_local(&pool, "device", 1);
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
    // Client program: its own adapter and only the server's ADDRESS.
    let panel = fixture_local(&pool, "panel", 2);
    let device_address = address("device", 1);
    let mut client = apps::ClientOnly::prepare(&panel, &device_address)
        .await
        .expect("client-only program");
    client
        .read
        .set_token("panel-credential")
        .expect("bounded credential");

    // Two concurrently pending native calls, awaited out of order.
    let first = client
        .read
        .invoke_in_place(|loan| loan.into_initializer().slot(1).finish())
        .expect("first call");
    let second = client
        .read
        .invoke_in_place(|loan| loan.into_initializer().slot(2).finish())
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
        assert_eq!(
            response.payload().expect("checked response").value,
            expected
        );
        let context = response.context().expect("response context");
        assert_eq!(context.kind, UMessageType::Response);
        assert_eq!(context.source, method);
        assert_eq!(context.sink, Some(reply_to.clone()));
        assert_eq!(context.request_id, Some(request.id.clone()));
        assert!(context.native_type_token.is_some());
        assert_eq!(request.source, reply_to);
        assert_eq!(request.sink, Some(method.clone()));
        assert_eq!(request.token.as_deref(), Some("panel-credential"));
    }
    assert_ne!(one.id, two.id);

    // A repeated operation through the stored client and its retained owner.
    client.request(3).await.expect("stored client request");
    assert_eq!(
        client
            .latest()
            .expect("checked retained response")
            .map(|reading| reading.value),
        Some(30)
    );
    assert_eq!(next(&mut requests).await.slot, 3);

    // Shutdown and deployment drop do not invalidate retained native responses.
    client.read.close().expect("client close");
    Serve::stop(&mut server.read).await.expect("server cleanup");
    drop(server);
    drop(device);
    drop(panel);
    assert_eq!(
        client
            .latest()
            .expect("checked retained response")
            .map(|reading| reading.value),
        Some(30)
    );
    assert_eq!(first.payload().expect("retained").value, 10);
    assert_eq!(second.payload().expect("retained").value, 20);
    drop(first);
    drop(second);
    drop(client);
    assert_eq!(pool.free_slots(), SLOTS);
}

/// Native gateway subset in caller-retained optional slots: generated names
/// for two receivers and two clients of the same kinds, a publisher and a late
/// server. Only `acquire` and `CleanupLeg` are used.
#[derive(Default)]
struct NativeGateway {
    relay: Option<gateway::Relay>,
    alarms_a: Option<gateway::Alarms>,
    alarms_b: Option<gateway::Alarms>,
    read_a: Option<gateway::Read>,
    read_b: Option<gateway::Read>,
    backup: Option<gateway::Backup>,
    cleanup: NativeGatewayCleanup,
}

#[derive(Default)]
struct NativeGatewayCleanup {
    alarms_a: CleanupLeg<ReceiverStopError>,
    alarms_b: CleanupLeg<ReceiverStopError>,
    read_a: CleanupLeg<SetupError>,
    read_b: CleanupLeg<SetupError>,
    backup: CleanupLeg<RpcServerStopError>,
}

impl NativeGatewayCleanup {
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

impl NativeGateway {
    async fn fill(
        &mut self,
        local: &gateway::Deployment,
        device_a: &dyn LocalUriProvider,
        device_b: &dyn LocalUriProvider,
        alarms: &mpsc::UnboundedSender<Seen>,
        gate: &Arc<Gate>,
        requests: &mpsc::UnboundedSender<Request>,
    ) -> Result<(), SetupError> {
        if self.relay.is_none() {
            self.relay = Some(gateway::relay(local)?);
        }
        acquire(
            &mut self.alarms_a,
            gateway::alarms(
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
            gateway::alarms(
                local,
                device_b,
                Alarm {
                    seen: alarms.clone(),
                    hold: Some(Arc::clone(gate)),
                },
            ),
        )
        .await?;
        acquire(&mut self.read_a, gateway::read(local, device_a)).await?;
        acquire(&mut self.read_b, gateway::read(local, device_b)).await?;
        acquire(
            &mut self.backup,
            gateway::backup(
                local,
                Scale {
                    factor: 1,
                    seen: requests.clone(),
                },
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
        let stop_a = alarms_a.as_mut().map(Receive::stop);
        let stop_b = alarms_b.as_mut().map(Receive::stop);
        cleanup.read_a.record(
            read_a.as_ref().map_or(Ok(()), |client| client.close()),
            |_| Completion::Pending,
        );
        cleanup.read_b.record(
            read_b.as_ref().map_or(Ok(()), |client| client.close()),
            |_| Completion::Pending,
        );
        let stop_backup = backup.as_mut().map(Serve::stop);
        async move {
            tokio::join!(
                cleanup
                    .alarms_a
                    .run(deadline, optional(stop_a), ReceiverStopError::completion),
                cleanup
                    .alarms_b
                    .run(deadline, optional(stop_b), ReceiverStopError::completion),
                cleanup.backup.run(
                    deadline,
                    optional(stop_backup),
                    RpcServerStopError::completion
                ),
            );
            cleanup.completion()
        }
    }
}

#[tokio::test(start_paused = true)]
async fn native_repeated_roles_survive_failed_or_cancelled_later_acquisition() {
    for cancel in [false, true] {
        let pool = pool();
        let gateway_address = address("gateway", 3);
        let peer_a = address("device-a", 1);
        let peer_b = address("device-b", 2);
        // Two independently deployed FixtureCore devices; each knows only the
        // gateway's address.
        let device_a = fixture_local(&pool, "device-a", 1);
        let device_b = fixture_local(&pool, "device-b", 2);
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
        let alarm_a = apps::selected::alarm_sender(&device_a, gateway_address.get_source_uri())
            .expect("device A notifier-only program");
        let alarm_b = apps::selected::alarm_sender(&device_b, gateway_address.get_source_uri())
            .expect("device B notifier-only program");

        // Gateway program over the fault witness and the same shared pool.
        let core = Arc::new(FaultCore::new(Arc::clone(&pool)));
        let local = fault_local(&core, independent_profile(), "gateway", 3);
        if cancel {
            core.hold_attach(5);
        } else {
            core.fail_attach(5);
        }
        let (alarms_tx, mut alarms) = mpsc::unbounded_channel();
        let gate = Arc::new(Gate::default());
        let mut setup = NativeGateway::default();
        let original = if cancel {
            tokio::select! {
                result = setup.fill(&local, &peer_a, &peer_b, &alarms_tx, &gate, &requests_tx) => {
                    panic!("native gateway setup unexpectedly completed: {result:?}")
                }
                () = core.attach_entered.notified() => None,
            }
        } else {
            Some(
                setup
                    .fill(&local, &peer_a, &peer_b, &alarms_tx, &gate, &requests_tx)
                    .await
                    .expect_err("late server acquisition refused"),
            )
        };
        assert!(setup.relay.is_some() && setup.alarms_a.is_some() && setup.alarms_b.is_some());
        assert!(setup.read_a.is_some() && setup.read_b.is_some() && setup.backup.is_none());
        assert_eq!((core.attach_calls(), core.live()), (5, 4));

        // Every retained owner still operates on the exact attachment it acquired.
        alarm_a
            .notify_in_place(|loan| loan.into_initializer().value(1).finish())
            .expect("device A alarm");
        let notice = next(&mut alarms).await;
        assert_eq!(notice.value, 1);
        assert_eq!(
            notice.source,
            peer_a.get_resource_uri(schema::ALARM.resource())
        );
        alarm_b
            .notify_in_place(|loan| loan.into_initializer().value(2).finish())
            .expect("device B alarm");
        let notice = next(&mut alarms).await;
        assert_eq!(notice.value, 2);
        assert_eq!(
            notice.source,
            peer_b.get_resource_uri(schema::ALARM.resource())
        );
        // Device B's receiver now retains its native delivery inside the handler.
        gate.entered.notified().await;
        let response_a = setup
            .read_a
            .as_ref()
            .expect("retained client A")
            .invoke_in_place(|loan| loan.into_initializer().slot(3).finish())
            .expect("call A")
            .response()
            .await
            .expect("response A");
        let response_b = setup
            .read_b
            .as_ref()
            .expect("retained client B")
            .invoke_in_place(|loan| loan.into_initializer().slot(3).finish())
            .expect("call B")
            .response()
            .await
            .expect("response B");
        assert_eq!(response_a.payload().expect("checked").value, 30);
        assert_eq!(response_b.payload().expect("checked").value, 60);
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
        core.fail_detach(1);
        assert_eq!(
            setup.close(Duration::from_millis(200)).await,
            Completion::Pending
        );
        assert_eq!(core.detach_log(), [1, 2, 3, 4]);
        assert_eq!(core.live(), 1);
        {
            let cleanup = &setup.cleanup;
            assert_eq!(cleanup.alarms_a.completion, Completion::Pending);
            assert_eq!(
                cleanup
                    .alarms_a
                    .first_error
                    .as_ref()
                    .map(|failure| (failure.attempt, *failure.error)),
                Some((1, ReceiverStopError::Detach(DETACH_REFUSED)))
            );
            assert!(cleanup.alarms_b.timed_out && cleanup.alarms_b.current.is_none());
            assert_eq!(cleanup.alarms_b.completion, Completion::Pending);
            assert_eq!(cleanup.read_a.completion, Completion::Succeeded);
            assert_eq!(cleanup.read_b.completion, Completion::Succeeded);
            assert_eq!(cleanup.backup.completion, Completion::Succeeded);
        }
        assert_eq!(
            setup
                .alarms_a
                .as_ref()
                .expect("exact owner retained")
                .stop_progress()
                .detach
                .first
                .as_ref()
                .map(|failure| (failure.attempt, failure.error)),
            Some((1, DETACH_REFUSED))
        );

        // Attempt 2 retries the same owners; only the failed attachment detaches.
        core.clear_detach_failure();
        gate.release.notify_one();
        assert_eq!(setup.close(BOUND).await, Completion::Succeeded);
        assert_eq!(core.detach_log(), [1, 2, 3, 4, 1]);
        assert_eq!(core.live(), 0);
        assert_eq!(
            setup
                .cleanup
                .alarms_a
                .first_error
                .as_ref()
                .map(|failure| (failure.attempt, *failure.error)),
            Some((1, ReceiverStopError::Detach(DETACH_REFUSED)))
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
            assert_eq!(original, Some(ATTACH_REFUSED));
        }
        assert!(original.is_some() != cancel);
        // Retained native responses outlive the gateway's cleanup.
        assert_eq!(response_a.payload().expect("retained").value, 30);
        Serve::stop(&mut server_a.read)
            .await
            .expect("device A cleanup");
        Serve::stop(&mut server_b.read)
            .await
            .expect("device B cleanup");
        drop(response_a);
        drop(response_b);
        drop(setup);
        drop(alarm_a);
        drop(alarm_b);
        assert_eq!(pool.free_slots(), SLOTS);
    }
}

/// A native server setup with a post-acquisition configuration step.
#[derive(Default)]
struct NativeKnownClients {
    server: Option<gateway::Backup>,
    cleanup: CleanupLeg<RpcServerStopError>,
}

impl NativeKnownClients {
    async fn fill(
        &mut self,
        local: &gateway::Deployment,
        known: &[UUri],
        handler: Scale,
    ) -> Result<(), SetupError> {
        // Advanced options stay on the primitive: one reserved client route.
        acquire(
            &mut self.server,
            schema::READ.native_server(
                local,
                handler,
                |value, loan| loan.into_initializer().value(value).finish(),
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
async fn native_server_prepare_client_refusal_after_acquisition_keeps_exact_owner() {
    let pool = pool();
    let core = Arc::new(FaultCore::new(Arc::clone(&pool)));
    let local = fault_local(&core, independent_profile(), "device", 1);
    let panel_a = address("panel-a", 2);
    let panel_b = address("panel-b", 3);
    let (requests_tx, mut requests) = mpsc::unbounded_channel();
    let mut setup = NativeKnownClients::default();
    let refused = setup
        .fill(
            &local,
            &[panel_a.get_source_uri(), panel_b.get_source_uri()],
            Scale {
                factor: 3,
                seen: requests_tx,
            },
        )
        .await
        .expect_err("second known client exceeds the reserved route capacity");
    assert_eq!(refused, SetupError::TableFull);
    assert!(setup.server.is_some());
    assert_eq!((core.attach_calls(), core.live()), (1, 1));

    // The retained exact server still serves the client it prepared.
    let client_program = fixture_local(&pool, "panel-a", 2);
    let client = apps::selected::read_client(&client_program, &address("device", 1))
        .await
        .expect("client-only program");
    let reply = client
        .invoke_in_place(|loan| loan.into_initializer().slot(5).finish())
        .expect("call")
        .response()
        .await
        .expect("prepared client served");
    assert_eq!(reply.payload().expect("checked").value, 15);
    assert_eq!(next(&mut requests).await.slot, 5);
    client.close().expect("client close");

    // Exact, retryable cleanup of the retained owner.
    core.fail_detach(1);
    setup.cleanup.begin();
    setup
        .cleanup
        .run(
            BOUND,
            optional(setup.server.as_mut().map(Serve::stop)),
            RpcServerStopError::completion,
        )
        .await;
    assert_eq!(setup.cleanup.completion, Completion::Pending);
    core.clear_detach_failure();
    setup.cleanup.begin();
    setup
        .cleanup
        .run(
            BOUND,
            optional(setup.server.as_mut().map(Serve::stop)),
            RpcServerStopError::completion,
        )
        .await;
    assert_eq!(setup.cleanup.completion, Completion::Succeeded);
    assert_eq!(
        setup
            .cleanup
            .first_error
            .as_ref()
            .map(|failure| (failure.attempt, failure.error.detach)),
        Some((1, Some(DETACH_REFUSED)))
    );
    assert_eq!(core.detach_log(), [1, 1]);
    assert_eq!(core.live(), 0);
    drop(reply);
    drop(client);
    drop(setup);
    assert_eq!(pool.free_slots(), SLOTS);
}

#[tokio::test]
async fn native_door_setup_token_refusal_after_client_acquisition_is_cleanup_only() {
    let core = Arc::new(FaultCore::new(pool()));
    let transport = Arc::new(UWireTransport::with_native_agreement(
        Arc::clone(&core),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        agreement(door_profile()),
    ));
    let mut rpc = door::rpc_options();
    // Shorter than the Door credential: refusal happens after acquisition.
    rpc.max_token_len = 4;
    let options = OperationOptions {
        rpc,
        ..OperationOptions::default()
    };
    let binding =
        door::native::NativeDoor::bind(transport, config(), options).expect("native door binding");
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut setup = door::native::NativeDoorSetup::default();
    let original = door::native::start(
        &mut setup,
        &binding,
        Arc::new(RecordingService::default()),
        observed.clone(),
    )
    .await
    .expect_err("credential refused after client acquisition");
    assert!(matches!(
        original,
        door::native::NativeAppError::Token(TokenError::TooLong)
    ));
    let slots = setup.slots();
    assert!(slots.state.is_held() && slots.changed.is_held() && slots.set.is_held());
    assert!(!slots.server.is_held() && !slots.notifications.is_held());
    assert!(!slots.subscription.is_held());
    assert_eq!((core.attach_calls(), core.live()), (1, 1));

    // The interrupted setup is cleanup-only: a refill creates no attachment.
    let again = door::native::start(
        &mut setup,
        &binding,
        Arc::new(RecordingService::default()),
        observed,
    )
    .await
    .expect_err("cleanup-only setup");
    assert!(matches!(
        again,
        door::native::NativeAppError::RoleSet(RoleSetError::AlreadyStarted)
    ));
    assert_eq!(core.attach_calls(), 1);
    assert!(setup.finish().is_err());

    core.fail_detach(1);
    assert_eq!(setup.close(BOUND).await, Completion::Pending);
    assert_eq!(
        setup
            .cleanup
            .set
            .first_error
            .as_ref()
            .map(|failure| (failure.attempt, *failure.error)),
        Some((1, DETACH_REFUSED))
    );
    core.clear_detach_failure();
    assert_eq!(setup.close(BOUND).await, Completion::Succeeded);
    assert_eq!(core.detach_log(), [1, 1]);
    assert_eq!(core.live(), 0);
    assert_eq!(
        setup
            .cleanup
            .set
            .first_error
            .as_ref()
            .map(|failure| (failure.attempt, *failure.error)),
        Some((1, DETACH_REFUSED))
    );
    assert!(matches!(
        original,
        door::native::NativeAppError::Token(TokenError::TooLong)
    ));
}

#[tokio::test]
async fn native_meter_setup_refuses_changed_deployment_and_keeps_interrupted_owners() {
    for cancel in [false, true] {
        let core_a = Arc::new(FaultCore::new(pool()));
        let core_b = Arc::new(FaultCore::new(pool()));
        // Same Rust deployment type; B has a different core, pool and agreement.
        let device_a = fault_local(&core_a, meter_profile("meter-a"), "meter", 1);
        let panel_a = fault_local(&core_a, meter_profile("meter-a"), "meter", 2);
        let device_b = fault_local(&core_b, meter_profile("meter-b"), "meter", 1);
        let panel_b = fault_local(&core_b, meter_profile("meter-b"), "meter", 2);
        let options = OperationOptions::default();
        if cancel {
            core_a.hold_attach(2);
        } else {
            core_a.fail_attach(2);
        }
        let mut setup = meter::native::MeterSetup::<FaultCore>::default();
        let original = if cancel {
            tokio::select! {
                result = meter::native::start(&mut setup, &device_a, &panel_a, options, Arc::new(RecordingService::default()), Arc::default(), Arc::default()) => {
                    panic!("native meter setup unexpectedly completed: {result:?}")
                }
                () = core_a.attach_entered.notified() => None,
            }
        } else {
            Some(
                meter::native::start(
                    &mut setup,
                    &device_a,
                    &panel_a,
                    options,
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
        assert_eq!((core_a.attach_calls(), core_a.live()), (2, 1));

        let refused = meter::native::start(
            &mut setup,
            &device_b,
            &panel_b,
            options,
            Arc::new(RecordingService::default()),
            Arc::default(),
            Arc::default(),
        )
        .await
        .expect_err("changed deployment refused");
        assert!(matches!(refused, SetupError::InvalidConfig(_)));
        assert_eq!((core_b.attach_calls(), core_b.live()), (0, 0));
        assert_eq!(core_a.attach_calls(), 2);

        core_a.fail_detach(1);
        assert_eq!(setup.close(BOUND).await, Completion::Pending);
        assert_eq!(
            setup
                .cleanup
                .client
                .first_error
                .as_ref()
                .map(|failure| (failure.attempt, *failure.error)),
            Some((1, DETACH_REFUSED))
        );
        core_a.clear_detach_failure();
        assert_eq!(setup.close(BOUND).await, Completion::Succeeded);
        assert_eq!(core_a.detach_log(), [1, 1]);
        assert_eq!(core_a.live(), 0);
        assert_eq!(
            setup
                .cleanup
                .client
                .first_error
                .as_ref()
                .map(|failure| (failure.attempt, *failure.error)),
            Some((1, DETACH_REFUSED))
        );
        if !cancel {
            assert_eq!(original, Some(ATTACH_REFUSED));
        }
        assert!(original.is_some() != cancel);
        assert_eq!((core_b.attach_calls(), core_b.live()), (0, 0));
    }
}
