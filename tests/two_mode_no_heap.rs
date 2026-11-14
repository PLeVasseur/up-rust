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

//! Proves that runtime mode does not touch the heap (R3).
//!
//! The binary has one allocator and every window counts all threads, from when
//! `enter_runtime` returns: activation, traffic, refusals, every overflow policy
//! and peer death, with lease drops inside the windows. A window fails on any
//! allocation, free or reallocation. Run with
//! `cargo test --test two_mode_no_heap`.
//!
//! When `UP_TWO_MODE_C_LEVEL` is set, each window also asserts the C allocator
//! counters; the process must then be started under the allocation observer.

#![allow(clippy::indexing_slicing)]

use std::alloc::System;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use up_rust::heap_guard::{c_level, observe, HeapGuard, Scope};
use up_rust::startup::LEAKED_HEAP_REGIONS;
use up_rust::test_util::handoff::InMemoryBus;
use up_rust::test_util::lending::{LendingBus, LendingOptions};
use up_rust::test_util::wires::{
    DoorState, DoorWire, LittleDoorWire, VariableDoorWire, DOOR_ENCODING,
};
use up_rust::test_util::TestClock;
use up_rust::two_mode::*;
use up_rust::{
    NativeProfile, NativeProfileAgreement, PayloadEncoding, StablePayloadExt, StaticUriProvider,
    UCode, UEntityId, UUri,
};

#[global_allocator]
static HEAP: HeapGuard = HeapGuard::new(System);

const METHOD: u16 = 0x0042;
const ROUNDS: u32 = 1000;

fn entity(authority: &str, type_id: u16) -> StaticUriProvider {
    StaticUriProvider::for_entity(authority, UEntityId::first_instance(type_id), 1).expect("uri")
}

fn memory() -> StartupMemory {
    StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 64 * 1024).expect("memory")
}

fn handoff(bus: InMemoryBus, peer: u64) -> TransportSetup {
    TransportSetup::handoff_in(&memory(), bus.participant(peer)).expect("setup")
}

fn handoff_on(bus: InMemoryBus, peer: u64, clock: &'static TestClock) -> TransportSetup {
    TransportSetup::handoff_on(&memory(), clock, bus.participant(peer)).expect("setup")
}

fn lending(bus: LendingBus, peer: u64) -> TransportSetup {
    TransportSetup::lending_in(&memory(), bus.participant(peer)).expect("setup")
}

fn lending_with(bus: LendingBus, peer: u64, options: LendingOptions) -> TransportSetup {
    TransportSetup::lending_in(&memory(), bus.participant(peer).with_options(options))
        .expect("setup")
}

fn topic() -> UUri {
    entity("door", 1).get_resource_uri(0x8001)
}

fn door(id: u32, open: u8) -> DoorState {
    DoorState { id, open }
}

fn publisher_on(setup: &TransportSetup) -> PublisherSetup<DoorState> {
    Publisher::<DoorState>::on(setup, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .max_token_len(16)
        .build()
        .expect("publisher")
}

fn subscriber_on(setup: &TransportSetup, qos: Qos) -> SubscriberSetup<DoorState> {
    Subscriber::<DoorState>::on(setup, topic())
        .wire(DoorWire)
        .also_wire(LittleDoorWire)
        .max_token_len(16)
        .qos(qos)
        .build()
        .expect("subscriber")
}

struct Pair {
    publisher: Publisher<DoorState>,
    subscriber: Subscriber<DoorState>,
    tx: TransportRuntime,
    rx: TransportRuntime,
}

fn activate(a: TransportSetup, b: TransportSetup, qos: Qos) -> Pair {
    let publisher = publisher_on(&a);
    let subscriber = subscriber_on(&b, qos);
    let tx = a.enter_runtime().expect("runtime");
    let rx = b.enter_runtime().expect("runtime");
    Pair {
        publisher: publisher.activate(&tx).expect("activate"),
        subscriber: subscriber.activate(&rx).expect("activate"),
        tx,
        rx,
    }
}

/// Starts workers before a window opens, so a thread's own start-up, which
/// allocates and frees, cannot land inside it.
#[derive(Default)]
struct Gate {
    arrived: AtomicUsize,
    go: AtomicBool,
    finished: AtomicUsize,
    leave: AtomicBool,
}

impl Gate {
    /// A worker's first call: reports that it has started, then waits for `open`.
    ///
    /// Keep the guard for the whole worker. Its drop is the worker's last act: it
    /// reports the worker finished and keeps the thread alive until `release`,
    /// since a finishing thread frees its own state. If the worker panics, it only
    /// reports, so the window does not wait on a dead worker.
    fn enter(&self) -> GateExit<'_> {
        self.arrived.fetch_add(1, Ordering::SeqCst);
        while !self.go.load(Ordering::SeqCst) {
            std::hint::spin_loop();
        }
        GateExit(self)
    }

    /// Waits until `workers` have started. Call before the window opens.
    fn await_workers(&self, workers: usize) {
        while self.arrived.load(Ordering::SeqCst) < workers {
            std::thread::yield_now();
        }
    }

    fn open(&self) {
        self.go.store(true, Ordering::SeqCst);
    }

    /// Waits inside the window until `workers` have done their work.
    fn await_finished(&self, workers: usize) {
        while self.finished.load(Ordering::SeqCst) < workers {
            std::hint::spin_loop();
        }
    }

    /// Lets every worker leave. Call after the window has closed.
    fn release(&self) {
        self.leave.store(true, Ordering::SeqCst);
    }

    /// A guard for the scope that owns this gate. If the scope unwinds, as when a
    /// window fails, it opens and releases the gate: `std::thread::scope` joins its
    /// workers before the panic goes on, and a worker still waiting on `open` or
    /// `release` would hang the binary instead of letting it fail.
    fn abandon_on_unwind(&self) -> GateAbandon<'_> {
        GateAbandon(self)
    }
}

