// SPDX-License-Identifier: Apache-2.0
//! All-thread windows for paths the warmed windows above never reach: the first
//! traffic on an unwarmed route and receiver, a window that contains subscription
//! activation, and a peer joining and leaving while traffic flows.

use super::{agreement, definition, measure, ready};
use crate::native_fixture;
use std::{
    hint::black_box,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;
use up_rust::bench_fixtures::payload_contract::{init_streamer_4k, StreamChunk4kV1};
use up_rust::communication::typed::{
    Delivery, Endpoint, Handler, Native, NativeConfig, PublishOptions,
};
use up_rust::communication::zero_copy::ReceiveConfig;
use up_rust::{
    HeaderLimits, LocalUriProvider, MessageFields, OverflowPolicy, PoolCore, PoolCoreConfig,
    PreparedTransportCore, RegistrationConfig, RegistrationInbox, RouteConfig, StableContainerWire,
    StaticUriProvider, UFrameFieldsMetadataCodec, UUri, UWireTransport,
};

fn core(slots: usize) -> Arc<PoolCore> {
    Arc::new(
        PoolCore::new(PoolCoreConfig {
            slots,
            slot_bytes: 8192,
            alignment: 8,
            max_routes: 8,
            max_registrations: 8,
        })
        .unwrap(),
    )
}

/// A route that has sent nothing before the window; the first send is measured.
fn first_send_unwarmed_route() {
    let core = core(8);
    let adapter = UWireTransport::with_native_agreement(
        Arc::clone(&core),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        agreement(),
    );
    let route = adapter
        .prepare_stable_route::<StreamChunk4kV1>(definition(), RouteConfig::default())
        .unwrap();
    measure("first_send_unwarmed_route", || {
        route
            .send(MessageFields::default(), |init| {
                init_streamer_4k(init.into_initializer(), 0).unwrap()
            })
            .unwrap();
    });
}

/// A receiver that has admitted nothing before the window; the first admission is
/// measured. The frame is taken and released outside the window.
fn first_receive_unwarmed() {
    let core = core(8);
    let adapter = UWireTransport::with_native_agreement(
        Arc::clone(&core),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        agreement(),
    );
    let route = adapter
        .prepare_stable_route::<StreamChunk4kV1>(definition(), RouteConfig::default())
        .unwrap();
    let admission = adapter
        .prepare_stable_receiver::<StreamChunk4kV1>(HeaderLimits::default())
        .unwrap();
    let inbox = Arc::new(
        RegistrationInbox::new(RegistrationConfig {
            capacity: 1,
            overflow: OverflowPolicy::DropNewest,
        })
        .unwrap(),
    );
    let mut subscription = ready(core.attach(&definition().source, None, inbox.clone())).unwrap();
    route
        .send(MessageFields::default(), |init| {
            init_streamer_4k(init.into_initializer(), 1).unwrap()
        })
        .unwrap();
    let received = ready(inbox.recv()).unwrap();
    core.detach(&mut subscription).unwrap();
    measure("first_receive_unwarmed_admit_and_borrow", || {
        let lease = admission.admit(received.try_retain().unwrap()).unwrap();
        black_box(lease.payload().unwrap());
        drop(lease);
    });
}

/// Setup-only subscription service; the window below includes the activation that
/// calls it, because activation is the boundary into traffic for a subscriber.
struct ActivationService;

#[async_trait::async_trait]
impl up_rust::core::usubscription::USubscription for ActivationService {
    async fn subscribe(
        &self,
        _: &UUri,
        _: Option<u64>,
        _: Option<u32>,
    ) -> Result<up_rust::communication::SubscriptionStatus, up_rust::UStatus> {
        Ok(up_rust::communication::SubscriptionStatus::Subscribed)
    }
    async fn unsubscribe(&self, _: &UUri) -> Result<(), up_rust::UStatus> {
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

struct CountDeliveries(Arc<AtomicUsize>);
impl Handler<StreamChunk4kV1> for CountDeliveries {
    async fn on_receive<D: Delivery<StreamChunk4kV1>>(&mut self, frame: D) {
        drop(frame);
        self.0.fetch_add(1, Ordering::Release);
    }
}

/// The window contains activation and the first publish and delivery after it, with
/// no earlier traffic on this endpoint.
fn activation_and_first_traffic(runtime: &Runtime) {
    let core = core(32);
    let transport = Arc::new(UWireTransport::with_native_agreement(
        Arc::new(native_fixture::FixtureCore(Arc::clone(&core))),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        agreement(),
    ));
    let provider = Arc::new(StaticUriProvider::new("allocation", 0x0001_0001, 1).unwrap());
    let topic = provider.get_resource_uri(0x8001);
    let endpoint = Endpoint::new(
        Native::new(
            transport,
            NativeConfig {
                route: RouteConfig::default(),
                receive: ReceiveConfig {
                    registration: RegistrationConfig {
                        capacity: 8,
                        overflow: OverflowPolicy::DropNewest,
                    },
                    header: HeaderLimits::default(),
                },
            },
        )
        .unwrap(),
        provider,
    );
    let publisher = endpoint
        .publisher::<StreamChunk4kV1>(0x8001, PublishOptions::default())
        .unwrap();
    let subscriber = endpoint.subscriber(Arc::new(ActivationService));
    let seen = Arc::new(AtomicUsize::new(0));
    let mut subscription = runtime
        .block_on(subscriber.subscription(topic, CountDeliveries(Arc::clone(&seen))))
        .unwrap();
    measure("activation_and_first_traffic", || {
        runtime.block_on(async {
            subscription.activate().await.unwrap();
            publisher
                .publish_in_place_with(MessageFields::default(), |init| {
                    init_streamer_4k(init.into_initializer(), 0).unwrap()
                })
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while seen.load(Ordering::Acquire) != 1 {
                assert!(Instant::now() < deadline, "first delivery did not arrive");
                tokio::task::yield_now().await;
            }
        })
    });
    runtime.block_on(subscription.unsubscribe()).unwrap();
    assert_eq!(core.free_slots(), 32);
}

/// A second peer attaches and detaches between sends. This is setup executed at
/// runtime, which the two-mode design must refuse; it measures today's cost.
fn peer_attach_detach_during_traffic() {
    let core = core(8);
    let adapter = UWireTransport::with_native_agreement(
        Arc::clone(&core),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        agreement(),
    );
    let route = adapter
        .prepare_stable_route::<StreamChunk4kV1>(definition(), RouteConfig::default())
        .unwrap();
    let inbox = |capacity| {
        Arc::new(
            RegistrationInbox::new(RegistrationConfig {
                capacity,
                overflow: OverflowPolicy::DropNewest,
            })
            .unwrap(),
        )
    };
    let established = inbox(4);
    let late_peer = inbox(4);
    let mut subscription =
        ready(core.attach(&definition().source, None, established.clone())).unwrap();
    let send = |sequence| {
        route
            .send(MessageFields::default(), |init| {
                init_streamer_4k(init.into_initializer(), sequence).unwrap()
            })
            .unwrap()
    };
    send(0);
    drop(ready(established.recv()).unwrap());
    let peer_source = definition().source;
    measure("peer_attach_detach_during_traffic", || {
        let mut peer = ready(core.attach(&peer_source, None, late_peer.clone())).unwrap();
        send(1);
        drop(ready(established.recv()).unwrap());
        drop(ready(late_peer.recv()).unwrap());
        core.detach(&mut peer).unwrap();
        send(2);
        drop(ready(established.recv()).unwrap());
    });
    core.detach(&mut subscription).unwrap();
}

pub(super) fn run(runtime: &Runtime) {
    first_send_unwarmed_route();
    first_receive_unwarmed();
    activation_and_first_traffic(runtime);
    peer_attach_detach_during_traffic();
}
