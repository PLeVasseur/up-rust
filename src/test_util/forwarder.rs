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

//! A test-only forwarder over the SDK's carrier primitives. Not for production.
//!
//! The real loop lives in `up-streamer` (Stage 7). This one is here so the
//! carrier contract and heap windows can drive the primitives the way a
//! Streamer worker will: check capacity before taking, take, send, and park
//! the frame that lost the race. No worker ever blocks: a frame that cannot go
//! out now is parked or dropped, and the worker moves on.

use std::fmt;

use up_rust_core::transport::{
    CarriedFrame, EgressRoute, Flow, IngressRoute, LoanError, ParkedFrame, ReceiveError, SendError,
    SendReport, TransportRuntime,
};

/// What one [`TestForwarder`] has done so far.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ForwarderStats {
    /// Frames sent on the egress.
    pub forwarded: u64,
    /// Frames lost because the egress refused them and they could not be parked.
    pub dropped: u64,
    /// Parked frames dropped because their time to live ran out while parked.
    pub expired_parked: u64,
    /// Times a forwarding step found the egress without capacity.
    pub no_capacity: u64,
}

/// One carrier route's forwarder: the loop a Streamer worker runs per route.
pub struct TestForwarder {
    ingress_runtime: TransportRuntime,
    ingress: IngressRoute,
    egress_runtime: TransportRuntime,
    egress: EgressRoute,
    parked: Option<ParkedFrame>,
    stats: ForwarderStats,
    last_report: Option<SendReport>,
}

impl fmt::Debug for TestForwarder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TestForwarder")
            .field("stats", &self.stats)
            .field("parked", &self.parked.is_some())
            .finish_non_exhaustive()
    }
}

impl TestForwarder {
    /// A forwarder from `ingress` to `egress`, on their runtimes.
    #[must_use]
    pub fn new(
        ingress_runtime: TransportRuntime,
        ingress: IngressRoute,
        egress_runtime: TransportRuntime,
        egress: EgressRoute,
    ) -> Self {
        Self {
            ingress_runtime,
            ingress,
            egress_runtime,
            egress,
            parked: None,
            stats: ForwarderStats::default(),
            last_report: None,
        }
    }

    /// What this forwarder has done so far.
    #[must_use]
    pub fn stats(&self) -> ForwarderStats {
        self.stats
    }

    /// What the egress reported about the last frame it was given.
    #[must_use]
    pub fn last_report(&self) -> Option<SendReport> {
        self.last_report
    }

    /// Whether a frame is parked, waiting for egress capacity.
    #[must_use]
    pub fn has_parked(&self) -> bool {
        self.parked.is_some()
    }

    /// Resends a parked frame. Returns `(may_continue, sent)`.
    fn resend_parked(&mut self) -> (bool, usize) {
        let Some(parked) = &self.parked else {
            return (true, 0);
        };
        if !self.egress_runtime.has_capacity(self.egress) {
            self.stats.no_capacity += 1;
            return (false, 0);
        }
        match self.egress_runtime.send(self.egress, &parked.frame()) {
            Ok(report) => {
                self.last_report = Some(report);
                self.stats.forwarded += 1;
                self.parked = None;
                (true, 1)
            }
            Err(SendError::Expired | SendError::ClockUnavailable) => {
                self.stats.expired_parked += 1;
                self.parked = None;
                (true, 0)
            }
            Err(_) => (false, 0),
        }
    }

    /// Runs one forwarding step and returns how many frames were sent.
    ///
    /// A parked frame goes first. Capacity is checked before anything is taken,
    /// because a native queue cannot put a frame back. Never blocks.
    ///
    /// # Errors
    /// [`ReceiveError::UnknownRoute`] if the ingress belongs to another transport.
    pub fn forward_once(&mut self) -> Result<usize, ReceiveError> {
        let (may_continue, resent) = self.resend_parked();
        if !may_continue {
            return Ok(resent);
        }
        if !self.egress_runtime.has_capacity(self.egress) {
            self.stats.no_capacity += 1;
            return Ok(resent);
        }
        let egress_runtime = self.egress_runtime;
        let egress = self.egress;
        let stats = &mut self.stats;
        let parked = &mut self.parked;
        let last_report = &mut self.last_report;
        let mut sent = resent;
        self.ingress_runtime
            .receive(
                self.ingress,
                &mut |frame: CarriedFrame<'_>| match egress_runtime.send(egress, &frame) {
                    Ok(report) => {
                        *last_report = Some(report);
                        stats.forwarded += 1;
                        sent += 1;
                        Flow::Continue
                    }
                    Err(
                        SendError::Busy
                        | SendError::SlotsExhausted
                        | SendError::Loan(LoanError::PoolExhausted),
                    ) => {
                        match frame.park() {
                            Ok(kept) => *parked = Some(kept),
                            Err(_lost) => stats.dropped += 1,
                        }
                        Flow::Stop
                    }
                    Err(_) => {
                        stats.dropped += 1;
                        Flow::Continue
                    }
                },
            )?;
        Ok(sent)
    }
}
