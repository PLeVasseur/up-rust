/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/
//! Hostile bytes and bounded-capacity rejection with all-thread accounting.

use super::{measure, ready};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{runtime::Runtime, sync::Semaphore};
use up_rust::bench_fixtures::payload_contract::{init_streamer_4k, StreamChunk4kV1};
use up_rust::{
    AdmissionError, Frame, HeaderLimits, InboxPushError, LoanError, LoanedPayload, MessageFields,
    NativeProfile, NativeProfileAgreement, OverflowPolicy, PayloadEncoding, PayloadLoanProvenance,
    PoolCore, PoolCoreConfig, PoolFrame, PreparedTransportCore, RegistrationConfig,
    RegistrationInbox, RouteConfig, RouteDefinition, RxAdmission, RxLease, SendError,
    StableContainerWire, StablePayload, TxTemplate, UFrameFieldsMetadataCodec, UFrameMetadata,
    UMessageType, UPriority, UUri, UUriPartsError, UWireMetadataCodec, UWireMetadataContext,
    ZeroCopyHandler, UUID,
};

fn core(slots: usize) -> Arc<PoolCore> {
    Arc::new(
        PoolCore::new(PoolCoreConfig {
            slots,
            slot_bytes: 8192,
            alignment: 8,
            max_routes: 8,
            max_registrations: 4,
        })
        .unwrap(),
    )
}

fn pool_exhausted() {
    let core = core(2);
    let route = TxTemplate::<StreamChunk4kV1>::new(super::agreement())
        .unwrap()
        .route(
            Arc::clone(&core),
            super::definition(),
            RouteConfig::default(),
        )
        .unwrap();
    let held: [_; 2] = std::array::from_fn(|_| {
        route
            .prepare(MessageFields::default(), |init| {
                init_streamer_4k(init.into_initializer(), 0).unwrap()
            })
            .unwrap()
    });
    measure("send_pool_exhausted", || {
        for _ in 0..10_000 {
            assert_eq!(
                route.send(MessageFields::default(), |_| panic!(
                    "exhaustion precedes filler"
                )),
                Err(SendError::Loan(LoanError::PoolExhausted))
            );
        }
    });
    drop(held);
    assert_eq!(core.free_slots(), 2);
}

struct Gate {
    entered: Arc<AtomicUsize>,
    gate: Arc<Semaphore>,
}
impl ZeroCopyHandler<StreamChunk4kV1, PoolFrame> for Gate {
    async fn on_receive(&mut self, lease: RxLease<StreamChunk4kV1, PoolFrame>) {
        self.entered.fetch_add(1, Ordering::Release);
        let _permit = self.gate.acquire().await;
        drop(lease);
    }
}

fn overflow(runtime: &Runtime) {
    let core = core(8);
    let agreement = super::agreement();
    let route = TxTemplate::<StreamChunk4kV1>::new(agreement.clone())
        .unwrap()
        .route(
            Arc::clone(&core),
            super::definition(),
            RouteConfig::default(),
        )
        .unwrap();
    let admission =
        RxAdmission::<StreamChunk4kV1>::new(agreement, HeaderLimits::default()).unwrap();
    let gate = Arc::new(Semaphore::new(0));
    let entered = Arc::new(AtomicUsize::new(0));
    let registration = {
        let _entered = runtime.enter();
        admission
            .register(
                RegistrationConfig {
                    capacity: 2,
                    overflow: OverflowPolicy::DropNewest,
                },
                Gate {
                    entered: Arc::clone(&entered),
                    gate: Arc::clone(&gate),
                },
                || {},
            )
            .unwrap()
    };
    let mut subscription =
        ready(core.attach(&super::definition().source, None, registration.inbox())).unwrap();
    let send = |sequence| {
        route
            .send(MessageFields::default(), |init| {
                init_streamer_4k(init.into_initializer(), sequence).unwrap()
            })
            .unwrap()
    };
    send(0);
    let deadline = Instant::now() + Duration::from_secs(10);
    while entered.load(Ordering::Acquire) == 0 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    send(1);
    send(2);
    assert_eq!(registration.inbox().len(), 2);
    measure("inbox_overflow_drop_newest", || {
        for sequence in 3..10_003 {
            send(sequence);
        }
    });
    assert_eq!(registration.stats().full.load(Ordering::Relaxed), 10_000);
    assert_eq!(core.free_slots(), 5);
    gate.close();
    core.detach(&mut subscription).unwrap();
    runtime.block_on(registration.stop()).unwrap();
    assert_eq!(entered.load(Ordering::Acquire), 3);
    assert_eq!(core.free_slots(), 8);
}

