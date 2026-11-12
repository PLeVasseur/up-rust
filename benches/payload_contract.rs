/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

//! Bounded payload-contract diagnostics.
//!
//! This executable records one-shot allocation and explicit-copy shape. Its
//! elapsed values are smoke diagnostics, not host-performance measurements.

#[path = "support/payload_heap_core.rs"]
mod heap_core;

use std::alloc::System;
use std::future::Future;
use std::hint::black_box;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Instant;

use bytes::Bytes;
use up_rust::bench_fixtures::payload_contract::{
    all_cases, case_by_kind, init_streamer_4k, stable_owned_fixture_for, validate_protobuf_bytes,
    PayloadContractCaseKind, StreamChunk4kV1,
};
use up_rust::communication::zero_copy::Publisher;
use up_rust::frame::metadata::try_project_umessage_to_frame_metadata;
use up_rust::heap_guard::{HeapGuard, HeapWindow, Scope};
use up_rust::{
    Frame, FrameSink, HeaderLimits, InitializedTx, PoolFrame, PreparedTransportCore,
    PreparedTxStorage, RouteLimits, TxFrameLayout, WrittenHeader,
};
use up_rust::{
    LoanedPayload, LocalUriProvider, MessageFields, NativeProfile, NativeProfileAgreement,
    PayloadEncoding, PayloadLoanProvenance, PoolCore, PoolCoreConfig, RouteConfig, RouteDefinition,
    StableContainerPayload, StableContainerWire, StablePayload, StablePayloadInit,
    StaticUriProvider, UEncodedLoanedRxFrame, UEncodedRxFrame, UFrameFieldsMetadataCodec,
    UFrameMetadata, UMessageBuilder, UMessageType, UTxBuffer, UUninitTxBuffer, UUri, UVecTxBuffer,
    UVecUninitTxBuffer, UWireMetadataCodec, UWireMetadataContext, UWireRx, UWireTransport,
};

#[global_allocator]
static ALLOCATOR: HeapGuard = HeapGuard::new(System);

/// Zero-heap subjects that used the heap. Every subject is reported and the
/// binary fails at the end, so one finding cannot hide the others.
static ZERO_SUBJECTS_WITH_HEAP_USE: AtomicUsize = AtomicUsize::new(0);

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("benchmark operation unexpectedly suspended"),
    }
}

fn measure<T>(
    name: &str,
    explicit_copies: usize,
    validations: usize,
    operation: impl FnOnce() -> T,
) -> T {
    let window = HeapWindow::open_counts_only(Scope::AllThreads);
    let started = Instant::now();
    let result = black_box(operation());
    let elapsed = started.elapsed();
    let report = window.finish();
    let counts = report.counts();
    println!(
        "subject={name}\tallocations={}\tallocated_bytes={}\tdeallocations={}\tdeallocated_bytes={}\treallocations={}\treallocated_bytes={}\texplicit_copies={explicit_copies}\tvalidations={validations}\telapsed_ns={}\thost_performance_claim=false",
        counts.allocations,
        counts.allocated_bytes,
        counts.deallocations,
        counts.deallocated_bytes,
        counts.reallocations,
        counts.reallocated_bytes,
        elapsed.as_nanos(),
    );
    let zero_heap_subject = matches!(
        name,
        "prepared_loan"
            | "selected_wire_borrow"
            | "adapter_core_stable_send"
            | "l2_stable_publish"
            | "core_only_loan_commit"
            | "direct_metadata_builder"
            | "in_place_receive"
    );
    if zero_heap_subject && !report.is_clean() {
        ZERO_SUBJECTS_WITH_HEAP_USE.fetch_add(1, Ordering::SeqCst);
        println!("heap_finding subject={name}: {report}");
    }
    result
}

fn benchmark_profile() -> NativeProfileAgreement {
    // Explicit one-type agreement for this same-process benchmark only.

    let profile = NativeProfile::new(
        "payload-contract-benchmark",
        1,
        PayloadEncoding::from_id(0xF301).unwrap(),
        [StreamChunk4kV1::native_representation()],
    )
    .unwrap();
    NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap()
}

