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

//! Contract suites any wire or binding runs from its own tests (R33).
//!
//! The test binary must install [`crate::heap_guard::HeapGuard`] as its global
//! allocator; the heap checks fail loudly if it is missing. There is no
//! standalone conformance kit.
//! Run contract lanes with `RUSTFLAGS="--cfg up_rust_test_hooks"`.
//! The mocks remain available without this build-wide test-only cfg.

use std::fmt::Debug;
use std::mem::MaybeUninit;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

use up_rust_core::memory::StartupMemory;
use up_rust_core::transport::qos::{Overflow, Qos, QosRefused, QosSetting, QosSupport, SendReport};
use up_rust_core::transport::{RouteCounters, SendError};
use up_rust_core::uattributes::UPriority;
use up_rust_core::wire::{EncodedSize, PayloadWriter, RuntimeHeap, Wire};

use crate::heap_guard::{self, HeapReport, Scope};
use crate::startup::LEAKED_HEAP_REGIONS;

/// What [`assert_wire_contract`] observed, for negative controls.
#[derive(Debug)]
pub struct WireContractReport {
    /// Heap events counted across the checked operations.
    pub heap_events: usize,
    /// Bytes written for the sample.
    pub encoded_len: usize,
}

const DEFAULT_CAPACITY: usize = 64 * 1024;

fn window_events(report: &HeapReport) -> usize {
    let counts = report.counts();
    counts
        .allocations
        .saturating_add(counts.deallocations)
        .saturating_add(counts.reallocations)
}

