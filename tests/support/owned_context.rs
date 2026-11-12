//! Independently named adoption fixture, distinct from the historical preset matrix.
use super::*;
use up_rust::wire::UWireMetadataCodec;

#[derive(Clone, Default)]
struct EncodedLoop {
    listeners: Arc<Mutex<Vec<Arc<dyn UEncodedOwnedListener>>>>,
}
#[async_trait::async_trait]
impl UOwnedTransportCore for EncodedLoop {
    async fn send_prepared_owned(&self, frame: PreparedOwnedFrame) -> Result<(), UStatus> {
        let (_, metadata, payload) = frame.into_parts();
        let frame = EncodedOwnedFrame::new(metadata, payload);
        let listeners = self.listeners.lock().unwrap().clone();
        for listener in listeners {
            listener.on_receive_encoded_owned(frame.clone()).await;
        }
        Ok(())
    }
    async fn attach_encoded_owned_listener(
        &self,
        _: &UUri,
        _: Option<&UUri>,
        listener: Arc<dyn UEncodedOwnedListener>,
    ) -> Result<OwnedAttachment, UStatus> {
        let registry = self.listeners.clone();
        let exact = listener.clone();
        let handle = OwnedAttachment::new(move || {
            registry
                .lock()
                .unwrap()
                .retain(|value| !Arc::ptr_eq(value, &exact));
            Ok(())
        });
        self.listeners.lock().unwrap().push(listener);
        Ok(handle)
    }
}
const JSON_WIRE: WireIdentity = WireIdentity::new("example.vehicle.door.fields-json-v1", 0xF102);
const PROTO_WIRE: WireIdentity =
    WireIdentity::new("example.vehicle.door.fields-protobuf-v1", 0xF103);
fn declaration<K: PayloadCodec>(wire: WireIdentity) -> OwnedWireDeclaration {
    OwnedWireDeclaration {
        wire,
        payload_family: K::payload_family().unwrap(),
        metadata_layout: UFRAME_FIELDS_METADATA_LAYOUT_ID,
        format_version: up_rust::wire::FORMAT_VERSION,
    }
}
fn carriage<K: PayloadCodec>(
    wire: WireIdentity,
) -> Arc<OwnedWireTransport<EncodedLoop, UFrameFieldsMetadataCodec>> {
    let local = declaration::<K>(wire);
    // Local fixture equality only. Real peers use the separately provisioned
    // declarations and verify-owned-context before SDK setup.
    let peer = declaration::<K>(wire);
    let context = OwnedWireContext::agree::<UFrameFieldsMetadataCodec>(local, &peer).unwrap();
    Arc::new(
        OwnedWireTransport::new(EncodedLoop::default(), UFrameFieldsMetadataCodec, context)
            .unwrap(),
    )
}

#[tokio::test]
async fn door_adoption_codec_envelope_orthogonality_all_five_roles() {
    // Same native-prefix/field-block envelope, changed codec and honest full-wire
    // declaration; old preset family labels are never repurposed.
    let a = serialized_flow::<_, DoorProtobuf>(carriage::<DoorProtobuf>(PROTO_WIRE), service())
        .await
        .unwrap();
    let b = serialized_flow::<_, DoorJson>(carriage::<DoorJson>(JSON_WIRE), service())
        .await
        .unwrap();
    // Hold each payload mapping fixed while changing to classic protobuf UMessage.
    let classic = || {
        Arc::new(ClassicCarriage::new(Arc::new(
            up_rust::local_transport::LocalTransport::default(),
        )))
    };
    let c = serialized_flow::<_, DoorProtobuf>(classic(), service())
        .await
        .unwrap();
    let d = serialized_flow::<_, DoorJson>(classic(), service())
        .await
        .unwrap();
    for mut result in [a, b, c, d] {
        result.sort_unstable();
        assert_eq!(result, [false, true]);
    }
}

#[test]
fn context_and_codec_mismatches_refuse_at_setup_and_presets_keep_exact_bytes() {
    let proto = declaration::<DoorProtobuf>(PROTOBUF_WIRE_ID);
    let mut wrong = proto;
    wrong.payload_family = DOOR_JSON_FAMILY;
    assert!(OwnedWireContext::agree::<UFrameFieldsMetadataCodec>(proto, &wrong).is_err());
    assert!(OwnedWireContext::agree::<UFrameFieldsMetadataCodec>(wrong, &wrong).is_err());
    let context = OwnedWireContext::agree::<UFrameFieldsMetadataCodec>(proto, &proto).unwrap();
    let selected = Arc::new(
        OwnedWireTransport::new(EncodedLoop::default(), UFrameFieldsMetadataCodec, context)
            .unwrap(),
    );
    assert!(
        Serialized::<_, DoorJson>::new(selected, PayloadDecodeLimit::new(64), registration())
            .is_err()
    );
    let preset = Arc::new(UWireTransport::new(
        EncodedLoop::default(),
        ProtobufWire,
        UFrameFieldsMetadataCodec,
    ));
    assert!(
        Serialized::<_, DoorJson>::new(preset, PayloadDecodeLimit::new(64), registration())
            .is_err()
    );
    let (door, _) = uris();
    let frame = UOwnedFrame::with_payload(
        UFrameMetadata::publish(door.get_resource_uri(0x8001))
            .with_payload_encoding(PayloadEncoding::PROTOBUF)
            .build()
            .unwrap(),
        DoorProtobuf::encode_payload_owned(&DoorState { open: true }).unwrap(),
    )
    .unwrap();
    let old = PreparedOwnedFrame::from_validated::<ProtobufWire, _>(
        frame.clone(),
        &UFrameFieldsMetadataCodec,
    )
    .unwrap();
    let explicit = PreparedOwnedFrame::from_context(
        frame.clone(),
        &UFrameFieldsMetadataCodec,
        context.metadata_context(),
    )
    .unwrap();
    assert_eq!(old, explicit);
    let raw = EncodedOwnedFrame::new(old.encoded_metadata(), old.payload().cloned());
    assert_eq!(
        raw.clone()
            .decode::<ProtobufWire, _>(&UFrameFieldsMetadataCodec)
            .unwrap(),
        frame
    );
    let json = declaration::<DoorJson>(JSON_WIRE);
    let json_context = OwnedWireContext::agree::<UFrameFieldsMetadataCodec>(json, &json).unwrap();
    assert!(raw
        .decode_with_context(&UFrameFieldsMetadataCodec, json_context.metadata_context())
        .is_err());
    assert!(OwnedWireTransport::new(EncodedLoop::default(), DifferentLayout, context).is_err());
}

struct DifferentLayout;
impl UWireMetadataCodec for DifferentLayout {
    const METADATA_LAYOUT_ID: WireIdentity = WireIdentity::new("example.other.layout", 0xF104);
    fn encoded_frame_metadata_len(
        &self,
        _: UWireMetadataContext,
        _: &UFrameMetadata,
    ) -> Result<usize, UWireMetadataError> {
        panic!("must refuse before codec operation")
    }
    fn encode_frame_metadata_into(
        &self,
        _: UWireMetadataContext,
        _: &UFrameMetadata,
        _: &mut [u8],
    ) -> Result<usize, UWireMetadataError> {
        panic!("must refuse before codec operation")
    }
    fn decode_frame_metadata(
        &self,
        _: UWireMetadataContext,
        _: &[u8],
    ) -> Result<UFrameMetadata, UWireMetadataError> {
        panic!("must refuse before codec operation")
    }
}
