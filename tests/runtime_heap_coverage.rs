// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Heap coverage the zero-allocation gates did not have: first use on a fresh
//! thread, the owned-frame family's send, refusal and receive paths, and a real
//! metadata codec in place of the dispatch stub.
//!
//! The rule under test: runtime paths neither acquire nor release heap memory.
//! Windows here are thread-scoped, so they run alongside each other in libtest.
//! A failing test is a finding against the rule, not a test to weaken.

use std::{
    alloc::System,
    future::Future,
    hint::black_box,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
    thread,
};
use up_rust::prelude::*;

use up_rust::bench_fixtures::payload_contract::{init_streamer_4k, StreamChunk4kV1};
use up_rust::heap_guard::{assert_no_heap, HeapGuard, Scope};
use up_rust::serialized_dispatch::{InlineOwnedCore, InlineOwnedTransport, OwnedMailbox};
use up_rust::wire::{ProtobufWire, UWireMetadataCodec, UWireMetadataContext};
use up_rust::{
    HeaderLimits, InMemoryOwnedTransport, MessageFields, NativeProfile, NativeProfileAgreement,
    OverflowPolicy, PayloadEncoding, PoolCore, PoolCoreConfig, PoolFrame, PreparedTransportCore,
    RegistrationConfig, RegistrationInbox, RouteConfig, RouteDefinition, RxAdmission,
    StableContainerWire, TxRoute, UFrameFieldsMetadataCodec, UFrameMetadata, UMessageType,
    UOwnedFrame, UOwnedListener, UOwnedTransport, UStatus, UUri, UWireTransport, UUID,
};

#[global_allocator]
static HEAP: HeapGuard = HeapGuard::new(System);

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture expected immediate completion"),
    }
}

