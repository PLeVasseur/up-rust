//! The newcomer's schema paths: a generated protobuf type needs no profile enum
//! or mapping code, one codec per type needs no enum, and JSON needs no hand-written
//! codec. In-process fixtures only: not physical or allocation evidence.
use bytes::Bytes;
use protobuf::well_known_types::wrappers::{StringValue, UInt32Value};
use std::sync::Arc;
use tokio::sync::mpsc;
use up_rust::{
    communication::application::{
        Delivery, Deployment, Handler, OperationOptions, ProtobufProfile, PublishOptions,
    },
    DecodePayload, EncodePayload, InMemoryOwnedTransport, PayloadCodecIdentity, PayloadEncoding,
    ProtobufPayload, StaticUriProvider, UEntityId, WireIdentity,
};

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Level {
    value: u32,
}
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    level: u32,
}

fn uint32(value: u32) -> UInt32Value {
    let mut wire = UInt32Value::new();
    wire.value = value;
    wire
}

up_rust::mapped_codec! {
    /// Maps both domain types onto one generated wire message.
    struct LevelProtobuf = "example.level.protobuf.v1" via ProtobufPayload, wire UInt32Value;
    Level => |level| uint32(level.value), |wire| Level { value: wire.value };
    Target => |target| uint32(target.level), |wire| Target { level: wire.value };
}
up_rust::json_codec!(struct LevelJson = ("example.level.json-v1", 0xF1A2) for Level, Target);

up_rust::schema_mapping!(
    /// One codec per type: no enum and nothing to choose at each deployment.
    struct Schema {
        Level => LevelProtobuf,
        Target => LevelProtobuf,
    }
);

up_rust::declare_operations! {
    event STATUS: StringValue = 0x8001;
    notification ALARM: Level = 0x8002;
    notification COMMAND: StringValue = 0x8003;
}

fn address(service: u16) -> StaticUriProvider {
    StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(service), 1)
        .expect("valid address")
}

fn deployment(transport: &Arc<InMemoryOwnedTransport>, service: u16) -> Deployment {
    Deployment::with_defaults(transport.clone(), Arc::new(address(service)))
        .expect("bounded defaults")
}

struct Forward<P>(mpsc::UnboundedSender<P>);
impl Handler<StringValue> for Forward<String> {
    async fn on_receive<D: Delivery<StringValue>>(&mut self, delivery: D) {
        if let Ok(value) = delivery.payload() {
            let _ = self.0.send(value.value.clone());
        }
    }
}
impl Handler<Level> for Forward<u32> {
    async fn on_receive<D: Delivery<Level>>(&mut self, delivery: D) {
        if let Ok(level) = delivery.payload() {
            let _ = self.0.send(level.value);
        }
    }
}

#[tokio::test]
async fn generated_protobuf_needs_no_profile_or_mapping() {
    let bus = Arc::new(InMemoryOwnedTransport::default());
    let device = deployment(&bus, 1).protobuf();
    let panel = deployment(&bus, 2).protobuf();
    let (sender, mut received) = mpsc::unbounded_channel();
    let mut receiver = COMMAND
        .receiver(&panel, device.address(), Forward(sender))
        .await
        .expect("receiver");
    let notifier = COMMAND
        .notifier(
            &device,
            panel.address().get_source_uri(),
            PublishOptions::default(),
        )
        .expect("notifier");
    let mut message = StringValue::new();
    message.value = "unlock".to_owned();
    notifier.notify(&message).await.expect("delivered");
    assert_eq!(received.recv().await.as_deref(), Some("unlock"));
    receiver.stop().await.expect("stopped");

    let status = STATUS.publisher(&device, PublishOptions::default());
    assert!(status.is_ok());
    let selected = ProtobufProfile;
    let _bound = deployment(&bus, 3).bind(selected);
}

#[tokio::test]
async fn single_codec_schema_needs_no_enum() {
    let bus = Arc::new(InMemoryOwnedTransport::default());
    let device = deployment(&bus, 1).bind(Schema);
    let panel = deployment(&bus, 2).bind(Schema);
    let (sender, mut received) = mpsc::unbounded_channel();
    let mut receiver = ALARM
        .receiver(&panel, device.address(), Forward(sender))
        .await
        .expect("receiver");
    let notifier = ALARM
        .notifier(
            &device,
            panel.address().get_source_uri(),
            OperationOptions::default().publish,
        )
        .expect("notifier");
    notifier
        .notify(&Level { value: 7 })
        .await
        .expect("delivered");
    assert_eq!(received.recv().await, Some(7));
    receiver.stop().await.expect("stopped");
}

#[test]
fn mapped_codec_takes_identity_from_its_base_codec() {
    assert_eq!(LevelProtobuf::name(), "example.level.protobuf.v1");
    assert_eq!(
        <LevelProtobuf as PayloadCodecIdentity>::encoding(),
        <ProtobufPayload as PayloadCodecIdentity>::encoding()
    );
    assert_eq!(
        <LevelProtobuf as PayloadCodecIdentity>::payload_family(),
        <ProtobufPayload as PayloadCodecIdentity>::payload_family()
    );
    let level = Level { value: 42 };
    let encoded = LevelProtobuf::encode_payload_owned(&level).expect("encoded");
    assert_eq!(
        <LevelProtobuf as DecodePayload<'_, Level>>::decode_payload(&encoded).expect("decoded"),
        level
    );
    assert_eq!(
        encoded,
        ProtobufPayload::encode_payload_owned(&uint32(42)).expect("same wire")
    );
}

#[test]
fn json_codec_declares_its_identity_and_round_trips() {
    assert_eq!(
        <LevelJson as PayloadCodecIdentity>::encoding(),
        PayloadEncoding::JSON
    );
    assert_eq!(
        <LevelJson as PayloadCodecIdentity>::payload_family(),
        Some(WireIdentity::new("example.level.json-v1", 0xF1A2))
    );
    let level = Level { value: 3 };
    let owned = LevelJson::encode_payload_owned(&level).expect("encoded");
    assert_eq!(owned, Bytes::from_static(br#"{"value":3}"#));
    assert_eq!(
        <LevelJson as DecodePayload<'_, Level>>::decode_payload(&owned).expect("decoded"),
        level
    );
}

#[test]
fn json_codec_measures_once_and_writes_the_exact_extent_in_place() {
    let target = Target { level: 12 };
    let layout = LevelJson::payload_layout(&target).expect("layout");
    assert_eq!(layout.align(), 1);
    let mut buffer = vec![0_u8; layout.len()];
    LevelJson::encode_payload(&target, &mut buffer).expect("written in place");
    assert_eq!(buffer.as_slice(), br#"{"level":12}"#);

    let mut too_short = vec![0_u8; layout.len() - 1];
    assert!(LevelJson::encode_payload(&target, &mut too_short).is_err());
    let mut too_long = vec![0_u8; layout.len() + 1];
    assert!(LevelJson::encode_payload(&target, &mut too_long).is_err());
}

#[test]
fn json_codec_rejects_payloads_outside_the_schema() {
    for malformed in [
        br#"{"value":"3"}"#.as_slice(),
        br#"{"value":3,"extra":1}"#.as_slice(),
        br#"{}"#.as_slice(),
        b"not json".as_slice(),
    ] {
        let result = <LevelJson as DecodePayload<'_, Level>>::decode_payload(malformed);
        assert!(result.is_err(), "{malformed:?}");
    }
}
