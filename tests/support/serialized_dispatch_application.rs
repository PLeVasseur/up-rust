//! Explicit legacy owned-carriage compatibility control. Publisher annotations
//! use LegacyPublisher; this is not a common bounded-core migration.
use super::*;
use up_rust::communication::application::{Deployment, Encoder, PublishOptions, LegacyPublisher};
use up_rust::wire::{UFrameFieldsMetadataCodec, FORMAT_VERSION};

struct EmptyPayload;
impl PayloadCodecIdentity for EmptyPayload {
    fn name() -> &'static str {
        "dispatch-test.empty-protobuf"
    }
    fn encoding() -> PayloadEncoding {
        PayloadEncoding::PROTOBUF
    }
    fn payload_family() -> Option<WireIdentity> {
        Some(PROTOBUF_PAYLOAD_FAMILY_ID)
    }
}
impl EncodePayload<()> for EmptyPayload {
    fn payload_layout(_: &()) -> Result<PayloadLayout, UWireError> {
        PayloadLayout::new(0, 1)
    }
    fn encode_payload(_: &(), target: &mut [u8]) -> Result<(), UWireError> {
        assert!(target.is_empty());
        Ok(())
    }
    fn encode_payload_owned(_: &()) -> Result<bytes::Bytes, UWireError> {
        Ok(bytes::Bytes::new())
    }
}
fn publisher(transport: Arc<dyn UOwnedTransport>) -> LegacyPublisher<()> {
    Deployment::new(
        transport,
        Arc::new(StaticUriProvider::new("dispatch", 0x10001, 1).unwrap()),
        PayloadDecodeLimit::new(16),
        RegistrationConfig {
            capacity: 1,
            overflow: OverflowPolicy::DropNewest,
        },
        32,
    )
    .unwrap()
    .publisher(
        0x8001,
        PublishOptions::default(),
        Encoder::from_schema_mapping::<EmptyPayload>().unwrap(),
    )
    .unwrap()
}
fn selected_context() -> OwnedWireContext {
    let declaration = OwnedWireDeclaration {
        wire: PROTOBUF_WIRE_ID,
        payload_family: PROTOBUF_PAYLOAD_FAMILY_ID,
        metadata_layout: UFRAME_FIELDS_METADATA_LAYOUT_ID,
        format_version: FORMAT_VERSION,
    };
    OwnedWireContext::agree::<UFrameFieldsMetadataCodec>(declaration, &declaration).unwrap()
}

#[test]
fn real_wire_encoding_cost_is_separate_from_dispatch_for_both_adapters() {
    let context = selected_context();
    let configured: Arc<dyn UOwnedTransport> = Arc::new(
        OwnedWireTransport::new(
            InlineOwnedCore::new(Sink::default()),
            UFrameFieldsMetadataCodec,
            context,
        )
        .unwrap(),
    );
    let preset: Arc<dyn UOwnedTransport> = Arc::new(UWireTransport::new(
        InlineOwnedCore::new(Sink::default()),
        ProtobufWire,
        UFrameFieldsMetadataCodec,
    ));
    for transport in [configured, preset] {
        let control = frame();
        let sent = control.clone();
        let (_, codec_cost) = count(|| {
            drop(
                PreparedOwnedFrame::from_context(
                    control,
                    &UFrameFieldsMetadataCodec,
                    context.metadata_context(),
                )
                .unwrap(),
            )
        });
        let (result, total) = count(|| ready(transport.send_owned(sent)));
        result.unwrap();
        assert!(codec_cost.allocations > 0);
        assert_eq!(
            total, codec_cost,
            "future dispatch must not add to real metadata encoding cost"
        );
        eprintln!(
            "real selected-wire send: codec={codec_cost:?}, total={total:?}, additional dispatch=0"
        );
    }
}

#[test]
fn domain_only_l2_publisher_retains_no_box_dispatch_and_reports_frame_cost() {
    let modern = publisher(Arc::new(InlineOwnedTransport::new(Sink::default())));
    let legacy = publisher(Arc::new(Sink::default()));
    ready(modern.publish(&())).unwrap();
    ready(legacy.publish(&())).unwrap();
    let (result, modern_cost) = count(|| ready(modern.publish(&())));
    result.unwrap();
    let (result, legacy_cost) = count(|| ready(legacy.publish(&())));
    result.unwrap();
    let source = UUri::try_from_parts("dispatch", 0x10001, 1, 0x8001).unwrap();
    let (_, frame_cost) = count(|| {
        let metadata = PayloadIdentity::Fixed(PayloadEncoding::PROTOBUF)
            .apply_to(UFrameMetadata::publish(source.clone()).build().unwrap())
            .unwrap();
        drop(UOwnedFrame::with_payload(metadata, bytes::Bytes::new()).unwrap());
    });
    assert_eq!(
        modern_cost, frame_cost,
        "public L2 future dispatch must add nothing to actual frame costs"
    );
    assert_eq!(legacy_cost.allocations, modern_cost.allocations + 1);
    assert_eq!(legacy_cost.reallocations, modern_cost.reallocations);
    eprintln!("L2 noop payload, real frame: frame={frame_cost:?}, modern={modern_cost:?}, legacy={legacy_cost:?}; frame allocations are not dispatch allocations");
}

