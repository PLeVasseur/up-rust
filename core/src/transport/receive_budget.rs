/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional information.
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

use core::cell::Cell;

/// SDK-owned allowance shared by every receive layer in one synchronous call.
///
/// A binding-author contract, not an application drain setting. The SDK creates
/// it from the route's bound. Bindings cannot construct, clone or reset it.
/// Take one unit BEFORE each raw receive attempt, including internal rejections.
/// An empty/error attempt may also consume a unit; keep its usual stop behavior.
/// SDK validation of that same frame does not consume another unit.
///
/// This call-local cell is not shared between threads or retained after return.
/// It introduces no caller-concurrency limit, synchronization or heap use.
#[derive(Debug)]
pub struct ReceiveBudget {
    remaining: Cell<usize>,
}

impl ReceiveBudget {
    pub(crate) const fn new(limit: usize) -> Self {
        Self { remaining: Cell::new(limit) }
    }

    /// Takes permission for one raw receive attempt. On refusal, leave the next
    /// frame queued: do not dequeue, inspect or discard it.
    pub fn try_take(&self) -> bool {
        let Some(next) = self.remaining.get().checked_sub(1) else {
            return false;
        };
        self.remaining.set(next);
        true
    }

    /// Remaining raw attempts. Observation consumes none.
    pub fn remaining(&self) -> usize {
        self.remaining.get()
    }
}

#[cfg(test)]
mod tests {
    use super::ReceiveBudget;

    #[test]
    fn exhaustion_does_not_wrap_or_refill() {
        let budget = ReceiveBudget::new(2);
        assert!(budget.try_take());
        assert!(budget.try_take());
        assert!(!budget.try_take());
        assert!(!budget.try_take());
        assert_eq!(budget.remaining(), 0);
        assert!(!ReceiveBudget::new(0).try_take());
    }

    #[test]
    fn nested_layers_share_the_same_allowance() {
        fn discard(budget: &ReceiveBudget) -> bool { budget.try_take() }
        let budget = ReceiveBudget::new(3);
        let forwarded = &budget;
        assert!(discard(forwarded));
        assert!(budget.try_take());
        assert_eq!(forwarded.remaining(), 1);
        assert!(discard(forwarded));
        assert!(!budget.try_take());
        assert_eq!(forwarded.remaining(), 0);
    }

    #[test]
    fn maximum_allowance_decrements_without_overflow() {
        let budget = ReceiveBudget::new(usize::MAX);
        assert!(budget.try_take());
        assert_eq!(budget.remaining(), usize::MAX - 1);
    }

    #[test]
    fn observing_or_stopping_early_does_not_spend_the_next_attempt() {
        let budget = ReceiveBudget::new(9);
        assert!(budget.try_take());
        assert_eq!(budget.remaining(), 8);
        assert_eq!(budget.remaining(), 8);
        assert!(budget.try_take());
        assert_eq!(budget.remaining(), 7);
    }

}
