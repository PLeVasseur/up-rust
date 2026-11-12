//! Controls for the SDK-owned role set: arbitrary subsets and repeated roles,
//! one attempt then cleanup-only, retained owners and original errors, bounded
//! retryable cleanup, and the primitives remaining usable on their own.
//! In-process fixtures only: not physical, cross-process or allocation evidence.
#[allow(dead_code)]
#[path = "support/independent_proof_carriage.rs"]
mod carriage;
#[cfg(feature = "up-l2-zero-copy")]
#[allow(dead_code)]
#[path = "support/independent_fault_core.rs"]
mod fault_core;
#[allow(dead_code)]
#[path = "../examples/support/independent_schema.rs"]
mod schema;
#[allow(dead_code)]
#[path = "support/independent_proof.rs"]
mod support;

use carriage::{injected, ScriptedCarriage};
use std::{sync::Arc, time::Duration};
use support::{address, Alarm, Request, Scale, Seen};
use tokio::sync::{mpsc, Notify};
use up_rust::{
    communication::{
        application::{
            acquire, Deployment, OperationOptions, Publisher, Receiver, RoleSetError, RoleSlot,
            RpcClient, RpcServer, SerializedCleanup, SerializedError, Subscription,
        },
        SubscriptionStatus,
    },
    core::usubscription::{ResetReason, SubscriptionInfo, USubscription},
    Completion, InMemoryOwnedTransport, LocalUriProvider, UStatus, UUri,
};

const BOUND: Duration = Duration::from_secs(1);

up_rust::role_set! {
    /// Repeated roles of one program: two receivers and two clients of the same kinds.
    mod gateway {
        relay: Publisher<schema::Reading>,
        alarms_a: Receiver,
        alarms_b: Receiver,
        read_a: RpcClient<schema::Query, schema::Reading>,
        read_b: RpcClient<schema::Query, schema::Reading>,
        backup: RpcServer,
    }
}

fn gateway_program(carriage: &Arc<ScriptedCarriage>) -> schema::LocalDeployment {
    schema::bind(
        Deployment::with_defaults(carriage.clone(), Arc::new(address("gateway", 3)))
            .expect("gateway deployment"),
    )
}

fn scripted() -> Arc<ScriptedCarriage> {
    Arc::new(ScriptedCarriage::new(Arc::new(
        InMemoryOwnedTransport::default(),
    )))
}

async fn fill_gateway(
    setup: &mut gateway::Setup,
    local: &schema::LocalDeployment,
    device_a: &dyn LocalUriProvider,
    device_b: &dyn LocalUriProvider,
    alarms: &mpsc::UnboundedSender<Seen>,
    requests: &mpsc::UnboundedSender<Request>,
) -> Result<(), SerializedError> {
    let options = OperationOptions::default();
    let slots = setup.begin()?;
    slots
        .relay
        .store(schema::TEMPERATURE.publisher(local, options.publish)?);
    slots
        .alarms_a
        .acquire(schema::ALARM.receiver(
            local,
            device_a,
            Alarm {
                seen: alarms.clone(),
                hold: None,
            },
        ))
        .await?;
    slots
        .alarms_b
        .acquire(schema::ALARM.receiver(
            local,
            device_b,
            Alarm {
                seen: alarms.clone(),
                hold: None,
            },
        ))
        .await?;
    slots
        .read_a
        .acquire(schema::READ.client(local, device_a, options.rpc))
        .await?;
    slots
        .read_b
        .acquire(schema::READ.client(local, device_b, options.rpc))
        .await?;
    slots
        .backup
        .acquire(schema::READ_BACKUP.server(
            local,
            Scale {
                factor: 1,
                seen: requests.clone(),
            },
            |value| schema::Reading { value },
            options.server,
        ))
        .await?;
    Ok(())
}

