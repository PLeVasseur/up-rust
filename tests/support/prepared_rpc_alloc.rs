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
//! RPC allocation windows include first contact, token rotation and pending waits.

use super::{measure, measure_async, ready};

// An actual pool core with a setup-time handle to its response sink, so the
// binding-closure path can be exercised while the client still has a pending call.
struct ClosingCore {
    pool: Arc<PoolCore>,
    sink: std::sync::OnceLock<Arc<dyn up_rust::FrameSink<PoolFrame>>>,
}

impl PreparedTransportCore for ClosingCore {
    type Route = <PoolCore as PreparedTransportCore>::Route;
    type Tx<'a> = <PoolCore as PreparedTransportCore>::Tx<'a>;
    type Rx = PoolFrame;
    type Subscription = <PoolCore as PreparedTransportCore>::Subscription;

    fn prepare_route(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        limits: up_rust::RouteLimits,
    ) -> Result<Self::Route, up_rust::SetupError> {
        self.pool.prepare_route(source, sink, limits)
    }
    fn loan<'a>(
        &'a self,
        route: &'a Self::Route,
        layout: up_rust::TxFrameLayout,
    ) -> Result<Self::Tx<'a>, LoanError> {
        self.pool.loan(route, layout)
    }
    fn publish<'a>(
        &'a self,
        route: &'a Self::Route,
        frame: up_rust::InitializedTx<Self::Tx<'a>>,
    ) -> Result<(), SendError> {
        self.pool.publish(route, frame)
    }
    async fn attach(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        inbox: Arc<dyn up_rust::FrameSink<PoolFrame>>,
    ) -> Result<Self::Subscription, up_rust::SetupError> {
        let subscription = self.pool.attach(source, sink, Arc::clone(&inbox)).await?;
        assert!(self.sink.set(inbox).is_ok());
        Ok(subscription)
    }
    fn detach(&self, subscription: &mut Self::Subscription) -> Result<(), up_rust::SetupError> {
        self.pool.detach(subscription)
    }
}
use std::{
    future::Future,
    sync::{atomic::Ordering, Arc},
    task::{Context, Waker},
    time::Duration,
};
use tokio::runtime::Runtime;
use up_rust::bench_fixtures::payload_contract::{init_streamer_4k, StreamChunk4kV1};
use up_rust::{
    AdmissionError, CallError, FrameMessageKind, FramePriority, HeaderLimits,
    InitializedStablePayload, LoanError, MessageFields, NativeProfile, NativeProfileAgreement,
    NativeProfileMode, NativeProfileTable, OverflowPolicy, PayloadEncoding, PoolCore,
    PoolCoreConfig, PoolFrame, PreparedRpcClient, PreparedRpcServer, PreparedTransportCore,
    RegistrationConfig, RegistrationInbox, RequestHandler, RouteConfig, RouteDefinition,
    RpcClientConfig, RpcServerConfig, RxAdmission, RxLease, SendError, StablePayload,
    StablePayloadInit, TokenError, TxTemplate, UCode, UUri,
};

#[repr(C)]
#[derive(Debug, StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "uprotocol.test.AllocationReply")]
struct Reply {
    sequence: u32,
    token_len: u32,
    token_checksum: u32,
}

fn checksum(token: &str) -> u32 {
    token.bytes().fold(0_u32, |sum, byte| {
        sum.wrapping_mul(31).wrapping_add(u32::from(byte))
    })
}

struct Handler;
impl RequestHandler<StreamChunk4kV1, Reply, PoolFrame> for Handler {
    type Reply = (u32, u32, u32);

    async fn handle(
        &mut self,
        request: RxLease<StreamChunk4kV1, PoolFrame>,
    ) -> Result<Self::Reply, UCode> {
        let sequence = request.payload().unwrap().meta.header.sequence;
        let header = request.header().unwrap();
        let token = header.token().unwrap_or("");
        let reply = (
            sequence,
            u32::try_from(token.len()).unwrap(),
            checksum(token),
        );
        tokio::task::yield_now().await;
        if sequence == u32::MAX {
            Err(UCode::PermissionDenied)
        } else {
            Ok(reply)
        }
    }

    fn write_reply<'a>(
        &mut self,
        reply: Self::Reply,
        init: <Reply as StablePayloadInit>::Initializer<'a>,
    ) -> InitializedStablePayload<'a, Reply> {
        init.sequence(reply.0)
            .token_len(reply.1)
            .token_checksum(reply.2)
            .finish()
    }
}

type Client = PreparedRpcClient<PoolCore, StreamChunk4kV1, Reply>;

