//! Run the same complete application using two real payload mappings.
//! InMemoryOwnedTransport and the subscription mock are demonstration fixtures,
//! never evidence for middleware-native acceptance.
#[allow(dead_code)]
#[path = "support/door.rs"]
mod door;
use std::sync::Arc;
use up_rust::{
    communication::SubscriptionStatus,
    core::usubscription::{MockUSubscription, USubscription},
    InMemoryOwnedTransport,
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
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), door::ExampleError> {
    let transport = Arc::new(InMemoryOwnedTransport::default());
    let proto =
        door::serialized_flow::<_, door::DoorProtobuf>(transport.clone(), service()).await?;
    let json = door::serialized_flow::<_, door::DoorJson>(transport, service()).await?;
    assert_eq!(proto.len(), 2);
    assert_eq!(json.len(), 2);
    Ok(())
}