/// A worker's way out of a [`Gate`]; see [`Gate::enter`].
struct GateExit<'a>(&'a Gate);

impl Drop for GateExit<'_> {
    fn drop(&mut self) {
        self.0.finished.fetch_add(1, Ordering::SeqCst);
        if !std::thread::panicking() {
            while !self.0.leave.load(Ordering::SeqCst) {
                std::hint::spin_loop();
            }
        }
    }
}

/// See [`Gate::abandon_on_unwind`].
struct GateAbandon<'a>(&'a Gate);

impl Drop for GateAbandon<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.go.store(true, Ordering::SeqCst);
            self.0.leave.store(true, Ordering::SeqCst);
        }
    }
}

struct Results {
    lines: Vec<String>,
    c_level: bool,
}

impl Results {
    /// Runs one window. Every subject must be clean: nothing acquired or released.
    fn window<T>(&mut self, subject: &str, operation: impl FnOnce() -> T) -> T {
        if self.c_level {
            let (value, report) = c_level::observe(operation);
            let rust = report.rust().counts();
            let c = report.c();
            self.lines.push(format!(
                "window={subject} rust_allocations={} rust_deallocations={} \
                 rust_reallocations={} c={c:?} clean={}",
                rust.allocations,
                rust.deallocations,
                rust.reallocations,
                report.is_clean()
            ));
            assert!(report.is_clean(), "{subject}: {report}");
            value
        } else {
            let (value, report) = observe(Scope::AllThreads, operation);
            let counts = report.counts();
            self.lines.push(format!(
                "window={subject} allocations={} deallocations={} reallocations={} clean={}",
                counts.allocations,
                counts.deallocations,
                counts.reallocations,
                counts.is_clean()
            ));
            assert!(counts.is_clean(), "{subject}: {report}");
            value
        }
    }

    fn note(&mut self, line: String) {
        self.lines.push(line);
    }
}

/// A wire that allocates despite declaring otherwise, so a clean result means something.
#[derive(Clone, Copy, Debug, Default)]
struct LyingWire;

impl Wire<DoorState> for LyingWire {
    fn describe(&self) -> WireInfo {
        DoorWire.describe()
    }

    fn encoded_size(&self, value: &DoorState) -> EncodedSize {
        DoorWire.encoded_size(value)
    }

    fn encode(&self, value: &DoorState, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        let staged = vec![value.open; 5];
        out.put(&staged)
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, DoorState>) -> Ready<'s, DoorState> {
        Wire::init_slot(&DoorWire, slot)
    }

    fn decode(
        &self,
        encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut DoorState,
    ) -> Result<(), WireError> {
        DoorWire.decode(encoding, bytes, into)
    }
}

/// Windows that must see the heap, so that clean windows mean something.
fn controls(results: &mut Results) {
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let lying = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8009)
        .wire(LyingWire)
        .build()
        .unwrap();
    let runtime = a.enter_runtime().unwrap();
    let lying = lying.activate(&runtime).unwrap();
    let ((), report) = observe(Scope::AllThreads, || {
        lying.publish(&door(1, 1)).unwrap();
    });
    assert!(
        report.counts().allocations >= 1 && report.counts().deallocations >= 1,
        "negative control: the guard must see the staged Vec: {report}"
    );
    results.note(format!(
        "control=lying-wire allocations={} deallocations={} (expected nonzero)",
        report.counts().allocations,
        report.counts().deallocations
    ));

    // A binding that locks a never-primed mutex in its first send. On pthread
    // targets the first lock allocates, and the first-traffic window must see it.
    let bus = LendingBus::new();
    let a = lending_with(
        bus,
        1,
        LendingOptions {
            lazy_mutex: true,
            ..LendingOptions::default()
        },
    );
    let b = lending(bus, 2);
    let publisher = publisher_on(&a);
    let subscriber = subscriber_on(&b, Qos::DEFAULT);
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let publisher = publisher.activate(&tx).unwrap();
    let _subscriber = subscriber.activate(&rx).unwrap();
    let ((), report) = observe(Scope::AllThreads, || {
        publisher.publish(&door(1, 1)).unwrap();
    });
    let saw_heap = !report.counts().is_clean();
    results.note(format!(
        "control=lazy-mutex-first-traffic allocations={} saw_heap={saw_heap} expected=true",
        report.counts().allocations
    ));
    assert!(
        saw_heap,
        "positive control: a binding that allocates on first traffic must fail this \
         window, on every target: {report}"
    );
}

