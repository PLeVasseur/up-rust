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

//! Per-route QoS: declared at initialization, checked against each binding's
//! capability data, refused at setup naming the route and setting, and never
//! silently downgraded (R44). Everything here is `Copy` and heap-free.

use core::fmt;
use core::time::Duration;

use crate::uattributes::UPriority;

/// Whether the transport itself may lose a message that `send` accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reliability {
    /// The transport may lose a message without telling anyone. Every binding honors this.
    BestEffort,
    /// No loss inside the transport. The only loss is what `Overflow` says, and it is counted.
    Reliable,
}

/// What happens when a receiver's queue is full. There is no unbounded wait (R40).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    /// Newest wins: the oldest queued message is discarded. iceoryx2 safe overflow.
    DropOldest,
    /// Oldest wins: the new message is discarded for that receiver. iceoryx2 `DiscardData`.
    DropNewest,
    /// The sender waits up to `timeout` for room, then reports failure.
    Wait {
        /// Longest a send may wait.
        timeout: Duration,
    },
}

/// A route's silence declaration. Unspecified and EventOnly both disable
/// monitoring; strict policies distinguish omission from an explicit choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deadline {
    /// No deadline declaration (the permissive default).
    Unspecified,
    /// Maximum permitted silence; a zero duration is refused at setup.
    After(Duration),
    /// Events are sporadic, so silence is explicitly not monitored.
    EventOnly,
}
impl Deadline {
    /// The monitoring duration, if one was declared.
    #[must_use]
    pub const fn duration(self) -> Option<Duration> {
        match self {
            Self::After(value) => Some(value),
            Self::Unspecified | Self::EventOnly => None,
        }
    }
}

/// The per-route contract. Declared once, during initialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Qos {
    /// Whether the transport may lose accepted messages.
    pub reliability: Reliability,
    /// What a full receiver queue does.
    pub overflow: Overflow,
    /// Messages each receiver can hold undelivered. Fixed at setup. At least 1.
    pub depth: u16,
    /// Messages replayed to a receiver that joins late. Never more than `depth`.
    pub history: u16,
    /// Longest silence a receiver tolerates on this route, a non-zero duration.
    /// `None` means not monitored; zero is refused at setup rather than read as
    /// "not monitored" or as "always missed".
    /// Deadlines measure actual arrival at the binding, including on pull routes;
    /// dequeuing is not a new arrival. A qualified SDK service observes silence
    /// without application polling. Setup refuses an enabled deadline unless its
    /// binding observation and service/publication delay have been qualified.
    pub deadline: Deadline,
    /// `Some(class)`: the route requires this handling. `None`: priority is carried, not acted on.
    pub priority: Option<UPriority>,
}

impl Qos {
    /// The sign-posted default (R29): lossy under overload, never slows a sender,
    /// honored by every binding, no replay, no monitoring.
    pub const DEFAULT: Qos = Qos {
        reliability: Reliability::BestEffort,
        overflow: Overflow::DropOldest,
        depth: 8,
        history: 0,
        deadline: Deadline::Unspecified,
        priority: None,
    };

    /// This QoS with reliable delivery.
    #[must_use]
    pub const fn reliable(mut self) -> Self {
        self.reliability = Reliability::Reliable;
        self
    }

    /// This QoS with a different overflow policy.
    #[must_use]
    pub const fn with_overflow(mut self, overflow: Overflow) -> Self {
        self.overflow = overflow;
        self
    }

    /// This QoS with a different queue depth.
    #[must_use]
    pub const fn with_depth(mut self, depth: u16) -> Self {
        self.depth = depth;
        self
    }

    /// This QoS with late-joiner history.
    #[must_use]
    pub const fn with_history(mut self, history: u16) -> Self {
        self.history = history;
        self
    }

