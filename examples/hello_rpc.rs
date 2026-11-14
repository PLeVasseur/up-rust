//! An RPC client and server in one process.
//!
//! Run with `cargo run --example hello_rpc --features typed-communication,test-util`.
//! `InMemoryOwnedTransport` is an in-process test carriage; each side owns its
//! deployment and sees the other side only as an address.

use protobuf::well_known_types::wrappers::StringValue;
use std::sync::Arc;
use up_rust::communication::application::{Delivery, Deployment, OperationOptions, RequestHandler};
use up_rust::{InMemoryOwnedTransport, StaticUriProvider, UCode, UEntityId};

up_rust::declare_operations! {
    rpc SAY_HELLO: StringValue => StringValue = 1;
}

/// Builds one protobuf string message.
fn text(value: String) -> StringValue {
    StringValue {
        value,
        ..Default::default()
    }
}

/// The server's business logic: it decides the reply, the writer given at setup builds it.
struct Greeter;

impl RequestHandler<StringValue> for Greeter {
    type Reply = String;

    async fn handle<D: Delivery<StringValue>>(&mut self, request: D) -> Result<String, UCode> {
        let name = request.payload().map_err(|_| UCode::InvalidArgument)?;
        Ok(format!("hello, {}", name.value))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bus = Arc::new(InMemoryOwnedTransport::default());
    let service = StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(0x0100), 1)?;
    let service = Deployment::with_defaults(bus.clone(), Arc::new(service))?.protobuf();
    let caller = StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(0x0200), 1)?;
    let caller = Deployment::with_defaults(bus, Arc::new(caller))?.protobuf();
    let options = OperationOptions::default();

    let mut server = SAY_HELLO
        .server(&service, Greeter, text, options.server)
        .await?;
    let client = SAY_HELLO
        .client(&caller, service.address(), options.rpc)
        .await?;

    let pending = client.invoke(&text("uProtocol".to_owned())).await?;
    let reply = pending.response().await?;
    println!("server replied: {}", reply.payload()?.value);

    server.stop().await?;
    Ok(())
}
