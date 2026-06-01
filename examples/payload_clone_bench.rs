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

use std::hint::black_box;
use std::time::{Duration, Instant};

use up_rust::{UMessage, UMessageBuilder, UPayloadFormat, UUri};

const CASES: &[(usize, usize)] = &[
    (0, 200_000),
    (1_024, 100_000),
    (64 * 1_024, 10_000),
    (1_024 * 1_024, 1_000),
    (4 * 1_024 * 1_024, 250),
];
const REPEATS: usize = 7;

fn build_message(payload_size: usize) -> UMessage {
    let topic = UUri::try_from("//bench/8000/1/9000").expect("valid benchmark topic URI");
    let payload = vec![0xA5; payload_size];
    UMessageBuilder::publish(topic)
        .build_with_payload(
            payload,
            UPayloadFormat::from_media_type("application/octet-stream")
                .expect("valid raw payload format"),
        )
        .expect("valid benchmark publish message")
}

fn clone_loop(message: &UMessage, clones: usize) -> Duration {
    let start = Instant::now();
    for _ in 0..clones {
        let cloned = black_box(message.clone());
        black_box(cloned);
    }
    start.elapsed()
}

fn median_ns(message: &UMessage, clones: usize) -> u128 {
    let mut runs = Vec::with_capacity(REPEATS);
    for _ in 0..REPEATS {
        runs.push(clone_loop(message, clones).as_nanos());
    }
    runs.sort_unstable();
    runs[REPEATS / 2]
}

fn main() {
    println!("| payload bytes | clones | median total ms | median ns/clone |");
    println!("| ---: | ---: | ---: | ---: |");
    for &(payload_size, clones) in CASES {
        let message = build_message(payload_size);
        let _ = clone_loop(&message, 100);
        let median = median_ns(&message, clones);
        let total_ms = median as f64 / 1_000_000.0;
        let ns_per_clone = median as f64 / clones as f64;
        println!("| {payload_size} | {clones} | {total_ms:.3} | {ns_per_clone:.1} |");
    }
}
