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

//! Contract suites and in-memory bindings for the two-mode transports (R33).
//!
//! There is no standalone conformance kit: every binding author calls these
//! contracts from their own tests. Everything here is `test-util` only.

pub mod carrier;
#[cfg(up_rust_test_hooks)]
pub mod contract;
pub mod forwarder;
pub mod handoff;
pub mod lending;
pub mod wires;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[cfg(up_rust_test_hooks)]
use up_rust_core::frame::header::{HeaderLimits, RxHeader, TxHeaderParts, UFrameHeaderError};
use up_rust_core::platform::{Monotonic, Platform, PlatformError, PlatformInit};
#[cfg(up_rust_test_hooks)]
use up_rust_core::transport::binding::PayloadBytes;
#[cfg(up_rust_test_hooks)]
use up_rust_core::transport::envelope::{
    __admit_for_tests, __encode_head_for_tests, __parse_for_tests, AdmittedWire, Envelope,
};
#[cfg(up_rust_test_hooks)]
use up_rust_core::wire::{WireIdentity, WirePayloadIdentity};

/// A controllable clock for deadline and TTL tests.
///
/// `Monotonic` advances only when a test moves it; the wall clock starts at
/// `wall_ms` and can be made to fail, to prove the fail-closed paths.
#[derive(Debug)]
pub struct TestClock {
    monotonic: AtomicU64,
    wall_ms: AtomicU64,
    wall_available: AtomicBool,
}

impl TestClock {
    /// A clock frozen at the given times.
    #[must_use]
    pub const fn new(monotonic_ns: u64, wall_ms: u64) -> Self {
        Self {
            monotonic: AtomicU64::new(monotonic_ns),
            wall_ms: AtomicU64::new(wall_ms),
            wall_available: AtomicBool::new(true),
        }
    }

    /// Leaks a clock so it can serve as `&'static dyn PlatformInit`.
    #[must_use]
    pub fn leak(monotonic_ns: u64, wall_ms: u64) -> &'static Self {
        Box::leak(Box::new(Self::new(monotonic_ns, wall_ms)))
    }

    /// Moves the monotonic clock forward.
    pub fn advance_ns(&self, nanos: u64) {
        self.monotonic.fetch_add(nanos, Ordering::SeqCst);
    }

    /// Moves the wall clock forward.
    pub fn advance_wall_ms(&self, millis: u64) {
        self.wall_ms.fetch_add(millis, Ordering::SeqCst);
    }

    /// Makes `unix_time_ms` fail, as a broken RTC would.
    pub fn break_wall_clock(&self) {
        self.wall_available.store(false, Ordering::SeqCst);
    }

    /// Restores the wall clock.
    pub fn repair_wall_clock(&self) {
        self.wall_available.store(true, Ordering::SeqCst);
    }
}

impl Platform for TestClock {
    fn now(&self) -> Monotonic {
        Monotonic(self.monotonic.load(Ordering::SeqCst))
    }

    fn unix_time_ms(&self) -> Option<u64> {
        self.wall_available
            .load(Ordering::SeqCst)
            .then(|| self.wall_ms.load(Ordering::SeqCst))
    }

    fn wait(
        &self,
        word: &core::sync::atomic::AtomicU32,
        expected: u32,
        deadline: Option<Monotonic>,
    ) -> up_rust_core::platform::WaitResult {
        // Tests never block on a frozen clock: a signal wait returns at once.
        let _ = (word, expected, deadline);
        up_rust_core::platform::WaitResult::TimedOut
    }

    fn wake_one(&self, _word: &core::sync::atomic::AtomicU32) {}

    fn wake_all(&self, _word: &core::sync::atomic::AtomicU32) {}
}

impl PlatformInit for TestClock {
    fn entropy(&self, out: &mut [u8]) -> Result<(), PlatformError> {
        out.fill(0x5A);
        Ok(())
    }

    unsafe fn prefault(&self, _ptr: *mut u8, _len: usize) -> Result<(), PlatformError> {
        Ok(())
    }

    fn runtime(&'static self) -> &'static dyn Platform {
        self
    }
}

/// One envelope with one admitted wire, for byte-level differential tests.
#[derive(Debug, Clone, Copy)]
#[cfg(up_rust_test_hooks)]
pub struct EnvelopeHarness {
    envelope: Envelope,
    wire: &'static AdmittedWire,
}

#[cfg(up_rust_test_hooks)]
impl EnvelopeHarness {
    /// Admits `wire_id` and `payload_family` for `envelope`.
    ///
    /// # Panics
    /// If the identity cannot be admitted; this is test setup.
    #[must_use]
    pub fn new(
        envelope: Envelope,
        wire_id: WireIdentity,
        payload_family: WireIdentity,
        identity: WirePayloadIdentity,
    ) -> Self {
        let memory = up_rust_core::memory::StartupMemory::from_source(
            &crate::startup::LEAKED_HEAP_REGIONS,
            16 * 1024,
        )
        .expect("test memory");
        let wire = __admit_for_tests(&memory, envelope, wire_id, payload_family, identity)
            .expect("the wire is admitted");
        Self { envelope, wire }
    }

    /// Writes a frame's head and appends the payload. Returns the head's length and all bytes.
    ///
    /// # Errors
    /// As the envelope's writer.
    pub fn encode(
        &self,
        parts: &TxHeaderParts<'_>,
        payload: Option<&[u8]>,
    ) -> Result<(usize, Vec<u8>), UFrameHeaderError> {
        let mut bytes = vec![0_u8; 32 * 1024];
        let head = __encode_head_for_tests(
            self.envelope,
            self.wire,
            parts,
            payload.map(<[u8]>::len),
            &mut bytes,
        )?;
        bytes.truncate(head);
        bytes.extend_from_slice(payload.unwrap_or_default());
        Ok((head, bytes))
    }

    /// Parses a frame.
    ///
    /// # Errors
    /// As the envelope's parser.
    pub fn parse<'a>(
        &self,
        metadata: &'a [u8],
        payload: PayloadBytes<'a>,
        limits: &HeaderLimits,
    ) -> Result<(RxHeader<'a>, PayloadBytes<'a>), UFrameHeaderError> {
        __parse_for_tests(self.envelope, self.wire, metadata, payload, limits)
    }
}