fn setup(slots: usize) -> (Arc<PoolCore>, NativeProfileAgreement, UUri) {
    let core = Arc::new(
        PoolCore::new(PoolCoreConfig {
            slots,
            slot_bytes: 8192,
            alignment: 8,
            max_routes: 32,
            max_registrations: 16,
        })
        .unwrap(),
    );
    let table = NativeProfileTable::new([
        (
            PayloadEncoding::private_use(0xF101),
            StreamChunk4kV1::native_representation(),
        ),
        (
            PayloadEncoding::private_use(0xF102),
            Reply::native_representation(),
        ),
    ])
    .unwrap();
    let profile = NativeProfile::new("rpc-allocation", 1, NativeProfileMode::Table(table)).unwrap();
    let agreement = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
    (
        core,
        agreement,
        UUri::try_from_wire_parts("service", 1, 1, 0x42).unwrap(),
    )
}

fn reply_to(id: u32) -> UUri {
    UUri::try_from_wire_parts("client", id, 1, 0).unwrap()
}

fn client(
    core: &Arc<PoolCore>,
    agreement: &NativeProfileAgreement,
    method: &UUri,
    id: u32,
    ttl: Duration,
) -> Client {
    ready(Client::new(
        Arc::clone(core),
        agreement.clone(),
        reply_to(id),
        method.clone(),
        RpcClientConfig {
            max_in_flight: 1,
            ttl,
            max_token_len: 2048,
            priority: FramePriority::CS4,
        },
    ))
    .unwrap()
}

fn server(
    runtime: &Runtime,
    core: &Arc<PoolCore>,
    agreement: &NativeProfileAgreement,
    method: &UUri,
    max_clients: usize,
) -> PreparedRpcServer<PoolCore, Reply> {
    runtime
        .block_on(PreparedRpcServer::start::<StreamChunk4kV1, _>(
            Arc::clone(core),
            agreement.clone(),
            method.clone(),
            Handler,
            RpcServerConfig {
                registration: RegistrationConfig {
                    capacity: 16,
                    overflow: OverflowPolicy::DropNewest,
                },
                header: HeaderLimits::default(),
                max_clients,
            },
        ))
        .unwrap()
}

async fn calls(client: &Client, count: u32, tokens: [&str; 2]) {
    for sequence in 0..count {
        let token = if sequence % 2 == 0 {
            tokens[0]
        } else {
            tokens[1]
        };
        client.set_token(token).unwrap();
        let reply = client
            .call(|init| init_streamer_4k(init.into_initializer(), sequence).unwrap())
            .await
            .unwrap();
        let value = reply.payload().unwrap();
        assert_eq!(value.sequence, sequence);
        assert_eq!(value.token_len, u32::try_from(token.len()).unwrap());
        assert_eq!(value.token_checksum, checksum(token));
        assert!(reply.header().unwrap().reqid.is_some());
    }
    assert_eq!(client.in_flight(), 0);
}

fn first_contact(runtime: &Runtime) {
    let (core, agreement, method) = setup(32);
    let server = server(runtime, &core, &agreement, &method, 8);
    let clients: Vec<_> = (1..=8)
        .map(|id| client(&core, &agreement, &method, id, Duration::from_secs(2)))
        .collect();
    let refused = client(&core, &agreement, &method, 9, Duration::from_millis(10));
    let large = [b'a'; 1536];
    let small = [b'b'; 900];
    let tokens = [
        std::str::from_utf8(&large).unwrap(),
        std::str::from_utf8(&small).unwrap(),
    ];
    measure("rpc_first_contact_8_new_clients", || {
        runtime.block_on(async {
            // No prepared reply-cache entries or earlier calls for any of these clients.
            for client in &clients {
                calls(client, 100, tokens).await;
            }
        })
    });
    assert_eq!(server.stats().responded.load(Ordering::Relaxed), 800);
    assert_eq!(server.stats().refused.load(Ordering::Relaxed), 0);
    measure("rpc_max_clients_refusal", || {
        runtime.block_on(async {
            assert!(matches!(
                refused
                    .call(|init| init_streamer_4k(init.into_initializer(), 0).unwrap())
                    .await,
                Err(CallError::TimedOut)
            ));
        })
    });
    assert_eq!(server.stats().refused.load(Ordering::Relaxed), 1);
    measure("rpc_token_multi_thread", || {
        runtime.block_on(calls(clients.first().unwrap(), 5000, tokens))
    });
    drop((refused, clients));
    runtime.block_on(server.stop()).unwrap();
    assert_eq!(core.free_slots(), 32);
}