/// `enter_runtime` and activating all six roles.
fn activation(results: &mut Results) {
    let bus = InMemoryBus::new(8, 4096);
    let a = handoff(bus, 1);
    let b = handoff(bus, 2);
    let provider = entity("door", 1);
    let dashboard = entity("dashboard", 2);
    let method = entity("door", 2).get_resource_uri(METHOD);
    let publisher = publisher_on(&a);
    let subscriber = subscriber_on(&b, Qos::DEFAULT);
    let notifier = Notifier::<DoorState>::on(&a, &provider, 0x8100, dashboard.get_source_uri())
        .wire(DoorWire)
        .build()
        .unwrap();
    let listener =
        NotificationListener::<DoorState>::on(&b, provider.get_resource_uri(0x8100), &dashboard)
            .wire(DoorWire)
            .build()
            .unwrap();
    let client = RpcClient::<DoorState, DoorState>::on(&a, &provider, method)
        .wire(DoorWire)
        .build()
        .unwrap();
    let server = RpcServer::<DoorState, DoorState>::on(&b, &entity("door", 2), METHOD)
        .wire(DoorWire)
        .build()
        .unwrap();
    let bystander = lending(LendingBus::new(), 9).enter_runtime().unwrap();

    let (tx, rx) = results.window("enter_runtime_handoff", || {
        (a.enter_runtime().unwrap(), b.enter_runtime().unwrap())
    });
    results.window("activate_all_six_roles", || {
        let _ = publisher.activate(&tx).unwrap();
        let _ = subscriber.activate(&rx).unwrap();
        let _ = notifier.activate(&tx).unwrap();
        let _ = listener.activate(&rx).unwrap();
        let _ = client.activate(&tx).unwrap();
        let _ = server.activate(&rx).unwrap();
    });

    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let publisher = publisher_on(&a);
    let wrong = publisher_on(&b);
    let subscriber = subscriber_on(&b, Qos::DEFAULT);
    let tx = results.window("enter_runtime_lending", || a.enter_runtime().unwrap());
    let rx = b.enter_runtime().unwrap();
    results.window("wrong_transport_activation_is_refused_with_no_heap", || {
        assert!(matches!(
            wrong.activate(&bystander),
            Err(StartupError::WrongTransport)
        ));
    });
    results.window("activate_lending", || {
        let _ = publisher.activate(&tx).unwrap();
        let _ = subscriber.activate(&rx).unwrap();
    });
}

fn pair_windows(results: &mut Results, label: &str, pair: &Pair) {
    results.window(&format!("{label}_first_traffic"), || {
        pair.publisher.publish(&door(1, 1)).unwrap();
        let received = pair.subscriber.try_recv().unwrap().expect("a frame");
        assert_eq!((received.id, received.open), (1, 1));
        assert_eq!(received.header().payload_encoding, Some(DOOR_ENCODING));
    });
    results.window(&format!("{label}_publish_receive_1000"), || {
        for id in 0..ROUNDS {
            pair.publisher.publish(&door(id, 1)).unwrap();
            let received = pair.subscriber.try_recv().unwrap().expect("a frame");
            assert_eq!(received.id, id);
        }
        assert!(pair.subscriber.try_recv().unwrap().is_none());
        let _ = pair.publisher.counters();
        let _ = pair.subscriber.counters();
        let _ = pair.tx.counters(pair.publisher.route_id());
        let _ = pair.rx.routes();
        let _ = pair.rx.waivers();
    });
    results.window(&format!("{label}_token_and_traceparent_1000"), || {
        let fields = MessageFields {
            token: Some("access"),
            ..MessageFields::default()
        };
        for id in 0..ROUNDS {
            pair.publisher.publish_with(&fields, &door(id, 0)).unwrap();
            let received = pair.subscriber.try_recv().unwrap().expect("a frame");
            assert_eq!(received.header().token(), Some("access"));
        }
    });
    results.window(&format!("{label}_held_leases_out_of_order"), || {
        pair.publisher.publish(&door(1, 0)).unwrap();
        let first = pair.subscriber.try_recv().unwrap().expect("a frame");
        pair.publisher.publish(&door(2, 0)).unwrap();
        let second = pair.subscriber.try_recv().unwrap().expect("a frame");
        drop(first);
        pair.publisher.publish(&door(3, 0)).unwrap();
        let third = pair.subscriber.try_recv().unwrap().expect("a frame");
        drop(second);
        drop(third);
    });
    results.window(&format!("{label}_refusals"), || {
        let value = door(1, 0);
        assert!(matches!(
            pair.publisher.publish_using::<VariableDoorWire>(&value),
            Err(SendError::WireNotAdmitted)
        ));
        let unknown = PayloadEncoding::private_use(0xF999);
        assert!(matches!(
            pair.publisher
                .publish_encoded(unknown, PayloadBytes::one(&[])),
            Err(SendError::EncodingNotAdmitted(_))
        ));
        let token_too_long = MessageFields {
            token: Some("a token much longer than sixteen bytes"),
            ..MessageFields::default()
        };
        assert!(matches!(
            pair.publisher.publish_with(&token_too_long, &value),
            Err(SendError::TokenTooLong)
        ));
        let traceparent = MessageFields {
            traceparent: Some("00-0-0-01"),
            ..MessageFields::default()
        };
        assert!(matches!(
            pair.publisher.publish_with(&traceparent, &value),
            Err(SendError::TraceparentRejected)
        ));
        let oversize = [0_u8; 64];
        assert!(pair
            .publisher
            .publish_encoded(DOOR_ENCODING, PayloadBytes::one(&oversize))
            .is_err());
        // An admitted encoding whose bytes do not decode.
        pair.publisher
            .publish_encoded(DOOR_ENCODING, PayloadBytes::one(&[1, 2]))
            .unwrap();
        assert!(pair.subscriber.try_recv().unwrap().is_none());
        assert!(pair.subscriber.try_recv().unwrap().is_none());
    });
    results.window(&format!("{label}_max_held"), || {
        let mut held: [Option<Received<DoorState>>; 4] = [None, None, None, None];
        for (id, slot) in held.iter_mut().enumerate() {
            pair.publisher.publish(&door(id as u32, 0)).unwrap();
            *slot = pair.subscriber.try_recv().unwrap();
        }
        pair.publisher.publish(&door(99, 0)).unwrap();
        assert!(matches!(
            pair.subscriber.try_recv(),
            Err(ReceiveError::MaxHeldExceeded)
        ));
        held[1] = None;
        assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, 99);
        drop(held);
    });
}