fn payload_metadata(profile: &NativeProfileAgreement) -> UFrameMetadata {
    UFrameMetadata::publish(
        UUri::try_from_parts("payload-contract-bench", 0x0001_4210, 1, 0x9000)
            .expect("benchmark URI is valid"),
    )
    .with_native_payload_identity(
        StableContainerPayload::<StreamChunk4kV1>::identity(profile)
            .expect("agreed benchmark representation"),
    )
    .build()
    .expect("benchmark metadata is valid")
}

#[derive(Debug)]
struct RawFrame {
    encoded_metadata: Vec<u8>,
    payload: Vec<u8>,
}

impl UEncodedRxFrame for RawFrame {
    type PayloadReader<'a> = Cursor<&'a [u8]>;
    type PayloadSlices<'a> = core::iter::Once<&'a [u8]>;

    fn encoded_metadata(&self) -> &[u8] {
        &self.encoded_metadata
    }

    fn payload_len(&self) -> usize {
        self.payload.len()
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        Cursor::new(&self.payload)
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        core::iter::once(&self.payload)
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        Some(&self.payload)
    }
}

impl UEncodedLoanedRxFrame for RawFrame {
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, up_rust::UWireError> {
        // `payload` is owned by this receive object and remains alive for the
        // returned borrow; no allocation, copy, or coalescing occurs here.
        Ok(LoanedPayload::new(
            &self.payload,
            PayloadLoanProvenance::OpaqueTransportLoan,
        ))
    }
}

impl Frame for RawFrame {
    fn encoded_metadata(&self) -> &[u8] {
        &self.encoded_metadata
    }
    fn payload(&self) -> Option<LoanedPayload<'_>> {
        Some(LoanedPayload::new(
            &self.payload,
            PayloadLoanProvenance::OwnedReceiveLease,
        ))
    }
}