fn rotating_current_thread() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        tokio::task::yield_now().await;
    });
    let (core, agreement, method) = setup(16);
    let server = server(&runtime, &core, &agreement, &method, 1);
    let client = client(&core, &agreement, &method, 1, Duration::from_secs(2));
    let large = [b'c'; 1536];
    let small = [b'd'; 900];
    let tokens = [
        std::str::from_utf8(&large).unwrap(),
        std::str::from_utf8(&small).unwrap(),
    ];
    runtime.block_on(async {
        // Tokio's current-thread scheduler creates a new deferred-waker Vec on
        // each block_on entry. Warm that context here and stay in the same entry
        // for measurement; no client's first request is used for warmup.
        tokio::task::yield_now().await;
        measure_async("rpc_token_1536_rotating", calls(&client, 5000, tokens)).await;
        measure_async("rpc_token_oversize_clear", async {
            client.set_token(tokens[0]).unwrap();
            let oversized = [b'e'; 2049];
            assert_eq!(
                client.set_token(std::str::from_utf8(&oversized).unwrap()),
                Err(TokenError::TooLong)
            );
            let reply = client
                .call(|init| init_streamer_4k(init.into_initializer(), 1).unwrap())
                .await
                .unwrap();
            assert_eq!(reply.payload().unwrap().token_checksum, checksum(tokens[0]));
            drop(reply);
            client.clear_token();
            let reply = client
                .call(|init| init_streamer_4k(init.into_initializer(), 2).unwrap())
                .await
                .unwrap();
            assert_eq!(reply.payload().unwrap().token_len, 0);
        })
        .await;
    });
    drop(client);
    runtime.block_on(server.stop()).unwrap();
    assert_eq!(core.free_slots(), 16);
}

fn status_only(runtime: &Runtime) {
    let (core, agreement, method) = setup(16);
    let server = server(runtime, &core, &agreement, &method, 1);
    let client = client(&core, &agreement, &method, 1, Duration::from_secs(2));
    measure("rpc_status_only_response", || {
        runtime.block_on(async {
            for _ in 0..2000 {
                assert!(matches!(
                    client
                        .call(|init| init_streamer_4k(init.into_initializer(), u32::MAX).unwrap())
                        .await,
                    Err(CallError::Status(UCode::PermissionDenied))
                ));
            }
        })
    });
    drop(client);
    runtime.block_on(server.stop()).unwrap();
    assert_eq!(core.free_slots(), 16);
}

