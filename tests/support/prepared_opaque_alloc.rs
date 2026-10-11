// SPDX-License-Identifier: Apache-2.0
//! In-place opaque forwarding, including unknown identities and rejected input.

use super::{measure, ready};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::runtime::Runtime;
use up_rust::{
    Frame, HeaderAdmission, HeaderHandler, HeaderLease, HeaderLimits, LoanedPayload,
    NativeTypeToken, OpaqueTemplate, OverflowPolicy, PayloadEncoding, PayloadLoanProvenance,
    PoolCore, PoolCoreConfig, PoolFrame, PreparedTransportCore, RegistrationConfig,
    RegistrationInbox, RouteConfig, RouteLimits, SendError, TxHeaderParts,
    UFrameFieldsMetadataCodec, UFrameHeaderError, UFrameMetadata, UProtocolNativeWire, UUri,
    UWireMetadataContext, UWireTransport,
};

struct Raw {
    header: Box<[u8]>,
    payload: &'static [u8],
}
impl Frame for Raw {
    fn encoded_metadata(&self) -> &[u8] {
        &self.header
    }
    fn payload(&self) -> Option<LoanedPayload<'_>> {
        Some(LoanedPayload::new(
            self.payload,
            PayloadLoanProvenance::OpaqueTransportLoan,
        ))
    }
}
struct Count {
    seen: Arc<AtomicUsize>,
}
impl HeaderHandler<PoolFrame> for Count {
    async fn on_receive(&mut self, frame: HeaderLease<PoolFrame>) {
        tokio::task::yield_now().await;
        assert_eq!(frame.payload().unwrap().bytes(), b"opaque");
        assert_eq!(
            frame.header().unwrap().payload_encoding.unwrap().id(),
            0xE001
        );
        assert_eq!(
            frame.header().unwrap().native_type_token,
            Some(NativeTypeToken::from_u32(0))
        );
        drop(frame);
        self.seen.fetch_add(1, Ordering::Release);
    }
}

pub(super) fn run(runtime: &Runtime) {
    let source = UUri::try_from_wire_parts("opaque", 1, 1, 0x8001).unwrap();
    let metadata = UFrameMetadata::publish(source.clone())
        .with_payload_encoding(PayloadEncoding::CONTRACT_DEFINED)
        .with_native_type_token(NativeTypeToken::from_u32(0))
        .build()
        .unwrap();
    let mut fields = TxHeaderParts::from(&metadata);
    // Received reserved identity, independently represented as raw carriage.
    fields.payload_encoding = Some(PayloadEncoding::from_id(0xE001).unwrap());
    let prefix = UFrameFieldsMetadataCodec
        .native_prefix(UWireMetadataContext::from_wire::<
            UProtocolNativeWire,
            UFrameFieldsMetadataCodec,
        >())
        .unwrap();
    let mut header = vec![0; up_rust::header_len(&prefix, &fields).unwrap()];
    up_rust::write_header(&prefix, &fields, &mut header).unwrap();
    let admission =
        HeaderAdmission::for_wire::<UProtocolNativeWire>(HeaderLimits::default()).unwrap();
    let incoming = admission
        .admit(Raw {
            header: header.into_boxed_slice(),
            payload: b"opaque",
        })
        .unwrap();
    let core = Arc::new(
        PoolCore::new(PoolCoreConfig {
            slots: 8,
            slot_bytes: 1024,
            alignment: 8,
            max_routes: 2,
            max_registrations: 2,
        })
        .unwrap(),
    );
    let route = OpaqueTemplate::for_wire::<UProtocolNativeWire>()
        .unwrap()
        .route(
            Arc::clone(&core),
            source.clone(),
            None,
            RouteLimits {
                max_metadata_len: 512,
                max_payload_len: 32,
                payload_alignment: 8,
            },
            RouteConfig::default(),
        )
        .unwrap();
    let inbox = Arc::new(
        RegistrationInbox::new(RegistrationConfig {
            capacity: 1,
            overflow: OverflowPolicy::DropNewest,
        })
        .unwrap(),
    );
    let mut subscription = ready(core.attach(&source, None, inbox.clone())).unwrap();
    measure(
        "opaque_admit_forward_and_reject_reserved_origination",
        || {
            for _ in 0..128 {
                route.forward(&incoming).unwrap();
                let received = admission.admit(ready(inbox.recv()).unwrap()).unwrap();
                assert_eq!(
                    received.header().unwrap().payload_encoding,
                    fields.payload_encoding
                );
                assert_eq!(received.payload().unwrap().bytes(), b"opaque");
                assert_eq!(
                    route.send_bytes(fields, Some(b"opaque")),
                    Err(SendError::Header(UFrameHeaderError::Invalid(
                        "reserved payload origination"
                    )))
                );
            }
        },
    );
    core.detach(&mut subscription).unwrap();
    let adapter = UWireTransport::new(
        Arc::clone(&core),
        UProtocolNativeWire,
        UFrameFieldsMetadataCodec,
    );
    let seen = Arc::new(AtomicUsize::new(0));
    let registration = runtime
        .block_on(adapter.register_header_handler(
            &source,
            None,
            HeaderLimits::default(),
            RegistrationConfig {
                capacity: 4,
                overflow: OverflowPolicy::DropNewest,
            },
            Count {
                seen: Arc::clone(&seen),
            },
        ))
        .unwrap();
    measure("opaque_registered_forward_and_drain", || {
        for _ in 0..4 {
            route.forward(&incoming).unwrap();
        }
        runtime.block_on(registration.stop()).unwrap();
    });
    assert_eq!(seen.load(Ordering::Acquire), 4);
    assert_eq!(core.free_slots(), 8);
}
