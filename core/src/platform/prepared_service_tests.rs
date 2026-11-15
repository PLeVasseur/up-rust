//! Actual-worker development fixture. Private arming is not setup admission.
//! Execute only in a separately supervised process: activated pthread resources
//! remain permanent until process exit, including after service cancellation.
extern crate std;

use super::*;
use crate::communication::{LocalUriProvider, two_mode::Subscriber};
use crate::communication::two_mode::retirement_integration_tests::native_worker_heap_window_active;
use crate::platform::{Monotonic, NATIVE};
use crate::platform::atomic::AtomicU64;
use crate::transport::{BindingRoute, InboundFrame, InboundRoute, ReceiveError,
    RuntimePort, TransportRuntime};
use crate::transport::DeadlineTimingReport;
use crate::transport::inbound::{InboundDispatch, RxCommon};
use crate::transport::report::{Counters, RouteId, RouteReport};
use crate::transport::tests::{ArmedDeadline, CoherentDeadline, MonitorConfig, TestUri, TestWire};

// The real TypedInbound still performs service, monitor/counter publication and
// wake. This permanent test observer only publishes completion evidence. It is
// not another payload queue, scheduler, clock or deadline state machine.
struct ObservedRoute {
    inner: &'static dyn InboundDispatch,
    credits: AtomicU32,
    wrong_thread: AtomicBool,
    first_ns: AtomicU64,
    second_ns: AtomicU64,
}

impl InboundDispatch for ObservedRoute {
    fn common(&self) -> &RxCommon { self.inner.common() }
    fn deliver(&'static self, frame: InboundFrame<'_>) -> Result<(), ReceiveError> {
        self.inner.deliver(frame)
    }
    fn pull_limit(&self) -> usize { self.inner.pull_limit() }
    fn binding_route(&self) -> BindingRoute { self.inner.binding_route() }
    fn startup_check(&self) -> Result<(), crate::transport::error::StartupError> {
        self.inner.startup_check()
    }
    fn report(&self) -> RouteReport { self.inner.report() }
    fn counters(&self) -> &Counters { self.inner.counters() }
    fn id(&self) -> RouteId { self.inner.id() }
    fn wake(&'static self) { self.inner.wake(); }
    fn service(&'static self, now: Monotonic) -> usize {
        if !native_worker_heap_window_active() {
            self.wrong_thread.store(true, Ordering::Relaxed);
        }
        let completed = self.inner.service(now);
        if completed != 0 {
            // This wrapper is reached by exactly one worker in this fixture.
            // Main never invokes service/wait/pump on either runtime.
            let next = self.credits.load(Ordering::Relaxed).saturating_add(1);
            if next == 1 { self.first_ns.store(now.0, Ordering::Relaxed); }
            if next == 2 { self.second_ns.store(now.0, Ordering::Relaxed); }
            self.credits.store(next, Ordering::Release);
            NATIVE.wake_all(&self.credits);
        }
        completed
    }
}

// A test failure timeout is not an admitted service delay or production J.
fn wait_for_credit(route: &ObservedRoute, expected: u32) {
    let limit = NATIVE.now().saturating_add(5_000_000_000);
    loop {
        let seen = route.credits.load(Ordering::Acquire);
        if seen >= expected { assert_eq!(seen, expected); return; }
        assert!(NATIVE.now() < limit, "actual worker failed to publish a deadline credit");
        NATIVE.wait(&route.credits, seen, Some(limit));
    }
}

// [utest->req~ssr-qos-deadline~3] [utest->req~ssr-runtime-no-heap~1]
// [utest->req~ssr-release-lease-drop~2]
#[test]
#[ignore = "requires fresh source/artifact review and one supervised process; retains activated pthread"]
fn native_worker_counts_rearms_and_counts_without_application_service() {
    static REGION: StartupRegion<{ 16 * 1024 * 1024 }> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let mut worker = prepare(&memory, &NATIVE).unwrap();
    assert!(worker.control.heap.calibrated.load(Ordering::Relaxed));
    assert!(worker.control.core().is_none());

    let setup = TransportSetup::handoff_on(&memory, &NATIVE, NullHandoff).unwrap();
    let subscriber = Subscriber::<u32>::on(&setup, TestUri(1).get_resource_uri(0x8001))
        .wire(TestWire { fail_encode: false }).build().unwrap();
    // Reserve every permanent object before setup seals the shared memory.
    // After setup, only these exclusive uninitialized slots are written; no
    // runtime placement/allocation or shared-to-mutable conversion is needed.
    let observer_slot = memory.place(core::mem::MaybeUninit::<ObservedRoute>::uninit()).unwrap();
    let rx_slot = memory.place(core::mem::MaybeUninit::<[&'static dyn InboundDispatch; 1]>::uninit()).unwrap();
    let core_slot = memory.place(core::mem::MaybeUninit::<RuntimeCore>::uninit()).unwrap();
    let config = MonitorConfig::new(5_000_000, 0).unwrap();
    let armed = memory.place(ArmedDeadline {
        monitor: CoherentDeadline::try_new(config, 0).unwrap(),
        report: DeadlineTimingReport::pending(None),
    }).unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let id = subscriber.activate(&runtime).unwrap().route_id();
    let inner = *runtime.core.rx.first().unwrap();
    assert_eq!(inner.id(), id);
    let observed: &'static ObservedRoute = observer_slot.write(ObservedRoute {
        inner, credits: AtomicU32::new(0), wrong_thread: AtomicBool::new(false),
        first_ns: AtomicU64::new(0), second_ns: AtomicU64::new(0),
    });
    let rx: &'static [&'static dyn InboundDispatch] =
        rx_slot.write([observed as &'static dyn InboundDispatch]);
    let origin = NATIVE.now().0;
    armed.monitor.arm(origin);
    // Test-private permanent construction: the public deadline setup path still
    // refuses absent qualification. No ValidatedJ or ACCEPTED_TIMING is changed.
    inner.common().monitor.publish(armed).unwrap();
    let core: &'static RuntimeCore = core_slot.write(RuntimeCore {
        activation_sample_ns: origin, id: runtime.core.id,
        binding: runtime.core.binding, shared: runtime.core.shared,
        tx: runtime.core.tx, rx, waivers: runtime.core.waivers, reports: runtime.core.reports,
    });
    let runtime = TransportRuntime { core };
    assert_eq!(runtime.deadline_report(id).unwrap().missed_gaps, 0);
    assert!(!native_worker_heap_window_active());