fn traffic(results: &mut Results) {
    let bus = InMemoryBus::new(16, 4096);
    let handoff_bus_pair = activate(handoff(bus, 1), handoff(bus, 2), Qos::DEFAULT);
    pair_windows(results, "handoff", &handoff_bus_pair);

    let bus = LendingBus::new();
    let lending_bus_pair = activate(lending(bus, 1), lending(bus, 2), Qos::DEFAULT);
    pair_windows(results, "lending", &lending_bus_pair);

    notifications(results);
}

fn notifications(results: &mut Results) {
    let dashboard = entity("dashboard", 2);
    let run = |results: &mut Results, label: &str, a: TransportSetup, b: TransportSetup| {
        let provider = entity("door", 1);
        let notifier = Notifier::<DoorState>::on(&a, &provider, 0x8100, dashboard.get_source_uri())
            .wire(DoorWire)
            .build()
            .unwrap();
        let listener = NotificationListener::<DoorState>::on(
            &b,
            provider.get_resource_uri(0x8100),
            &dashboard,
        )
        .wire(DoorWire)
        .build()
        .unwrap();
        let tx = a.enter_runtime().unwrap();
        let rx = b.enter_runtime().unwrap();
        let notifier = notifier.activate(&tx).unwrap();
        let listener = listener.activate(&rx).unwrap();
        results.window(&format!("{label}_notify_1000"), || {
            for id in 0..ROUNDS {
                notifier.notify(&door(id, 1)).unwrap();
                assert_eq!(listener.try_recv().unwrap().expect("a frame").id, id);
            }
        });
    };
    let bus = InMemoryBus::new(16, 4096);
    run(results, "handoff", handoff(bus, 1), handoff(bus, 2));
    let bus = LendingBus::new();
    run(results, "lending", lending(bus, 1), lending(bus, 2));
}

fn overflow(results: &mut Results) {
    let depth2 = |overflow| Qos::DEFAULT.with_depth(2).with_overflow(overflow);
    // A plain `try_recv` reader gets the route's policy: the bus's queue is the
    // route's queue, as a pull binding's must be, and reports what it discards.
    let bus = InMemoryBus::new(32, 4096);
    let pair = activate(
        handoff(bus, 1),
        handoff(bus, 2),
        depth2(Overflow::DropOldest),
    );
    results.window("handoff_drop_oldest_inbox", || {
        for id in 0..8 {
            pair.publisher.publish(&door(id, 0)).unwrap();
        }
        assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, 6);
        assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, 7);
    });
    assert_eq!(pair.subscriber.counters().dropped_oldest, 6);

    let bus = InMemoryBus::new(32, 4096);
    let pair = activate(
        handoff(bus, 1),
        handoff(bus, 2),
        depth2(Overflow::DropNewest),
    );
    results.window("handoff_drop_newest_inbox", || {
        for id in 0..8 {
            pair.publisher.publish(&door(id, 0)).unwrap();
        }
        assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, 0);
        assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, 1);
    });
    assert_eq!(pair.subscriber.counters().dropped_newest, 6);

    // The middleware's queue is the queue on a lending route.
    for (policy, first) in [(Overflow::DropOldest, 6), (Overflow::DropNewest, 0)] {
        let bus = LendingBus::new();
        let pair = activate(lending(bus, 1), lending(bus, 2), depth2(policy));
        results.window(&format!("lending_overflow_{policy:?}"), || {
            for id in 0..8 {
                pair.publisher.publish(&door(id, 0)).unwrap();
            }
            assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, first);
            assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, first + 1);
        });
    }

    // A full medium refuses a handoff send with `Busy` (a medium of one frame in
    // flight), beside a route whose policy drops the oldest.
    let bus = InMemoryBus::with_medium(8, 4096, 1);
    let busy = activate(handoff(bus, 1), handoff(bus, 2), Qos::DEFAULT);
    let bus = InMemoryBus::new(32, 4096);
    let dropping = activate(
        handoff(bus, 1),
        handoff(bus, 2),
        depth2(Overflow::DropOldest),
    );
    results.window("qos_drop_oldest_and_busy_transport_500", || {
        for id in 0..250 {
            busy.publisher.publish(&door(id, 0)).unwrap();
            assert!(matches!(
                busy.publisher.publish(&door(id, 0)),
                Err(SendError::Busy)
            ));
            let _ = busy.subscriber.try_recv().unwrap();
            for offset in 0..4 {
                dropping.publisher.publish(&door(offset, 0)).unwrap();
            }
            assert_eq!(dropping.subscriber.try_recv().unwrap().unwrap().id, 2);
            assert_eq!(dropping.subscriber.try_recv().unwrap().unwrap().id, 3);
        }
    });

    // An exhausted loan pool on a lending route.
    let bus = LendingBus::new();
    let a = lending_with(
        bus,
        1,
        LendingOptions {
            slots: 1,
            ..LendingOptions::default()
        },
    );
    let b = lending(bus, 2);
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .slots(1)
        .build()
        .unwrap();
    let subscriber = subscriber_on(&b, Qos::DEFAULT);
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let publisher = publisher.activate(&tx).unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    results.window("lending_pool_exhausted_500", || {
        for id in 0..500 {
            publisher.publish(&door(id, 0)).unwrap();
            assert!(matches!(
                publisher.publish(&door(id, 0)),
                Err(SendError::Loan(LoanError::PoolExhausted))
            ));
            let _ = subscriber.try_recv().unwrap();
        }
    });

    // `Wait` that runs out of time on a full queue.
    let bus = LendingBus::new();
    let qos = Qos::DEFAULT.with_depth(1).with_overflow(Overflow::Wait {
        timeout: Duration::from_millis(1),
    });
    let pair = activate(lending(bus, 1), lending(bus, 2), qos);
    pair.publisher.publish(&door(1, 0)).unwrap();
    results.window("lending_wait_times_out", || {
        assert!(matches!(
            pair.publisher.publish(&door(2, 0)),
            Err(SendError::TimedOut)
        ));
    });

    // A publisher blocked in a `Wait` until the receiver drains.
    let bus = LendingBus::new();
    let qos = Qos::DEFAULT.with_depth(1).with_overflow(Overflow::Wait {
        timeout: Duration::from_secs(5),
    });
    let pair = activate(lending(bus, 1), lending(bus, 2), qos);
    pair.publisher.publish(&door(1, 0)).unwrap();
    let gate = Gate::default();
    let released = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let subscriber = pair.subscriber;
        let (gate, released) = (&gate, &released);
        let _abandon = gate.abandon_on_unwind();
        let worker = scope.spawn(move || {
            let _exit = gate.enter();
            let _ = subscriber.try_recv().unwrap().expect("a frame");
            released.store(true, Ordering::SeqCst);
        });
        gate.await_workers(1);
        results.window("lending_wait_completes_when_space_opens", || {
            gate.open();
            pair.publisher.publish(&door(2, 0)).unwrap();
            gate.await_finished(1);
        });
        gate.release();
        // The scope alone returns once the worker's result is dropped, before the
        // thread frees its last bytes, which the next all-thread window would
        // count. `join` waits until the thread has ended.
        worker.join().expect("the worker");
    });
    assert!(released.load(Ordering::SeqCst));
}

