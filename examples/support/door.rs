//! One application with real Protobuf/JSON mappings and unchanged business logic.
use bytes::Bytes;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use up_rust::{
    communication::typed::*, core::usubscription::USubscription, DecodePayload, EncodePayload,
    LocalUriProvider, OverflowPolicy, PayloadCodec, PayloadCodecIdentity, PayloadDecodeLimit,
    PayloadEncoding, PayloadLayout, ProtobufPayload, RegistrationConfig, StaticUriProvider, UCode,
    UEntityId, UOwnedTransport, UPriority, UWireError,
};

#[repr(C)]
#[derive(Debug, PartialEq)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    derive(up_rust::StablePayload, up_rust::StablePayloadInit)
)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    stable_payload(type_name = "vehicle.DoorState", revision = 1)
)]
pub(crate) struct DoorState {
    pub(crate) open: bool,
}
#[repr(C)]
#[derive(Debug, PartialEq)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    derive(up_rust::StablePayload, up_rust::StablePayloadInit)
)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    stable_payload(type_name = "vehicle.SetDoor", revision = 1)
)]
pub(crate) struct SetDoor {
    pub(crate) open: bool,
}

/// Application schema mapping: both domain types have an agreed boolean field.
/// The route, not encoding/size inference, selects which schema is expected.
pub(crate) trait DoorValue {
    fn open(&self) -> bool;
    fn from_open(open: bool) -> Self;
}
impl DoorValue for DoorState {
    fn open(&self) -> bool {
        self.open
    }
    fn from_open(open: bool) -> Self {
        Self { open }
    }
}
impl DoorValue for SetDoor {
    fn open(&self) -> bool {
        self.open
    }
    fn from_open(open: bool) -> Self {
        Self { open }
    }
}

#[derive(Debug)]
pub(crate) struct DoorProtobuf;
impl PayloadCodecIdentity for DoorProtobuf {
    fn payload_family() -> Option<up_rust::WireIdentity> {
        Some(up_rust::PROTOBUF_PAYLOAD_FAMILY_ID)
    }
    fn name() -> &'static str {
        "vehicle.door.protobuf.BoolValue.v1"
    }
    fn encoding() -> PayloadEncoding {
        PayloadEncoding::PROTOBUF
    }
}
fn proto<T: DoorValue>(value: &T) -> protobuf::well_known_types::wrappers::BoolValue {
    let mut wire = protobuf::well_known_types::wrappers::BoolValue::new();
    wire.value = value.open();
    wire
}
impl<T: DoorValue> EncodePayload<T> for DoorProtobuf {
    fn payload_layout(value: &T) -> Result<PayloadLayout, UWireError> {
        ProtobufPayload::payload_layout(&proto(value))
    }
    fn encode_payload(value: &T, dst: &mut [u8]) -> Result<(), UWireError> {
        ProtobufPayload::encode_payload(&proto(value), dst)
    }
    fn encode_payload_owned(value: &T) -> Result<Bytes, UWireError> {
        ProtobufPayload::encode_payload_owned(&proto(value))
    }
}
impl<T: DoorValue> DecodePayload<'_, T> for DoorProtobuf {
    fn decode_payload(src: &[u8]) -> Result<T, UWireError> {
        let wire: protobuf::well_known_types::wrappers::BoolValue =
            ProtobufPayload::decode_payload(src)?;
        Ok(T::from_open(wire.value))
    }
}

#[derive(Debug)]
pub(crate) struct DoorJson;
// Deployment-private fixture family, declared in examples/declarations/door-json.json.
// The alias is not a registered assignment; peers must match the full literal.
pub(crate) const DOOR_JSON_FAMILY: up_rust::WireIdentity =
    up_rust::WireIdentity::new("example.vehicle.door.json-v1", 0xF101);
impl PayloadCodecIdentity for DoorJson {
    fn payload_family() -> Option<up_rust::WireIdentity> {
        Some(DOOR_JSON_FAMILY)
    }
    fn name() -> &'static str {
        "vehicle.door.json.open.v1"
    }
    fn encoding() -> PayloadEncoding {
        PayloadEncoding::JSON
    }
}
impl<T: DoorValue> EncodePayload<T> for DoorJson {
    fn payload_layout(value: &T) -> Result<PayloadLayout, UWireError> {
        PayloadLayout::new(Self::encode_payload_owned(value)?.len(), 1)
    }
    fn encode_payload(value: &T, dst: &mut [u8]) -> Result<(), UWireError> {
        let bytes = Self::encode_payload_owned(value)?;
        if dst.len() != bytes.len() {
            return Err(UWireError::invalid_payload("JSON destination extent"));
        }
        dst.copy_from_slice(&bytes);
        Ok(())
    }
    fn encode_payload_owned(value: &T) -> Result<Bytes, UWireError> {
        serde_json::to_vec(&serde_json::json!({"open": value.open()}))
            .map(Bytes::from)
            .map_err(|e| UWireError::invalid_payload(e.to_string()))
    }
}
impl<T: DoorValue> DecodePayload<'_, T> for DoorJson {
    fn decode_payload(src: &[u8]) -> Result<T, UWireError> {
        let value: serde_json::Value =
            serde_json::from_slice(src).map_err(|e| UWireError::invalid_payload(e.to_string()))?;
        let object = value
            .as_object()
            .ok_or_else(|| UWireError::invalid_payload("door JSON object required"))?;
        if object.len() != 1 {
            return Err(UWireError::invalid_payload(
                "door JSON requires exactly open",
            ));
        }
        let open = object
            .get("open")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| UWireError::invalid_payload("door open must be boolean"))?;
        Ok(T::from_open(open))
    }
}