#[tokio::test]
async fn repeated_roles_are_acquired_and_closed_as_one_set() {
    let carriage = scripted();
    let local = gateway_program(&carriage);
    let (alarms, _alarm_events) = mpsc::unbounded_channel();
    let (requests, _request_events) = mpsc::unbounded_channel();
    let mut setup = gateway::Setup::default();
    fill_gateway(
        &mut setup,
        &local,
        &address("device-a", 1),
        &address("device-b", 2),
        &alarms,
        &requests,
    )
    .await
    .expect("every role acquired");
    assert_eq!(carriage.live(), 5);
    let mut app = setup.finish().expect("complete set");
    assert_eq!(app.close(BOUND).await, Completion::Succeeded);
    assert_eq!(app.completion(), Completion::Succeeded);
    assert_eq!(app.cleanup.alarms_b.completion, Completion::Succeeded);
    assert_eq!(carriage.live(), 0);
}

#[tokio::test]
async fn one_attempt_then_cleanup_only_and_incomplete_sets_keep_their_owners() {
    let carriage = scripted();
    let local = gateway_program(&carriage);
    let mut setup = gateway::Setup::default();
    let slots = setup.begin().expect("first attempt");
    slots.relay.store(
        schema::TEMPERATURE
            .publisher(&local, OperationOptions::default().publish)
            .expect("publisher"),
    );
    assert!(matches!(setup.begin(), Err(RoleSetError::AlreadyStarted)));
    assert!(matches!(
        setup.finish(),
        Err(RoleSetError::Missing { role: "alarms_a" })
    ));
    assert!(setup.slots().relay.is_held());
    assert_eq!(setup.close(BOUND).await, Completion::Succeeded);
    assert!(matches!(setup.begin(), Err(RoleSetError::CleanupStarted)));
    assert!(matches!(setup.finish(), Err(RoleSetError::CleanupStarted)));
    assert!(setup.slots().relay.is_held());
    assert!(matches!(
        SerializedError::from(RoleSetError::AlreadyStarted),
        SerializedError::Config(_)
    ));
}

#[tokio::test(start_paused = true)]
async fn failed_or_cancelled_setup_keeps_exact_owners_and_the_original_error() {
    for cancel in [false, true] {
        let carriage = scripted();
        let local = gateway_program(&carriage);
        if cancel {
            carriage.hold_attach(5);
        } else {
            carriage.fail_attach(5);
        }
        let (alarms, _alarm_events) = mpsc::unbounded_channel();
        let (requests, _request_events) = mpsc::unbounded_channel();
        let (peer_a, peer_b) = (address("device-a", 1), address("device-b", 2));
        let mut setup = gateway::Setup::default();
        let original = if cancel {
            tokio::select! {
                result = fill_gateway(&mut setup, &local, &peer_a, &peer_b, &alarms, &requests) => {
                    panic!("setup unexpectedly completed: {result:?}")
                }
                () = carriage.attach_entered.notified() => None,
            }
        } else {
            Some(
                fill_gateway(&mut setup, &local, &peer_a, &peer_b, &alarms, &requests)
                    .await
                    .expect_err("fifth attachment refused"),
            )
        };
        let slots = setup.slots();
        assert!(slots.relay.is_held() && slots.alarms_a.is_held() && slots.alarms_b.is_held());
        assert!(slots.read_a.is_held() && slots.read_b.is_held());
        assert!(!slots.backup.is_held());
        assert_eq!((carriage.attach_calls(), carriage.live()), (5, 4));
        if !cancel {
            assert!(
                matches!(&original, Some(SerializedError::Transport(status)) if *status == injected("attach#5"))
            );
        }

        // Cleanup-only: a refill is refused without touching the carriage.
        let effects = carriage.effects();
        assert!(matches!(
            fill_gateway(&mut setup, &local, &peer_a, &peer_b, &alarms, &requests).await,
            Err(SerializedError::Config(_))
        ));
        assert_eq!(carriage.effects(), effects);

        carriage.fail_detach(1);
        assert_eq!(setup.close(BOUND).await, Completion::Pending);
        let first = setup
            .cleanup
            .alarms_a
            .first_error
            .as_ref()
            .expect("first detach failure retained");
        assert_eq!(first.attempt, 1);
        assert!(matches!(&first.error.detach, Some(status) if *status == injected("detach#1")));
        carriage.clear_detach_failure();
        assert_eq!(setup.close(BOUND).await, Completion::Succeeded);
        assert_eq!(carriage.live(), 0);
        assert_eq!(setup.cleanup.alarms_a.attempt, 2);
        assert!(setup.cleanup.alarms_a.first_error.is_some());
        assert_eq!(setup.completion(), Completion::Succeeded);
        assert!(original.is_some() != cancel);
    }
}

