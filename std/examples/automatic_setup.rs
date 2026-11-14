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

//! Compile-only call shapes: automatic provisioning with either binding family.
#![forbid(unsafe_code)]

use up_rust_core::transport::{
    BindingSetup, HandoffRuntime, LendingRuntime, StartupError, TransportSetup,
};
use up_rust_std::TransportSetupExt;

/// The binding value supplies transport-specific configuration once.
pub fn lending<S>(binding: S) -> Result<TransportSetup, StartupError>
where
    S: BindingSetup,
    S::Running: LendingRuntime,
{
    TransportSetup::lending(binding)
}

/// Handoff bindings use the same SDK setup type and route builders.
pub fn handoff<S>(binding: S) -> Result<TransportSetup, StartupError>
where
    S: BindingSetup,
    S::Running: HandoffRuntime,
{
    TransportSetup::handoff(binding)
}
