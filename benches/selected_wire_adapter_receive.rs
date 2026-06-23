/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

mod support;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use support::{encoded_frame_for, BenchCore, CountingZeroCopyListener};
use tokio::runtime::Runtime;
use up_rust::{UProtocolNativeWire, UWithNativePrefixProtobufMetadata, UZeroCopyTransport};

fn bench_adapter_receive(c: &mut Criterion) {
    let rt = Runtime::new().expect("tokio runtime");
    let source = support::source_uri();
    let other_source = support::other_source_uri();

    c.bench_function("selected_wire_adapter_receive/accepted", |b| {
        b.iter(|| {
            let core = BenchCore::default();
            core.push_rx(encoded_frame_for::<UProtocolNativeWire>(source.clone()));
            let transport = core.with_native_prefix_protobuf_metadata(UProtocolNativeWire);
            let frame = rt
                .block_on(transport.receive_zero_copy(black_box(&source), None))
                .expect("accepted receive");
            black_box(frame)
        });
    });

    c.bench_function(
        "selected_wire_adapter_receive/rejected_then_accepted",
        |b| {
            b.iter(|| {
                let core = BenchCore::default();
                core.push_rx(encoded_frame_for::<UProtocolNativeWire>(
                    other_source.clone(),
                ));
                core.push_rx(encoded_frame_for::<UProtocolNativeWire>(source.clone()));
                let transport = core.with_native_prefix_protobuf_metadata(UProtocolNativeWire);
                let frame = rt
                    .block_on(transport.receive_zero_copy(black_box(&source), None))
                    .expect("receive after reject");
                black_box(frame)
            });
        },
    );

    c.bench_function("selected_wire_adapter_receive/listener_fanout_drop", |b| {
        b.iter(|| {
            let core = BenchCore::default();
            let transport = core
                .clone()
                .with_native_prefix_protobuf_metadata(UProtocolNativeWire);
            let listener = Arc::new(CountingZeroCopyListener::default());
            rt.block_on(transport.register_zero_copy_listener(&source, None, listener.clone()))
                .expect("register listener");
            rt.block_on(core.inject(encoded_frame_for::<UProtocolNativeWire>(
                other_source.clone(),
            )));
            rt.block_on(core.inject(encoded_frame_for::<UProtocolNativeWire>(source.clone())));
            black_box((listener.count(), core.listener_count()))
        });
    });
}

criterion_group!(benches, bench_adapter_receive);
criterion_main!(benches);