    /// This QoS with a deadline.
    #[must_use]
    pub const fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = Deadline::After(deadline);
        self
    }

    /// Explicitly declares sporadic events, without changing permissive behavior.
    #[must_use]
    pub const fn event_only(mut self) -> Self {
        self.deadline = Deadline::EventOnly;
        self
    }

    /// This QoS with a required priority class.
    #[must_use]
    pub const fn with_priority(mut self, class: UPriority) -> Self {
        self.priority = Some(class);
        self
    }
}

impl Default for Qos {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What a binding can honor, declared as data per route kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct QosSupport {
    /// `Reliable` routes.
    pub reliable: bool,
    /// `Overflow::DropOldest`.
    pub drop_oldest: bool,
    /// `Overflow::DropNewest`.
    pub drop_newest: bool,
    /// `Overflow::Wait`.
    pub wait: bool,
    /// Largest `depth` the binding can reserve.
    pub max_depth: u16,
    /// Largest `history` the binding can replay; 0 means a late joiner gets nothing.
    pub max_history: u16,
    /// Distinct priority classes the binding acts on; 0 means carried only.
    pub priority_levels: u8,
}

impl QosSupport {
    /// Starts with no supported policies. A binding enables only what it honors.
    #[must_use]
    pub const fn new() -> Self {
        Self::NONE
    }
    /// Sets [`Self::reliable`].
    #[must_use]
    pub const fn with_reliable(mut self, value: bool) -> Self {
        self.reliable = value;
        self
    }
    /// Sets [`Self::drop_oldest`].
    #[must_use]
    pub const fn with_drop_oldest(mut self, value: bool) -> Self {
        self.drop_oldest = value;
        self
    }
    /// Sets [`Self::drop_newest`].
    #[must_use]
    pub const fn with_drop_newest(mut self, value: bool) -> Self {
        self.drop_newest = value;
        self
    }
    /// Sets [`Self::wait`].
    #[must_use]
    pub const fn with_wait(mut self, value: bool) -> Self {
        self.wait = value;
        self
    }
    /// Sets [`Self::max_depth`].
    #[must_use]
    pub const fn with_max_depth(mut self, value: u16) -> Self {
        self.max_depth = value;
        self
    }
    /// Sets [`Self::max_history`].
    #[must_use]
    pub const fn with_max_history(mut self, value: u16) -> Self {
        self.max_history = value;
        self
    }
    /// Sets [`Self::priority_levels`].
    #[must_use]
    pub const fn with_priority_levels(mut self, value: u8) -> Self {
        self.priority_levels = value;
        self
    }

    /// A binding that can offer nothing: every route is refused at setup.
    pub const NONE: QosSupport = QosSupport {
        reliable: false,
        drop_oldest: false,
        drop_newest: false,
        wait: false,
        max_depth: 0,
        max_history: 0,
        priority_levels: 0,
    };
}

/// Which setting a refusal is about. Stable and `Copy`, for reports and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum QosSetting {
    /// `Reliability`.
    Reliability,
    /// `Overflow`.
    Overflow,
    /// `depth`.
    Depth,
    /// `history`.
    History,
    /// `priority`.
    Priority,
    /// `deadline`: zero is refused; the unit is nanoseconds.
    Deadline,
    /// A second declaration of the same route disagrees with the first.
    PeerMismatch,
}

impl fmt::Display for QosSetting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Reliability => "reliability",
            Self::Overflow => "overflow",
            Self::Depth => "depth",
            Self::History => "history",
            Self::Priority => "priority",
            Self::Deadline => "deadline",
            Self::PeerMismatch => "peer agreement",
        })
    }
}

/// A refused request. Names the setting; never silently downgraded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QosRefused {
    /// Which setting.
    pub setting: QosSetting,
    /// The value asked for, in the setting's natural unit.
    pub requested: u32,
    /// The most the binding can do; 0 where the setting is unsupported.
    pub available: u32,
}

const fn refuse(setting: QosSetting, requested: u32, available: u32) -> QosRefused {
    QosRefused {
        setting,
        requested,
        available,
    }
}

