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
//! Allocation windows through canonical L2 routes and service-backed registrations.

use super::measure;
use crate::native_fixture;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;
use up_rust::bench_fixtures::payload_contract::{
    case_by_kind, init_streamer_4k, validate_stable_payload, PayloadContractCaseKind,
    StreamChunk4kV1,
};
use up_rust::communication::typed::{
    Delivery, Endpoint, Handler, Native, NativeConfig, PublishOptions,
};
use up_rust::communication::zero_copy::ReceiveConfig;
use up_rust::{
    HeaderLimits, LocalUriProvider, MessageFields, OverflowPolicy, PoolCore, PoolCoreConfig,
    RegistrationConfig, RouteConfig, StableContainerWire, StaticUriProvider,
    UFrameFieldsMetadataCodec, UMessageType, UUri, UWireTransport,
};

const TRACEPARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";

// Setup-only service fixture: actual local role registration/delivery is measured,
// while remote service implementation and its control-plane RPC are not simulated
// as data-plane acceptance. Every service call occurs outside a counting window.
struct Service {
    topic: UUri,
    active: AtomicUsize,
}

#[async_trait::async_trait]
impl up_rust::core::usubscription::USubscription for Service {
    async fn subscribe(
        &self,
        topic: &UUri,
        _: Option<u64>,
        _: Option<u32>,
    ) -> Result<up_rust::communication::SubscriptionStatus, up_rust::UStatus> {
        assert!(!super::COUNTING.load(Ordering::SeqCst));
        assert_eq!(topic, &self.topic);
        self.active.fetch_add(1, Ordering::Relaxed);
        Ok(up_rust::communication::SubscriptionStatus::Subscribed)
    }
    async fn unsubscribe(&self, topic: &UUri) -> Result<(), up_rust::UStatus> {
        assert!(!super::COUNTING.load(Ordering::SeqCst));
        assert_eq!(topic, &self.topic);
        assert!(self.active.fetch_sub(1, Ordering::Relaxed) > 0);
        Ok(())
    }
    async fn fetch_subscriptions_by_topic(
        &self,
        _: &UUri,
    ) -> Result<Vec<up_rust::core::usubscription::SubscriptionInfo>, up_rust::UStatus> {
        unreachable!("unused service operation")
    }
    async fn fetch_subscriptions_by_subscriber(
        &self,
        _: &UUri,
    ) -> Result<Vec<up_rust::core::usubscription::SubscriptionInfo>, up_rust::UStatus> {
        unreachable!("unused service operation")
    }
    async fn register_for_notifications(&self, _: &UUri) -> Result<(), up_rust::UStatus> {
        unreachable!("unused service operation")
    }
    async fn unregister_for_notifications(&self, _: &UUri) -> Result<(), up_rust::UStatus> {
        unreachable!("unused service operation")
    }
    async fn fetch_subscribers(&self, _: &UUri) -> Result<Vec<UUri>, up_rust::UStatus> {
        unreachable!("unused service operation")
    }
    async fn reset(
        &self,
        _: up_rust::core::usubscription::ResetReason,
        _: Option<String>,
    ) -> Result<(), up_rust::UStatus> {
        unreachable!("unused service operation")
    }
}

struct Observer {
    seen: Arc<AtomicUsize>,
    sequence: u32,
    source: UUri,
    sink: Option<UUri>,
    kind: UMessageType,
    traceparent: Option<&'static str>,
}

impl Handler<StreamChunk4kV1> for Observer {
    async fn on_receive<D: Delivery<StreamChunk4kV1>>(&mut self, frame: D) {
        // A real await holds the lease, while each subscriber independently checks
        // FIFO, all fixture bytes and decoded metadata before reporting completion.
        tokio::task::yield_now().await;
        validate_stable_payload(
            case_by_kind(PayloadContractCaseKind::Streamer4k),
            self.sequence,
            frame.payload().unwrap(),
        )
        .unwrap();
        let header = frame.context().unwrap();
        assert_eq!(header.kind, self.kind);
        assert_eq!(header.source, self.source);
        assert_eq!(header.sink, self.sink);
        assert_eq!(header.traceparent, self.traceparent);
        self.sequence += 1;
        drop(frame);
        self.seen.fetch_add(1, Ordering::Release);
    }
}

fn core() -> Arc<PoolCore> {
    Arc::new(
        PoolCore::new(PoolCoreConfig {
            slots: 32,
            slot_bytes: 8192,
            alignment: 8,
            max_routes: 8,
            max_registrations: 8,
        })
        .unwrap(),
    )
}

fn config(block: bool) -> ReceiveConfig {
    ReceiveConfig {
        registration: RegistrationConfig {
            capacity: 8,
            overflow: if block {
                OverflowPolicy::Block {
                    deadline: Duration::from_secs(2),
                }
            } else {
                OverflowPolicy::DropNewest
            },
        },
        header: HeaderLimits::default(),
    }
}

async fn await_count(seen: &AtomicUsize, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while seen.load(Ordering::Acquire) < expected {
        assert!(
            Instant::now() < deadline,
            "receiver did not finish its offered batch"
        );
        tokio::task::yield_now().await;
    }
}

