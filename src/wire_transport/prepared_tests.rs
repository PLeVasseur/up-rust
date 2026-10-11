// SPDX-License-Identifier: Apache-2.0
//! Prepared-core tests for adapter filtering, storage ownership and shutdown.

use super::*;
use crate::wire::{UProtocolNativeWire, UWireMetadataCodec};
use crate::{
    Frame, FrameSink, HeaderHandler, HeaderLease, HeaderLimits, InitializedTx, LoanError,
    NativeProfile, NativeProfileAgreement, NativeProfileMode, NativeProfileTable, OverflowPolicy,
    PayloadEncoding, PayloadLoanProvenance, PoolCore, PoolCoreConfig, PoolFrame,
    PreparedTransportCore, RegistrationConfig, RegistrationInbox, RouteConfig, RouteDefinition,
    RouteLimits, RxLease, SendError, SetupError, StablePayload, TxFrameLayout, TxHeaderParts, UUri,
    WireRegistrationStopError, ZeroCopyHandler,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

enum Input {
    Pool(PoolFrame),
    Raw {
        metadata: Vec<u8>,
        payload: Option<Vec<u8>>,
    },
}
impl Frame for Input {
    fn encoded_metadata(&self) -> &[u8] {
        match self {
            Self::Pool(frame) => frame.encoded_metadata(),
            Self::Raw { metadata, .. } => metadata,
        }
    }
    fn payload(&self) -> Option<LoanedPayload<'_>> {
        match self {
            Self::Pool(frame) => frame.payload(),
            Self::Raw { payload, .. } => payload
                .as_ref()
                .map(|bytes| LoanedPayload::new(bytes, PayloadLoanProvenance::OwnedReceiveLease)),
        }
    }
}
struct Sink(Arc<dyn FrameSink<Input>>);
impl FrameSink<PoolFrame> for Sink {
    fn push(&self, frame: PoolFrame) {
        self.0.push(Input::Pool(frame));
    }
    fn close(&self) {
        self.0.close();
    }
}

struct Core {
    pool: Arc<PoolCore>,
    fail_register: AtomicBool,
    fail_unregister: AtomicBool,
    filters: Mutex<Vec<(bool, UUri, Option<UUri>)>>,
    sinks: Mutex<Vec<Arc<dyn FrameSink<Input>>>>,
}
struct Subscription {
    physical: <PoolCore as PreparedTransportCore>::Subscription,
    source: UUri,
    sink: Option<UUri>,
    inbox: Arc<dyn FrameSink<Input>>,
}
impl Core {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            pool: Arc::new(
                PoolCore::new(PoolCoreConfig {
                    slots: 8,
                    slot_bytes: 4096,
                    alignment: 8,
                    max_routes: 8,
                    max_registrations: 8,
                })
                .unwrap(),
            ),
            fail_register: AtomicBool::new(false),
            fail_unregister: AtomicBool::new(false),
            filters: Mutex::new(Vec::new()),
            sinks: Mutex::new(Vec::new()),
        })
    }
    fn inject(&self, metadata: &[u8], payload: Option<&[u8]>) {
        let sinks = self.sinks.lock().unwrap().clone();
        for sink in sinks {
            sink.push(Input::Raw {
                metadata: metadata.to_vec(),
                payload: payload.map(<[u8]>::to_vec),
            });
        }
    }
}
impl PreparedTransportCore for Core {
    type Route = <PoolCore as PreparedTransportCore>::Route;
    type Tx<'a> = <PoolCore as PreparedTransportCore>::Tx<'a>;
    type Rx = Input;
    type Subscription = Subscription;
    fn prepare_route(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        limits: RouteLimits,
    ) -> Result<Self::Route, SetupError> {
        self.pool.prepare_route(source, sink, limits)
    }
    fn loan<'a>(
        &'a self,
        route: &'a Self::Route,
        layout: TxFrameLayout,
    ) -> Result<Self::Tx<'a>, LoanError> {
        self.pool.loan(route, layout)
    }
    fn publish<'a>(
        &'a self,
        route: &'a Self::Route,
        frame: InitializedTx<Self::Tx<'a>>,
    ) -> Result<(), SendError> {
        self.pool.publish(route, frame)
    }
    async fn attach(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        inbox: Arc<dyn FrameSink<Input>>,
    ) -> Result<Self::Subscription, SetupError> {
        if self.fail_register.load(Ordering::Relaxed) {
            return Err(SetupError::TableFull);
        }
        let physical = self
            .pool
            .attach(source, sink, Arc::new(Sink(Arc::clone(&inbox))))
            .await?;
        self.sinks.lock().unwrap().push(Arc::clone(&inbox));
        self.filters
            .lock()
            .unwrap()
            .push((true, source.clone(), sink.cloned()));
        Ok(Subscription {
            physical,
            source: source.clone(),
            sink: sink.cloned(),
            inbox,
        })
    }
    fn detach(&self, subscription: &mut Self::Subscription) -> Result<(), SetupError> {
        if self.fail_unregister.load(Ordering::Relaxed) {
            return Err(SetupError::InvalidConfig("injected detach failure"));
        }
        self.pool.detach(&mut subscription.physical)?;
        self.sinks
            .lock()
            .unwrap()
            .retain(|sink| !Arc::ptr_eq(sink, &subscription.inbox));
        self.filters.lock().unwrap().push((
            false,
            subscription.source.clone(),
            subscription.sink.clone(),
        ));
        Ok(())
    }
}