/// Removal blocks until released, so one role can hang while the others finish.
#[derive(Default)]
struct HungRemoval {
    release: Notify,
}
#[async_trait::async_trait]
impl USubscription for HungRemoval {
    async fn subscribe(
        &self,
        _: &UUri,
        _: Option<u64>,
        _: Option<u32>,
    ) -> Result<SubscriptionStatus, UStatus> {
        Ok(SubscriptionStatus::Subscribed)
    }
    async fn unsubscribe(&self, _: &UUri) -> Result<(), UStatus> {
        self.release.notified().await;
        Ok(())
    }
    async fn fetch_subscribers(&self, _: &UUri) -> Result<Vec<UUri>, UStatus> {
        unreachable!("not used by this control")
    }
    async fn fetch_subscriptions_by_topic(
        &self,
        _: &UUri,
    ) -> Result<Vec<SubscriptionInfo>, UStatus> {
        unreachable!("not used by this control")
    }
    async fn fetch_subscriptions_by_subscriber(
        &self,
        _: &UUri,
    ) -> Result<Vec<SubscriptionInfo>, UStatus> {
        unreachable!("not used by this control")
    }
    async fn register_for_notifications(&self, _: &UUri) -> Result<(), UStatus> {
        unreachable!("not used by this control")
    }
    async fn unregister_for_notifications(&self, _: &UUri) -> Result<(), UStatus> {
        unreachable!("not used by this control")
    }
    async fn reset(&self, _: ResetReason, _: Option<String>) -> Result<(), UStatus> {
        unreachable!("not used by this control")
    }
}

up_rust::role_set! {
    /// One subscription and one server; each closes under its own deadline.
    mod watcher {
        events: Subscription,
        server: RpcServer,
    }
}

#[tokio::test(start_paused = true)]
async fn a_hung_role_cannot_starve_the_others_and_retry_resolves_it() {
    let carriage = scripted();
    let local = gateway_program(&carriage);
    let (alarms, _alarm_events) = mpsc::unbounded_channel();
    let (requests, _request_events) = mpsc::unbounded_channel();
    let service = Arc::new(HungRemoval::default());
    let mut setup = watcher::Setup::default();
    let slots = setup.begin().expect("first attempt");
    slots
        .events
        .acquire(schema::TEMPERATURE.subscription(
            &local,
            &address("device-a", 1),
            service.clone(),
            Alarm {
                seen: alarms,
                hold: None,
            },
        ))
        .await
        .expect("subscription");
    slots
        .server
        .acquire(schema::READ_BACKUP.server(
            &local,
            Scale {
                factor: 1,
                seen: requests,
            },
            |value| schema::Reading { value },
            OperationOptions::default().server,
        ))
        .await
        .expect("server");
    let mut app = setup.finish().expect("complete set");
    app.events.activate().await.expect("activation");

    assert_eq!(app.close(BOUND).await, Completion::Pending);
    assert!(app.cleanup.events.timed_out);
    assert_eq!(app.cleanup.server.completion, Completion::Succeeded);
    service.release.notify_one();
    assert_eq!(app.close(BOUND).await, Completion::Succeeded);
    assert!(!app.cleanup.events.timed_out);
    assert_eq!(app.completion(), Completion::Succeeded);
    assert_eq!(carriage.live(), 0);
}