fn time_and_peers(results: &mut Results) {
    // Peer death, restart and a stale incarnation, inside the windows.
    let bus = InMemoryBus::new(16, 4096);
    let pair = activate(
        handoff(bus, 1),
        handoff(bus, 2),
        Qos::DEFAULT.with_deadline(Duration::from_secs(3600)),
    );
    results.window("peer_death_deadline_restart_stale_200", || {
        for id in 0..200 {
            let _ = pair.rx.service();
            bus.kill(1);
            pair.publisher.publish(&door(id, 0)).unwrap();
            assert!(pair.subscriber.try_recv().unwrap().is_none());
            let incarnation = bus.restart(1);
            pair.publisher.publish(&door(id, 0)).unwrap();
            assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, id);
            bus.set_incarnation(1, incarnation - 1);
            pair.publisher.publish(&door(id, 0)).unwrap();
            assert!(pair.subscriber.try_recv().unwrap().is_none());
            bus.set_incarnation(1, incarnation);
        }
    });

    let bus = LendingBus::new();
    let pair = activate(lending(bus, 1), lending(bus, 2), Qos::DEFAULT);
    results.window("lending_peer_death_and_restart_200", || {
        for id in 0..200 {
            bus.kill(1);
            pair.publisher.publish(&door(id, 0)).unwrap();
            assert!(pair.subscriber.try_recv().unwrap().is_none());
            let _ = bus.restart(1);
            pair.publisher.publish(&door(id, 0)).unwrap();
            assert_eq!(pair.subscriber.try_recv().unwrap().unwrap().id, id);
        }
    });

    // TTL expiry, a broken clock and the deadline monitor, on the frozen clock.
    let clock = TestClock::leak(1_000, 1_700_000_000_000);
    let bus = InMemoryBus::new(16, 4096);
    let a = handoff_on(bus, 1, clock);
    let b = handoff_on(bus, 2, clock);
    let publisher = Publisher::<DoorState>::on(&a, &entity("door", 1), 0x8001)
        .wire(DoorWire)
        .ttl(Duration::from_millis(10))
        .build()
        .unwrap();
    let subscriber = subscriber_on(&b, Qos::DEFAULT.with_deadline(Duration::from_millis(5)));
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let publisher = publisher.activate(&tx).unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    results.window("ttl_expiry_clock_failure_and_deadline_monitor_200", || {
        for id in 0..200 {
            publisher.publish(&door(id, 0)).unwrap();
            clock.advance_wall_ms(20);
            assert!(subscriber.try_recv().unwrap().is_none(), "expired");
            publisher.publish(&door(id, 0)).unwrap();
            clock.break_wall_clock();
            assert!(subscriber.try_recv().unwrap().is_none(), "clock failed");
            clock.repair_wall_clock();
            clock.advance_ns(10_000_000);
            let _ = rx.service();
        }
    });
    let counters = subscriber.counters();
    assert_eq!(counters.expired, 200);
    assert_eq!(counters.clock_unavailable, 200);
    assert!(counters.deadline_missed > 0);
}

struct RpcSide {
    client: RpcClient<DoorState, DoorState>,
    server: RpcServer<DoorState, DoorState>,
}

fn rpc_side(
    a: &TransportSetup,
    b: &TransportSetup,
    clients: &[UUri],
    ttl: Duration,
) -> (
    RpcClientSetup<DoorState, DoorState>,
    RpcServerSetup<DoorState, DoorState>,
) {
    let method = entity("door", 2).get_resource_uri(METHOD);
    (
        RpcClient::<DoorState, DoorState>::on(a, &entity("door", 1), method)
            .wire(DoorWire)
            .ttl(ttl)
            .max_in_flight(2)
            .build()
            .unwrap(),
        RpcServer::<DoorState, DoorState>::on(b, &entity("door", 2), METHOD)
            .wire(DoorWire)
            .clients(clients)
            .build()
            .unwrap(),
    )
}

