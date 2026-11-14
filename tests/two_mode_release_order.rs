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

//! The order in which a released RPC call slot is given back.
//!
//! Releasing a call slot disposes of its retained frame and its waker before the
//! slot can be claimed again: a new owner must not lose its waker to the old
//! release, or have its reservation lose a frame. Disposal runs code this test
//! controls, a frame's `Drop` and the wake hook's `Clear` event, so the test needs
//! no scheduler and no timing. From inside those it tries to claim the only slot
//! of the table, from this thread or from another, and the claim must be refused.
//! Once the release has finished, the claim must succeed.
//!
//! Every path a release takes is covered: a response not taken, a native response
//! taken and still retained, no response at all, and a call dropped while its
//! response is being delivered, which hands the release to the deliverer. The
//! thread variants pause the release inside the disposal while another thread
//! tries to claim, which is what `-Zmiri-many-seeds` varies.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use up_rust::startup::LEAKED_HEAP_REGIONS;
use up_rust::test_util::handoff::InMemoryBus;
use up_rust::test_util::lending::{
    LendFrame, LendTx, LendingBus, LendingParticipant, RunningLender,
};
use up_rust::test_util::wires::{DoorState, DoorWire};
use up_rust::two_mode::*;
use up_rust::{
    NativeProfile, NativeProfileAgreement, PayloadEncoding, StablePayloadExt, StaticUriProvider,
    UEntityId, UUri,
};
use up_rust_core::transport::wake::{RouteWake, WakeEvent};

/// Long enough to outlast every call, Miri's interpreter included.
const TTL: Duration = Duration::from_secs(3600);
const METHOD: u16 = 0x0001;

fn entity(authority: &str, type_id: u16) -> StaticUriProvider {
    StaticUriProvider::for_entity(authority, UEntityId::first_instance(type_id), 1).expect("uri")
}

fn memory() -> StartupMemory {
    StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 64 * 1024).expect("memory")
}

fn door(id: u32, open: u8) -> DoorState {
    DoorState { id, open }
}

fn client_uri() -> UUri {
    entity("door", 1).get_source_uri()
}

fn method_uri() -> UUri {
    entity("door", 2).get_resource_uri(METHOD)
}

// ------------------------------------------------------------- the probes

/// What happened inside a release, in the order it happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Disposal {
    /// A retained frame's `Drop` ran.
    FrameDropped,
    /// The wake hook's `Clear` event ran.
    WakerCleared,
    /// The deliverer read the frame again, in the DELIVERING phase.
    MetadataReadInDelivery,
}

/// What a disposal did about the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Observed {
    /// The disposal tried nothing.
    NothingTried,
    /// The claim was refused, with this error.
    Refused(CallError),
    /// The claim succeeded.
    Claimed,
}

type Event = (Disposal, Observed);

/// Runs at each disposal of the release under test. Hooks are plain `fn` pointers
/// and frames are moved into SDK storage, so they find it here.
type OnDisposal = Box<dyn FnMut(Disposal) -> Observed>;

thread_local! {
    static ON_DISPOSAL: RefCell<Option<OnDisposal>> = const { RefCell::new(None) };
}

/// Runs the current hook for one event. Taken out for the call, so a disposal the
/// hook itself causes cannot recurse into it.
fn notify(event: Disposal, log: &RefCell<Vec<Event>>) {
    let hook = ON_DISPOSAL.with(|slot| slot.borrow_mut().take());
    if let Some(mut hook) = hook {
        let observed = hook(event);
        log.borrow_mut().push((event, observed));
        ON_DISPOSAL.with(|slot| *slot.borrow_mut() = Some(hook));
    }
}

thread_local! {
    static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
}

fn notify_disposal(event: Disposal) {
    EVENTS.with(|log| notify(event, log));
}