fn block_deadline() {
    let inbox = RegistrationInbox::new(RegistrationConfig {
        capacity: 1,
        overflow: OverflowPolicy::Block {
            deadline: Duration::from_millis(1),
        },
    })
    .unwrap();
    inbox.push(1_u32).unwrap();
    measure("inbox_block_deadline_expiry", || {
        assert_eq!(inbox.push(2), Err(InboxPushError::DeadlineExceeded));
    });
    assert_eq!(inbox.stats().blocked.load(Ordering::Relaxed), 1);
    assert_eq!(inbox.stats().deadline_expired.load(Ordering::Relaxed), 1);
    assert_eq!(ready(inbox.recv()), Some(1));
}

fn send_fields() {
    let core = core(4);
    let template = TxTemplate::<StreamChunk4kV1>::new(super::agreement()).unwrap();
    let route = template
        .route(
            Arc::clone(&core),
            super::definition(),
            RouteConfig {
                max_token_len: 16,
                allow_traceparent: false,
            },
        )
        .unwrap();
    let disabled = template
        .route(
            Arc::clone(&core),
            super::definition(),
            RouteConfig {
                max_token_len: 0,
                allow_traceparent: true,
            },
        )
        .unwrap();
    let response = template
        .route(
            Arc::clone(&core),
            RouteDefinition {
                kind: UMessageType::Response,
                source: UUri::try_from_wire_parts("server", 0x0001_0001, 1, 1).unwrap(),
                sink: Some(UUri::try_from_wire_parts("client", 0x0001_0002, 1, 0).unwrap()),
                priority: Some(UPriority::CS4),
                ttl: None,
                permission_level: None,
            },
            RouteConfig::default(),
        )
        .unwrap();
    let trace = [b'x'; 64];
    let long_trace = std::str::from_utf8(&trace).unwrap();
    let reqid = UUID::build();
    measure("send_token_and_traceparent_rejected", || {
        for _ in 0..10_000 {
            for (fields, expected) in [
                (
                    MessageFields {
                        token: Some("token-that-is-too-long"),
                        ..MessageFields::default()
                    },
                    SendError::TokenTooLong,
                ),
                (
                    MessageFields {
                        traceparent: Some("trace"),
                        ..MessageFields::default()
                    },
                    SendError::TraceparentRejected,
                ),
            ] {
                assert_eq!(
                    route.send(fields, |_| panic!("field rejection precedes filler")),
                    Err(expected)
                );
            }
            assert_eq!(
                disabled.send(
                    MessageFields {
                        token: Some(""),
                        ..MessageFields::default()
                    },
                    |_| panic!("token disabled")
                ),
                Err(SendError::TokenNotAllowed)
            );
            assert_eq!(
                disabled.send(
                    MessageFields {
                        traceparent: Some(long_trace),
                        ..MessageFields::default()
                    },
                    |_| panic!("trace too long")
                ),
                Err(SendError::TraceparentRejected)
            );
            assert_eq!(
                response.send(MessageFields::default(), |_| panic!("missing request ID")),
                Err(SendError::MissingRequestId)
            );
            assert_eq!(
                response.send(
                    MessageFields {
                        reqid: Some(&reqid),
                        priority: Some(UPriority::CS1),
                        ..MessageFields::default()
                    },
                    |_| panic!("invalid priority")
                ),
                Err(SendError::PriorityRejected)
            );
        }
    });
    assert_eq!(core.free_slots(), 4);
}

