use super::*;
use up_rust::communication::application::RoleSetError;

#[tokio::test]
async fn native_outer_cancel_retains_combined_original_errors_and_retry_outcome() {
    let (core, binding) = setup();
    let mut service = MockUSubscription::new();
    service.expect_subscribe().times(1).returning(|_, _, _| {
        Err(UStatus::fail_with_code(
            UCode::Unavailable,
            "native activation original",
        ))
    });
    service.expect_unsubscribe().times(1).returning(|_| {
        Err(UStatus::fail_with_code(
            UCode::Unavailable,
            "native removal original",
        ))
    });
    service
        .expect_fetch_subscribers()
        .times(1)
        .returning(|_| Ok(Vec::new()));
    let service = Arc::new(service);
    let mut setup = NativeDoorSetup::default();
    start(&mut setup, &binding, service.clone(), Default::default())
        .await
        .unwrap();
    let mut app = setup.finish().unwrap();
    app.notifications.stop().await.unwrap();
    let control = Control::new(false);
    app.notifications = binding
        .changed_receiver(Waiting(control.clone()))
        .await
        .unwrap();
    app.changed
        .notify_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    bounded(control.entered.notified()).await;
    let original = app.activate().await.unwrap_err();
    core.fail_detach.store(true, Ordering::SeqCst);
    poll_once_pending(app.close(CLOSE_DEADLINE)).await;
    assert_eq!(
        app.notifications
            .stop_progress()
            .detach
            .first
            .as_ref()
            .unwrap()
            .error,
        SetupError::Worker
    );
    assert_eq!(
        app.notifications
            .stop_progress()
            .detach
            .first
            .as_ref()
            .unwrap()
            .attempt,
        1
    );
    assert!(app.subscription.service_history().first.is_some());
    assert!(app.cleanup.set.first_error.is_some());
    assert!(app.cleanup.notifications.current.is_none());
    let subscribers = service
        .fetch_subscribers(&door::uris().0.get_resource_uri(door::STATE.resource()))
        .await
        .unwrap();
    assert!(subscribers.is_empty());
    app.subscription.reconcile(SubscriptionStatus::Unsubscribed);
    core.fail_detach.store(false, Ordering::SeqCst);
    control.release.notify_one();
    assert_eq!(
        bounded(app.close(CLOSE_DEADLINE)).await,
        Completion::Succeeded
    );
    assert!(
        matches!(original, up_rust::communication::typed::SubscriptionActivationError::Service(status) if status == UStatus::fail_with_code(UCode::Unavailable, "native activation original"))
    );
    assert_eq!(
        app.subscription
            .service_history()
            .first
            .as_ref()
            .unwrap()
            .error,
        UStatus::fail_with_code(UCode::Unavailable, "native removal original")
    );
    assert_eq!(
        app.notifications
            .stop_progress()
            .detach
            .first
            .as_ref()
            .unwrap()
            .attempt,
        1
    );
    assert!(app.notifications.stop_progress().detach.latest.is_none());
    assert_eq!(core.pool.free_slots(), 16);
}

#[tokio::test]
async fn native_partial_setup_failed_or_cancelled_retains_first_exact_attachment() {
    for cancel in [false, true] {
        let (core, binding) = setup();
        if cancel {
            core.hold_attach.store(2, Ordering::SeqCst);
        } else {
            core.fail_attach.store(2, Ordering::SeqCst);
        }
        let (other_core, other_binding) = setup();
        let mut setup = NativeDoorSetup::default();
        let mut original = None;
        if cancel {
            tokio::select! {
                result = start(&mut setup, &binding, Arc::new(MockUSubscription::new()), Default::default()) => panic!("setup unexpectedly completed: {result:?}"),
                () = core.attach_entered.notified() => {},
            }
        } else {
            original = Some(
                start(
                    &mut setup,
                    &binding,
                    Arc::new(MockUSubscription::new()),
                    Default::default(),
                )
                .await
                .unwrap_err(),
            );
        }
        assert!(setup.slots().set.is_held());
        assert!(!setup.slots().server.is_held());
        let before = other_core.next_id.load(Ordering::SeqCst);
        assert!(matches!(
            start(
                &mut setup,
                &other_binding,
                Arc::new(MockUSubscription::new()),
                Default::default()
            )
            .await,
            Err(door::native::NativeAppError::RoleSet(
                RoleSetError::AlreadyStarted
            ))
        ));
        assert_eq!(other_core.next_id.load(Ordering::SeqCst), before);

        core.fail_detach.store(true, Ordering::SeqCst);
        assert_eq!(
            bounded(setup.close(Duration::from_secs(10))).await,
            Completion::Pending
        );
        assert_eq!(setup.cleanup.set.first_error.as_ref().unwrap().attempt, 1);
        assert_eq!(
            *setup.cleanup.set.first_error.as_ref().unwrap().error,
            SetupError::Worker
        );
        core.fail_detach.store(false, Ordering::SeqCst);
        assert_eq!(
            bounded(setup.close(Duration::from_secs(10))).await,
            Completion::Succeeded
        );
        assert_eq!(core.attempts.lock().unwrap().as_slice(), &[1, 1]);
        assert!(setup.cleanup.set.first_error.is_some());
        if !cancel {
            assert!(matches!(
                original,
                Some(door::native::NativeAppError::Setup(SetupError::Worker))
            ));
        }
    }
}

#[tokio::test]
async fn native_handler_panic_reaches_finite_failed_completion_with_original_error() {
    let (core, binding) = setup();
    let mut service = MockUSubscription::new();
    service.expect_subscribe().times(1).returning(|_, _, _| {
        Err(UStatus::fail_with_code(
            UCode::Unavailable,
            "original operation",
        ))
    });
    service.expect_unsubscribe().times(1).returning(|_| Ok(()));
    let mut setup = NativeDoorSetup::default();
    start(&mut setup, &binding, Arc::new(service), Default::default())
        .await
        .unwrap();
    let mut app = setup.finish().unwrap();
    app.notifications.stop().await.unwrap();
    let control = Control::new(true);
    app.notifications = binding
        .changed_receiver(Waiting(control.clone()))
        .await
        .unwrap();
    app.changed
        .notify_in_place(|loan| loan.into_initializer().open(true).finish())
        .unwrap();
    bounded(control.entered.notified()).await;
    let original = app.activate().await.unwrap_err();
    control.release.notify_one();
    assert_eq!(bounded(app.close(CLOSE_DEADLINE)).await, Completion::Failed);
    assert_eq!(
        app.notifications
            .stop_progress()
            .worker
            .first
            .as_ref()
            .unwrap()
            .error,
        RegistrationStopError::HandlerPanicked
    );
    assert!(matches!(
        original,
        up_rust::communication::typed::SubscriptionActivationError::Service(_)
    ));
    assert_eq!(bounded(app.close(CLOSE_DEADLINE)).await, Completion::Failed);
    assert_eq!(core.pool.free_slots(), 16);
}
