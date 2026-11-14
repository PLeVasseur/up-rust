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

//! Typestate markers for APIs that distinguish unchecked values from values
//! whose invariants have been validated. Each carrier defines its permitted
//! operations and validation transition.

/// Typestate marker: not yet validated.
///
/// Values in this state come from decode paths or `new_unchecked`
/// constructors; the only way forward is the type's `validate` method,
/// which transitions to [`Validated`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unvalidated {}

/// Typestate marker: validation has passed.
///
/// A carrier using this marker has completed its required invariant checks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Validated {}