fn topic(resource: u16) -> UUri {
    UUri::try_from_wire_parts("vehicle", 0x4210, 1, resource).unwrap()
}
fn config() -> RegistrationConfig {
    RegistrationConfig {
        capacity: 8,
        overflow: OverflowPolicy::DropNewest,
    }
}
fn encode<W: UWire>(metadata: &UFrameMetadata) -> Vec<u8> {
    UFrameFieldsMetadataCodec
        .encode_frame_metadata(
            UWireMetadataContext::from_wire::<W, UFrameFieldsMetadataCodec>(),
            metadata,
        )
        .unwrap()
}
fn publish(resource: u16) -> UFrameMetadata {
    UFrameMetadata::publish(topic(resource))
        .with_payload_encoding(PayloadEncoding::RAW)
        .build()
        .unwrap()
}
struct Capture(Arc<Mutex<Vec<HeaderLease<Input>>>>);
impl HeaderHandler<Input> for Capture {
    async fn on_receive(&mut self, lease: HeaderLease<Input>) {
        tokio::task::yield_now().await;
        self.0.lock().unwrap().push(lease);
    }
}

#[test_case::test_case(0x9000; "exact source")]
#[test_case::test_case(u16::MAX; "partial source")]
#[tokio::test]
async fn decoded_filter_and_retained_storage_survive_adapter_lifecycle(resource: u16) {
    let core = Core::new();
    let adapter = UWireTransport::new(
        Arc::clone(&core),
        UProtocolNativeWire,
        UFrameFieldsMetadataCodec,
    );
    let frames = Arc::new(Mutex::new(Vec::new()));
    let source = topic(resource);
    let receiver = adapter
        .register_header_handler(
            &source,
            None,
            HeaderLimits::default(),
            config(),
            Capture(Arc::clone(&frames)),
        )
        .await
        .unwrap();
    let wrong =
        UFrameMetadata::publish(UUri::try_from_wire_parts("other", 0x4210, 1, 0x9000).unwrap())
            .with_payload_encoding(PayloadEncoding::RAW)
            .build()
            .unwrap();
    core.inject(
        &encode::<UProtocolNativeWire>(&wrong),
        Some(b"other authority"),
    );
    core.inject(b"invalid selected wire metadata", Some(b"invalid"));
    core.inject(
        &encode::<UProtocolNativeWire>(&publish(0x9000)),
        Some(b"retained"),
    );
    receiver.stop().await.unwrap();
    assert_eq!(
        *core.filters.lock().unwrap(),
        [(true, source.clone(), None), (false, source, None)]
    );
    assert!(core.sinks.lock().unwrap().is_empty());
    let frame = frames.lock().unwrap().pop().unwrap();
    assert!(frames.lock().unwrap().is_empty());
    let pointer = frame.payload().unwrap().bytes().as_ptr();
    drop((adapter, core));
    assert_eq!(frame.payload().unwrap().bytes(), b"retained");
    assert_eq!(frame.payload().unwrap().bytes().as_ptr(), pointer);
}

