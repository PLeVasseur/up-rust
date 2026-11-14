//! Unrun successor-3 controls: observation is not authenticated destination.
//! These use the existing allocating test doubles, not a native-resource proof.
#![cfg(all(feature = "test-util", feature = "typed-communication"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic,
    clippy::indexing_slicing, clippy::too_many_lines, missing_docs)]

#[path = "support/review_doubles.rs"]
mod doubles;
use doubles::*;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use core::time::Duration;
use up_rust_core::communication::two_mode::RpcClient;
use up_rust_core::transport::binding::{BindingRefusal, BindingRoute, BindingSetup,
    Capabilities, Flow, HandoffRuntime, InboundFrame, OutgoingFrame, PayloadBytes,
    PeerRef, PreparedRoute, ReplyToken, RouteDeclaration, RouteKind, RuntimePort};
use up_rust_core::transport::qos::QosSupport;
use up_rust_core::transport::{CallError, HandoffFailure, PeerLossOutcome,
    ReceiveBudget, ReceiveError, SendReport, TransportSetup};
use up_rust_core::uattributes::UMessageType;

const WALL: u64 = 2_000_000_000_000;
const OLD: PeerRef = PeerRef { peer: 100, incarnation: 7 };
const NEW: PeerRef = PeerRef { peer: 100, incarnation: 8 };

