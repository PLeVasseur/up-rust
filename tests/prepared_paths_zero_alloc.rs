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
//! Harness-free, all-thread allocation checks for prepared native operations.

#[path = "support/prepared_opaque_alloc.rs"]
mod opaque_alloc;
#[path = "support/prepared_rejection_alloc.rs"]
mod rejection_alloc;
#[path = "support/prepared_role_alloc.rs"]
mod role_alloc;
#[path = "support/prepared_rpc_alloc.rs"]
mod rpc_alloc;
#[path = "support/prepared_send_rejection_alloc.rs"]
mod send_rejection_alloc;

use std::{
    alloc::{GlobalAlloc, Layout, System},
    future::Future,
    mem::MaybeUninit,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};
use up_rust::bench_fixtures::payload_contract::{init_streamer_4k, StreamChunk4kV1};
use up_rust::{
    Frame, HeaderLimits, InboxPushError, MessageFields, NativeProfile, NativeProfileAgreement,
    NativeProfileMode, NativeProfileTable, OverflowPolicy, PayloadEncoding, PoolCore,
    PoolCoreConfig, PoolFrame, PreparedTransportCore, RegistrationConfig, RegistrationInbox,
    RouteConfig, RouteDefinition, RxLease, StableContainerWire, StablePayload, StablePayloadInit,
    TokenBuffer, TokenError, UFrameFieldsMetadataCodec, UFrameMetadata, UMessageType, UUri,
    UWireTransport, ZeroCopyHandler, UUID,
};

static COUNTING: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static WATCHED: AtomicUsize = AtomicUsize::new(0);
static WATCHED_LEN: AtomicUsize = AtomicUsize::new(0);
static DROP_WIPED: AtomicBool = AtomicBool::new(false);
struct Counter;

fn charge(bytes: usize) {
    if COUNTING.load(Ordering::SeqCst) {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(bytes, Ordering::Relaxed);
    }
}

// SAFETY: allocation operations are forwarded unchanged to System. Accounting
// uses only static atomics. The optional wipe observation reads a known live
// token allocation immediately before forwarding its deallocation.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        charge(layout.size());
        // SAFETY: forward the caller's valid layout unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        charge(layout.size());
        // SAFETY: forward the caller's valid layout unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        charge(size);
        // SAFETY: pointer/layout/size are the caller's original allocation contract.
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if WATCHED
            .compare_exchange(pointer as usize, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let len = WATCHED_LEN.load(Ordering::SeqCst);
            if len <= layout.size() {
                // SAFETY: the watched pointer is the initialized TokenBuffer
                // allocation, still live here; the length is within this layout.
                let bytes = unsafe { std::slice::from_raw_parts(pointer, len) };
                DROP_WIPED.store(bytes.iter().all(|byte| *byte == 0), Ordering::SeqCst);
            }
        }
        // SAFETY: forward the original allocation and layout exactly once.
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counter = Counter;

struct Window;
impl Window {
    fn start() -> Self {
        CALLS.store(0, Ordering::SeqCst);
        BYTES.store(0, Ordering::SeqCst);
        assert!(!COUNTING.swap(true, Ordering::SeqCst));
        Self
    }

    fn finish(self, name: &str) {
        drop(self);
        let calls = CALLS.load(Ordering::SeqCst);
        let bytes = BYTES.load(Ordering::SeqCst);
        println!("subject={name}\tallocations={calls}\tallocated_bytes={bytes}\tall_threads=true");
        assert_eq!(
            (calls, bytes),
            (0, 0),
            "allocation contract failed for {name}"
        );
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        COUNTING.store(false, Ordering::SeqCst);
    }
}

fn measure<T>(name: &str, operation: impl FnOnce() -> T) -> T {
    let window = Window::start();
    let value = operation();
    window.finish(name);
    value
}

async fn measure_async<T>(name: &str, operation: impl Future<Output = T>) -> T {
    let window = Window::start();
    let value = operation.await;
    window.finish(name);
    value
}

fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("operation must complete synchronously"),
    }
}

fn agreement() -> NativeProfileAgreement {
    let table = NativeProfileTable::new([(
        PayloadEncoding::private_use(0xF101),
        StreamChunk4kV1::native_representation(),
    )])
    .unwrap();
    let profile =
        NativeProfile::new("prepared-allocation", 1, NativeProfileMode::Table(table)).unwrap();
    NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap()
}

fn definition() -> RouteDefinition {
    RouteDefinition {
        kind: UMessageType::Publish,
        source: UUri::try_from_wire_parts("allocation", 1, 1, 0x8001).unwrap(),
        sink: None,
        priority: None,
        ttl: None,
        permission_level: None,
    }
}