    worker.activate(DeadlineServiceEntry::new(core)).unwrap();
    wait_for_credit(observed, 1);
    let first = runtime.deadline_report(id).unwrap();
    assert_eq!((first.origin_ns, first.missed_gaps), (origin, 1));
    assert_eq!(runtime.counters(id).deadline_missed, 1);
    // The first gap is already counted. This one ingress rearm cannot itself
    // supply another credit; only the worker's later service can do so.
    let arrival = NATIVE.now();
    RuntimePort { core }.arrived(InboundRoute(0), arrival).unwrap();
    wait_for_credit(observed, 2);
    let second = runtime.deadline_report(id).unwrap();
    assert_eq!((second.origin_ns, second.missed_gaps), (arrival.0, 2));
    assert_eq!(runtime.counters(id).deadline_missed, 2);
    assert!(!first.missing_sample && !second.missing_sample);
    assert!(!first.count_exhausted && !second.count_exhausted);
    assert!(!observed.wrong_thread.load(Ordering::Relaxed));
    assert!(observed.first_ns.load(Ordering::Relaxed) >= origin);
    assert!(observed.second_ns.load(Ordering::Relaxed) >= arrival.0);

    // Initialization-rollback semantics after activation: cancel service and
    // retain the pthread. This does not join, exit or reclaim runtime resources.
    worker.cancel().unwrap();
    let limit = NATIVE.now().saturating_add(5_000_000_000);
    while worker.control.heap.finished.load(Ordering::Acquire) == 0 {
        assert!(NATIVE.now() < limit, "worker did not finish its observation window");
        NATIVE.wait(&worker.control.heap.finished, 0, Some(limit));
    }
    assert!(worker.join.is_none());
    assert_eq!(worker.control.command.load(Ordering::Acquire), RETAIN_FOREVER);
    assert!(worker.control.heap.valid.load(Ordering::Relaxed));
    let allocs = worker.control.heap.allocs.load(Ordering::Relaxed);
    let frees = worker.control.heap.frees.load(Ordering::Relaxed);
    assert_eq!((allocs, frees), (0, 0));
    std::println!("actual worker: 2 service credits, rearm preserved, Rust allocs={allocs}, frees={frees}; initialization calibration=1 alloc/1 free; no J claim");
}