#[derive(Debug)]
pub(crate) struct DoorObserver(pub(crate) Arc<Mutex<Vec<bool>>>);
impl Handler<DoorState> for DoorObserver {
    async fn on_receive<D: Delivery<DoorState>>(&mut self, delivery: D) {
        // A native delivery re-checks on every borrow; report a failed check, never panic.
        match delivery.payload() {
            Ok(state) => {
                // The borrow remains bound to its retained delivery across this await.
                tokio::task::yield_now().await;
                if let Ok(mut observed) = self.0.lock() {
                    observed.push(state.open);
                }
            }
            Err(error) => eprintln!("door observer rejected a delivery: {error}"),
        }
    }
}
#[derive(Debug)]
pub(crate) struct DoorController;
impl RequestHandler<SetDoor> for DoorController {
    type Reply = bool;
    async fn handle<D: Delivery<SetDoor>>(&mut self, request: D) -> Result<bool, UCode> {
        if request.context().map_err(|_| UCode::InvalidArgument)?.token != Some("door-control") {
            return Err(UCode::PermissionDenied);
        }
        let open = request.payload().map_err(|_| UCode::InvalidArgument)?.open;
        tokio::task::yield_now().await;
        Ok(open)
    }
}

pub(crate) fn registration() -> RegistrationConfig {
    RegistrationConfig {
        capacity: 8,
        overflow: OverflowPolicy::DropNewest,
    }
}
pub(crate) fn rpc_options() -> RpcOptions {
    RpcOptions {
        max_in_flight: 4,
        ttl: Duration::from_secs(2),
        max_token_len: 32,
        priority: UPriority::CS4,
    }
}
pub(crate) type ExampleError = Box<dyn std::error::Error + Send + Sync>;
pub(crate) fn uris() -> (Arc<StaticUriProvider>, Arc<StaticUriProvider>) {
    (
        Arc::new(
            StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(1), 1).unwrap(),
        ),
        Arc::new(
            StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(2), 1).unwrap(),
        ),
    )
}

/// Complete five-role flow. Changing K changes only setup/mapping, not this body.
/// Changing T changes only the compatible configured carriage supplied here.
pub(crate) async fn serialized_flow<T, K>(
    transport: Arc<T>,
    service: Arc<dyn USubscription>,
) -> Result<Vec<bool>, ExampleError>
where
    T: UOwnedTransport + 'static,
    K: PayloadCodec
        + EncodePayload<DoorState>
        + EncodePayload<SetDoor>
        + for<'a> DecodePayload<'a, DoorState>
        + for<'a> DecodePayload<'a, SetDoor>
        + 'static,
{
    let (door_uri, panel_uri) = uris();
    let setup = Serialized::<_, K>::new(transport, PayloadDecodeLimit::new(1024), registration())?;
    let door = Endpoint::new(setup.clone(), door_uri.clone());
    let panel = Endpoint::new(setup, panel_uri.clone());
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut subscription = panel
        .subscriber(service)
        .subscription::<DoorState, _>(
            door_uri.get_resource_uri(0x8001),
            DoorObserver(observed.clone()),
        )
        .await?;
    subscription.activate().await?;
    let mut receiver = panel
        .listen::<DoorState, _>(
            door_uri.get_resource_uri(0x8002),
            DoorObserver(observed.clone()),
        )
        .await?;
    let publisher = door.publisher::<DoorState>(0x8001, PublishOptions::default())?;
    let notifier = door.notifier::<DoorState>(
        0x8002,
        panel_uri.get_source_uri(),
        PublishOptions::default(),
    )?;
    let mut server = door
        .rpc_server::<SetDoor, DoorState, _, _>(
            0x42,
            DoorController,
            |open| DoorState { open },
            ServerOptions { max_clients: 4 },
        )
        .await?;
    server.prepare_client(&panel_uri.get_source_uri())?;
    let client = panel
        .rpc_client::<SetDoor, DoorState>(door_uri.get_resource_uri(0x42), rpc_options())
        .await?;
    client.set_token("door-control")?;
    publisher.publish(&DoorState { open: false }).await?;
    notifier.notify(&DoorState { open: true }).await?;
    let pending = client.invoke(&SetDoor { open: true }).await?;
    let response = pending.response().await?;
    assert!(response.payload()?.open);
    subscription.unsubscribe().await?;
    receiver.stop().await?;
    server.stop().await?;
    client.close()?;
    let values = observed.lock().unwrap().clone();
    Ok(values)
}