#[repr(C)]
#[derive(Debug, StablePayload)]
#[stable_payload(type_name = "uprotocol.test.AdmissionBits")]
struct Checked {
    number: u64,
    flag: bool,
    padding: [u8; 7],
}
#[repr(align(8))]
struct Aligned([u8; 24]);
static VALID: Aligned = Aligned([0; 24]);
static INVALID: Aligned = Aligned([
    0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
]);

struct RawFrame {
    header: Arc<[u8]>,
    bytes: Option<&'static [u8]>,
}
impl Frame for RawFrame {
    fn encoded_metadata(&self) -> &[u8] {
        &self.header
    }
    fn payload(&self) -> Option<LoanedPayload<'_>> {
        self.bytes
            .map(|bytes| LoanedPayload::new(bytes, PayloadLoanProvenance::OpaqueTransportLoan))
    }
}

fn encode(metadata: &UFrameMetadata) -> Arc<[u8]> {
    UFrameFieldsMetadataCodec
        .encode_frame_metadata(
            UWireMetadataContext::from_wire::<StableContainerWire, UFrameFieldsMetadataCodec>(),
            metadata,
        )
        .unwrap()
        .into()
}

fn admission_rejections() {
    let profile = NativeProfile::new(
        "admission-allocation",
        1,
        PayloadEncoding::private_use(0xF101),
        [
            Checked::native_representation(),
            StreamChunk4kV1::native_representation(),
        ],
    )
    .unwrap();
    let agreement = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
    let admission =
        RxAdmission::<Checked>::new(agreement.clone(), HeaderLimits::default()).unwrap();
    let other =
        RxAdmission::<StreamChunk4kV1>::new(agreement.clone(), HeaderLimits::default()).unwrap();
    let metadata = UFrameMetadata::publish(super::definition().source)
        .with_native_payload_identity(admission.identity())
        .build()
        .unwrap();
    let header = encode(&metadata);
    let foreign = encode(
        &UFrameMetadata::publish(super::definition().source)
            .with_native_payload_identity(other.identity())
            .build()
            .unwrap(),
    );
    let valid = VALID.0.get(..16).unwrap();
    measure("admission_wrong_payload_type", || {
        for _ in 0..10_000 {
            assert!(matches!(
                admission.admit(RawFrame {
                    header: Arc::clone(&foreign),
                    bytes: Some(valid)
                }),
                Err(AdmissionError::WrongPayloadType)
            ));
        }
    });
    let truncated: Arc<[u8]> = header.get(..1).unwrap().into();
    let token_header = encode(
        &UFrameMetadata::publish(super::definition().source)
            .with_native_payload_identity(admission.identity())
            .with_token("credential")
            .build()
            .unwrap(),
    );
    let no_tokens =
        RxAdmission::<Checked>::new(agreement, HeaderLimits { max_token_len: 0 }).unwrap();
    measure("admission_presence_length_alignment_bits_header", || {
        for _ in 0..128 {
            for (bytes, error) in [
                (None, AdmissionError::PayloadPresence),
                (
                    Some(VALID.0.get(..15).unwrap()),
                    AdmissionError::PayloadLength,
                ),
                (
                    Some(VALID.0.get(1..17).unwrap()),
                    AdmissionError::PayloadAlignment,
                ),
                (
                    Some(INVALID.0.get(..16).unwrap()),
                    AdmissionError::PayloadFieldBits,
                ),
            ] {
                assert_eq!(
                    admission
                        .admit(RawFrame {
                            header: Arc::clone(&header),
                            bytes
                        })
                        .unwrap_err(),
                    error
                );
            }
            assert!(matches!(
                admission.admit(RawFrame {
                    header: Arc::clone(&truncated),
                    bytes: Some(valid)
                }),
                Err(AdmissionError::Header(_))
            ));
            assert!(matches!(
                no_tokens.admit(RawFrame {
                    header: Arc::clone(&token_header),
                    bytes: Some(valid)
                }),
                Err(AdmissionError::Header(
                    up_rust::UFrameHeaderError::TokenTooLong
                ))
            ));
            let accepted = admission
                .admit(RawFrame {
                    header: Arc::clone(&header),
                    bytes: Some(valid),
                })
                .unwrap();
            assert_eq!(accepted.payload().unwrap().number, 0);
            assert!(!accepted.payload().unwrap().flag);
        }
    });
    let mut work = header.to_vec();
    let mut rejected = 0;
    measure("parse_mutated_headers", || {
        for round in 0..50_000_usize {
            work.copy_from_slice(&header);
            let index = (round * 7) % work.len();
            let byte = work.get_mut(index).unwrap();
            *byte = byte.wrapping_add(u8::try_from(1 + round % 251).unwrap());
            let cut = if round % 5 == 0 {
                work.len() - round % work.len()
            } else {
                work.len()
            };
            if up_rust::parse_header(
                admission.prefix(),
                work.get(..cut).unwrap(),
                &HeaderLimits::default(),
            )
            .is_err()
            {
                rejected += 1;
            }
        }
    });
    assert!(rejected > 0);
}