fn rpc_windows(results: &mut Results, label: &str, rpc: &RpcSide) {
    results.window(
        &format!("{label}_rpc_is_heap_free_and_pollable_1000"),
        || {
            for id in 0..ROUNDS {
                let mut call = rpc.client.invoke(&door(id, 0)).unwrap();
                let request = rpc.server.try_recv().unwrap().expect("a request");
                let value = request.id;
                request.respond(&door(value + 1, 0)).unwrap();
                let reply = call.try_response().unwrap().expect("a response");
                assert_eq!(reply.id, id + 1);
            }
        },
    );
    results.window(&format!("{label}_rpc_refusals"), || {
        let mut first = rpc.client.invoke(&door(1, 0)).unwrap();
        let second = rpc.client.invoke(&door(2, 0)).unwrap();
        assert!(matches!(
            rpc.client.invoke(&door(3, 0)),
            Err(CallError::SlotsExhausted)
        ));
        rpc.server
            .try_recv()
            .unwrap()
            .expect("a request")
            .respond_failure(UCode::PermissionDenied)
            .unwrap();
        assert!(matches!(
            first.try_response(),
            Err(CallError::Rejected(UCode::PermissionDenied))
        ));
        drop(first);
        let _ = rpc.server.try_recv().unwrap();
        drop(second);
    });
}

fn rpc(results: &mut Results) {
    let bus = InMemoryBus::new(16, 4096);
    let a = handoff(bus, 1);
    let b = handoff(bus, 2);
    let (client, server) = rpc_side(&a, &b, &[], Duration::from_secs(30));
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let handoff_rpc = RpcSide {
        client: client.activate(&tx).unwrap(),
        server: server.activate(&rx).unwrap(),
    };
    rpc_windows(results, "handoff", &handoff_rpc);

    // A lending transport needs the client list up front.
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let (client, server) = rpc_side(
        &a,
        &b,
        std::slice::from_ref(&entity("door", 1).get_source_uri()),
        Duration::from_secs(30),
    );
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let lending_rpc = RpcSide {
        client: client.activate(&tx).unwrap(),
        server: server.activate(&rx).unwrap(),
    };
    rpc_windows(results, "lending", &lending_rpc);

    // The server is told about an unknown client.
    let bus = InMemoryBus::new(16, 4096);
    let a = handoff(bus, 1);
    let b = handoff(bus, 2);
    let (client, server) = rpc_side(
        &a,
        &b,
        std::slice::from_ref(&entity("someone-else", 9).get_source_uri()),
        Duration::from_secs(30),
    );
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let client = client.activate(&tx).unwrap();
    let server = server.activate(&rx).unwrap();
    results.window(
        "unknown_rpc_client_is_refused_at_runtime_with_no_heap",
        || {
            for id in 0..200 {
                let call = client.invoke(&door(id, 0)).unwrap();
                assert!(server.try_recv().unwrap().is_none());
                drop(call);
            }
        },
    );
    assert_eq!(server.counters().unknown_client, 200);

    // Deadlines: the tick, polling, and a call failed by a restarting or lost peer.
    let clock = TestClock::leak(1_000, 1_700_000_000_000);
    let bus = InMemoryBus::new(16, 4096);
    let a = handoff_on(bus, 1, clock);
    let b = handoff_on(bus, 2, clock);
    let (client, server) = rpc_side(&a, &b, &[], Duration::from_millis(5));
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let client = client.activate(&tx).unwrap();
    let server = server.activate(&rx).unwrap();
    let peer = |incarnation| PeerRef {
        peer: 2,
        incarnation,
    };
    results.window("rpc_deadline_restart_and_lost_peer_200", || {
        for round in 0..200_u64 {
            // Answered once, so the peer's incarnation is known.
            let mut call = client.invoke(&door(1, 0)).unwrap();
            let request = server.try_recv().unwrap().expect("a request");
            request.respond(&door(2, 0)).unwrap();
            assert!(call.try_response().unwrap().is_some());
            drop(call);

            let mut by_tick = client.invoke(&door(1, 0)).unwrap();
            let _ = server.try_recv().unwrap();
            clock.advance_ns(10_000_000);
            assert_eq!(tx.service(), 1);
            assert!(matches!(by_tick.try_response(), Err(CallError::TimedOut)));
            drop(by_tick);

            let mut by_poll = client.invoke(&door(1, 0)).unwrap();
            let _ = server.try_recv().unwrap();
            clock.advance_ns(10_000_000);
            assert!(matches!(by_poll.try_response(), Err(CallError::TimedOut)));
            drop(by_poll);

            let mut restarted = client.invoke(&door(1, 0)).unwrap();
            let _ = server.try_recv().unwrap();
            let incarnation = bus.restart(2);
            bus.report_peer_restarted(1, peer(incarnation));
            assert!(matches!(
                restarted.try_response(),
                Err(CallError::PeerRestarted)
            ));
            drop(restarted);

            let mut lost = client.invoke(&door(1, 0)).unwrap();
            let _ = server.try_recv().unwrap();
            bus.report_peer_lost(1, peer(incarnation));
            assert!(matches!(lost.try_response(), Err(CallError::PeerLost)));
            drop(lost);
            let _ = round;
        }
    });

    // A late response, after its call was dropped.
    let bus = InMemoryBus::new(16, 4096);
    let a = handoff(bus, 1);
    let b = handoff(bus, 2);
    let (client, server) = rpc_side(&a, &b, &[], Duration::from_secs(30));
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let client = client.activate(&tx).unwrap();
    let server = server.activate(&rx).unwrap();
    results.window("a_late_response_matches_no_call_200", || {
        for id in 0..200 {
            let call = client.invoke(&door(id, 0)).unwrap();
            let request = server.try_recv().unwrap().expect("a request");
            drop(call);
            request.respond(&door(id, 0)).unwrap();
            let _ = tx.pump();
        }
    });
}