/// Arms the probes for one release and starts a fresh record of it.
fn arm(hook: impl FnMut(Disposal) -> Observed + 'static) {
    EVENTS.with(|log| log.borrow_mut().clear());
    ON_DISPOSAL.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

/// Disarms the probes and returns what they saw.
fn disarm() -> Vec<Event> {
    ON_DISPOSAL.with(|slot| *slot.borrow_mut() = None);
    EVENTS.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

/// A claim for a slot that failed reports its error; a success keeps the claim.
fn observed_claim<T>(claim: Result<T, CallError>, kept: &RefCell<Vec<T>>) -> Observed {
    match claim {
        Ok(call) => {
            kept.borrow_mut().push(call);
            Observed::Claimed
        }
        Err(error) => Observed::Refused(error),
    }
}

/// The wake hook: only the `Clear` event is of interest. A plain `fn`, as
/// `up-rust-tokio`'s is.
fn wake_hook(_cell: &RouteWake, event: WakeEvent) {
    if event == WakeEvent::Clear {
        notify_disposal(Disposal::WakerCleared);
    }
}

fn install_wake_hook<Resp, M>(call: &Call<Resp, M>)
where
    Resp: Send + Sync + 'static,
    M: WireMode,
{
    call.__wake()
        .expect("the call's waker cell")
        .install_hook(wake_hook);
}

/// A lending frame that reports its own disposal, and its reads.
struct ProbeFrame {
    inner: LendFrame,
    /// The deliverer reads the metadata once while parsing and again in DELIVERING.
    metadata_reads: AtomicU32,
}

impl Frame for ProbeFrame {
    fn encoded_metadata(&self) -> &[u8] {
        // The first read parses the frame, before the slot is claimed for delivery.
        // The second happens once the frame is at its final address, in DELIVERING.
        if self.metadata_reads.fetch_add(1, Ordering::Relaxed) == 1 {
            notify_disposal(Disposal::MetadataReadInDelivery);
        }
        self.inner.encoded_metadata()
    }

    fn payload(&self) -> Option<LoanedPayload<'_>> {
        self.inner.payload()
    }

    fn peer(&self) -> PeerRef {
        self.inner.peer()
    }
}

impl Drop for ProbeFrame {
    fn drop(&mut self) {
        notify_disposal(Disposal::FrameDropped);
    }
}

/// The in-memory lending binding, with its received frames wrapped in [`ProbeFrame`].
struct ProbeSetup {
    inner: LendingParticipant,
}

struct ProbeRunning {
    inner: RunningLender,
}

impl BindingSetup for ProbeSetup {
    type Running = ProbeRunning;

    fn name(&self) -> &'static str {
        "release-probe"
    }

    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn qos_support(&self, kind: RouteKind) -> QosSupport {
        self.inner.qos_support(kind)
    }

    fn prepare_route(
        &mut self,
        route: &RouteDeclaration<'_>,
    ) -> Result<PreparedRoute, BindingRefusal> {
        self.inner.prepare_route(route)
    }

    fn into_running(self, port: RuntimePort) -> Result<ProbeRunning, BindingRefusal> {
        Ok(ProbeRunning {
            inner: self.inner.into_running(port)?,
        })
    }
}

impl LendingRuntime for ProbeRunning {
    fn receive_bounded(&self, route: BindingRoute, budget: &ReceiveBudget) -> Option<ProbeFrame> {
        self.inner.receive_bounded(route, budget).map(|inner| ProbeFrame {
            inner,
            metadata_reads: AtomicU32::new(0),
        })
    }

    type Tx<'a> = LendTx<'a>;
    type Rx = ProbeFrame;

    fn loan(&self, route: BindingRoute, layout: TxFrameLayout) -> Result<LendTx<'_>, LoanError> {
        self.inner.loan(route, layout)
    }

    fn publish<'a>(
        &'a self,
        route: BindingRoute,
        frame: InitializedTx<LendTx<'a>>,
    ) -> Result<SendReport, SendError> {
        self.inner.publish(route, frame)
    }

    fn receive(&self, route: BindingRoute) -> Option<ProbeFrame> {
        self.inner.receive(route).map(|inner| ProbeFrame {
            inner,
            metadata_reads: AtomicU32::new(0),
        })
    }
}

// --------------------------------------------------------------- the rigs

/// A one-slot client on the probing lending binding, and a plain server.
struct EncodedRig {
    client: RpcClient<DoorState, DoorState>,
    server: RpcServer<DoorState, DoorState>,
    client_runtime: TransportRuntime,
}

fn encoded_rig() -> EncodedRig {
    let bus = LendingBus::new();
    let a = TransportSetup::lending_in(
        &memory(),
        ProbeSetup {
            inner: bus.participant(1),
        },
    )
    .expect("setup");
    let b = TransportSetup::lending_in(&memory(), bus.participant(2)).expect("setup");
    let client = RpcClient::<DoorState, DoorState>::on(&a, &entity("door", 1), method_uri())
        .wire(DoorWire)
        .max_in_flight(1)
        .ttl(TTL)
        .build()
        .expect("client");
    let server = RpcServer::<DoorState, DoorState>::on(&b, &entity("door", 2), METHOD)
        .wire(DoorWire)
        .clients(std::slice::from_ref(&client_uri()))
        .build()
        .expect("server");
    let client_runtime = a.enter_runtime().expect("runtime");
    let server_runtime = b.enter_runtime().expect("runtime");
    EncodedRig {
        client: client.activate(&client_runtime).expect("activate"),
        server: server.activate(&server_runtime).expect("activate"),
        client_runtime,
    }
}