/// Exercises a wire: repeatable encoding, honest `Exact`, decode independent of
/// prior contents, malformed input refused with `Err` and no panic, and, for a
/// wire declaring [`RuntimeHeap::None`], no heap events.
///
/// `fill_sample` writes the sample value; `fill_other` writes a value different
/// from the sample. Both run over slots, so this works for very large types.
///
/// # Panics
/// If the wire breaks a rule. This is a test assertion, not a runtime path.
pub fn assert_wire_contract<T, W>(
    wire: &W,
    fill_sample: impl Fn(&mut T),
    fill_other: impl Fn(&mut T),
) -> WireContractReport
where
    T: PartialEq + Debug + 'static,
    W: Wire<T>,
{
    let info = wire.describe();
    let encoding = info.identity.encoding;
    let capacity = info.max_encoded_len.unwrap_or(DEFAULT_CAPACITY);
    let memory =
        StartupMemory::from_source(&LEAKED_HEAP_REGIONS, capacity.saturating_mul(2).max(4096))
            .expect("startup memory for the wire contract");
    let values = crate::two_mode::startup_values::<T, W>(&memory, wire, 3)
        .expect("startup slots for the wire contract");
    let [sample, other, into] = values else {
        panic!("three contract slots");
    };
    fill_sample(sample);
    fill_other(other);
    assert_ne!(*sample, *other, "fill_other must differ from fill_sample");
    let mut first = vec![MaybeUninit::<u8>::uninit(); capacity];
    let mut second = vec![MaybeUninit::<u8>::uninit(); capacity];
    let describe_events = {
        let ((), report) = heap_guard::observe(Scope::AllThreads, || {
            let _ = wire.describe();
        });
        window_events(&report)
    };
    let mut encoded_events = 0_usize;
    let first_len = {
        let (result, report) = heap_guard::observe(Scope::AllThreads, || {
            let size = wire.encoded_size(sample);
            let mut writer = PayloadWriter::new(&mut first);
            let outcome = wire.encode(sample, &mut writer);
            let written = writer.written();
            (size, outcome, written)
        });
        encoded_events = encoded_events.saturating_add(window_events(&report));
        let (size, outcome, written) = result;
        outcome.expect("the wire encodes its sample");
        if let EncodedSize::Exact(len) = size {
            assert_eq!(written, len, "EncodedSize::Exact must be honest");
        }
        written
    };
    let (second_len, report) = heap_guard::observe(Scope::AllThreads, || {
        let mut writer = PayloadWriter::new(&mut second);
        wire.encode(sample, &mut writer)
            .expect("the wire encodes its sample again");
        writer.written()
    });
    encoded_events = encoded_events.saturating_add(window_events(&report));
    assert_eq!(first_len, second_len, "encoding must be repeatable");
    // SAFETY: `encode` initialized the first `len` bytes of each buffer.
    let (first_bytes, second_bytes) = unsafe {
        (
            std::slice::from_raw_parts(first.as_ptr().cast::<u8>(), first_len),
            std::slice::from_raw_parts(second.as_ptr().cast::<u8>(), second_len),
        )
    };
    assert_eq!(first_bytes, second_bytes, "encoding must be repeatable");
    // `into` holds the other value: decode must not depend on prior contents.
    fill_other(into);
    let (decoded, report) = heap_guard::observe(Scope::AllThreads, || {
        wire.decode(encoding, first_bytes, into)
    });
    let decode_events = window_events(&report);
    decoded.expect("the wire decodes its own encoding");
    assert_eq!(
        *into, *sample,
        "decode must not depend on prior slot contents"
    );
    let mut malformed = 0_usize;
    if !first_bytes.is_empty() {
        let truncated = first_bytes.get(..first_bytes.len() - 1).unwrap_or_default();
        let (refused, report) = heap_guard::observe(Scope::AllThreads, || {
            catch_unwind(AssertUnwindSafe(|| wire.decode(encoding, truncated, into)))
        });
        malformed = malformed.saturating_add(window_events(&report));
        let refused = refused.expect("decode must not panic on a truncated frame");
        assert!(refused.is_err(), "a truncated frame must be refused");
    }
    let mut oversized = first_bytes.to_vec();
    oversized.extend_from_slice(&[0_u8; 8]);
    let (refused, report) = heap_guard::observe(Scope::AllThreads, || {
        catch_unwind(AssertUnwindSafe(|| wire.decode(encoding, &oversized, into)))
    });
    malformed = malformed.saturating_add(window_events(&report));
    let refused = refused.expect("decode must not panic on an oversized frame");
    assert!(refused.is_err(), "an oversized frame must be refused");
    if !first_bytes.is_empty() {
        let (refused, report) = heap_guard::observe(Scope::AllThreads, || {
            catch_unwind(AssertUnwindSafe(|| wire.decode(encoding, &[], into)))
        });
        malformed = malformed.saturating_add(window_events(&report));
        let refused = refused.expect("decode must not panic on an empty frame");
        assert!(refused.is_err(), "an empty frame must be refused");
    }
    let encode_claims_none = matches!(info.encode_heap(), RuntimeHeap::None);
    let decode_claims_none = matches!(info.decode_heap(), RuntimeHeap::None);
    if encode_claims_none {
        assert_eq!(
            describe_events + encoded_events,
            0,
            "a heap-free encode touched the heap"
        );
    }
    if decode_claims_none {
        assert_eq!(
            decode_events + malformed,
            0,
            "a heap-free decode touched the heap"
        );
    }
    WireContractReport {
        heap_events: describe_events + encoded_events + decode_events + malformed,
        encoded_len: first_len,
    }
}

/// What a binding contract fixture supplies.
pub trait BindingFixture {
    /// Name, for failure messages.
    fn name(&self) -> &'static str;

    /// Enters runtime and makes the fixture's routes ready for traffic.
    fn start(&mut self);

    /// Publishes one value. Returns false if the route refuses.
    fn publish(&mut self, value: u32) -> bool;

    /// Receives one value, if one is ready; the lease is dropped by the fixture.
    fn receive(&mut self) -> Option<u32>;

    /// Delivers one malformed frame through the binding's receive path.
    fn deliver_malformed(&mut self);

    /// Restarts the publisher peer; returns (`id`, `incarnation`) before and after.
    fn restart_peer(&mut self) -> ((u64, u64), (u64, u64));

    /// Checks truncation honesty if the binding declares it; `None` otherwise.
    fn check_truncation(&mut self) -> Option<bool> {
        None
    }

    /// The binding's QoS capability for ordinary routes.
    fn qos_support(&self) -> QosSupport;

    /// Whether a route with this QoS is admitted; `Err` carries the refusal.
    fn try_route(&mut self, qos: Qos) -> Result<(), QosRefused>;
}