#[derive(Clone, Copy, Debug, Default, up_rust::StablePayload, up_rust::StablePayloadInit)]
#[repr(C)]
#[stable_payload(type_name = "uprotocol.test.NativeDoor")]
struct NativeDoor {
    id: u32,
    open: u32,
}

fn native(results: &mut Results) {
    let profile = NativeProfile::new(
        "test",
        1,
        PayloadEncoding::private_use(0xF777),
        [NativeDoor::native_representation()],
    )
    .unwrap();
    let agreement = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
    let agreement = native_agreement_table(&agreement);
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let publisher = Publisher::<NativeDoor>::on(&a, &entity("door", 1), 0x8001)
        .wire(NativeWire::new(agreement))
        .build()
        .unwrap();
    let subscriber = Subscriber::<NativeDoor>::on(&b, topic())
        .wire(NativeWire::new(agreement))
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let publisher: Publisher<NativeDoor, InPlace> = publisher.activate(&tx).unwrap();
    let subscriber: Subscriber<NativeDoor, InPlace> = subscriber.activate(&rx).unwrap();
    results.window(
        "serialized_and_native_traffic_touch_no_heap_native_1000",
        || {
            for id in 0..ROUNDS {
                publisher
                    .publish_in_place(|init| init.into_initializer().id(id).open(1).finish())
                    .unwrap();
                let lease = subscriber.try_recv().unwrap().expect("a frame");
                assert_eq!(lease.payload().unwrap().id, id);
            }
        },
    );
    results.window("native_held_lease_and_refusal", || {
        publisher
            .publish_in_place(|init| init.into_initializer().id(1).open(1).finish())
            .unwrap();
        let first = subscriber.try_recv().unwrap().expect("a frame");
        publisher
            .publish_in_place(|init| init.into_initializer().id(2).open(1).finish())
            .unwrap();
        let second = subscriber.try_recv().unwrap().expect("a frame");
        assert_eq!(first.payload().unwrap().id, 1);
        assert_eq!(second.payload().unwrap().id, 2);
        drop(first);
        drop(second);
    });

    // RPC in place: both messages are built in loans, and a response stays in
    // transport storage until its call drops.
    let bus = LendingBus::new();
    let a = lending(bus, 1);
    let b = lending(bus, 2);
    let method = entity("door", 2).get_resource_uri(METHOD);
    let known = entity("door", 1).get_source_uri();
    let client = RpcClient::<NativeDoor, NativeDoor>::on(&a, &entity("door", 1), method)
        .wire(NativeWire::new(agreement))
        .ttl(Duration::from_secs(30))
        .build()
        .unwrap();
    let server = RpcServer::<NativeDoor, NativeDoor>::on(&b, &entity("door", 2), METHOD)
        .wire(NativeWire::new(agreement))
        .clients(std::slice::from_ref(&known))
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let client: RpcClient<NativeDoor, NativeDoor, InPlace> = client.activate(&tx).unwrap();
    let server: RpcServer<NativeDoor, NativeDoor, InPlace> = server.activate(&rx).unwrap();
    results.window("native_rpc_in_place_round_trip_1000", || {
        for id in 0..ROUNDS {
            let mut call = client
                .invoke_in_place(|init| init.into_initializer().id(id).open(1).finish())
                .unwrap();
            let request = server.try_recv().unwrap().expect("a request");
            let asked = request.payload().unwrap().id;
            request
                .respond_in_place(|init| init.into_initializer().id(asked + 1).open(0).finish())
                .unwrap();
            let reply = call.try_response().unwrap().expect("a response");
            assert_eq!(reply.payload().unwrap().id, id + 1);
        }
    });
    results.window("native_rpc_failure_status_and_late_response_200", || {
        for id in 0..200_u32 {
            let mut call = client
                .invoke_in_place(|init| init.into_initializer().id(id).open(1).finish())
                .unwrap();
            let request = server.try_recv().unwrap().expect("a request");
            if id % 2 == 0 {
                request.respond_failure(UCode::Internal).unwrap();
                assert!(matches!(
                    call.try_response(),
                    Err(CallError::Rejected(UCode::Internal))
                ));
            } else {
                drop(call);
                request
                    .respond_in_place(|init| init.into_initializer().id(id).open(0).finish())
                    .unwrap();
                let _ = tx.pump();
            }
        }
    });
}