/// Runs `operation` as the first heap-relevant work on a brand-new thread and
/// asserts it uses no heap. Anything lazy that the path touches shows up here.
fn on_fresh_thread<T, F>(subject: &'static str, operation: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let joined = thread::Builder::new()
        .name("fresh-thread-probe".to_owned())
        .spawn(move || assert_no_heap(subject, Scope::Enrolled, operation))
        .expect("spawn fresh thread")
        .join();
    match joined {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

fn agreement() -> NativeProfileAgreement {
    let profile = NativeProfile::new(
        "runtime-heap-coverage",
        1,
        PayloadEncoding::private_use(0xF101),
        [StreamChunk4kV1::native_representation()],
    )
    .unwrap();
    NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap()
}

fn definition() -> RouteDefinition {
    RouteDefinition {
        kind: UMessageType::Publish,
        source: UUri::try_from_wire_parts("coverage", 0x0001_0001, 1, 0x8001).unwrap(),
        sink: None,
        priority: None,
        ttl: None,
        permission_level: None,
    }
}

/// Fully set-up prepared route and receiver; nothing here is warmed.
struct PreparedFixture {
    core: Arc<PoolCore>,
    route: TxRoute<PoolCore, StreamChunk4kV1>,
    admission: RxAdmission<StreamChunk4kV1>,
}

fn prepared_fixture() -> PreparedFixture {
    let core = Arc::new(
        PoolCore::new(PoolCoreConfig {
            slots: 8,
            slot_bytes: 8192,
            alignment: 8,
            max_routes: 4,
            max_registrations: 4,
        })
        .unwrap(),
    );
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
    PreparedFixture {
        core,
        route,
        admission,
    }
}

// ---- first use on a fresh thread ----

#[test]
fn fresh_thread_first_uuid_is_heap_free() {
    on_fresh_thread("first UUID::build on a fresh thread", || {
        black_box(UUID::build());
    });
}

#[test]
fn fresh_thread_first_prepared_send_is_heap_free() {
    let fixture = prepared_fixture();
    on_fresh_thread("first prepared send on a fresh thread", move || {
        fixture
            .route
            .send(MessageFields::default(), |init| {
                init_streamer_4k(init.into_initializer(), 0).unwrap()
            })
            .unwrap();
        fixture
    });
}

#[test]
fn fresh_thread_first_admission_and_borrow_is_heap_free() {
    let fixture = prepared_fixture();
    let inbox = Arc::new(
        RegistrationInbox::new(RegistrationConfig {
            capacity: 1,
            overflow: OverflowPolicy::DropNewest,
        })
        .unwrap(),
    );
    let mut subscription = ready(
        fixture
            .core
            .attach(&definition().source, None, inbox.clone()),
    )
    .unwrap();
    fixture
        .route
        .send(MessageFields::default(), |init| {
            init_streamer_4k(init.into_initializer(), 0).unwrap()
        })
        .unwrap();
    let received: PoolFrame = ready(inbox.recv()).unwrap();
    fixture.core.detach(&mut subscription).unwrap();
    on_fresh_thread("first admission on a fresh thread", move || {
        let lease = fixture.admission.admit(received).unwrap();
        black_box(lease.payload().unwrap());
        (fixture, lease)
    });
}

// ---- owned-frame family ----

fn owned_frame() -> UOwnedFrame {
    UOwnedFrame::new(
        UFrameMetadata::publish(UUri::try_from_parts("coverage", 0x10001, 1, 0x8001).unwrap())
            .build()
            .unwrap(),
        None,
    )
    .unwrap()
}

#[test]
fn owned_send_into_a_bounded_mailbox_is_heap_free() {
    let mailbox = Arc::new(InlineOwnedTransport::new(OwnedMailbox::new(1).unwrap()));
    let transport: Arc<dyn UOwnedTransport> = mailbox.clone();
    let frame = owned_frame();
    assert_no_heap("owned send into a mailbox", Scope::Enrolled, || {
        ready(transport.send_owned(frame)).unwrap();
    });
    let taken = assert_no_heap("owned take from a mailbox", Scope::Enrolled, || {
        mailbox.inner().try_take().unwrap().unwrap()
    });
    drop(taken);
}

#[test]
fn owned_refusal_of_a_full_mailbox_is_heap_free() {
    let mailbox = Arc::new(InlineOwnedTransport::new(OwnedMailbox::new(1).unwrap()));
    let transport: Arc<dyn UOwnedTransport> = mailbox.clone();
    ready(transport.send_owned(owned_frame())).unwrap();
    let frame = owned_frame();
    let status = assert_no_heap("owned refusal of a full mailbox", Scope::Enrolled, || {
        ready(transport.send_owned(frame)).unwrap_err()
    });
    assert_eq!(status.code(), up_rust::UCode::ResourceExhausted);
}

/// The real field-block metadata codec on the owned send path, through the same
/// `UWireTransport` that the serialized-dispatch controls drive with a stub.
#[test]
fn owned_send_through_the_real_metadata_codec_is_heap_free() {
    #[derive(Default)]
    struct DiscardingCore(AtomicUsize);
    impl up_rust::serialized_dispatch::StaticOwnedCore for DiscardingCore {
        async fn send_prepared_owned(
            &self,
            frame: up_rust::wire_transport::PreparedOwnedFrame,
        ) -> Result<(), UStatus> {
            self.0.fetch_add(1, Ordering::Relaxed);
            drop(frame);
            Ok(())
        }
    }
    let transport: Arc<dyn UOwnedTransport> = Arc::new(UWireTransport::new(
        InlineOwnedCore::new(DiscardingCore::default()),
        ProtobufWire,
        UFrameFieldsMetadataCodec,
    ));
    let frame = owned_frame();
    assert_no_heap(
        "owned send through the real metadata codec",
        Scope::Enrolled,
        || ready(transport.send_owned(frame)).unwrap(),
    );
}

/// Decode side of the real codec, as used when an owned frame is received.
#[test]
fn owned_receive_metadata_decode_is_heap_free() {
    let metadata =
        UFrameMetadata::publish(UUri::try_from_parts("coverage", 0x10001, 1, 0x8001).unwrap())
            .build()
            .unwrap();
    let context = UWireMetadataContext::from_wire::<ProtobufWire, UFrameFieldsMetadataCodec>();
    let encoded = UFrameFieldsMetadataCodec
        .encode_frame_metadata(context, &metadata)
        .unwrap();
    let decoded = assert_no_heap("owned metadata decode", Scope::Enrolled, || {
        UFrameFieldsMetadataCodec
            .decode_frame_metadata(context, &encoded)
            .unwrap()
    });
    drop(decoded);
}

/// A listener on the in-memory owned transport: per-send matching, the frame
/// clone and the boxed listener future are all in the window.
#[test]
fn owned_delivery_to_an_in_memory_listener_is_heap_free() {
    #[derive(Default)]
    struct Noop(AtomicUsize);
    #[async_trait::async_trait]
    impl UOwnedListener for Noop {
        async fn on_receive_owned(&self, frame: UOwnedFrame) {
            self.0.fetch_add(1, Ordering::Relaxed);
            drop(frame);
        }
    }
    let transport = Arc::new(InMemoryOwnedTransport::default());
    let topic = UUri::try_from_parts("coverage", 0x10001, 1, 0x8001).unwrap();
    let listener = Arc::new(Noop::default());
    ready(UOwnedTransport::register_owned_listener(
        &*transport,
        &topic,
        None,
        Arc::clone(&listener) as Arc<dyn UOwnedListener>,
    ))
    .unwrap();
    let frame = owned_frame();
    assert_no_heap(
        "owned delivery to an in-memory listener",
        Scope::Enrolled,
        || {
            ready(UOwnedTransport::send_owned(&*transport, frame)).unwrap();
        },
    );
    assert_eq!(listener.0.load(Ordering::Relaxed), 1);
}
