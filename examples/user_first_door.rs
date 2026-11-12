//! Advanced lifecycle example: retained cleanup after a lost removal acknowledgement,
//! using bounded attempts. Start with `hello_publisher`, `hello_rpc` and the application
//! guide; come back here when a service needs this level of cleanup recovery.
#[allow(dead_code)]
#[path = "support/user_first_door.rs"]
mod door;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use up_rust::{
    communication::SubscriptionStatus,
    core::usubscription::{MockUSubscription, USubscription},
    InMemoryOwnedTransport, LocalUriProvider, UCode, UStatus, UUri,
};

fn service(subscriber: UUri) -> Arc<dyn USubscription> {
    let remote = Arc::new(AtomicBool::new(false));
    let mut service = MockUSubscription::new();
    let subscribed = remote.clone();
    service
        .expect_subscribe()
        .times(1)
        .returning(move |_, _, _| {
            subscribed.store(true, Ordering::Release);
            Ok(SubscriptionStatus::Subscribed)
        });
    let removed = remote.clone();
    service.expect_unsubscribe().times(1).returning(move |_| {
        removed.store(false, Ordering::Release);
        Err(UStatus::fail_with_code(
            UCode::Unavailable,
            "removal acknowledgement lost",
        ))
    });
    service.expect_fetch_subscribers().returning(move |_| {
        Ok(if remote.load(Ordering::Acquire) {
            vec![subscriber.clone()]
        } else {
            Vec::new()
        })
    });
    Arc::new(service)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), door::AppError> {
    for encoding in [door::Encoding::Protobuf, door::Encoding::Json] {
        let schema = door::deployment(Arc::new(InMemoryOwnedTransport::default()), encoding)?;
        let (_device, panel) = door::uris();
        let service = service(panel.get_source_uri());
        let mut setup = door::roles::Setup::default();
        let observed = Arc::new(Mutex::new(Vec::new()));
        if let Err(original) = door::start(&mut setup, &schema, service, observed).await {
            while setup.close(Duration::from_secs(2)).await == up_rust::Completion::Pending {
                eprintln!(
                    "retaining partial setup: original={original:?}; cleanup={:?}",
                    setup.cleanup
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            return Err(original);
        }
        let mut retained = door::Recovery::new(setup.finish()?);
        retained.run().await;
        while retained.app.close(Duration::from_secs(2)).await == up_rust::Completion::Pending {
            eprintln!(
                "retaining app: operation={:?}; cleanup={:?}",
                retained.operation, retained.app.cleanup
            );
            retained
                .app
                .subscription
                .observe_remote(Duration::from_secs(2))
                .await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        eprintln!(
            "resource completion={:?}; operation={:?}; cleanup={:?}",
            retained.app.completion(),
            retained.operation,
            retained.app.cleanup
        );
        if let Some(error) = retained.operation.take() {
            return Err(error);
        }
        if retained.app.completion() == up_rust::Completion::Failed {
            return Err(door::AppError::CleanupFailed);
        }
    }
    Ok(())
}