/// Runs the binding contract (design 9.2). `HeapGuard` must be the allocator.
///
/// # Panics
/// If the binding breaks a rule.
pub fn assert_binding_contract<F: BindingFixture>(fixture: &mut F) {
    let name = fixture.name();
    fixture.start();
    // Lazy state forced by the time `enter_runtime` returns: first traffic is clean.
    let (published, report) = heap_guard::observe(Scope::AllThreads, || fixture.publish(1));
    assert!(published, "{name}: the first publish must be accepted");
    assert_eq!(
        window_events(&report),
        0,
        "{name}: first send touched the heap: {report:?}"
    );
    let (received, report) = heap_guard::observe(Scope::AllThreads, || fixture.receive());
    assert_eq!(
        received,
        Some(1),
        "{name}: the first receive must see the frame"
    );
    assert_eq!(
        window_events(&report),
        0,
        "{name}: first receive touched the heap"
    );
    // Steady state, including the Frame drop inside the window.
    let (ok, report) = heap_guard::observe(Scope::AllThreads, || {
        (2..200_u32).all(|value| fixture.publish(value) && fixture.receive() == Some(value))
    });
    assert!(ok, "{name}: steady traffic must work");
    assert_eq!(
        window_events(&report),
        0,
        "{name}: steady traffic (including frame drops) touched the heap: {report:?}"
    );
    // A malformed peer frame neither panics nor allocates.
    let ((), report) = heap_guard::observe(Scope::AllThreads, || fixture.deliver_malformed());
    assert_eq!(
        window_events(&report),
        0,
        "{name}: malformed input touched the heap"
    );
    assert_eq!(
        fixture.receive(),
        None,
        "{name}: malformed input was not refused"
    );
    // Restart: same peer id, higher incarnation.
    let ((id_before, inc_before), (id_after, inc_after)) = fixture.restart_peer();
    assert_eq!(
        id_before, id_after,
        "{name}: a peer id must be stable across restarts"
    );
    assert!(
        inc_after > inc_before,
        "{name}: incarnation must rise across a restart"
    );
    assert!(inc_before >= 1, "{name}: incarnation must start at 1");
    if let Some(honest) = fixture.check_truncation() {
        assert!(honest, "{name}: truncation was not honest");
    }
    // QoS capability honored or refused.
    let support = fixture.qos_support();
    let refused = fixture.try_route(Qos::DEFAULT.with_depth(0));
    assert!(
        matches!(
            refused,
            Err(QosRefused {
                setting: QosSetting::Depth,
                ..
            })
        ),
        "{name}: zero depth must be refused"
    );
    let wait = Qos::DEFAULT.with_overflow(Overflow::Wait {
        timeout: Duration::from_millis(5),
    });
    if support.wait {
        assert!(
            fixture.try_route(wait).is_ok(),
            "{name}: declared Wait must be admitted"
        );
    } else {
        assert!(
            matches!(
                fixture.try_route(wait),
                Err(QosRefused {
                    setting: QosSetting::Overflow,
                    ..
                })
            ),
            "{name}: undeclared Wait must be refused"
        );
    }
}

/// One route on the binding under test, with one receiver attached (`Main`) and
/// a second receiver that can join late.
pub trait ContractRoute {
    /// Publishes a value through the runtime path.
    ///
    /// # Errors
    /// The route's refusal.
    fn publish(&mut self, value: u32) -> Result<SendReport, SendError>;
    /// Pulls one value from the main receiver, or `None` if none is queued.
    fn receive(&mut self) -> Option<u32>;
    /// Pulls one value from the late receiver, or `None` if none is queued.
    fn receive_late(&mut self) -> Option<u32>;
    /// Attaches the late receiver, then lets the sender notice it.
    fn join_late(&mut self);
    /// Kills the main receiver abruptly, as a process crash would (R38, R40).
    fn kill_main_receiver(&mut self);
    /// Drains the main receiver from another thread after `delay`.
    fn drain_main_after(&mut self, delay: Duration);
    /// The main receiver's route counters, so the contract can check that each
    /// overflow discard is counted on the route, wherever the queue is. `None`, the
    /// default, fails those checks unless the binding declares
    /// [`QosUnderTest::route_counts_drops`] false.
    fn counters(&self) -> Option<RouteCounters> {
        None
    }
}

