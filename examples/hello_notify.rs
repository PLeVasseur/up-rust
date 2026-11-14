//! A one-way notification between two addressed programs in one process.
//!
//! Run with `cargo run --example hello_notify --features typed-communication,test-util`.
//! `InMemoryOwnedTransport` is an in-process test carriage; each side owns its
//! deployment and sees the other side only as an address.

use protobuf::well_known_types::wrappers::StringValue;
use std::sync::Arc;
use up_rust::communication::application::{Delivery, Deployment, Handler, PublishOptions};
use up_rust::{InMemoryOwnedTransport, StaticUriProvider, UEntityId};

up_rust::declare_operations! {
    notification ALARM: StringValue = 0x8002;
}

/// The receiving side's business logic.
struct PrintAlarm;

impl Handler<StringValue> for PrintAlarm {
    async fn on_receive<D: Delivery<StringValue>>(&mut self, delivery: D) {
        match delivery.payload() {
            Ok(alarm) => println!("alarm received: {}", alarm.value),
            Err(error) => eprintln!("rejected a delivery: {error}"),
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bus = Arc::new(InMemoryOwnedTransport::default());
    let sensor = StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(0x0100), 1)?;
    let sensor = Deployment::with_defaults(bus.clone(), Arc::new(sensor))?.protobuf();
    let panel = StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(0x0200), 1)?;
    let panel = Deployment::with_defaults(bus, Arc::new(panel))?.protobuf();

    // The receiver needs the sender's address, never its deployment.
    let mut receiver = ALARM.receiver(&panel, sensor.address(), PrintAlarm).await?;
    let notifier = ALARM.notifier(
        &sensor,
        panel.address().get_source_uri(),
        PublishOptions::default(),
    )?;

    let message = StringValue {
        value: "door ajar".to_owned(),
        ..Default::default()
    };
    notifier.notify(&message).await?;

    // Stopping drains what was already accepted, so the handler has printed by now.
    receiver.stop().await?;
    Ok(())
}