/// Mutates peer observations inside send, after the SDK reserved the call.
/// The fixture then accepts that send on its new simulated destination.
struct SwitchSetup {
    shared: &'static Shared,
    switch: &'static AtomicBool,
    failed_by_old_loss: &'static AtomicUsize,
}
struct SwitchRunning {
    inner: HandoffRunning,
    shared: &'static Shared,
    switch: &'static AtomicBool,
    failed_by_old_loss: &'static AtomicUsize,
}
impl BindingSetup for SwitchSetup {
    type Running = SwitchRunning;
    fn name(&self) -> &'static str { "g2-send-boundary-double" }
    fn capabilities(&self) -> Capabilities { capabilities() }
    fn qos_support(&self, _: RouteKind) -> QosSupport { all_qos() }
    fn prepare_route(&mut self, route: &RouteDeclaration<'_>)
        -> Result<PreparedRoute, BindingRefusal>
    {
        HandoffDouble(self.shared).prepare_route(route)
    }
    fn into_running(self, port: RuntimePort) -> Result<Self::Running, BindingRefusal> {
        Ok(SwitchRunning {
            inner: HandoffDouble(self.shared).into_running(port)?,
            shared: self.shared, switch: self.switch,
            failed_by_old_loss: self.failed_by_old_loss,
        })
    }
}
impl HandoffRuntime for SwitchRunning {
    fn send(&self, route: BindingRoute, frame: OutgoingFrame<'_>)
        -> Result<SendReport, HandoffFailure>
    {
        if self.switch.swap(false, Ordering::SeqCst) {
            let outcome = self.shared.port().report_peer_lost(self.shared.route(0), OLD);
            let failed = match outcome {
                PeerLossOutcome::Applied { peer_removed: true, calls_failed } => calls_failed,
                other => panic!("unexpected test loss outcome: {other:?}"),
            };
            self.failed_by_old_loss.store(failed, Ordering::SeqCst);
            self.shared.port().peer_restarted(self.shared.route(0), NEW);
        }
        self.inner.send(route, frame)
    }
    fn receive_bounded(&self, route: BindingRoute, budget: &ReceiveBudget,
        sink: &mut dyn FnMut(InboundFrame<'_>) -> Flow) -> usize
    {
        self.inner.receive_bounded(route, budget, sink)
    }
}

fn deliver_response(shared: &Shared, request: &[u8], from: PeerRef, value: u32)
    -> Result<(), ReceiveError>
{
    let response = whole_frame(&Spec {
        kind: UMessageType::Response, id: uuid_at(WALL, 900),
        reqid: Some(sent_request_id(request)), ttl: None,
        source: uri(3, 1), sink: Some(uri(2, 0)), token: None,
        payload: Some(&value.to_le_bytes()),
    });
    shared.port().deliver(shared.route(0), InboundFrame {
        metadata: &[], payload: PayloadBytes::one(&response), peer: from,
        reply: ReplyToken::default(),
    })
}

// [utest->req~ssr-peer-failure-normal~1]
// [utest->req~ssr-peer-rpc-ends~1]
#[test]
fn g2_peer_change_inside_send_does_not_fail_an_observationally_bound_call() {
    let shared = Shared::leak();
    let clock = Clock::leak(1_000_000_000, WALL);
    let switch = Box::leak(Box::new(AtomicBool::new(false)));
    let failed = Box::leak(Box::new(AtomicUsize::new(usize::MAX)));
    let setup = TransportSetup::handoff_on(&memory(), clock, SwitchSetup {
        shared, switch, failed_by_old_loss: failed,
    }).unwrap();
    let client = RpcClient::<u32, u32>::on(&setup, &Uri(2), uri(3, 1))
        .wire(U32Wire { hook: None }).ttl(Duration::from_millis(10))
        .max_in_flight(2).build().unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let client = client.activate(&runtime).unwrap();

    let mut seed = client.invoke(&1).unwrap();
    let request = shared.sent.lock().unwrap().last().unwrap().clone();
    deliver_response(shared, &request, OLD, 11).unwrap();
    assert_eq!(*seed.try_response().unwrap().unwrap(), 11);
    drop(seed);
    switch.store(true, Ordering::SeqCst);
    let mut call = client.invoke(&2).unwrap();
    assert_eq!(failed.load(Ordering::SeqCst), 0,
        "the observed OLD peer must not become destination authority");
    assert!(call.try_response().unwrap().is_none());
    assert_eq!(shared.port().report_peer_lost(shared.route(0), OLD),
        PeerLossOutcome::Applied { peer_removed: false, calls_failed: 0 });
    assert_eq!(shared.port().report_peer_lost(shared.route(0), NEW),
        PeerLossOutcome::Applied { peer_removed: true, calls_failed: 0 });
    assert!(call.try_response().unwrap().is_none());
    clock.advance_ms(11);
    assert!(matches!(call.try_response(), Err(CallError::TimedOut)));
}

// [utest->req~ssr-peer-rpc-ends~1]
#[test]
fn g2_unknown_call_survives_unrelated_restart_and_still_accepts_a_correlated_response() {
    let shared = Shared::leak();
    let clock = Clock::leak(1_000_000_000, WALL);
    let setup = TransportSetup::handoff_on(&memory(), clock, HandoffDouble(shared)).unwrap();
    let client = RpcClient::<u32, u32>::on(&setup, &Uri(2), uri(3, 1))
        .wire(U32Wire { hook: None }).ttl(Duration::from_millis(10)).build().unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let client = client.activate(&runtime).unwrap();
    // Establish a real lower-incarnation table entry before the restart event.
    // Without this seed, peer_restarted only admits a new peer and never reaches
    // the Restarted branch whose effect on UNKNOWN calls this control checks.
    let unrelated = PeerRef { peer: 999, incarnation: 1 };
    let mut seed = client.invoke(&0).unwrap();
    let seed_request = shared.sent.lock().unwrap().last().unwrap().clone();
    deliver_response(shared, &seed_request, unrelated, 10).unwrap();
    assert_eq!(*seed.try_response().unwrap().unwrap(), 10);
    drop(seed);
    let mut call = client.invoke(&1).unwrap();
    let request = shared.sent.lock().unwrap().last().unwrap().clone();
    shared.port().peer_restarted(shared.route(0), PeerRef { peer: 999, incarnation: u64::MAX });
    assert!(call.try_response().unwrap().is_none());
    // This stale response proves the event advanced the existing peer row.
    assert_eq!(deliver_response(shared, &request, unrelated, 99),
        Err(ReceiveError::StalePeer));
    assert!(call.try_response().unwrap().is_none());
    deliver_response(shared, &request, NEW, 22).unwrap();
    assert_eq!(*call.try_response().unwrap().unwrap(), 22);
}