fn pending_and_late(runtime: &Runtime) {
    let (core, agreement, method) = setup(16);
    let client = client(&core, &agreement, &method, 1, Duration::from_secs(30));
    let requests = Arc::new(
        RegistrationInbox::new(RegistrationConfig {
            capacity: 1,
            overflow: OverflowPolicy::DropNewest,
        })
        .unwrap(),
    );
    let mut subscription =
        ready(core.attach(&reply_to(1), Some(&method), requests.clone())).unwrap();
    let admission =
        RxAdmission::<StreamChunk4kV1>::new(agreement.clone(), HeaderLimits::default()).unwrap();
    let reply_definition = RouteDefinition {
        kind: FrameMessageKind::Response,
        source: method,
        sink: Some(reply_to(1)),
        priority: Some(FramePriority::CS4),
        ttl: None,
        permission_level: None,
    };
    let reply = TxTemplate::<Reply>::new(agreement.clone())
        .unwrap()
        .route(
            Arc::clone(&core),
            reply_definition.clone(),
            RouteConfig::default(),
        )
        .unwrap();
    let wrong_type = TxTemplate::<StreamChunk4kV1>::new(agreement)
        .unwrap()
        .route(Arc::clone(&core), reply_definition, RouteConfig::default())
        .unwrap();
    let _entered = runtime.enter();
    let id = measure("rpc_too_many_in_flight", || {
        let mut pending =
            std::pin::pin!(
                client.call(|init| init_streamer_4k(init.into_initializer(), 0).unwrap())
            );
        assert!(pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        assert_eq!(client.in_flight(), 1);
        for _ in 0..10_000 {
            assert!(matches!(
                ready(
                    client.call(|_| panic!("full pending table must reject before initialization"))
                ),
                Err(CallError::TooManyInFlight)
            ));
        }
        let request = admission.admit(ready(requests.recv()).unwrap()).unwrap();
        request.header().unwrap().id.clone()
        // The pending future and retained request drop inside the measured window.
    });
    assert_eq!(client.in_flight(), 0);
    measure("rpc_late_response", || {
        reply
            .send(
                MessageFields {
                    reqid: Some(&id),
                    ..MessageFields::default()
                },
                |init| {
                    init.into_initializer()
                        .sequence(0)
                        .token_len(0)
                        .token_checksum(0)
                        .finish()
                },
            )
            .unwrap();
    });
    assert_eq!(client.stats().unmatched.load(Ordering::Relaxed), 1);
    measure("rpc_response_wrong_type", || {
        let mut pending =
            std::pin::pin!(
                client.call(|init| init_streamer_4k(init.into_initializer(), 1).unwrap())
            );
        assert!(pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        let request = admission.admit(ready(requests.recv()).unwrap()).unwrap();
        let id = request.header().unwrap().id.clone();
        wrong_type
            .send(
                MessageFields {
                    reqid: Some(&id),
                    ..MessageFields::default()
                },
                |init| init_streamer_4k(init.into_initializer(), 1).unwrap(),
            )
            .unwrap();
        assert!(matches!(
            ready(pending),
            Err(CallError::Response(AdmissionError::WrongPayloadType))
        ));
    });
    assert_eq!(client.in_flight(), 0);
    core.detach(&mut subscription).unwrap();
    assert_eq!(core.free_slots(), 16);
}

fn timeout(runtime: &Runtime) {
    let (core, agreement, method) = setup(16);
    let client = client(&core, &agreement, &method, 1, Duration::from_millis(1));
    measure("rpc_timeout", || {
        runtime.block_on(async {
            for _ in 0..200 {
                assert!(matches!(
                    client
                        .call(|init| init_streamer_4k(init.into_initializer(), 0).unwrap())
                        .await,
                    Err(CallError::TimedOut)
                ));
                assert_eq!(client.in_flight(), 0);
            }
        })
    });
    assert_eq!(core.free_slots(), 16);
}

fn send_failure() {
    let (core, agreement, method) = setup(1);
    let client = client(&core, &agreement, &method, 1, Duration::from_secs(2));
    let held_route = TxTemplate::<StreamChunk4kV1>::new(agreement)
        .unwrap()
        .route(
            Arc::clone(&core),
            super::definition(),
            RouteConfig::default(),
        )
        .unwrap();
    let held = held_route
        .prepare(MessageFields::default(), |init| {
            init_streamer_4k(init.into_initializer(), 0).unwrap()
        })
        .unwrap();
    measure("rpc_send_pool_exhausted", || {
        assert!(matches!(
            ready(client.call(|_| panic!("exhausted pool must reject before initialization"))),
            Err(CallError::Send(SendError::Loan(LoanError::PoolExhausted)))
        ));
        assert_eq!(client.in_flight(), 0);
    });
    drop(held);
    assert_eq!(core.free_slots(), 1);
}

fn closed(runtime: &Runtime) {
    let (pool, agreement, method) = setup(16);
    let core = Arc::new(ClosingCore {
        pool: Arc::clone(&pool),
        sink: std::sync::OnceLock::new(),
    });
    let client = ready(PreparedRpcClient::<_, StreamChunk4kV1, Reply>::new(
        Arc::clone(&core),
        agreement,
        reply_to(1),
        method,
        RpcClientConfig {
            max_in_flight: 1,
            ttl: Duration::from_secs(30),
            max_token_len: 2048,
            priority: FramePriority::CS4,
        },
    ))
    .unwrap();
    struct WakeCount(std::sync::atomic::AtomicUsize);
    impl std::task::Wake for WakeCount {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    let wake_count = Arc::new(WakeCount(std::sync::atomic::AtomicUsize::new(0)));
    let waker = Waker::from(Arc::clone(&wake_count));
    let _entered = runtime.enter();
    measure("rpc_closed_pending_and_new_call", || {
        let mut pending =
            std::pin::pin!(
                client.call(|init| init_streamer_4k(init.into_initializer(), 0).unwrap())
            );
        assert!(pending
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending());
        core.sink.get().unwrap().close();
        assert_eq!(wake_count.0.load(Ordering::Relaxed), 1);
        assert!(matches!(ready(pending), Err(CallError::Closed)));
        assert_eq!(client.in_flight(), 0);
        assert!(matches!(
            ready(client.call(|_| panic!("closed client must reject before initialization"))),
            Err(CallError::Closed)
        ));
    });
    assert_eq!(pool.free_slots(), 16);
}

pub(super) fn run(runtime: &Runtime) {
    // All setup, allocations for endpoints and worker creation happen before the
    // windows. Every client's first call, observers, timers and teardown of its
    // per-call state are counted without subtraction on all process threads.
    first_contact(runtime);
    rotating_current_thread();
    status_only(runtime);
    pending_and_late(runtime);
    timeout(runtime);
    send_failure();
    closed(runtime);
}
