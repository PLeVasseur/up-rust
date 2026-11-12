//! The smallest uProtocol application: declare one event and publish it.
//!
//! Run with `cargo run --example hello_publisher --features typed-communication,test-util`.
//! `InMemoryOwnedTransport` is an in-process test carriage; a real program passes its
//! binding's transport to `Deployment::with_defaults` instead.

use protobuf::well_known_types::wrappers::StringValue;
use std::sync::Arc;
use up_rust::communication::application::{Deployment, PublishOptions};
use up_rust::{InMemoryOwnedTransport, StaticUriProvider, UEntityId};

up_rust::declare_operations! {
    event DOOR_STATUS: StringValue = 0x8001;
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Who this program is: entity type 0x1234, its first instance, major version 1.
    let address = StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(0x1234), 1)?;
    let transport = Arc::new(InMemoryOwnedTransport::default());
    let local = Deployment::with_defaults(transport, Arc::new(address))?.protobuf();

    let status = DOOR_STATUS.publisher(&local, PublishOptions::default())?;
    let message = StringValue {
        value: "door closed".to_owned(),
        ..Default::default()
    };
    // Publishing needs no subscriber: no role requires an opposite peer.
    status.publish(&message).await?;
    println!("published: {}", message.value);
    Ok(())
}