#[tokio::test]
async fn sinkless_and_directed_workers_do_not_duplicate_notifications() {
    let core = Core::new();
    let adapter = UWireTransport::new(
        Arc::clone(&core),
        UProtocolNativeWire,
        UFrameFieldsMetadataCodec,
    );
    let publishes = Arc::new(Mutex::new(Vec::new()));
    let notifications = Arc::new(Mutex::new(Vec::new()));
    let source = topic(0x9000);
    let sink = UUri::try_from_wire_parts("sink", 2, 1, 0).unwrap();
    let pubsub = adapter
        .register_header_handler(
            &source,
            None,
            HeaderLimits::default(),
            config(),
            Capture(Arc::clone(&publishes)),
        )
        .await
        .unwrap();
    let directed = adapter
        .register_header_handler(
            &source,
            Some(&sink),
            HeaderLimits::default(),
            config(),
            Capture(Arc::clone(&notifications)),
        )
        .await
        .unwrap();
    let notification = UFrameMetadata::notification(source, sink)
        .with_payload_encoding(PayloadEncoding::RAW)
        .build()
        .unwrap();
    core.inject(
        &encode::<UProtocolNativeWire>(&notification),
        Some(b"notification"),
    );
    pubsub.stop().await.unwrap();
    directed.stop().await.unwrap();
    assert!(publishes.lock().unwrap().is_empty());
    assert_eq!(notifications.lock().unwrap().len(), 1);
}

async fn handler_released<T>(owner: &Arc<T>) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while Arc::strong_count(owner) != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn failed_register_releases_worker_and_successful_detach_drains() {
    let core = Core::new();
    let adapter = UWireTransport::new(
        Arc::clone(&core),
        UProtocolNativeWire,
        UFrameFieldsMetadataCodec,
    );
    let owner = Arc::new(Mutex::new(Vec::new()));
    core.fail_register.store(true, Ordering::Relaxed);
    assert!(matches!(
        adapter
            .register_header_handler(
                &topic(0x9000),
                None,
                HeaderLimits::default(),
                config(),
                Capture(Arc::clone(&owner))
            )
            .await,
        Err(SetupError::TableFull)
    ));
    handler_released(&owner).await;
    assert!(core.sinks.lock().unwrap().is_empty());
    core.fail_register.store(false, Ordering::Relaxed);
    let receiver = adapter
        .register_header_handler(
            &topic(0x9000),
            None,
            HeaderLimits::default(),
            config(),
            Capture(Arc::clone(&owner)),
        )
        .await
        .unwrap();
    receiver.stop().await.unwrap();
    assert_eq!(Arc::strong_count(&owner), 1);
}

