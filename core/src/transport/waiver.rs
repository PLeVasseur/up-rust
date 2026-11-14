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

//! What a binding's middleware does with the heap in runtime mode (R4, R41).
//!
//! A binding ships a [`RuntimePermit`] stating the pre-agreed use: which
//! operations, and the measured bound. That is not approval. A deployment files
//! a [`RuntimeWaiver`] against it, recorded on the runtime, findable in the code,
//! and approved by someone other than its author. A waiver covers allocation
//! inside the middleware only. It can never cover allocation in SDK or binding
//! code, nor a panic.

/// Which runtime operations of a binding's middleware use the heap.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum HeapOperation {
    /// Sending.
    Send,
    /// Receiving.
    Receive,
    /// RPC correlation.
    Rpc,
    /// Handling a change of peers.
    PeerChange,
}

/// What a binding ships when its middleware touches the heap at runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimePermit {
    /// Stable identifier, `UPW-<binding>-<n>`.
    pub id: &'static str,
    /// The rule this permit concerns, such as `runtime.no_heap_acquire`.
    pub rule: &'static str,
    /// Binding and middleware version the permit was measured against.
    pub binding: &'static str,
    /// The exact operations in which the middleware touches the heap.
    pub operations: &'static [HeapOperation],
    /// The measured bound, such as per connection change.
    pub bound: &'static str,
}

/// A deployment's recorded acceptance of a [`RuntimePermit`] (R41).
///
/// Use [`runtime_waiver!`](crate::runtime_waiver) so the record names its location.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeWaiver {
    /// Stable waiver identifier.
    pub id: &'static str,
    /// The permit this waiver accepts.
    pub permit: &'static str,
    /// Why this deployment needs it, including the alternative rejected.
    pub reason: &'static str,
    /// Where the waived transport may run, such as a QM partition.
    pub placement: &'static str,
    /// Who wrote the waiver.
    pub author: &'static str,
    /// Who approved it; must differ from the author (R41).
    pub approved_by: &'static str,
    /// Date of approval.
    pub date: &'static str,
    /// Measurement this waiver relies on.
    pub evidence: &'static str,
    /// What must trigger review, such as a middleware version change.
    pub review_trigger: &'static str,
    /// Source location of the record.
    pub location: &'static str,
}

impl RuntimeWaiver {
    pub(crate) fn check(&self) -> Result<(), &'static str> {
        let required = [
            self.id,
            self.permit,
            self.reason,
            self.placement,
            self.author,
            self.approved_by,
            self.date,
            self.evidence,
            self.review_trigger,
            self.location,
        ];
        if required.iter().any(|field| field.is_empty()) {
            return Err("every waiver field is required");
        }
        if self.author == self.approved_by {
            return Err("the approver must not be the author");
        }
        Ok(())
    }
}

/// Files a [`RuntimeWaiver`] at the call site, recording its source location.
#[macro_export]
macro_rules! runtime_waiver {
    (
        id: $id:expr, permit: $permit:expr, reason: $reason:expr, placement: $placement:expr,
        author: $author:expr, approved_by: $approved:expr, date: $date:expr,
        evidence: $evidence:expr, review_trigger: $review:expr $(,)?
    ) => {{
        static WAIVER: $crate::transport::waiver::RuntimeWaiver =
            $crate::transport::waiver::RuntimeWaiver {
                id: $id,
                permit: $permit,
                reason: $reason,
                placement: $placement,
                author: $author,
                approved_by: $approved,
                date: $date,
                evidence: $evidence,
                review_trigger: $review,
                location: concat!(file!(), ":", line!()),
            };
        &WAIVER
    }};
}

/// Distinguishes accepted deviations, known failures and middleware heap permits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DeviationKind {
    /// An explicitly accepted and bounded exception.
    Accepted,
    /// An unresolved failure; acknowledgement does not turn it into a guarantee.
    KnownFailure,
    /// Middleware heap use, still requiring a named runtime waiver.
    MiddlewareHeap,
}

/// A binding's versioned deviation, attached only to affected routes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Deviation {
    /// Stable repository-owned identifier.
    pub id: &'static str,
    /// Revision; acknowledgements of earlier revisions do not carry forward.
    pub revision: u32,
    /// Whether it is accepted, unresolved, or a middleware heap permit.
    pub kind: DeviationKind,
    /// Human-readable description; the binding repository holds its evidence.
    pub title: &'static str,
    /// Whether it can interfere with other routes in the process.
    pub can_interfere: bool,
}