#[derive(Default)]
struct ObservePool(std::sync::atomic::AtomicUsize);
impl FrameSink<PoolFrame> for ObservePool {
    fn push(&self, frame: PoolFrame) {
        black_box(frame.encoded_metadata());
        black_box(frame.payload().expect("present benchmark payload").bytes());
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn close(&self) {}
}

fn main() {
    let profile = benchmark_profile();
    println!(
        "native_profile_domain={}\tnative_profile_version={}\tnative_profile_digest={}",
        profile.profile().domain(),
        profile.profile().version(),
        hex::encode(profile.profile().content_digest())
    );
    let smoke = std::env::var_os("UP_PAYLOAD_BENCH_SMOKE").is_some();
    let iterations = if smoke { 1 } else { 10 };
    println!(
        "mode={}\titerations={iterations}\telapsed_values_are_diagnostic_only=true",
        if smoke { "smoke" } else { "bounded" }
    );

    for case in all_cases() {
        println!(
            "payload={}\tsemantic_bytes={}\tstable_bytes={}\tstable_alignment={}",
            case.name(),
            case.semantic_reference_len(),
            up_rust::bench_fixtures::payload_contract::stable_payload_len(case),
            up_rust::bench_fixtures::payload_contract::stable_payload_align(case),
        );
    }

    let case = case_by_kind(PayloadContractCaseKind::Streamer4k);
    let canonical = stable_owned_fixture_for(case, 1, &profile).expect("canonical stable fixture");
    let core = Arc::new(
        PoolCore::new(PoolCoreConfig {
            slots: 4,
            slot_bytes: 8192,
            alignment: 8,
            max_routes: 8,
            max_registrations: 1,
        })
        .expect("reserved benchmark pool"),
    );
    let transport = UWireTransport::with_native_agreement(
        Arc::clone(&core),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        profile.clone(),
    );
    let definition = RouteDefinition {
        kind: UMessageType::Publish,
        source: UUri::try_from_parts("payload-contract-bench", 0x0001_4210, 1, 0x9000).unwrap(),
        sink: None,
        priority: None,
        ttl: None,
        permission_level: None,
    };
    let route = transport
        .prepare_stable_route::<StreamChunk4kV1>(definition.clone(), RouteConfig::default())
        .unwrap();
    let heap_transport = UWireTransport::with_native_agreement(
        Arc::new(heap_core::HeapCore::new()),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        profile.clone(),
    );
    let heap_route = heap_transport
        .prepare_stable_route::<StreamChunk4kV1>(definition.clone(), RouteConfig::default())
        .unwrap();
    let template = transport
        .prepare_stable_template::<StreamChunk4kV1>()
        .unwrap();
    let admission = transport
        .prepare_stable_receiver::<StreamChunk4kV1>(HeaderLimits::default())
        .unwrap();
    let core_route = core
        .prepare_route(
            &definition.source,
            None,
            RouteLimits {
                max_metadata_len: UFrameFieldsMetadataCodec
                    .encoded_frame_metadata_len(
                        UWireMetadataContext::from_wire::<
                            StableContainerWire,
                            UFrameFieldsMetadataCodec,
                        >(),
                        &payload_metadata(&profile),
                    )
                    .expect("fixed core-control header extent"),
                max_payload_len: canonical.bytes.len(),
                payload_alignment: canonical.stable_align,
            },
        )
        .unwrap();
    let provider = StaticUriProvider::new("payload-contract-bench", 0x0001_4210, 1).unwrap();
    assert_eq!(provider.get_resource_uri(0x9000), definition.source);
    let observer = Arc::new(ObservePool::default());
    let mut observation = ready(core.attach(&definition.source, None, observer.clone())).unwrap();
    let _ = up_rust::UUID::build();
    println!("storage=pool_reserved_at_setup\tprepared_loan_scope=metadata_build_encode_loan_and_typed_init\tmetadata_work_retained=true\tfuture_polling=noop_waker");

    for sequence in 0..iterations {
        measure("generated_uninitialized_init", 0, 1, || {
            let mut buffer = UVecUninitTxBuffer::with_alignment(
                payload_metadata(&profile),
                canonical.bytes.len(),
                canonical.stable_align,
            )
            .expect("uninitialized loan");
            {
                let initialized = init_streamer_4k(
                    StreamChunk4kV1::init(buffer.payload_uninit_mut())
                        .expect("stable initializer construction"),
                    sequence,
                )
                .expect("generated stable initialization");
                black_box(initialized.as_bytes());
            }
            // SAFETY: The generated typestate initializer completed every field.
            unsafe { buffer.assume_payload_initialized() }
        });

        measure("initialized_loan_copy", 1, 0, || {
            let mut buffer = UVecTxBuffer::with_alignment(
                payload_metadata(&profile),
                canonical.bytes.len(),
                canonical.stable_align,
            )
            .expect("initialized loan");
            buffer.payload_mut().copy_from_slice(&canonical.bytes);
            buffer
        });

        measure("direct_memcpy_control", 1, 0, || {
            let mut bytes = vec![0_u8; canonical.bytes.len()];
            bytes.copy_from_slice(&canonical.bytes);
            bytes
        });

        measure("direct_metadata_builder", 0, 1, || {
            payload_metadata(&profile)
        });
        measure("protobuf_message_projection", 0, 2, || {
            let mut builder = UMessageBuilder::publish(
                UUri::try_from_parts("payload-contract-bench", 0x0001_4210, 1, 0x9000)
                    .expect("benchmark URI is valid"),
            );
            let message = builder
                .build_with_payload(Bytes::from_static(b"x"), PayloadEncoding::RAW)
                .expect("benchmark message");
            try_project_umessage_to_frame_metadata(&message).expect("metadata projection")
        });

        let codec = UFrameFieldsMetadataCodec;
        let metadata = payload_metadata(&profile);
        let encoded_metadata = measure("metadata_encode", 0, 1, || {
            codec
                .encode_frame_metadata(
                    UWireMetadataContext::from_wire::<
                        StableContainerWire,
                        UFrameFieldsMetadataCodec,
                    >(),
                    &metadata,
                )
                .expect("metadata encode")
        });
        measure("metadata_decode", 0, 1, || {
            codec
                .decode_frame_metadata(
                    UWireMetadataContext::from_wire::<
                        StableContainerWire,
                        UFrameFieldsMetadataCodec,
                    >(),
                    &encoded_metadata,
                )
                .expect("metadata decode")
        });

        measure("prepared_loan", 0, 1, || {
            // The complete original metadata construction and encoding remain;
            // the new prepared contract additionally initializes the real loan.
            let prepared = route
                .prepare_metadata(&payload_metadata(&profile), |init| {
                    init_streamer_4k(init.into_initializer(), sequence).unwrap()
                })
                .expect("prepared TX loan");
            drop(prepared);
        });

        let receive = UWireRx::<
            RawFrame,
            StableContainerWire,
            UFrameFieldsMetadataCodec,
        >::try_from_encoded_with_native_agreement(
            RawFrame {
                encoded_metadata: encoded_metadata.clone(),
                payload: canonical.bytes.clone(),
            },
            &codec,
            profile.clone(),
        )
        .expect("selected-wire receive");
        measure("selected_wire_borrow", 0, 1, || {
            receive
                .borrow_payload::<StreamChunk4kV1>()
                .expect("stable borrow")
        });

        let raw = RawFrame {
            encoded_metadata: encoded_metadata.clone(),
            payload: canonical.bytes.clone(),
        };
        measure("in_place_receive", 0, 2, || {
            let lease = admission.admit(raw).expect("in-place receive");
            black_box(lease.payload().expect("checked stable payload"));
        });
        measure("core_only_loan_commit", 0, 0, || {
            let mut buffer = core
                .loan(
                    &core_route,
                    TxFrameLayout {
                        metadata_len: encoded_metadata.len(),
                        payload_len: Some(canonical.bytes.len()),
                        payload_alignment: canonical.stable_align,
                    },
                )
                .unwrap();
            for (out, byte) in buffer
                .metadata_uninit_mut()
                .iter_mut()
                .zip(&encoded_metadata)
            {
                out.write(*byte);
            }
            for byte in buffer.payload_uninit_mut().unwrap() {
                byte.write(0);
            }
            buffer
                .record_header(WrittenHeader {
                    len: encoded_metadata.len(),
                    token: None,
                })
                .unwrap();
            // SAFETY: every byte in the declared header/payload extents was
            // written above; the prepared header has no credential token. The
            // core-only control exposes no typed payload reference.
            let initialized = unsafe { InitializedTx::from_initialized(buffer) };
            core.publish(&core_route, initialized)
                .expect("core-only loan and commit");
        });

        measure("adapter_core_stable_send", 0, 2, || {
            route
                .send_metadata(&payload_metadata(&profile), |init| {
                    init_streamer_4k(init.into_initializer(), sequence).unwrap()
                })
                .expect("adapter/core send")
        });

        measure("heap_core_stable_send", 0, 2, || {
            heap_route
                .send_metadata(&payload_metadata(&profile), |init| {
                    init_streamer_4k(init.into_initializer(), sequence)
                        .expect("generated stable initialization")
                })
                .expect("adapter/core send")
        });

        measure("l2_stable_publish", 0, 2, || {
            // URI construction, route metadata, encoding and the actual L2 send
            // remain in the measured operation. Only reserved storage and the
            // immutable template are setup inputs.
            let route = template
                .route(
                    Arc::clone(&core),
                    RouteDefinition {
                        kind: UMessageType::Publish,
                        source: provider.get_resource_uri(0x9000),
                        sink: None,
                        priority: None,
                        ttl: None,
                        permission_level: None,
                    },
                    RouteConfig::default(),
                )
                .expect("reserved publish route");
            let publisher = Publisher::from_route(route).expect("publisher");
            publisher
                .publish_stable(MessageFields::default(), |init| {
                    init_streamer_4k(init.into_initializer(), sequence)
                        .expect("generated stable initialization")
                })
                .expect("L2 stable publish")
        });
    }

    assert_eq!(
        observer.0.load(Ordering::Relaxed),
        usize::try_from(iterations * 3).unwrap()
    );
    core.detach(&mut observation).unwrap();
    assert_eq!(core.free_slots(), 4);
    for case in all_cases() {
        let bytes = up_rust::bench_fixtures::payload_contract::protobuf_encoded_bytes_for(case, 1)
            .expect("protobuf fixture encoding");
        validate_protobuf_bytes(case, 1, &bytes).expect("protobuf fixture validation");
    }
    let findings = ZERO_SUBJECTS_WITH_HEAP_USE.load(Ordering::SeqCst);
    if findings != 0 {
        println!("RESULT: {findings} zero-heap benchmark subject rows used the heap");
        std::process::exit(1);
    }
}
