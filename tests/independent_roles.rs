//! Independently deployed role subsets, exercised over in-process fixtures.
//! This is not physical or cross-process evidence.
#[allow(dead_code)]
#[path = "../examples/support/independent_apps.rs"]
mod apps;
#[allow(dead_code)]
#[path = "../examples/support/independent_layers.rs"]
mod layers;
#[allow(dead_code)]
#[path = "../examples/support/independent_schema.rs"]
mod schema;

use std::sync::Arc;
use up_rust::{
    communication::application::*, InMemoryOwnedTransport, StaticUriProvider, UEntityId, UWireError,
};

up_rust::declare_operations! {}
mod event_only {
    up_rust::declare_operations! { pub event READING: super::schema::Reading = 0x8001; }
}
mod notifications_only {
    up_rust::declare_operations! {
        pub notification FIRST: super::schema::Reading = 0x8001;
        pub notification SECOND: super::schema::Reading = 0x8002;
    }
}
mod rpc_only {
    up_rust::declare_operations! {
        pub rpc FIRST: super::schema::Query => super::schema::Reading = 1;
        pub rpc SECOND: super::schema::Query => super::schema::Reading = 2;
    }
}

struct EncodeOnly;
impl EncodeProfile<schema::Reading> for EncodeOnly {
    fn encoder(&self) -> Result<Encoder<schema::Reading>, UWireError> {
        Encoder::from_schema_mapping::<schema::ReadingProtobuf>()
    }
}
struct DecodeOnly;
impl DecodeProfile<schema::Reading> for DecodeOnly {
    fn decoder(&self) -> Result<Decoder<schema::Reading>, UWireError> {
        Decoder::from_schema_mapping::<schema::ReadingProtobuf>()
    }
}
struct ClientDirections;
impl EncodeProfile<schema::Query> for ClientDirections {
    fn encoder(&self) -> Result<Encoder<schema::Query>, UWireError> {
        Encoder::from_schema_mapping::<schema::QueryProtobuf>()
    }
}
impl DecodeProfile<schema::Reading> for ClientDirections {
    fn decoder(&self) -> Result<Decoder<schema::Reading>, UWireError> {
        Decoder::from_schema_mapping::<schema::ReadingProtobuf>()
    }
}
struct ServerDirections;
impl DecodeProfile<schema::Query> for ServerDirections {
    fn decoder(&self) -> Result<Decoder<schema::Query>, UWireError> {
        Decoder::from_schema_mapping::<schema::QueryProtobuf>()
    }
}
impl EncodeProfile<schema::Reading> for ServerDirections {
    fn encoder(&self) -> Result<Encoder<schema::Reading>, UWireError> {
        Encoder::from_schema_mapping::<schema::ReadingProtobuf>()
    }
}

fn local() -> Deployment {
    Deployment::with_defaults(
        Arc::new(InMemoryOwnedTransport::default()),
        Arc::new(
            StaticUriProvider::for_entity("independent", UEntityId::first_instance(1), 1).unwrap(),
        ),
    )
    .unwrap()
}

#[test]
fn zero_one_and_repeated_declarations() {
    assert_eq!(event_only::READING.resource(), 0x8001);
    assert_ne!(
        notifications_only::FIRST.resource(),
        notifications_only::SECOND.resource()
    );
    assert_ne!(rpc_only::FIRST.resource(), rpc_only::SECOND.resource());
    assert_ne!(schema::TEMPERATURE.resource(), schema::PRESSURE.resource());
}

#[test]
fn publisher_needs_no_decoder_or_peer() {
    let local = BoundDeployment::new(local(), EncodeOnly);
    let _: Publisher<schema::Reading> = schema::TEMPERATURE
        .publisher(&local, Default::default())
        .unwrap();
}

#[test]
fn named_defaults_are_bounded_and_overridable() {
    let defaults = DeploymentOptions::default();
    assert_eq!(defaults.registration.capacity, 64);
    assert_eq!(defaults.payload_limit.max_payload_bytes(), 1024 * 1024);
    assert_eq!(OperationOptions::default().rpc.max_in_flight, 16);
    let mut invalid = defaults;
    invalid.registration.capacity = 0;
    assert!(Deployment::configured(
        Arc::new(InMemoryOwnedTransport::default()),
        Arc::new(
            StaticUriProvider::for_entity("independent", UEntityId::first_instance(1), 1).unwrap(),
        ),
        invalid,
    )
    .is_err());
}

#[tokio::test]
async fn multiple_publishers_and_direct_l1_without_receivers() {
    let local = schema::bind(local());
    let publishers = apps::MultiplePublishers::new(&local).unwrap();
    publishers
        .temperature
        .publish(&schema::Reading { value: 21 })
        .await
        .unwrap();
    publishers
        .pressure
        .publish(&schema::Reading { value: 101 })
        .await
        .unwrap();
    layers::direct_l1_publish(
        local.deployment().transport().as_ref(),
        local.address(),
        &schema::Reading { value: 22 },
    )
    .await
    .unwrap();
}

#[allow(dead_code)]
async fn subscriber_has_no_encoder<H: Handler<schema::Reading>>(
    local: Deployment,
    source: &dyn up_rust::LocalUriProvider,
    service: Arc<dyn up_rust::core::usubscription::USubscription>,
    handler: H,
) -> Result<Subscription, SerializedError> {
    schema::TEMPERATURE
        .subscription(
            &BoundDeployment::new(local, DecodeOnly),
            source,
            service,
            handler,
        )
        .await
}

#[allow(dead_code)]
async fn client_has_only_local_directions(
    local: Deployment,
    server: &dyn up_rust::LocalUriProvider,
) -> Result<RpcClient<schema::Query, schema::Reading>, SerializedError> {
    schema::READ
        .client(
            &BoundDeployment::new(local, ClientDirections),
            server,
            OperationOptions::default().rpc,
        )
        .await
}

#[allow(dead_code)]
async fn server_has_only_local_directions<H: RequestHandler<schema::Query, Reply = u32>>(
    local: Deployment,
    handler: H,
) -> Result<RpcServer, SerializedError> {
    schema::READ
        .server(
            &BoundDeployment::new(local, ServerDirections),
            handler,
            |value| schema::Reading { value },
            OperationOptions::default().server,
        )
        .await
}

#[test]
fn reusable_l2_protocol_is_not_a_business_handler() {
    let local = local();
    let _: layers::PublicationProtocol<schema::Reading> =
        layers::PublicationProtocol::for_schema::<schema::ReadingProtobuf>(
            local.transport().clone(),
            local.uri_provider(),
            &schema::TEMPERATURE,
        )
        .unwrap();
}