/// Traffic on all threads at once, inside the window. Workers are created first
/// and start on a flag, so no thread creation lands in the window.
fn concurrency(results: &mut Results) {
    fn hammer(results: &mut Results, label: &str, pair: &Pair) {
        const WORKERS: usize = 2;
        const PER: u32 = 2000;
        let gate = Gate::default();
        let producers_done = AtomicUsize::new(0);
        let consumed = AtomicU32::new(0);
        let consumer_done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let _abandon = gate.abandon_on_unwind();
            let mut handles = Vec::with_capacity(WORKERS + 1);
            for worker in 0..WORKERS {
                let publisher = pair.publisher;
                let (gate, producers_done) = (&gate, &producers_done);
                handles.push(scope.spawn(move || {
                    let _exit = gate.enter();
                    for id in 0..PER {
                        while publisher
                            .publish(&door(worker as u32 * 10_000 + id, 0))
                            .is_err()
                        {
                            std::hint::spin_loop();
                        }
                    }
                    producers_done.fetch_add(1, Ordering::SeqCst);
                }));
            }
            {
                let subscriber = pair.subscriber;
                let (gate, producers_done, consumed, consumer_done) =
                    (&gate, &producers_done, &consumed, &consumer_done);
                handles.push(scope.spawn(move || {
                    let _exit = gate.enter();
                    loop {
                        let producing = producers_done.load(Ordering::SeqCst) < WORKERS;
                        match subscriber.try_recv() {
                            Ok(Some(_)) => {
                                consumed.fetch_add(1, Ordering::SeqCst);
                            }
                            _ if !producing => break,
                            _ => std::hint::spin_loop(),
                        }
                    }
                    consumer_done.store(true, Ordering::SeqCst);
                }));
            }
            gate.await_workers(WORKERS + 1);
            results.window(&format!("{label}_all_threads_at_once"), || {
                gate.open();
                while !consumer_done.load(Ordering::SeqCst) {
                    std::hint::spin_loop();
                }
                gate.await_finished(WORKERS + 1);
            });
            gate.release();
            // As in `overflow`: joined, so no worker's last frees land in a later window.
            for handle in handles {
                handle.join().expect("a worker");
            }
        });
        assert!(consumed.load(Ordering::SeqCst) > 0);
    }
    let bus = InMemoryBus::new(64, 4096);
    let handoff_pair = activate(
        handoff(bus, 1),
        handoff(bus, 2),
        Qos::DEFAULT.with_depth(32),
    );
    hammer(results, "handoff", &handoff_pair);
    let bus = LendingBus::new();
    let lending_pair = activate(
        lending_with(
            bus,
            1,
            LendingOptions {
                slots: 64,
                ..LendingOptions::default()
            },
        ),
        lending(bus, 2),
        Qos::DEFAULT.with_depth(32),
    );
    hammer(results, "lending", &lending_pair);
}

/// The wait path, on the real platform: a bounded wait that runs out, and one a
/// frame ends.
fn waiting(results: &mut Results) {
    let bus = InMemoryBus::new(8, 4096);
    let pair = activate(handoff(bus, 1), handoff(bus, 2), Qos::DEFAULT);
    results.window("wait_runs_out_of_time", || {
        let token = pair.rx.wait_token();
        let outcome = pair.rx.wait(token, Duration::from_millis(1));
        assert_eq!(outcome, SignalWait::TimedOut);
    });
    let woken = OnceLock::new();
    let gate = Gate::default();
    std::thread::scope(|scope| {
        let publisher = pair.publisher;
        let (woken, gate) = (&woken, &gate);
        let _abandon = gate.abandon_on_unwind();
        let worker = scope.spawn(move || {
            let _exit = gate.enter();
            // The handoff bus does not push, but it signals the route readable
            // when the frame lands: that is the notify the waiter wakes on.
            publisher.publish(&door(1, 0)).unwrap();
            let _ = woken.set(());
        });
        gate.await_workers(1);
        results.window("wait_ends_on_notify", || {
            let token = pair.rx.wait_token();
            gate.open();
            let outcome = pair.rx.wait(token, Duration::from_secs(5));
            assert_eq!(outcome, SignalWait::Notified);
            gate.await_finished(1);
        });
        gate.release();
        // As in `overflow`: joined, so its last frees land in no later window.
        worker.join().expect("the worker");
    });
    assert!(woken.get().is_some());
}

/// The harness's own check, outside every window: a window that fails, or a
/// worker that panics, inside a gate's scope fails the run instead of hanging it.
fn a_failure_inside_a_gate_ends_its_scope() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let gate = Gate::default();
    let window_failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::thread::scope(|scope| {
            let _abandon = gate.abandon_on_unwind();
            let gate = &gate;
            scope.spawn(move || {
                let _exit = gate.enter();
            });
            gate.await_workers(1);
            // A window failing before it opens the gate.
            std::panic::resume_unwind(Box::new("a failing window"));
        });
    }));
    let gate = Gate::default();
    let worker_failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::thread::scope(|scope| {
            let _abandon = gate.abandon_on_unwind();
            let gate = &gate;
            scope.spawn(move || {
                let _exit = gate.enter();
                std::panic::resume_unwind(Box::new("a failing worker"));
            });
            gate.await_workers(1);
            gate.open();
            gate.await_finished(1);
            gate.release();
        });
    }));
    std::panic::set_hook(hook);
    assert!(
        window_failed.is_err() && worker_failed.is_err(),
        "both failures reach the caller"
    );
}

fn main() {
    a_failure_inside_a_gate_ends_its_scope();
    // Parking a guarded thread stack here would allocate in a window; everything
    // runs on this thread, or on workers created before their window.
    let mut results = Results {
        lines: Vec::with_capacity(64),
        c_level: std::env::var_os("UP_TWO_MODE_C_LEVEL").is_some(),
    };
    controls(&mut results);
    activation(&mut results);
    traffic(&mut results);
    overflow(&mut results);
    time_and_peers(&mut results);
    rpc(&mut results);
    native(&mut results);
    waiting(&mut results);
    concurrency(&mut results);
    for line in &results.lines {
        println!("{line}");
    }
    let windows = results
        .lines
        .iter()
        .filter(|line| line.starts_with("window="))
        .count();
    println!("two_mode_no_heap: all {windows} windows clean, plus the controls");
}
