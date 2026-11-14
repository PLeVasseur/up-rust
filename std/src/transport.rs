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

//! Heap-backed constructors for the common SDK setup type on native platforms.

use crate::startup::LEAKED_HEAP_REGIONS;
use up_rust_core::memory::StartupMemory;
use up_rust_core::transport::{
    BindingSetup, HandoffRuntime, LendingRuntime, StartupError, TransportSetup,
};

/// Adds automatically provisioned startup storage to [`TransportSetup`].
///
/// With this trait imported, `TransportSetup::lending(binding)` and
/// `TransportSetup::handoff(binding)` return the same setup type as the explicit
/// memory constructors. Only startup storage provisioning changes. Fixed-memory
/// deployments continue to use the core's `lending_in`/`handoff_in` constructors.
pub trait TransportSetupExt: Sized {
    /// Wraps a lending binding using automatic startup memory and the native platform.
    ///
    /// # Errors
    /// Returns startup allocation failure or the errors of
    /// [`TransportSetup::lending_in`].
    fn lending<S>(binding: S) -> Result<Self, StartupError>
    where
        S: BindingSetup,
        S::Running: LendingRuntime;

    /// Wraps a handoff binding using automatic startup memory and the native platform.
    ///
    /// # Errors
    /// Returns startup allocation failure or the errors of
    /// [`TransportSetup::handoff_in`].
    fn handoff<S>(binding: S) -> Result<Self, StartupError>
    where
        S: BindingSetup,
        S::Running: HandoffRuntime;
}

impl TransportSetupExt for TransportSetup {
    fn lending<S>(binding: S) -> Result<Self, StartupError>
    where
        S: BindingSetup,
        S::Running: LendingRuntime,
    {
        // Preserve the existing constructor's source request. The heap source
        // rounds small requests to its documented minimum region size.
        let memory = StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 64 * 1024)?;
        TransportSetup::lending_in(&memory, binding)
    }

    fn handoff<S>(binding: S) -> Result<Self, StartupError>
    where
        S: BindingSetup,
        S::Running: HandoffRuntime,
    {
        let memory = StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 64 * 1024)?;
        TransportSetup::handoff_in(&memory, binding)
    }
}