/// A binding under the QoS contract.
pub trait QosUnderTest {
    /// A route on this binding.
    type Route: ContractRoute;
    /// What the binding claims it can honor.
    fn support(&self) -> QosSupport;
    /// Builds the route. Must return `Err(QosRefused)` for anything unclaimed.
    ///
    /// # Errors
    /// The refusal for an unclaimed capability.
    fn open(&mut self, resource: u16, qos: &Qos) -> Result<Self::Route, QosRefused>;
    /// Whether a send reports the drops at its receivers, in `SendReport::dropped_full`.
    ///
    /// True for a binding whose queue is shared with the sender, such as a lending
    /// transport. A handoff binding that drops at the receiver's inbox cannot know,
    /// and counts the drops there (`dropped_newest`).
    fn sender_learns_of_drops(&self) -> bool {
        true
    }
    /// Whether every overflow discard reaches the receiving route's counters, as
    /// the SDK counts its own queue: an evicted frame in `carried` and
    /// `dropped_oldest`, a refused one in `dropped_newest`. A pull binding's queue
    /// is the route's queue, so it reports each discard it makes with
    /// `RuntimePort::dropped`.
    ///
    /// True by default. A binding whose middleware discards without telling it
    /// returns false; the route's counters then miss those drops, and the binding's
    /// documentation must say so.
    fn route_counts_drops(&self) -> bool {
        true
    }
}

/// Why a drop check could not read the route's counters.
const NO_COUNTERS: &str = "the route's counters are needed to check that each discard is \
     counted: implement `ContractRoute::counters`, or declare `route_counts_drops` false";

/// One contract check's outcome.
#[derive(Debug)]
pub enum Outcome {
    /// The check ran and passed.
    Passed,
    /// The binding does not claim this capability, and refused it correctly.
    RefusedCorrectly,
    /// The check failed.
    Failed(String),
}

impl Outcome {
    /// Whether the check passed or was correctly refused.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        !matches!(self, Self::Failed(_))
    }
}

