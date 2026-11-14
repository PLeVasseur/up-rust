//! Test-private synthetic arming of real SDK routes. No timing admission record
//! is created. Host instrumentation may allocate/lock; it qualifies no platform.
extern crate std;

use super::*;
use crate::communication::{LocalUriProvider, two_mode::{Publisher, Subscriber}};
use crate::memory::StartupRegion;
use crate::platform::{Monotonic, Platform, PlatformError, PlatformInit, WaitResult};
use crate::platform::atomic::{AtomicU32, Ordering};
use crate::transport::{InboundRoute, RuntimePort, TransportRuntime, TransportSetup};
use crate::transport::deadline_timing::{ArmedDeadline, DeadlineTimingReport};
use crate::transport::report::RouteId;
use crate::transport::tests::{NullHandoff, TestUri, TestWire, WALL_MS};
use std::boxed::Box;
use std::sync::{Mutex, OnceLock};

#[derive(Default)]
struct ObservedClock {
    ns: AtomicU32,
    reads: AtomicU32,
    wakes: AtomicU32,
    wait_action: AtomicU32,
    wait_errors: AtomicU32,
    runtime: OnceLock<(TransportRuntime, RouteId)>,
    observed: Mutex<Option<DeadlineReport>>,
}

impl Platform for ObservedClock {
    fn now(&self) -> Monotonic {
        self.reads.fetch_add(1, Ordering::Relaxed);
        Monotonic(u64::from(self.ns.load(Ordering::Relaxed)))
    }
    fn unix_time_ms(&self) -> Option<u64> { Some(WALL_MS) }
    fn wait(&self, _: &AtomicU32, _: u32, _: Option<Monotonic>) -> WaitResult {
        // Signal::wait_on has registered its waiter before this callback.
        // Trigger ingress here so the production notify path must invoke wake_all.
        // No thread/barrier is needed, and no Leaf borrow is live at this point.
        let action = self.wait_action.swap(0, Ordering::Relaxed);
        if let Some((runtime, _)) = self.runtime.get() {
            let port = RuntimePort { core: runtime.core };
            let result = match action {
                1 => port.arrived(InboundRoute(0), Monotonic(2048)),
                2 => port.arrival_sample_missing(InboundRoute(0)),
                _ => Ok(()),
            };
            if result.is_err() { self.wait_errors.fetch_add(1, Ordering::Relaxed); }
        } else if action != 0 {
            self.wait_errors.fetch_add(1, Ordering::Relaxed);
        }
        WaitResult::TimedOut
    }
    fn wake_one(&self, word: &AtomicU32) { self.wake_all(word); }
    fn wake_all(&self, _: &AtomicU32) {
        self.wakes.fetch_add(1, Ordering::Relaxed);
        if let Some((runtime, id)) = self.runtime.get() {
            // Reenter the public report query from notification. The production
            // record borrow must have ended before this platform method runs.
            let report = runtime.deadline_report(*id);
            *self.observed.lock().unwrap() = report;
        }
    }
}

