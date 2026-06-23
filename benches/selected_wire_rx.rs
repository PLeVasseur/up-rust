/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

mod support;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use support::{allocation_sample, encoded_frame_for, native_wire_rx, reset_allocations};
use up_rust::UProtocolNativeWire;

fn bench_rx(c: &mut Criterion) {
    let frame = encoded_frame_for::<UProtocolNativeWire>(support::source_uri());

    c.bench_function("selected_wire_rx/try_from_encoded", |b| {
        b.iter(|| {
            reset_allocations();
            let rx = native_wire_rx(black_box(frame.clone()));
            let sample = allocation_sample();
            black_box((rx, sample))
        });
    });
}

criterion_group!(benches, bench_rx);
criterion_main!(benches);