/// Sends a request and answers it, so a response waits for the client.
fn answered_call(rig: &EncodedRig) -> Call<DoorState> {
    let call = rig.client.invoke(&door(1, 0)).expect("the one slot");
    assert!(
        matches!(
            rig.client.invoke(&door(2, 0)),
            Err(CallError::SlotsExhausted)
        ),
        "the table has exactly one slot",
    );
    let request = rig.server.try_recv().expect("receive").expect("a request");
    let id = request.id;
    request.respond(&door(id + 100, 1)).expect("respond");
    call
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, up_rust::StablePayload, up_rust::StablePayloadInit,
)]
#[repr(C)]
#[stable_payload(type_name = "uprotocol.test.NativeReleaseDoor")]
struct NativeDoor {
    id: u32,
    open: u32,
}

fn native_agreement() -> &'static NativeAgreement {
    let profile = NativeProfile::new(
        "release-order",
        1,
        PayloadEncoding::private_use(0xF778),
        [NativeDoor::native_representation()],
    )
    .expect("profile");
    let agreement = NativeProfileAgreement::new(std::sync::Arc::new(profile.clone()), &profile)
        .expect("agreement");
    native_agreement_table(&agreement)
}

type NativeClient = RpcClient<NativeDoor, NativeDoor, InPlace>;

struct NativeRig {
    client: NativeClient,
    server: RpcServer<NativeDoor, NativeDoor, InPlace>,
}

fn native_rig() -> NativeRig {
    let agreement = native_agreement();
    let bus = LendingBus::new();
    let a = TransportSetup::lending_in(
        &memory(),
        ProbeSetup {
            inner: bus.participant(1),
        },
    )
    .expect("setup");
    let b = TransportSetup::lending_in(&memory(), bus.participant(2)).expect("setup");
    let client = RpcClient::<NativeDoor, NativeDoor>::on(&a, &entity("door", 1), method_uri())
        .wire(NativeWire::new(agreement))
        .max_in_flight(1)
        .ttl(TTL)
        .build()
        .expect("client");
    let server = RpcServer::<NativeDoor, NativeDoor>::on(&b, &entity("door", 2), METHOD)
        .wire(NativeWire::new(agreement))
        .clients(std::slice::from_ref(&client_uri()))
        .build()
        .expect("server");
    let tx = a.enter_runtime().expect("runtime");
    let rx = b.enter_runtime().expect("runtime");
    NativeRig {
        client: client.activate(&tx).expect("activate"),
        server: server.activate(&rx).expect("activate"),
    }
}

/// A call to a native server, answered and taken: the frame stays in the slot.
fn native_taken_call(rig: &NativeRig) -> Call<NativeDoor, InPlace> {
    let mut call = rig
        .client
        .invoke_in_place(|init| init.into_initializer().id(1).open(0).finish())
        .expect("the one slot");
    let request = rig.server.try_recv().expect("receive").expect("a request");
    request
        .respond_in_place(|init| init.into_initializer().id(2).open(1).finish())
        .expect("respond");
    {
        let reply = call.try_response().expect("poll").expect("a response");
        assert_eq!(
            *reply.payload().expect("payload"),
            NativeDoor { id: 2, open: 1 }
        );
    }
    call
}

/// A one-slot client on the copying in-memory handoff transport: no frame to retain.
fn handoff_client() -> RpcClient<DoorState, DoorState> {
    let bus = InMemoryBus::new(8, 4096);
    let a = TransportSetup::handoff_in(&memory(), bus.participant(1)).expect("setup");
    let client = RpcClient::<DoorState, DoorState>::on(&a, &entity("door", 1), method_uri())
        .wire(DoorWire)
        .max_in_flight(1)
        .ttl(TTL)
        .build()
        .expect("client");
    let runtime = a.enter_runtime().expect("runtime");
    client.activate(&runtime).expect("activate")
}

// ------------------------------------------------- claiming, here and there