#[tokio::test]
async fn acquire_keeps_an_existing_owner_and_reports_the_original_error() {
    let mut slot = RoleSlot::<u32>::default();
    let refused: Result<&mut u32, u8> = slot.acquire(async { Err(9) }).await;
    assert_eq!(refused, Err(9));
    assert!(!slot.is_held());
    assert_eq!(slot.acquire(async { Ok::<_, u8>(1) }).await, Ok(&mut 1));
    assert_eq!(slot.acquire(async { Ok::<_, u8>(2) }).await, Ok(&mut 1));
    assert_eq!(slot.store(3), &mut 1);
    assert_eq!(slot.take(), Some(1));
    assert!(!slot.is_held());
}

#[tokio::test]
async fn the_primitives_still_compose_without_the_role_set() {
    let carriage = scripted();
    let local = gateway_program(&carriage);
    let (requests, _request_events) = mpsc::unbounded_channel();
    let mut server: Option<RpcServer> = None;
    acquire(
        &mut server,
        schema::READ_BACKUP.server(
            &local,
            Scale {
                factor: 1,
                seen: requests,
            },
            |value| schema::Reading { value },
            OperationOptions::default().server,
        ),
    )
    .await
    .expect("server");
    assert_eq!(carriage.live(), 1);
    let mut cleanup = SerializedCleanup::default();
    let completion = cleanup
        .close::<schema::Query, schema::Reading>(None, None, server.as_mut(), None, BOUND)
        .await;
    assert_eq!(completion, Completion::Succeeded);
    assert_eq!(cleanup.server.completion, Completion::Succeeded);
    assert_eq!(carriage.live(), 0);
}

#[cfg(feature = "up-l2-zero-copy")]
mod native {
    use super::*;
    use crate::fault_core::{FaultCore, ATTACH_REFUSED, DETACH_REFUSED};
    use up_rust::{
        communication::application::native as binding, NativeLoanCore, NativeProfile,
        NativeProfileAgreement, PayloadEncoding, PoolCore, PoolCoreConfig, SetupError,
        StablePayload,
    };

    up_rust::role_set! {
        /// The same shape as the serialized gateway, over any native core.
        mod gateway<Core: NativeLoanCore> {
            relay: binding::Publisher<Core, schema::Reading>,
            alarms_a: binding::Receiver<Core>,
            alarms_b: binding::Receiver<Core>,
            read_a: binding::RpcClient<Core, schema::Query, schema::Reading>,
            read_b: binding::RpcClient<Core, schema::Query, schema::Reading>,
            backup: binding::RpcServer<Core, schema::Reading>,
        }
    }