impl PlatformInit for ObservedClock {
    fn entropy(&self, out: &mut [u8]) -> Result<(), PlatformError> {
        out.fill(0x5a);
        Ok(())
    }
    /// # Safety
    /// The caller supplies the writable, unaliased, uninitialized range required
    /// by PlatformInit::prefault. This test implementation never accesses that
    /// range or retains its pointer. It provides no physical residency guarantee.
    unsafe fn prefault(&self, _: *mut u8, _: usize) -> Result<(), PlatformError> { Ok(()) }
    fn runtime(&'static self) -> &'static dyn Platform { self }
}

// [utest->req~ssr-qos-deadline~3] [utest->req~ssr-error-no-silent-loss~3]
// [utest->req~ssr-atomic-width~1]
#[test]
fn public_route_query_ingress_service_and_reentrant_notification_are_composed() {
    static REGION: StartupRegion<{ 1 << 20 }> = StartupRegion::new();
    let memory = REGION.claim().unwrap();
    let clock: &'static ObservedClock = Box::leak(Box::new(ObservedClock::default()));
    let setup = TransportSetup::handoff_on(&memory, clock, NullHandoff).unwrap();
    let first = Subscriber::<u32>::on(&setup, TestUri(1).get_resource_uri(0x8001))
        .wire(TestWire { fail_encode: false }).build().unwrap();
    let second = Subscriber::<u32>::on(&setup, TestUri(1).get_resource_uri(0x8002))
        .wire(TestWire { fail_encode: false }).build().unwrap();
    let unarmed = Subscriber::<u32>::on(&setup, TestUri(1).get_resource_uri(0x8003))
        .wire(TestWire { fail_encode: false }).build().unwrap();
    let send = Publisher::<u32>::on(&setup, &TestUri(1), 0x8004_u16)
        .wire(TestWire { fail_encode: false }).build().unwrap();
    // All permanent fixture storage is reserved before initialization ends.
    let monitors = [0, 0].map(|origin| memory.place(ArmedDeadline {
        monitor: monitor(origin), report: DeadlineTimingReport::pending(None),
    }).unwrap());
    let runtime = setup.enter_runtime().unwrap();
    let first = first.activate(&runtime).unwrap().route_id();
    let second = second.activate(&runtime).unwrap().route_id();
    let unarmed = unarmed.activate(&runtime).unwrap().route_id();
    let send = send.activate(&runtime).unwrap().route_id();
    for id in [first, second, unarmed, send, RouteId(u32::MAX)] {
        assert_eq!(runtime.deadline_report(id), None);
    }

    // Private fixture only: do not add a production catalog entry or expose a
    // public arming bypass. Test real RxCommon/DeadlineSlot/API wiring in place.
    for (id, armed) in [first, second].into_iter().zip(monitors) {
        let route = runtime.core.rx.iter().find(|route| route.id() == id).unwrap();
        route.common().monitor.publish(armed).unwrap();
    }
    clock.runtime.set((runtime, first)).unwrap();
    let port = RuntimePort { core: runtime.core };
    let signal = runtime.core.shared.signal();
    let before_wake = clock.wakes.load(Ordering::Relaxed);
    clock.wait_action.store(1, Ordering::Relaxed);
    assert_eq!(signal.wait_on(clock, signal.token(), Some(Monotonic(10_000))),
        crate::signal::SignalWait::Notified);
    assert!(clock.wakes.load(Ordering::Relaxed) > before_wake);
    assert_eq!(clock.wait_errors.load(Ordering::Relaxed), 0);
    let first_report = runtime.deadline_report(first).unwrap();
    assert_eq!((first_report.origin_ns, first_report.missed_gaps), (2048, 1));
    assert_eq!(*clock.observed.lock().unwrap(), Some(first_report));
    assert_eq!(runtime.deadline_report(second).unwrap().missed_gaps, 0);
    let schedule = runtime.core.rx.first().unwrap().deadline_schedule(2048, 2048).unwrap();
    assert_eq!(schedule.wait_request(2048, 10_000),
        crate::transport::deadline_monitor::WaitRequest::Until(3073));

    let before_wake = clock.wakes.load(Ordering::Relaxed);
    clock.wait_action.store(2, Ordering::Relaxed);
    assert_eq!(signal.wait_on(clock, signal.token(), Some(Monotonic(10_000))),
        crate::signal::SignalWait::Notified);
    assert!(clock.wakes.load(Ordering::Relaxed) > before_wake);
    assert_eq!(clock.wait_errors.load(Ordering::Relaxed), 0);
    let missing = runtime.deadline_report(first).unwrap();
    assert!(missing.missing_sample && !missing.count_exhausted);
    assert_eq!((missing.origin_ns, missing.missed_gaps), (2048, 1));
    assert_eq!(*clock.observed.lock().unwrap(), Some(missing));
    port.arrived(InboundRoute(1), Monotonic(4096)).unwrap();
    assert_eq!(runtime.deadline_report(second).unwrap().origin_ns, 4096);
    assert_eq!(runtime.deadline_report(first), Some(missing));
    assert!(port.arrived(InboundRoute(u32::MAX), Monotonic(1)).is_err());
    assert!(port.arrival_sample_missing(InboundRoute(u32::MAX)).is_err());

    clock.ns.store(6145, Ordering::Relaxed);
    let reads = clock.reads.load(Ordering::Relaxed);
    let wakes = clock.wakes.load(Ordering::Relaxed);
    let counters = runtime.counters(first);
    for _ in 0..3 { assert_eq!(runtime.deadline_report(first), Some(missing)); }
    assert_eq!(clock.reads.load(Ordering::Relaxed), reads);
    assert_eq!(clock.wakes.load(Ordering::Relaxed), wakes);
    assert_eq!(runtime.counters(first).deadline_missed, counters.deadline_missed);
    runtime.service();
    assert!(clock.reads.load(Ordering::Relaxed) > reads);
    let after = runtime.deadline_report(first).unwrap();
    assert_eq!((after.origin_ns, after.missed_gaps), (2048, 2));
    assert!(after.missing_sample);
    assert_eq!(runtime.deadline_report(second).unwrap().missed_gaps, 2);
    for id in [unarmed, send, RouteId(u32::MAX)] {
        assert_eq!(runtime.deadline_report(id), None);
    }

    // The same local ID in a second runtime selects its own monitor, never the
    // first runtime's state. This query promises no globally unique route ID.
    static OTHER_REGION: StartupRegion<{ 1 << 18 }> = StartupRegion::new();
    let other_memory = OTHER_REGION.claim().unwrap();
    let other_clock: &'static ObservedClock = Box::leak(Box::new(ObservedClock::default()));
    let setup = TransportSetup::handoff_on(&other_memory, other_clock, NullHandoff).unwrap();
    let other = Subscriber::<u32>::on(&setup, TestUri(2).get_resource_uri(0x8001))
        .wire(TestWire { fail_encode: false }).build().unwrap();
    let armed = other_memory.place(ArmedDeadline {
        monitor: monitor(9000), report: DeadlineTimingReport::pending(None),
    }).unwrap();
    let other_runtime = setup.enter_runtime().unwrap();
    let other_id = other.activate(&other_runtime).unwrap().route_id();
    assert_eq!(other_id, first);
    other_runtime.core.rx.first().unwrap().common().monitor.publish(armed).unwrap();
    let other = other_runtime.deadline_report(first).unwrap();
    assert_eq!((other.origin_ns, other.missed_gaps), (9000, 0));
    assert!(!other.missing_sample && !other.count_exhausted);
    assert_eq!(runtime.deadline_report(first), Some(after));
}
