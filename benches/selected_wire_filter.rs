/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

mod support;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use support::{encoded_frame_for, BenchCore};
use tokio::runtime::Runtime;
use up_rust::{UProtocolNativeWire, UWithNativePrefixProtobufMetadata, UZeroCopyTransport};

fn bench_filter(c: &mut Criterion) {
    let rt = Runtime::new().expect("tokio runtime");
    let source = support::source_uri();
    let wildcard = support::wildcard_source_uri();

    c.bench_function("selected_wire_filter/exact_receive", |b| {
        b.iter(|| {
            let core = BenchCore::default();
            core.push_rx(encoded_frame_for::<UProtocolNativeWire>(source.clone()));
            let transport = core.with_native_prefix_protobuf_metadata(UProtocolNativeWire);
            let frame = rt
                .block_on(transport.receive_zero_copy(black_box(&source), None))
                .expect("receive exact");
            black_box(frame)
        });
    });

    c.bench_function("selected_wire_filter/wildcard_receive", |b| {
        b.iter(|| {
            let core = BenchCore::default();
            core.push_rx(encoded_frame_for::<UProtocolNativeWire>(source.clone()));
            let transport = core.with_native_prefix_protobuf_metadata(UProtocolNativeWire);
            let frame = rt
                .block_on(transport.receive_zero_copy(black_box(&wildcard), None))
                .expect("receive wildcard");
            black_box(frame)
        });
    });
}

criterion_group!(benches, bench_filter);
criterion_main!(benches);