/// A thread that claims the slot on request, for the releasing thread to wait on.
struct Claimant {
    go: Option<mpsc::Sender<()>>,
    outcome: mpsc::Receiver<Observed>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Claimant {
    fn start<C: Send + 'static>(claim: impl Fn() -> Result<C, CallError> + Send + 'static) -> Self {
        let (go, requests) = mpsc::channel::<()>();
        let (results, outcome) = mpsc::channel::<Observed>();
        let thread = thread::spawn(move || {
            // A claim that succeeds is held until the test is done.
            let kept = RefCell::new(Vec::new());
            while requests.recv().is_ok() {
                let observed = observed_claim(claim(), &kept);
                if results.send(observed).is_err() {
                    break;
                }
            }
        });
        Self {
            go: Some(go),
            outcome,
            thread: Some(thread),
        }
    }

    /// Asks the other thread to claim, and waits for its answer. No timing: the
    /// caller is paused here, inside the disposal, until the claim has been tried.
    fn claim(&self) -> Observed {
        self.go
            .as_ref()
            .expect("running")
            .send(())
            .expect("the claimant is running");
        self.outcome.recv().expect("the claimant's answer")
    }

    /// Stops the thread, releasing anything it claimed.
    fn finish(mut self) {
        drop(self.go.take());
        if let Some(thread) = self.thread.take() {
            thread.join().expect("the claimant thread");
        }
    }
}

const REFUSED: Observed = Observed::Refused(CallError::SlotsExhausted);

fn expect_events(seen: &[Event], expected: &[Event]) {
    assert_eq!(seen, expected, "what happened during the release");
}

// -------------------------------------------------------------- the tests

/// An encoded response that was never taken: the release drops the retained
/// frame and clears the waker, and only then frees the slot.
#[test]
fn an_untaken_response_keeps_the_slot_until_its_frame_and_waker_are_gone() {
    let rig = encoded_rig();
    let call = answered_call(&rig);
    install_wake_hook(&call);
    assert_eq!(
        rig.client_runtime.pump(),
        1,
        "the response is delivered and held"
    );

    let kept = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&kept);
    let client = rig.client;
    arm(move |_| observed_claim(client.invoke(&door(3, 0)), &sink));
    drop(call);
    let seen = disarm();

    expect_events(
        &seen,
        &[
            (Disposal::FrameDropped, REFUSED),
            (Disposal::WakerCleared, REFUSED),
        ],
    );
    assert!(
        kept.borrow().is_empty(),
        "no claim succeeded during the release"
    );
    let next = rig.client.invoke(&door(4, 0));
    assert!(
        next.is_ok(),
        "the slot is free once the release has finished"
    );
}

/// The same release, with the claim tried by another thread while this one is
/// paused inside each disposal.
#[test]
fn an_untaken_response_refuses_a_claim_from_another_thread() {
    let rig = encoded_rig();
    let call = answered_call(&rig);
    install_wake_hook(&call);
    assert_eq!(
        rig.client_runtime.pump(),
        1,
        "the response is delivered and held"
    );

    let client = rig.client;
    let claimant = Rc::new(Claimant::start(move || client.invoke(&door(3, 0))));
    let on_disposal = Rc::clone(&claimant);
    arm(move |_| on_disposal.claim());
    drop(call);
    let seen = disarm();

    expect_events(
        &seen,
        &[
            (Disposal::FrameDropped, REFUSED),
            (Disposal::WakerCleared, REFUSED),
        ],
    );
    assert_eq!(
        claimant.claim(),
        Observed::Claimed,
        "the other thread can claim the slot once the release has finished",
    );
    Rc::try_unwrap(claimant).ok().expect("one owner").finish();
}

/// A native response stays in transport memory until its call is dropped, so the
/// frame is disposed of by the release even though the response was taken.
#[test]
fn a_taken_native_response_keeps_the_slot_until_its_frame_and_waker_are_gone() {
    let rig = native_rig();
    let call = native_taken_call(&rig);
    install_wake_hook(&call);

    let kept = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&kept);
    let client = rig.client;
    let claim =
        move || client.invoke_in_place(|init| init.into_initializer().id(9).open(0).finish());
    arm(move |_| observed_claim(claim(), &sink));
    drop(call);
    let seen = disarm();

    expect_events(
        &seen,
        &[
            (Disposal::FrameDropped, REFUSED),
            (Disposal::WakerCleared, REFUSED),
        ],
    );
    assert!(
        kept.borrow().is_empty(),
        "no claim succeeded during the release"
    );
    let next = rig
        .client
        .invoke_in_place(|init| init.into_initializer().id(10).open(0).finish());
    assert!(
        next.is_ok(),
        "the slot is free once the release has finished"
    );
}