#[test]
fn domain_only_l2_publisher_selected_wire_retains_no_box_dispatch() {
    let modern = publisher(Arc::new(
        OwnedWireTransport::new(
            InlineOwnedCore::new(Sink::default()),
            UFrameFieldsMetadataCodec,
            selected_context(),
        )
        .unwrap(),
    ));
    let legacy = publisher(Arc::new(
        OwnedWireTransport::new(
            Sink::default(),
            UFrameFieldsMetadataCodec,
            selected_context(),
        )
        .unwrap(),
    ));
    ready(modern.publish(&())).unwrap();
    ready(legacy.publish(&())).unwrap();
    let (result, modern_cost) = count(|| ready(modern.publish(&())));
    result.unwrap();
    let (result, legacy_cost) = count(|| ready(legacy.publish(&())));
    result.unwrap();
    let source = UUri::try_from_parts("dispatch", 0x10001, 1, 0x8001).unwrap();
    let context = selected_context();
    let (_, frame_and_codec_cost) = count(|| {
        let metadata = PayloadIdentity::Fixed(PayloadEncoding::PROTOBUF)
            .apply_to(UFrameMetadata::publish(source.clone()).build().unwrap())
            .unwrap();
        let frame = UOwnedFrame::with_payload(metadata, bytes::Bytes::new()).unwrap();
        drop(
            PreparedOwnedFrame::from_context(
                frame,
                &UFrameFieldsMetadataCodec,
                context.metadata_context(),
            )
            .unwrap(),
        );
    });
    assert_eq!(
        modern_cost, frame_and_codec_cost,
        "L2 + selected-wire dispatch must add nothing to frame/envelope costs"
    );
    assert_eq!(legacy_cost.allocations, modern_cost.allocations + 1);
    assert_eq!(legacy_cost.reallocations, modern_cost.reallocations);
    assert!(
        modern_cost.allocations > 0,
        "real frame/envelope storage must be counted"
    );
    eprintln!("L2 real frame + selected envelope: modern={modern_cost:?}, legacy={legacy_cost:?}; modern includes actual serialization costs");
}

#[test]
fn real_protobuf_payload_and_envelope_allocations_are_not_hidden() {
    type Value = protobuf::well_known_types::wrappers::UInt32Value;
    let context = selected_context();
    let transport: Arc<dyn UOwnedTransport> = Arc::new(
        OwnedWireTransport::new(
            InlineOwnedCore::new(Sink::default()),
            UFrameFieldsMetadataCodec,
            context,
        )
        .unwrap(),
    );
    let setup = Deployment::new(
        transport,
        Arc::new(StaticUriProvider::new("dispatch", 0x10001, 1).unwrap()),
        PayloadDecodeLimit::new(64),
        RegistrationConfig {
            capacity: 1,
            overflow: OverflowPolicy::DropNewest,
        },
        32,
    )
    .unwrap();
    let publisher: LegacyPublisher<Value> = setup
        .publisher(
            0x8001,
            PublishOptions::default(),
            Encoder::from_schema_mapping::<ProtobufPayload>().unwrap(),
        )
        .unwrap();
    let mut value = Value::new();
    value.value = 17;
    ready(publisher.publish(&value)).unwrap();
    let source = UUri::try_from_parts("dispatch", 0x10001, 1, 0x8001).unwrap();
    let (_, actual_encoding_cost) = count(|| {
        let payload = ProtobufPayload::encode_payload_owned(&value).unwrap();
        let metadata = PayloadIdentity::Fixed(PayloadEncoding::PROTOBUF)
            .apply_to(UFrameMetadata::publish(source.clone()).build().unwrap())
            .unwrap();
        let frame = UOwnedFrame::with_payload(metadata, payload).unwrap();
        drop(
            PreparedOwnedFrame::from_context(
                frame,
                &UFrameFieldsMetadataCodec,
                context.metadata_context(),
            )
            .unwrap(),
        );
    });
    let (result, total) = count(|| ready(publisher.publish(&value)));
    result.unwrap();
    assert!(actual_encoding_cost.allocations > 0);
    assert_eq!(total, actual_encoding_cost);
    eprintln!("real protobuf payload + frame + envelope={actual_encoding_cost:?}, full L2 send={total:?}, dispatch addition=0");
}