fn authority_rejections() {
    let long = [b'a'; 129];
    let long = std::str::from_utf8(&long).unwrap();
    measure("wire_uri_rejection_variants", || {
        for (authority, expected) in [
            (long, UUriPartsError::AuthorityTooLong),
            ("[", UUriPartsError::MalformedAuthority),
            ("vin/x", UUriPartsError::AuthorityHasTrailingComponent),
            ("vin:80", UUriPartsError::AuthorityHasPort),
            ("user@vin", UUriPartsError::AuthorityHasUserinfo),
            ("INVALID", UUriPartsError::InvalidAuthorityCharacters),
            ("[v1.xy]", UUriPartsError::UnsupportedHost),
        ] {
            assert_eq!(
                UUri::try_from_wire_parts(authority, 0x0001_0001, 1, 0),
                Err(expected)
            );
        }
    });
    let invalid: Vec<_> = ["[v1.xy]", "vin/x", "vin?x", "vin#x"]
        .into_iter()
        .flat_map(|authority| [false, true].map(move |sink| (authority, sink)))
        .map(|(authority, sink)| {
            let mut fields = vec![1, if sink { 2 } else { 1 }, 0, 0];
            fields.extend_from_slice(&(u32::from(sink)).to_le_bytes());
            fields.extend_from_slice(&0x7000_u64.to_le_bytes());
            fields.extend_from_slice(&0x8000_0000_0000_0000_u64.to_le_bytes());
            let mut uri = |authority: &str, resource: u16| {
                fields.extend_from_slice(&1_u32.to_le_bytes());
                fields.extend_from_slice(&resource.to_le_bytes());
                fields.push(1);
                fields.push(u8::try_from(authority.len()).unwrap());
                fields.extend_from_slice(authority.as_bytes());
            };
            uri(if sink { "vehicle" } else { authority }, 0x8001);
            if sink {
                uri(authority, 0);
            }
            let mut framed = u32::try_from(fields.len()).unwrap().to_le_bytes().to_vec();
            framed.extend_from_slice(&fields);
            framed
        })
        .collect();
    measure("received_bad_authorities_rejected_in_place", || {
        for bytes in &invalid {
            assert!(matches!(
                up_rust::parse_header(&[], bytes, &HeaderLimits::default()),
                Err(up_rust::UFrameHeaderError::Uri(_))
            ));
        }
    });
}

pub(super) fn run(runtime: &Runtime) {
    pool_exhausted();
    overflow(runtime);
    block_deadline();
    send_fields();
    admission_rejections();
    authority_rejections();
}