fn publish(
    runtime: &Runtime,
    name: &str,
    fanout: usize,
    traceparent: bool,
    block: bool,
    count: u32,
) {
    let core = core();
    let transport = Arc::new(UWireTransport::with_native_agreement(
        Arc::new(native_fixture::FixtureCore(Arc::clone(&core))),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        super::agreement(),
    ));
    let provider = Arc::new(StaticUriProvider::new("allocation", 0x0001_0001, 1).unwrap());
    let topic = provider.get_resource_uri(0x8001);
    let endpoint = Endpoint::new(
        Native::new(
            transport,
            NativeConfig {
                route: RouteConfig {
                    max_token_len: 2048,
                    allow_traceparent: traceparent,
                },
                receive: config(block),
            },
        )
        .unwrap(),
        provider,
    );
    let publisher = endpoint
        .publisher::<StreamChunk4kV1>(0x8001, PublishOptions::default())
        .unwrap();
    let service = Arc::new(Service {
        topic: topic.clone(),
        active: AtomicUsize::new(0),
    });
    let subscriber = endpoint.subscriber(service.clone());
    let seen = Arc::new(AtomicUsize::new(0));
    let mut subscriptions = Vec::with_capacity(fanout);
    let traceparent = traceparent.then_some(TRACEPARENT);
    for _ in 0..fanout {
        let mut subscription = runtime
            .block_on(subscriber.subscription(
                topic.clone(),
                Observer {
                    seen: Arc::clone(&seen),
                    sequence: 0,
                    source: topic.clone(),
                    sink: None,
                    kind: UMessageType::Publish,
                    traceparent,
                },
            ))
            .unwrap();
        runtime.block_on(subscription.activate()).unwrap();
        subscriptions.push(subscription);
    }
    let run = async |start: u32, end: u32| {
        for sequence in start..end {
            publisher
                .publish_in_place_with(
                    MessageFields {
                        traceparent,
                        ..MessageFields::default()
                    },
                    |init| init_streamer_4k(init.into_initializer(), sequence).unwrap(),
                )
                .unwrap();
            // DropNewest is bounded by offered batches. Block deliberately sends
            // from this separate calling thread while runtime workers drain it.
            if !block && (sequence + 1).is_multiple_of(4) {
                await_count(&seen, usize::try_from(sequence + 1).unwrap() * fanout).await;
            }
        }
        await_count(&seen, usize::try_from(end).unwrap() * fanout).await;
    };
    runtime.block_on(run(0, 16));
    measure(name, || runtime.block_on(run(16, 16 + count)));
    assert_eq!(
        seen.load(Ordering::Acquire),
        usize::try_from(count + 16).unwrap() * fanout
    );
    for mut subscription in subscriptions {
        runtime.block_on(subscription.unsubscribe()).unwrap();
    }
    assert_eq!(service.active.load(Ordering::Relaxed), 0);
    assert_eq!(core.free_slots(), 32);
}

fn notify(runtime: &Runtime) {
    let core = core();
    let transport = Arc::new(UWireTransport::with_native_agreement(
        Arc::new(native_fixture::FixtureCore(Arc::clone(&core))),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        super::agreement(),
    ));
    let provider = Arc::new(StaticUriProvider::new("allocation", 0x0001_0001, 1).unwrap());
    let source = provider.get_resource_uri(0x8002);
    let destination = provider.get_source_uri();
    let endpoint = Endpoint::new(
        Native::new(
            transport,
            NativeConfig {
                route: RouteConfig::default(),
                receive: config(false),
            },
        )
        .unwrap(),
        provider,
    );
    let notifier = endpoint
        .notifier::<StreamChunk4kV1>(0x8002, destination.clone(), PublishOptions::default())
        .unwrap();
    let seen = Arc::new(AtomicUsize::new(0));
    let mut receiver = runtime
        .block_on(endpoint.listen(
            source.clone(),
            Observer {
                seen: Arc::clone(&seen),
                sequence: 0,
                source,
                sink: Some(destination),
                kind: UMessageType::Notification,
                traceparent: None,
            },
        ))
        .unwrap();
    let run = async |start, end| {
        for sequence in start..end {
            notifier
                .notify_in_place_with(MessageFields::default(), |init| {
                    init_streamer_4k(init.into_initializer(), sequence).unwrap()
                })
                .unwrap();
            if (sequence + 1).is_multiple_of(4) {
                await_count(&seen, usize::try_from(sequence + 1).unwrap()).await;
            }
        }
        await_count(&seen, usize::try_from(end).unwrap()).await;
    };
    runtime.block_on(run(0, 16));
    measure("notify_listener", || runtime.block_on(run(16, 20_016)));
    runtime.block_on(receiver.stop()).unwrap();
    assert_eq!(core.free_slots(), 32);
}

pub(super) fn run(runtime: &Runtime) {
    publish(runtime, "publish_async_handler", 1, false, false, 20_000);
    publish(runtime, "publish_fanout_3", 3, false, false, 20_000);
    publish(runtime, "publish_with_traceparent", 1, true, false, 20_000);
    notify(runtime);
    publish(runtime, "publish_4k_payload", 1, false, false, 10_000);
    publish(
        runtime,
        "publish_multi_thread_block_policy",
        1,
        false,
        true,
        50_000,
    );
}