/// With no response the release has only the waker to clear, and still holds the
/// slot while it does.
#[test]
fn a_call_with_no_response_keeps_the_slot_until_its_waker_is_cleared() {
    let client = handoff_client();
    let call = client.invoke(&door(1, 0)).expect("the one slot");
    assert!(matches!(
        client.invoke(&door(2, 0)),
        Err(CallError::SlotsExhausted)
    ));
    install_wake_hook(&call);

    let kept = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&kept);
    arm(move |_| observed_claim(client.invoke(&door(3, 0)), &sink));
    drop(call);
    let seen = disarm();

    expect_events(&seen, &[(Disposal::WakerCleared, REFUSED)]);
    assert!(
        kept.borrow().is_empty(),
        "no claim succeeded during the release"
    );
    assert!(
        client.invoke(&door(4, 0)).is_ok(),
        "the slot is free afterwards"
    );
}

/// The waker path from another thread.
#[test]
fn a_call_with_no_response_refuses_a_claim_from_another_thread() {
    let client = handoff_client();
    let call = client.invoke(&door(1, 0)).expect("the one slot");
    install_wake_hook(&call);

    let claimant = Rc::new(Claimant::start(move || client.invoke(&door(3, 0))));
    let on_disposal = Rc::clone(&claimant);
    arm(move |_| on_disposal.claim());
    drop(call);
    let seen = disarm();

    expect_events(&seen, &[(Disposal::WakerCleared, REFUSED)]);
    assert_eq!(claimant.claim(), Observed::Claimed);
    Rc::try_unwrap(claimant).ok().expect("one owner").finish();
}

/// A call dropped while its response is being delivered: the release returns at
/// once, and the deliverer disposes of the frame and the waker, freeing the slot
/// only after both. `MetadataReadInDelivery` is what drops the call.
#[test]
fn a_call_dropped_during_delivery_leaves_the_slot_held_until_the_deliverer_is_done() {
    let rig = encoded_rig();
    let call = answered_call(&rig);
    install_wake_hook(&call);

    let kept = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&kept);
    let client = rig.client;
    let mut pending_call = Some(call);
    arm(move |event| {
        if event == Disposal::MetadataReadInDelivery {
            // The deliverer holds DELIVERING now, so this release only marks the
            // slot abandoned and returns.
            drop(pending_call.take().expect("the call"));
            return Observed::NothingTried;
        }
        observed_claim(client.invoke(&door(3, 0)), &sink)
    });
    assert_eq!(rig.client_runtime.pump(), 1, "the response was delivered");
    let seen = disarm();

    expect_events(
        &seen,
        &[
            (Disposal::MetadataReadInDelivery, Observed::NothingTried),
            (Disposal::FrameDropped, REFUSED),
            (Disposal::WakerCleared, REFUSED),
        ],
    );
    assert!(
        kept.borrow().is_empty(),
        "no claim succeeded during the release"
    );
    let counters = rig.client.response_counters();
    assert_eq!(
        counters.carried, 1,
        "the delivery completed against the abandoned call"
    );
    assert_eq!(
        counters.refused, 0,
        "so the call really was dropped during delivery"
    );
    let next = rig.client.invoke(&door(4, 0));
    assert!(
        next.is_ok(),
        "the slot is free once the deliverer has finished"
    );
}

/// The same hand-off, with the claim tried by another thread while the deliverer
/// is paused inside each disposal.
#[test]
fn a_call_dropped_during_delivery_refuses_a_claim_from_another_thread() {
    let rig = encoded_rig();
    let call = answered_call(&rig);
    install_wake_hook(&call);

    let client = rig.client;
    let claimant = Rc::new(Claimant::start(move || client.invoke(&door(3, 0))));
    let on_disposal = Rc::clone(&claimant);
    let mut pending_call = Some(call);
    arm(move |event| {
        if event == Disposal::MetadataReadInDelivery {
            drop(pending_call.take().expect("the call"));
            return Observed::NothingTried;
        }
        on_disposal.claim()
    });
    assert_eq!(rig.client_runtime.pump(), 1, "the response was delivered");
    let seen = disarm();

    expect_events(
        &seen,
        &[
            (Disposal::MetadataReadInDelivery, Observed::NothingTried),
            (Disposal::FrameDropped, REFUSED),
            (Disposal::WakerCleared, REFUSED),
        ],
    );
    assert_eq!(
        claimant.claim(),
        Observed::Claimed,
        "the other thread can claim the slot once the deliverer has finished",
    );
    Rc::try_unwrap(claimant).ok().expect("one owner").finish();
}