/// A short, stable name for each check plus its outcome, so a run can be diffed.
pub type Results = Vec<(&'static str, Outcome)>;

const TIMEOUT: Duration = Duration::from_millis(40);
const BURST: u32 = 12;

fn drain<R: ContractRoute>(route: &mut R) -> Vec<u32> {
    let mut seen = Vec::new();
    while let Some(value) = route.receive() {
        seen.push(value);
        if seen.len() > (BURST as usize) * 8 {
            break;
        }
    }
    seen
}

macro_rules! check {
    ($cond:expr, $($arg:tt)+) => {
        if !$cond {
            return Outcome::Failed(format!($($arg)+));
        }
    };
}

/// Runs the whole QoS suite. Failures are collected, not panicked, so a
/// deliberately lying binding can be shown to fail.
pub fn run_qos_contract<B: QosUnderTest>(binding: &mut B) -> Results {
    let support = binding.support();
    vec![
        (
            "defaults-are-honored-everywhere",
            default_route_is_accepted(binding),
        ),
        (
            "zero-depth-refused",
            invalid_setting_refused(binding, Qos::DEFAULT.with_depth(0), QosSetting::Depth),
        ),
        (
            "history-above-depth-refused",
            invalid_setting_refused(
                binding,
                Qos::DEFAULT.with_depth(2).with_history(3),
                QosSetting::History,
            ),
        ),
        (
            "unrepresentable-depth-refused",
            depth_above_limit_refused(binding, support),
        ),
        (
            "drop-oldest-keeps-newest-depth",
            drop_oldest(binding, support),
        ),
        (
            "drop-newest-keeps-oldest-depth",
            drop_newest(binding, support),
        ),
        (
            "wait-times-out-with-an-error",
            wait_times_out(binding, support),
        ),
        (
            "wait-succeeds-once-drained",
            wait_succeeds_after_drain(binding, support),
        ),
        (
            "wait-never-hangs-on-a-dead-peer",
            wait_with_dead_peer(binding, support),
        ),
        (
            "reliable-loses-nothing-within-depth",
            reliable_within_depth(binding, support),
        ),
        (
            "history-replays-to-a-late-joiner",
            history_replay(binding, support),
        ),
        (
            "priority-required-or-refused",
            priority_required(binding, support),
        ),
        (
            "steady-state-touches-no-heap",
            heap_free_runtime(binding, support),
        ),
    ]
}

fn default_route_is_accepted<B: QosUnderTest>(binding: &mut B) -> Outcome {
    match binding.open(0x8100, &Qos::DEFAULT) {
        Ok(_) => Outcome::Passed,
        Err(refused) => Outcome::Failed(format!("the default must be honored: {refused:?}")),
    }
}

fn invalid_setting_refused<B: QosUnderTest>(
    binding: &mut B,
    qos: Qos,
    expected: QosSetting,
) -> Outcome {
    match binding.open(0x8101, &qos) {
        Err(refused) if refused.setting == expected => Outcome::Passed,
        Err(refused) => Outcome::Failed(format!("refused for the wrong reason: {refused:?}")),
        Ok(_) => Outcome::Failed(format!("{qos:?} must be refused, not coerced")),
    }
}

fn depth_above_limit_refused<B: QosUnderTest>(binding: &mut B, support: QosSupport) -> Outcome {
    let too_deep = support.max_depth.saturating_add(1);
    if too_deep == u16::MAX {
        return Outcome::Passed;
    }
    match binding.open(0x8102, &Qos::DEFAULT.with_depth(too_deep)) {
        Err(refused)
            if refused.setting == QosSetting::Depth && refused.requested == u32::from(too_deep) =>
        {
            Outcome::RefusedCorrectly
        }
        Err(refused) => Outcome::Failed(format!("wrong refusal: {refused:?}")),
        Ok(_) => Outcome::Failed("a depth above the binding's limit must be refused".into()),
    }
}

fn expect_refused<B: QosUnderTest>(
    binding: &mut B,
    resource: u16,
    qos: &Qos,
    expected: QosSetting,
) -> Outcome {
    match binding.open(resource, qos) {
        Err(refused) if refused.setting == expected => Outcome::RefusedCorrectly,
        Err(refused) => Outcome::Failed(format!("refused for the wrong setting: {refused:?}")),
        Ok(_) => Outcome::Failed(format!(
            "not claimed, so it must be refused at setup: {qos:?}"
        )),
    }
}

fn drop_oldest<B: QosUnderTest>(binding: &mut B, support: QosSupport) -> Outcome {
    let qos = Qos::DEFAULT
        .with_depth(3)
        .with_overflow(Overflow::DropOldest);
    if !support.drop_oldest {
        return expect_refused(binding, 0x8110, &qos, QosSetting::Overflow);
    }
    let mut route = match binding.open(0x8110, &qos) {
        Ok(route) => route,
        Err(refused) => return Outcome::Failed(format!("claimed but refused: {refused:?}")),
    };
    for value in 1..=BURST {
        if let Err(failure) = route.publish(value) {
            return Outcome::Failed(format!("publish {value} failed: {failure:?}"));
        }
    }
    let seen = drain(&mut route);
    let want: Vec<u32> = (BURST - 2..=BURST).collect();
    check!(
        seen == want,
        "wanted the newest three, in order: {want:?}, got {seen:?}"
    );
    if binding.route_counts_drops() {
        let Some(counters) = route.counters() else {
            return Outcome::Failed(NO_COUNTERS.to_owned());
        };
        check!(
            counters.dropped_oldest == BURST - 3 && counters.carried == BURST,
            "each of the {} evicted frames must be counted on the route, received and \
             dropped: {counters:?}",
            BURST - 3
        );
    }
    Outcome::Passed
}

fn drop_newest<B: QosUnderTest>(binding: &mut B, support: QosSupport) -> Outcome {
    let qos = Qos::DEFAULT
        .with_depth(3)
        .with_overflow(Overflow::DropNewest);
    if !support.drop_newest {
        return expect_refused(binding, 0x8111, &qos, QosSetting::Overflow);
    }
    let mut route = match binding.open(0x8111, &qos) {
        Ok(route) => route,
        Err(refused) => return Outcome::Failed(format!("claimed but refused: {refused:?}")),
    };
    let mut reported_drops = 0_u64;
    for value in 1..=BURST {
        match route.publish(value) {
            Ok(report) => reported_drops += report.dropped_full,
            Err(failure) => return Outcome::Failed(format!("publish {value} failed: {failure:?}")),
        }
    }
    let seen = drain(&mut route);
    check!(
        seen == vec![1, 2, 3],
        "wanted the oldest three, got {seen:?}"
    );
    if binding.sender_learns_of_drops() {
        check!(
            reported_drops == u64::from(BURST - 3),
            "the sender must learn of each drop: reported {reported_drops}"
        );
    }
    if binding.route_counts_drops() {
        let Some(counters) = route.counters() else {
            return Outcome::Failed(NO_COUNTERS.to_owned());
        };
        check!(
            counters.dropped_newest == BURST - 3 && counters.carried == 3,
            "each of the {} refused frames must be counted on the route as dropped, not \
             carried: {counters:?}",
            BURST - 3
        );
    }
    Outcome::Passed
}

fn wait_times_out<B: QosUnderTest>(binding: &mut B, support: QosSupport) -> Outcome {
    let qos = Qos::DEFAULT
        .with_depth(1)
        .with_overflow(Overflow::Wait { timeout: TIMEOUT });
    if !support.wait {
        return expect_refused(binding, 0x8112, &qos, QosSetting::Overflow);
    }
    let mut route = match binding.open(0x8112, &qos) {
        Ok(route) => route,
        Err(refused) => return Outcome::Failed(format!("claimed but refused: {refused:?}")),
    };
    check!(route.publish(1).is_ok(), "the first publish must fit");
    let started = Instant::now();
    let outcome = route.publish(2);
    let elapsed = started.elapsed();
    check!(
        matches!(outcome, Err(SendError::TimedOut)),
        "expected TimedOut, got {outcome:?}"
    );
    check!(
        elapsed >= TIMEOUT,
        "gave up early: {elapsed:?} < {TIMEOUT:?}"
    );
    check!(
        elapsed < TIMEOUT + Duration::from_secs(5),
        "took too long: {elapsed:?}"
    );
    let seen = drain(&mut route);
    check!(
        seen == vec![1],
        "the timed-out message must not be delivered: {seen:?}"
    );
    Outcome::Passed
}

fn wait_succeeds_after_drain<B: QosUnderTest>(binding: &mut B, support: QosSupport) -> Outcome {
    let qos = Qos::DEFAULT.with_depth(1).with_overflow(Overflow::Wait {
        timeout: Duration::from_secs(5),
    });
    if !support.wait {
        return expect_refused(binding, 0x8113, &qos, QosSetting::Overflow);
    }
    let mut route = match binding.open(0x8113, &qos) {
        Ok(route) => route,
        Err(refused) => return Outcome::Failed(format!("claimed but refused: {refused:?}")),
    };
    check!(route.publish(1).is_ok(), "the first publish must fit");
    route.drain_main_after(Duration::from_millis(30));
    let outcome = route.publish(2);
    check!(
        matches!(outcome, Ok(report) if report.delivered == 1),
        "publish must succeed once drained: {outcome:?}"
    );
    Outcome::Passed
}

fn wait_with_dead_peer<B: QosUnderTest>(binding: &mut B, support: QosSupport) -> Outcome {
    let qos = Qos::DEFAULT
        .with_depth(1)
        .with_overflow(Overflow::Wait { timeout: TIMEOUT });
    if !support.wait {
        return expect_refused(binding, 0x8114, &qos, QosSetting::Overflow);
    }
    let mut route = match binding.open(0x8114, &qos) {
        Ok(route) => route,
        Err(refused) => return Outcome::Failed(format!("claimed but refused: {refused:?}")),
    };
    check!(route.publish(1).is_ok(), "the first publish must fit");
    route.kill_main_receiver();
    let started = Instant::now();
    let outcome = route.publish(2);
    let elapsed = started.elapsed();
    // Either outcome is fine; hanging is not, and neither is a panic (R40).
    check!(
        elapsed < TIMEOUT + Duration::from_secs(5),
        "a dead peer must not hang the sender: {elapsed:?}"
    );
    check!(
        matches!(outcome, Ok(_) | Err(SendError::TimedOut)),
        "unexpected failure after peer death: {outcome:?}"
    );
    Outcome::Passed
}

fn reliable_within_depth<B: QosUnderTest>(binding: &mut B, support: QosSupport) -> Outcome {
    let qos = Qos::DEFAULT
        .reliable()
        .with_depth(4)
        .with_overflow(Overflow::Wait {
            timeout: Duration::from_secs(5),
        });
    if !support.reliable {
        return expect_refused(binding, 0x8115, &qos, QosSetting::Reliability);
    }
    if !support.wait {
        return expect_refused(binding, 0x8115, &qos, QosSetting::Overflow);
    }
    let mut route = match binding.open(0x8115, &qos) {
        Ok(route) => route,
        Err(refused) => return Outcome::Failed(format!("claimed but refused: {refused:?}")),
    };
    for value in 1..=4 {
        check!(
            route.publish(value).is_ok(),
            "publish {value} within depth must succeed"
        );
    }
    let seen = drain(&mut route);
    check!(
        seen == vec![1, 2, 3, 4],
        "reliable delivery lost or reordered: {seen:?}"
    );
    Outcome::Passed
}

fn history_replay<B: QosUnderTest>(binding: &mut B, support: QosSupport) -> Outcome {
    let qos = Qos::DEFAULT.with_depth(4).with_history(3);
    if support.max_history < 3 {
        return expect_refused(binding, 0x8116, &qos, QosSetting::History);
    }
    let mut route = match binding.open(0x8116, &qos) {
        Ok(route) => route,
        Err(refused) => return Outcome::Failed(format!("claimed but refused: {refused:?}")),
    };
    for value in 1..=8 {
        if let Err(failure) = route.publish(value) {
            return Outcome::Failed(format!("publish {value} failed: {failure:?}"));
        }
    }
    route.join_late();
    let mut replayed = Vec::new();
    while let Some(value) = route.receive_late() {
        replayed.push(value);
        if replayed.len() > 16 {
            break;
        }
    }
    check!(
        replayed == vec![6, 7, 8],
        "a late joiner must get the last three, oldest first: {replayed:?}"
    );
    Outcome::Passed
}

fn priority_required<B: QosUnderTest>(binding: &mut B, support: QosSupport) -> Outcome {
    let qos = Qos::DEFAULT.with_priority(UPriority::CS5);
    if support.priority_levels == 0 {
        return expect_refused(binding, 0x8117, &qos, QosSetting::Priority);
    }
    match binding.open(0x8117, &qos) {
        Ok(_) => Outcome::Passed,
        Err(refused) => Outcome::Failed(format!("claimed priority but refused: {refused:?}")),
    }
}

fn heap_free_runtime<B: QosUnderTest>(binding: &mut B, support: QosSupport) -> Outcome {
    let mut policies = Vec::new();
    if support.drop_oldest {
        policies.push(Overflow::DropOldest);
    }
    if support.drop_newest {
        policies.push(Overflow::DropNewest);
    }
    if policies.is_empty() {
        return Outcome::RefusedCorrectly;
    }
    for (index, overflow) in policies.into_iter().enumerate() {
        let qos = Qos::DEFAULT.with_depth(2).with_overflow(overflow);
        let mut route = match binding.open(0x8120 + u16::try_from(index).unwrap_or(0), &qos) {
            Ok(route) => route,
            Err(refused) => return Outcome::Failed(format!("claimed but refused: {refused:?}")),
        };
        // Warm up: first use of any path may be lazy, and that is what setup should force.
        for value in 0..64 {
            let _ = route.publish(value);
            let _ = route.receive();
        }
        let (ok, report) = heap_guard::observe(Scope::AllThreads, || {
            for value in 0..512 {
                if route.publish(value).is_err() {
                    return false;
                }
                let _ = route.receive();
            }
            true
        });
        check!(ok, "a steady-state publish failed");
        check!(
            window_events(&report) == 0,
            "{overflow:?}: steady state must not touch the heap, saw {} events",
            window_events(&report)
        );
    }
    Outcome::Passed
}
