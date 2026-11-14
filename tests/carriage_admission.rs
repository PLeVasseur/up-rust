//! Publish, notify and their receivers have their own setup admission, separate from
//! RPC, so a binding whose one-way carriage assumes one payload encoding can refuse a
//! mismatch before the first send. In-process fixtures only: not physical evidence.
use protobuf::well_known_types::wrappers::UInt32Value;
use std::sync::Arc;
use tokio::sync::mpsc;
use up_rust::{
    communication::application::{
        Delivery, Deployment, Handler, OperationOptions, PublishOptions, RequestHandler,
    },
    InMemoryOwnedTransport, OwnedAttachment, PayloadEncoding, PayloadIdentity, ProtobufPayload,
    StaticUriProvider, UCode, UEntityId, UOwnedFrame, UOwnedListener, UOwnedTransport,
    UOwnedTransportImpl, UStatus, UUri, UWireError, WireIdentity,
};

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Level {
    value: u32,
}

fn uint32(value: u32) -> UInt32Value {
    let mut wire = UInt32Value::new();
    wire.value = value;
    wire
}

up_rust::json_codec!(struct LevelJson = ("example.carriage.json-v1", 0xF1A3) for Level);
up_rust::mapped_codec! {
    struct LevelProtobuf = "example.carriage.protobuf.v1" via ProtobufPayload, wire UInt32Value;
    Level => |level| uint32(level.value), |wire| Level { value: wire.value };
}
up_rust::schema_mapping!(struct JsonSchema { Level => LevelJson });
up_rust::schema_mapping!(struct ProtobufSchema { Level => LevelProtobuf });
up_rust::declare_operations! {
    event STATUS: Level = 0x8001;
    notification ALARM: Level = 0x8002;
    rpc READ: Level => Level = 1;
}

/// One-way carriage that assumes protobuf, as SOME/IP does for its configured encoding.
#[derive(Default)]
struct OneEncoding {
    bus: InMemoryOwnedTransport,
}

#[async_trait::async_trait]
impl UOwnedTransportImpl for OneEncoding {
    fn admit_payload_contract(
        &self,
        identity: PayloadIdentity,
        family: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        UOwnedTransport::admit_payload_contract(&self.bus, identity, family)
    }
    fn admit_carriage_payload_contract(
        &self,
        identity: PayloadIdentity,
        _family: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        if identity == PayloadIdentity::Fixed(PayloadEncoding::PROTOBUF) {
            Ok(())
        } else {
            Err(UWireError::invalid_payload(
                "one-way carriage assumes protobuf",
            ))
        }
    }
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.bus.send_owned(frame).await
    }
    async fn attach_validated_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<OwnedAttachment, UStatus> {
        self.bus.attach_owned_listener(source, sink, listener).await
    }
}

fn deployment(carriage: &Arc<OneEncoding>, service: u16) -> Deployment {
    let address = StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(service), 1)
        .expect("valid address");
    Deployment::with_defaults(carriage.clone(), Arc::new(address)).expect("bounded defaults")
}

struct Forward(mpsc::UnboundedSender<u32>);
impl Handler<Level> for Forward {
    async fn on_receive<D: Delivery<Level>>(&mut self, delivery: D) {
        if let Ok(level) = delivery.payload() {
            let _ = self.0.send(level.value);
        }
    }
}

struct Echo;
impl RequestHandler<Level> for Echo {
    type Reply = u32;
    async fn handle<D: Delivery<Level>>(&mut self, request: D) -> Result<u32, UCode> {
        Ok(request.payload().map_err(|_| UCode::InvalidArgument)?.value)
    }
}

#[tokio::test]
async fn one_way_roles_with_another_encoding_are_refused_at_setup() {
    let carriage = Arc::new(OneEncoding::default());
    let device = deployment(&carriage, 1).bind(JsonSchema);
    let panel = deployment(&carriage, 2).bind(JsonSchema);
    let (sender, _received) = mpsc::unbounded_channel();

    assert!(STATUS
        .publisher(&device, PublishOptions::default())
        .is_err());
    assert!(ALARM
        .notifier(
            &device,
            panel.address().get_source_uri(),
            PublishOptions::default(),
        )
        .is_err());
    assert!(ALARM
        .receiver(&panel, device.address(), Forward(sender))
        .await
        .is_err());
}

#[tokio::test]
async fn one_way_roles_with_the_assumed_encoding_still_work() {
    let carriage = Arc::new(OneEncoding::default());
    let device = deployment(&carriage, 1).bind(ProtobufSchema);
    let panel = deployment(&carriage, 2).bind(ProtobufSchema);
    let (sender, mut received) = mpsc::unbounded_channel();
    let mut receiver = ALARM
        .receiver(&panel, device.address(), Forward(sender))
        .await
        .expect("receiver");
    let notifier = ALARM
        .notifier(
            &device,
            panel.address().get_source_uri(),
            PublishOptions::default(),
        )
        .expect("notifier");
    notifier
        .notify(&Level { value: 9 })
        .await
        .expect("delivered");
    assert_eq!(received.recv().await, Some(9));
    receiver.stop().await.expect("stopped");
}

#[tokio::test]
async fn rpc_roles_keep_the_general_admission() {
    let carriage = Arc::new(OneEncoding::default());
    let device = deployment(&carriage, 1).bind(JsonSchema);
    let panel = deployment(&carriage, 2).bind(JsonSchema);
    let mut server = READ
        .server(
            &panel,
            Echo,
            |value| Level { value },
            OperationOptions::default().server,
        )
        .await
        .expect("a JSON RPC server is admitted");
    let client = READ
        .client(&device, panel.address(), OperationOptions::default().rpc)
        .await
        .expect("a JSON RPC client is admitted");
    let pending = client.invoke(&Level { value: 5 }).await.expect("sent");
    let reply = pending.response().await.expect("answered");
    assert_eq!(reply.payload().expect("decoded").value, 5);
    server.stop().await.expect("stopped");
}