#[cfg(feature = "up-l2-zero-copy")]
pub(crate) async fn native_flow<C: up_rust::NativeLoanCore>(
    transport: Arc<up_rust::communication::zero_copy::StableTransport<C>>,
    service: Arc<dyn USubscription>,
) -> Result<Vec<bool>, ExampleError> {
    let (door_uri, panel_uri) = uris();
    let setup = Native::new(
        transport,
        NativeConfig {
            route: up_rust::RouteConfig::default(),
            receive: up_rust::communication::zero_copy::ReceiveConfig {
                registration: registration(),
                header: up_rust::HeaderLimits { max_token_len: 32 },
            },
        },
    )?;
    let door = Endpoint::new(setup.clone(), door_uri.clone());
    let panel = Endpoint::new(setup, panel_uri.clone());
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut subscription = panel
        .subscriber(service)
        .subscription::<DoorState, _>(
            door_uri.get_resource_uri(0x8001),
            DoorObserver(observed.clone()),
        )
        .await?;
    subscription.activate().await?;
    let mut receiver = panel
        .listen::<DoorState, _>(
            door_uri.get_resource_uri(0x8002),
            DoorObserver(observed.clone()),
        )
        .await?;
    let publisher = door.publisher::<DoorState>(0x8001, PublishOptions::default())?;
    let notifier = door.notifier::<DoorState>(
        0x8002,
        panel_uri.get_source_uri(),
        PublishOptions::default(),
    )?;
    let mut server = door
        .rpc_server::<SetDoor, DoorState, _, _>(
            0x42,
            DoorController,
            |open, init| init.into_initializer().open(open).finish(),
            ServerOptions { max_clients: 4 },
        )
        .await?;
    server.prepare_client(&panel_uri.get_source_uri())?;
    let client = panel
        .rpc_client::<SetDoor, DoorState>(door_uri.get_resource_uri(0x42), rpc_options())
        .await?;
    client.set_token("door-control")?;
    publisher.publish_in_place(|loan| loan.into_initializer().open(false).finish())?;
    notifier.notify_in_place(|loan| loan.into_initializer().open(true).finish())?;
    let pending = client.invoke_in_place(|loan| loan.into_initializer().open(true).finish())?;
    // The request is already committed here; no TX loan enters this future.
    let response = pending.response().await?;
    assert!(response.payload()?.open);
    subscription.unsubscribe().await?;
    receiver.stop().await?;
    server.stop().await?;
    client.close()?;
    let values = observed.lock().unwrap().clone();
    Ok(values)
}

/// Classic protobuf UMessage migration retains real serialized payload mapping.
/// This only changes the envelope at the boundary; the payload stays DoorJson.
pub(crate) async fn classic_publish<T: up_rust::UTransport>(
    transport: &T,
    state: &DoorState,
) -> Result<(), ExampleError> {
    let (door, _) = uris();
    let bytes = DoorJson::encode_payload_owned(state)?;
    let message = up_rust::UMessageBuilder::publish(door.get_resource_uri(0x8001))
        .build_with_payload(bytes, PayloadEncoding::JSON)?;
    transport.send(message).await?;
    Ok(())
}

/// Demonstration-only carriage that exercises actual whole-frame envelope bytes.
/// It copies/allocates normally; it is never a native capability or physical proof.
#[derive(Debug)]
pub(crate) struct EnvelopeLoop<E> {
    bus: up_rust::InMemoryOwnedTransport,
    envelope: std::marker::PhantomData<fn() -> E>,
}
impl<E> Default for EnvelopeLoop<E> {
    fn default() -> Self {
        Self {
            bus: up_rust::InMemoryOwnedTransport::default(),
            envelope: std::marker::PhantomData,
        }
    }
}
#[async_trait::async_trait]
impl<E: up_rust::UFrameEnvelope> up_rust::UOwnedTransportImpl for EnvelopeLoop<E> {
    fn admit_payload_contract(
        &self,
        identity: up_rust::PayloadIdentity,
        family: Option<up_rust::WireIdentity>,
    ) -> Result<(), UWireError> {
        UOwnedTransport::admit_payload_contract(&self.bus, identity, family)
    }
    async fn send_validated_owned(
        &self,
        frame: up_rust::UOwnedFrame,
    ) -> Result<(), up_rust::UStatus> {
        let bytes = E::serialize_frame(&frame)
            .map_err(|e| up_rust::UStatus::fail_with_code(UCode::InvalidArgument, e.to_string()))?;
        let decoded = E::deserialize_frame(&bytes)
            .map_err(|e| up_rust::UStatus::fail_with_code(UCode::InvalidArgument, e.to_string()))?;
        self.bus.send_owned(decoded).await
    }
    async fn attach_validated_owned_listener(
        &self,
        source: &up_rust::UUri,
        sink: Option<&up_rust::UUri>,
        listener: Arc<dyn up_rust::UOwnedListener>,
    ) -> Result<up_rust::OwnedAttachment, up_rust::UStatus> {
        self.bus.attach_owned_listener(source, sink, listener).await
    }
}