struct Observe(Arc<AtomicUsize>);
impl ZeroCopyHandler<StreamChunk4kV1, PoolFrame> for Observe {
    async fn on_receive(&mut self, lease: RxLease<StreamChunk4kV1, PoolFrame>) {
        std::hint::black_box(lease.payload().unwrap());
        drop(lease);
        self.0.fetch_add(1, Ordering::Release);
    }
}

fn main() {
    // Every runtime worker initializes its thread-local UUID state before the
    // caller opens any measurement window; no new workers are requested later.
    let started = Arc::new(AtomicUsize::new(0));
    let ready_threads = Arc::clone(&started);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .on_thread_start(move || {
            std::hint::black_box(UUID::build());
            ready_threads.fetch_add(1, Ordering::Release);
        })
        .enable_all()
        .build()
        .unwrap();
    while started.load(Ordering::Acquire) != 2 {
        std::thread::yield_now();
    }
    std::hint::black_box(UUID::build());

    runtime.block_on(async {
        tokio::task::yield_now().await;
    });
    // A barrier ensures each worker polls one warmup task before either can
    // finish. This initializes Tokio's per-worker deferred-waker storage as well
    // as the UUID state initialized by on_thread_start. No RPC client is called.
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let warmers: Vec<_> = (0..2)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            runtime.spawn(async move {
                barrier.wait();
                tokio::task::yield_now().await;
            })
        })
        .collect();
    for warmer in warmers {
        runtime.block_on(warmer).unwrap();
    }
    #[repr(C)]
    #[derive(up_rust::StablePayload, up_rust::StablePayloadInit)]
    #[stable_payload(type_name = "uprotocol.test.RepeatingAllocationPattern")]
    struct Pattern {
        bytes: [u8; 4097],
    }
    up_rust::StableContainerPayload::<Pattern>::representation_proven().unwrap();
    let mut pattern_storage = MaybeUninit::<Pattern>::uninit();
    // SAFETY: MaybeUninit<Pattern> owns this exact writable byte extent with the
    // required Pattern alignment, and the exclusive borrow does not escape it.
    let pattern_bytes = unsafe {
        std::slice::from_raw_parts_mut(
            std::ptr::from_mut(&mut pattern_storage).cast::<MaybeUninit<u8>>(),
            std::mem::size_of::<Pattern>(),
        )
    };
    measure("repeating_array_initialization_and_rejection", || {
        let value = Pattern::init(pattern_bytes)
            .unwrap()
            .bytes_fill_pattern(&[3, 5, 8])
            .unwrap()
            .finish();
        for (index, byte) in value.bytes.iter().enumerate() {
            assert_eq!(*byte, *[3, 5, 8].get(index % 3).unwrap());
        }
        assert!(matches!(
            Pattern::init(pattern_bytes)
                .unwrap()
                .bytes_fill_pattern(&[]),
            Err(up_rust::StablePayloadPatternError::EmptyPattern)
        ));
    });
    let agreement = agreement();
    let core = Arc::new(
        PoolCore::new(PoolCoreConfig {
            slots: 16,
            slot_bytes: 8192,
            alignment: 8,
            max_routes: 16,
            max_registrations: 8,
        })
        .unwrap(),
    );
    let adapter = UWireTransport::with_native_agreement(
        Arc::clone(&core),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        agreement,
    );
    let route = adapter
        .prepare_stable_route::<StreamChunk4kV1>(definition(), RouteConfig::default())
        .unwrap();
    let admission = adapter
        .prepare_stable_receiver::<StreamChunk4kV1>(HeaderLimits::default())
        .unwrap();
    for _ in 0..8 {
        route
            .send(MessageFields::default(), |init| {
                init_streamer_4k(init.into_initializer(), 1).unwrap()
            })
            .unwrap();
    }

    measure("prepared_route_send", || {
        for sequence in 0..128 {
            route
                .send(MessageFields::default(), |init| {
                    init_streamer_4k(init.into_initializer(), sequence).unwrap()
                })
                .unwrap();
        }
    });
    measure("direct_metadata_builder", || {
        for _ in 0..128 {
            std::hint::black_box(
                UFrameMetadata::publish(UUri::try_from_parts("allocation", 1, 1, 0x8001).unwrap())
                    .with_native_payload_identity(admission.identity())
                    .build()
                    .unwrap(),
            );
        }
    });
    let template = adapter
        .prepare_stable_template::<StreamChunk4kV1>()
        .unwrap();
    measure("reserved_route_creation", || {
        for _ in 0..128 {
            drop(
                template
                    .route(Arc::clone(&core), definition(), RouteConfig::default())
                    .unwrap(),
            );
        }
    });

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
            init_streamer_4k(init.into_initializer(), 7).unwrap()
        })
        .unwrap();
    let received = ready(inbox.recv()).unwrap();
    core.detach(&mut subscription).unwrap();
    drop(admission.admit(received.try_retain().unwrap()).unwrap());
    measure("prepared_receive_admit_and_borrow", || {
        for _ in 0..128 {
            let lease = admission.admit(received.try_retain().unwrap()).unwrap();
            std::hint::black_box(lease.payload().unwrap());
            drop(lease);
        }
    });
    measure("received_header_parse", || {
        for _ in 0..128 {
            std::hint::black_box(
                up_rust::parse_header(
                    admission.prefix(),
                    received.encoded_metadata(),
                    &HeaderLimits::default(),
                )
                .unwrap(),
            );
        }
    });
    let mut token = TokenBuffer::new(2048).unwrap();
    token.set("warm").unwrap();
    token.clear();
    measure("token_set_clear_oversize", || {
        for _ in 0..128 {
            token.set("credential").unwrap();
            token.clear();
        }
        let oversized = [b'x'; 2049];
        assert_eq!(
            token.set(std::str::from_utf8(&oversized).unwrap()),
            Err(TokenError::TooLong)
        );
    });
    token.set("credential").unwrap();
    WATCHED_LEN.store(token.capacity(), Ordering::SeqCst);
    WATCHED.store(token.as_str().unwrap().as_ptr() as usize, Ordering::SeqCst);
    measure("token_drop_and_wipe", || drop(token));
    assert!(DROP_WIPED.load(Ordering::SeqCst));

    let closed = RegistrationInbox::new(RegistrationConfig {
        capacity: 1,
        overflow: OverflowPolicy::DropNewest,
    })
    .unwrap();
    closed.close();
    measure("closed_inbox_rejection", || {
        for _ in 0..128 {
            assert_eq!(closed.push(1_u32), Err(InboxPushError::Closed));
        }
    });
    let full = RegistrationInbox::new(RegistrationConfig {
        capacity: 1,
        overflow: OverflowPolicy::DropNewest,
    })
    .unwrap();
    full.push(1_u32).unwrap();
    measure("full_inbox_rejection", || {
        for _ in 0..128 {
            assert_eq!(full.push(2), Err(InboxPushError::Full));
        }
    });

    let seen = Arc::new(AtomicUsize::new(0));
    let registration = {
        let _entered = runtime.enter();
        admission
            .register(
                RegistrationConfig {
                    capacity: 8,
                    overflow: OverflowPolicy::Block {
                        deadline: std::time::Duration::from_secs(1),
                    },
                },
                Observe(Arc::clone(&seen)),
                || {},
            )
            .unwrap()
    };
    let mut subscription =
        ready(core.attach(&definition().source, None, registration.inbox())).unwrap();
    route
        .send(MessageFields::default(), |init| {
            init_streamer_4k(init.into_initializer(), 0).unwrap()
        })
        .unwrap();
    while seen.load(Ordering::Acquire) != 1 {
        std::thread::yield_now();
    }
    measure("generic_worker_send_admit_borrow_drain_stop", || {
        for sequence in 0..128 {
            route
                .send(MessageFields::default(), |init| {
                    init_streamer_4k(init.into_initializer(), sequence).unwrap()
                })
                .unwrap();
        }
        runtime.block_on(registration.stop()).unwrap();
    });
    core.detach(&mut subscription).unwrap();
    assert_eq!(seen.load(Ordering::Acquire), 129);
    rpc_alloc::run(&runtime);
    role_alloc::run(&runtime);
    rejection_alloc::run(&runtime);
    send_rejection_alloc::run();
    opaque_alloc::run(&runtime);
    let outer = up_rust::ListenerAdmission::new();
    let inner = up_rust::ListenerAdmission::new();
    measure("callback_admission_nested_pending_context", || {
        for _ in 0..128 {
            let release = AtomicBool::new(false);
            let mut dispatched =
                std::pin::pin!(
                    outer.dispatch(|| inner.dispatch(|| std::future::poll_fn(|_| {
                        assert!(outer.is_current_callback() && inner.is_current_callback());
                        if !release.load(Ordering::Relaxed) {
                            Poll::Pending
                        } else {
                            Poll::Ready(())
                        }
                    })))
                );
            assert!(dispatched
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending());
            assert!(!outer.is_current_callback() && !inner.is_current_callback());
            release.store(true, Ordering::Relaxed);
            assert!(dispatched
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_ready());
            assert_eq!(outer.entered_count(), 0);
            assert_eq!(inner.entered_count(), 0);
        }
    });
    println!("RESULT: all implemented prepared-path allocation subjects are zero");
}