/// The check every route passes at setup, before any resource is reserved.
///
/// Lives in the SDK (R5), so a binding author writes only the capability data.
/// No heap on any path, including the error paths.
///
/// # Errors
/// Returns a [`QosRefused`] naming the first setting the binding cannot honor.
pub const fn check_qos(qos: &Qos, support: &QosSupport) -> Result<(), QosRefused> {
    // Refuse what would otherwise be silently truncated or coerced.
    if qos.depth == 0 {
        return Err(refuse(QosSetting::Depth, 0, 1));
    }
    if qos.history > qos.depth {
        return Err(refuse(
            QosSetting::History,
            qos.history as u32,
            qos.depth as u32,
        ));
    }
    if let Deadline::After(deadline) = qos.deadline {
        if deadline.is_zero() {
            return Err(refuse(QosSetting::Deadline, 0, 1));
        }
    }
    if matches!(qos.reliability, Reliability::Reliable) && !support.reliable {
        return Err(refuse(QosSetting::Reliability, 1, 0));
    }
    let supported = match qos.overflow {
        Overflow::DropOldest => support.drop_oldest,
        Overflow::DropNewest => support.drop_newest,
        Overflow::Wait { .. } => support.wait,
    };
    if !supported {
        return Err(refuse(QosSetting::Overflow, 1, 0));
    }
    if qos.depth > support.max_depth {
        return Err(refuse(
            QosSetting::Depth,
            qos.depth as u32,
            support.max_depth as u32,
        ));
    }
    if qos.history > support.max_history {
        return Err(refuse(
            QosSetting::History,
            qos.history as u32,
            support.max_history as u32,
        ));
    }
    if let Some(class) = qos.priority {
        if support.priority_levels == 0 {
            return Err(refuse(QosSetting::Priority, class as u32, 0));
        }
    }
    Ok(())
}

/// Observable effects of one send, without promising peer consumption.
/// Counts are per receiving route. A queue-full replacement can both accept the
/// current message and discard an older message, contributing to both fields.
/// `counts_exact == false` means the fields are lower bounds, not exact totals;
/// zero then means unknown, not proof that no receiver/discard exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SendReport {
    /// Receiving routes that accepted this message (or a known lower bound).
    pub delivered: u64,
    /// Receiving routes where this send caused a queue-full discard, including
    /// DropOldest replacement or DropNewest rejection (or a known lower bound).
    pub dropped_full: u64,
    /// Whether both counts are complete for this send at the binding boundary.
    pub counts_exact: bool,
}
impl Default for SendReport {
    fn default() -> Self { Self::exact(0, 0) }
}
impl SendReport {
    /// Complete counts observed at the binding boundary, including exact zero.
    pub const fn exact(delivered: u64, dropped_full: u64) -> Self {
        Self { delivered, dropped_full, counts_exact: true }
    }
    /// No receiver counts are available from this binding operation.
    pub const fn unknown() -> Self {
        Self { delivered: 0, dropped_full: 0, counts_exact: false }
    }
    /// Combines disjoint observed receiver effects, preserving unknown totals.
    pub fn append(&mut self, other: Self) {
        self.counts_exact &= other.counts_exact;
        match self.delivered.checked_add(other.delivered) {
            Some(n) => self.delivered = n,
            None => { self.delivered = u64::MAX; self.counts_exact = false; }
        }
        match self.dropped_full.checked_add(other.dropped_full) {
            Some(n) => self.dropped_full = n,
            None => { self.dropped_full = u64::MAX; self.counts_exact = false; }
        }
    }
    /// Records one known acceptance without representing overflow as exact.
    pub fn accepted(&mut self) {
        if let Some(n) = self.delivered.checked_add(1) { self.delivered = n; }
        else { self.counts_exact = false; }
    }
    /// Records one known queue-full discard without losing an overflow signal.
    pub fn discarded_full(&mut self) {
        if let Some(n) = self.dropped_full.checked_add(1) { self.dropped_full = n; }
        else { self.counts_exact = false; }
    }
}