    fn agreement() -> NativeProfileAgreement {
        let profile = NativeProfile::new(
            "role-set",
            1,
            PayloadEncoding::private_use(0xF100),
            [
                schema::Reading::native_representation(),
                schema::Query::native_representation(),
            ],
        )
        .expect("native profile");
        NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).expect("agreement")
    }

    fn fault_core() -> Arc<FaultCore> {
        Arc::new(FaultCore::new(Arc::new(
            PoolCore::new(PoolCoreConfig {
                slots: 32,
                slot_bytes: 4096,
                alignment: 8,
                max_routes: 32,
                max_registrations: 16,
            })
            .expect("heap-backed fixture pool"),
        )))
    }

    async fn fill(
        setup: &mut gateway::Setup<FaultCore>,
        local: &binding::Deployment<FaultCore>,
        device_a: &dyn LocalUriProvider,
        device_b: &dyn LocalUriProvider,
        alarms: &mpsc::UnboundedSender<Seen>,
        requests: &mpsc::UnboundedSender<Request>,
    ) -> Result<(), SetupError> {
        let options = OperationOptions::default();
        let slots = setup.begin()?;
        slots
            .relay
            .store(schema::TEMPERATURE.native_publisher(local, options.publish)?);
        slots
            .alarms_a
            .acquire(schema::ALARM.native_receiver(
                local,
                device_a,
                Alarm {
                    seen: alarms.clone(),
                    hold: None,
                },
            ))
            .await?;
        slots
            .alarms_b
            .acquire(schema::ALARM.native_receiver(
                local,
                device_b,
                Alarm {
                    seen: alarms.clone(),
                    hold: None,
                },
            ))
            .await?;
        slots
            .read_a
            .acquire(schema::READ.native_client(local, device_a, options.rpc))
            .await?;
        slots
            .read_b
            .acquire(schema::READ.native_client(local, device_b, options.rpc))
            .await?;
        slots
            .backup
            .acquire(schema::READ_BACKUP.native_server(
                local,
                Scale {
                    factor: 1,
                    seen: requests.clone(),
                },
                |value, loan| loan.into_initializer().value(value).finish(),
                options.server,
            ))
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn with_defaults_deployment_acquires_and_closes_a_repeated_native_set() {
        let core = fault_core();
        let local = binding::Deployment::with_defaults(
            Arc::clone(&core),
            agreement(),
            Arc::new(address("gateway", 3)),
        )
        .expect("native defaults");
        let (alarms, _alarm_events) = mpsc::unbounded_channel();
        let (requests, _request_events) = mpsc::unbounded_channel();
        let mut setup = gateway::Setup::default();
        fill(
            &mut setup,
            &local,
            &address("device-a", 1),
            &address("device-b", 2),
            &alarms,
            &requests,
        )
        .await
        .expect("every role acquired");
        assert_eq!(core.live(), 5);
        let mut app = setup.finish().expect("complete set");
        assert_eq!(app.close(BOUND).await, Completion::Succeeded);
        assert_eq!(core.live(), 0);
        assert_eq!(core.pool.free_slots(), 32);
    }

    #[tokio::test]
    async fn failed_native_setup_keeps_exact_owners_and_retries_cleanup() {
        let core = fault_core();
        core.fail_attach(5);
        let local = binding::Deployment::with_defaults(
            Arc::clone(&core),
            agreement(),
            Arc::new(address("gateway", 3)),
        )
        .expect("native defaults");
        let (alarms, _alarm_events) = mpsc::unbounded_channel();
        let (requests, _request_events) = mpsc::unbounded_channel();
        let (peer_a, peer_b) = (address("device-a", 1), address("device-b", 2));
        let mut setup = gateway::Setup::default();
        let original = fill(&mut setup, &local, &peer_a, &peer_b, &alarms, &requests)
            .await
            .expect_err("fifth attachment refused");
        assert_eq!(original, ATTACH_REFUSED);
        assert!(setup.slots().read_b.is_held() && !setup.slots().backup.is_held());
        assert_eq!((core.attach_calls(), core.live()), (5, 4));
        assert!(matches!(setup.begin(), Err(RoleSetError::AlreadyStarted)));
        assert_eq!(
            SetupError::from(RoleSetError::AlreadyStarted),
            SetupError::InvalidConfig(
                "setup already started; clean up retained owners and create a new setup"
            )
        );

        core.fail_detach(1);
        assert_eq!(setup.close(BOUND).await, Completion::Pending);
        let first = setup
            .cleanup
            .alarms_a
            .first_error
            .as_ref()
            .expect("first detach failure retained");
        assert_eq!(first.attempt, 1);
        assert_eq!(
            *first.error,
            up_rust::communication::zero_copy::ReceiverStopError::Detach(DETACH_REFUSED)
        );
        core.clear_detach_failure();
        assert_eq!(setup.close(BOUND).await, Completion::Succeeded);
        assert_eq!(core.live(), 0);
        assert!(setup.cleanup.alarms_a.first_error.is_some());
    }
}