#[tokio::test]
async fn failed_detach_preserves_handle_for_retry_with_admission_closed() {
    let core = Core::new();
    let adapter = UWireTransport::new(
        Arc::clone(&core),
        UProtocolNativeWire,
        UFrameFieldsMetadataCodec,
    );
    let owner = Arc::new(Mutex::new(Vec::new()));
    let mut receiver = adapter
        .register_header_handler(
            &topic(0x9000),
            None,
            HeaderLimits::default(),
            config(),
            Capture(Arc::clone(&owner)),
        )
        .await
        .unwrap();
    core.fail_unregister.store(true, Ordering::Relaxed);
    assert!(receiver.detach().is_err());
    assert_eq!(core.sinks.lock().unwrap().len(), 1);
    core.inject(
        &encode::<UProtocolNativeWire>(&publish(0x9000)),
        Some(b"must not enter"),
    );
    core.fail_unregister.store(false, Ordering::Relaxed);
    receiver.detach().unwrap();
    receiver.stop().await.unwrap();
    assert!(owner.lock().unwrap().is_empty());
    assert!(core.sinks.lock().unwrap().is_empty());
    assert_eq!(core.filters.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn registrations_are_instance_owned_and_drop_releases_each_worker() {
    let first_core = Core::new();
    let second_core = Core::new();
    let first = UWireTransport::new(
        Arc::clone(&first_core),
        UProtocolNativeWire,
        UFrameFieldsMetadataCodec,
    );
    let second = UWireTransport::new(
        Arc::clone(&second_core),
        UProtocolNativeWire,
        UFrameFieldsMetadataCodec,
    );
    let owner = Arc::new(Mutex::new(Vec::new()));
    let a = first
        .register_header_handler(
            &topic(0x9000),
            None,
            HeaderLimits::default(),
            config(),
            Capture(Arc::clone(&owner)),
        )
        .await
        .unwrap();
    let b = second
        .register_header_handler(
            &topic(0x9000),
            None,
            HeaderLimits::default(),
            config(),
            Capture(Arc::clone(&owner)),
        )
        .await
        .unwrap();
    drop(a);
    assert!(first_core.sinks.lock().unwrap().is_empty());
    assert_eq!(second_core.sinks.lock().unwrap().len(), 1);
    drop((b, first, second));
    handler_released(&owner).await;
    assert!(second_core.sinks.lock().unwrap().is_empty());
}

#[repr(C)]
#[derive(Debug, crate::StablePayload, crate::StablePayloadInit)]
#[stable_payload(type_name = "uprotocol.test.PreparedWirePayload")]
struct Value {
    bytes: [u8; 4],
}
fn agreement(single: bool) -> NativeProfileAgreement {
    let representation = Value::native_representation();
    let mode = if single {
        NativeProfileMode::ContractDefined(
            crate::NativeContract::new("wire.event", representation).unwrap(),
        )
    } else {
        NativeProfileMode::Table(
            NativeProfileTable::new([(PayloadEncoding::private_use(0xF111), representation)])
                .unwrap(),
        )
    };
    crate::test_support::self_agreement(NativeProfile::new("wire-tests", 1, mode).unwrap())
}
struct TypedCapture(Arc<Mutex<Vec<RxLease<Value, Input>>>>);
impl ZeroCopyHandler<Value, Input> for TypedCapture {
    async fn on_receive(&mut self, frame: RxLease<Value, Input>) {
        self.0.lock().unwrap().push(frame);
    }
}
fn definition() -> RouteDefinition {
    RouteDefinition {
        kind: crate::UMessageType::Publish,
        source: topic(0x9000),
        sink: None,
        priority: None,
        ttl: None,
        permission_level: None,
    }
}

#[test_case::test_case(false; "private table")]
#[test_case::test_case(true; "contract defined")]
#[tokio::test]
async fn native_send_receive_retains_values_metadata_and_agreement(single: bool) {
    let core = Core::new();
    let agreement = agreement(single);
    let adapter = UWireTransport::with_native_agreement(
        Arc::clone(&core),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        agreement.clone(),
    );
    let frames = Arc::new(Mutex::new(Vec::new()));
    let registration = adapter
        .register_stable_handler(
            &topic(0x9000),
            None,
            HeaderLimits::default(),
            config(),
            TypedCapture(Arc::clone(&frames)),
        )
        .await
        .unwrap();
    let route = adapter
        .prepare_stable_route::<Value>(definition(), RouteConfig::default())
        .unwrap();
    let admission = adapter
        .prepare_stable_receiver::<Value>(HeaderLimits::default())
        .unwrap();
    let metadata = UFrameMetadata::publish(topic(0x9000))
        .with_native_payload_identity(admission.identity())
        .build()
        .unwrap();
    route
        .send_metadata(&metadata, |init| {
            init.into_initializer()
                .bytes_from_slice(b"wire")
                .unwrap()
                .finish()
        })
        .unwrap();
    registration.stop().await.unwrap();
    let frame = frames.lock().unwrap().pop().unwrap();
    assert_eq!(frame.payload().unwrap().bytes, *b"wire");
    assert_eq!(frame.header().unwrap().id, *metadata.id());
    assert_eq!(
        frame.header().unwrap().payload_encoding,
        metadata.payload_encoding().copied()
    );
    assert_eq!(
        frame.header().unwrap().native_type_token,
        metadata.native_type_token()
    );
    assert_eq!(frame.native_agreement().profile(), agreement.profile());
    let pointer = frame.payload().unwrap() as *const Value;
    drop((route, adapter, core, admission, agreement));
    assert_eq!(frame.payload().unwrap() as *const Value, pointer);
    assert_eq!(frame.payload().unwrap().bytes, *b"wire");
}

#[tokio::test]
async fn prepared_inbox_pull_retains_generation_and_layout() {
    let core = Core::new();
    let adapter = UWireTransport::with_native_agreement(
        Arc::clone(&core),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        agreement(false),
    );
    let inbox = Arc::new(RegistrationInbox::new(config()).unwrap());
    let mut subscription = core
        .attach(&topic(0x9000), None, inbox.clone())
        .await
        .unwrap();
    let admission = adapter
        .prepare_stable_receiver::<Value>(HeaderLimits::default())
        .unwrap();
    let route = adapter
        .prepare_stable_route::<Value>(definition(), RouteConfig::default())
        .unwrap();
    route
        .send(crate::MessageFields::default(), |init| {
            init.into_initializer()
                .bytes_from_slice(b"pull")
                .unwrap()
                .finish()
        })
        .unwrap();
    let raw = inbox.recv().await.unwrap();
    let header = UFrameFieldsMetadataCodec
        .decode_frame_metadata(
            UWireMetadataContext::from_wire::<StableContainerWire, UFrameFieldsMetadataCodec>(),
            raw.encoded_metadata(),
        )
        .unwrap();
    assert_eq!(header.source(), &topic(0x9000));
    assert_eq!(raw.payload().unwrap().bytes().len(), 4);
    let frame = admission.admit(raw).unwrap();
    core.detach(&mut subscription).unwrap();
    assert_eq!(frame.payload().unwrap().bytes, *b"pull");
    assert_eq!(
        frame.native_agreement().profile(),
        adapter.native_agreement().unwrap().profile()
    );
}

#[tokio::test]
async fn typed_filter_rejects_foreign_topic_even_after_broad_physical_delivery() {
    let core = Core::new();
    let adapter = UWireTransport::with_native_agreement(
        Arc::clone(&core),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        agreement(false),
    );
    let frames = Arc::new(Mutex::new(Vec::new()));
    let registration = adapter
        .register_stable_handler(
            &topic(0x9000),
            None,
            HeaderLimits::default(),
            config(),
            TypedCapture(Arc::clone(&frames)),
        )
        .await
        .unwrap();
    let admission = adapter
        .prepare_stable_receiver::<Value>(HeaderLimits::default())
        .unwrap();
    for resource in [0x9001, 0x9000] {
        let metadata = UFrameMetadata::publish(topic(resource))
            .with_native_payload_identity(admission.identity())
            .build()
            .unwrap();
        core.inject(&encode::<StableContainerWire>(&metadata), Some(b"wire"));
    }
    registration.stop().await.unwrap();
    assert_eq!(frames.lock().unwrap().len(), 1);
    assert_eq!(
        frames
            .lock()
            .unwrap()
            .first()
            .unwrap()
            .header()
            .unwrap()
            .source,
        topic(0x9000)
    );
}

#[tokio::test]
async fn opaque_bytes_roundtrip_uses_prepared_codec_and_actual_pool() {
    let core = Core::new();
    let adapter = UWireTransport::new(Arc::clone(&core), ProtobufWire, UFrameFieldsMetadataCodec);
    let value = protobuf::well_known_types::wrappers::StringValue {
        value: "selected".into(),
        special_fields: Default::default(),
    };
    let bytes =
        <crate::ProtobufPayload as crate::EncodePayload<_>>::encode_payload_owned(&value).unwrap();
    let metadata = UFrameMetadata::publish(topic(0x9000))
        .with_payload_encoding(PayloadEncoding::PROTOBUF)
        .build()
        .unwrap();
    let frames = Arc::new(Mutex::new(Vec::new()));
    let registration = adapter
        .register_header_handler(
            &topic(0x9000),
            None,
            HeaderLimits::default(),
            config(),
            Capture(Arc::clone(&frames)),
        )
        .await
        .unwrap();
    let route = adapter
        .prepare_opaque_template()
        .unwrap()
        .route(
            Arc::clone(&core),
            topic(0x9000),
            None,
            RouteLimits {
                max_metadata_len: 512,
                max_payload_len: bytes.len(),
                payload_alignment: 1,
            },
            RouteConfig::default(),
        )
        .unwrap();
    route
        .send_bytes(TxHeaderParts::from(&metadata), Some(&bytes))
        .unwrap();
    registration.stop().await.unwrap();
    let frame = frames.lock().unwrap().pop().unwrap();
    let restored: protobuf::well_known_types::wrappers::StringValue =
        <crate::ProtobufPayload as crate::DecodePayload<_>>::decode_payload(
            frame.payload().unwrap().bytes(),
        )
        .unwrap();
    assert_eq!(restored, value);
    assert_eq!(frame.header().unwrap().id, *metadata.id());
}

#[tokio::test]
async fn stop_reports_detach_error_after_draining_without_revoking_delivered_storage() {
    let core = Core::new();
    let adapter = UWireTransport::new(
        Arc::clone(&core),
        UProtocolNativeWire,
        UFrameFieldsMetadataCodec,
    );
    let frames = Arc::new(Mutex::new(Vec::new()));
    let registration = adapter
        .register_header_handler(
            &topic(0x9000),
            None,
            HeaderLimits::default(),
            config(),
            Capture(Arc::clone(&frames)),
        )
        .await
        .unwrap();
    core.inject(
        &encode::<UProtocolNativeWire>(&publish(0x9000)),
        Some(b"keep"),
    );
    core.fail_unregister.store(true, Ordering::Relaxed);
    assert!(matches!(
        registration.stop().await,
        Err(WireRegistrationStopError::Detach(_))
    ));
    assert_eq!(frames.lock().unwrap().len(), 1);
    assert_eq!(
        frames
            .lock()
            .unwrap()
            .first()
            .unwrap()
            .payload()
            .unwrap()
            .bytes(),
        b"keep"
    );
}
