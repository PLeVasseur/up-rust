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

//! Heap-backed startup memory for the `std` path (R15).
//!
//! The implementation lives in the slim `up-rust-std` crate. These re-exports
//! preserve the existing facade paths; the dependency-free core is unchanged.

pub use up_rust_std::startup::{
    heap_startup_memory, FixedVec, FixedVecIter, LeakedHeapRegions, MemoryReport,
    RegionSource, StartupError, StartupMemory, StartupRegion, LEAKED_HEAP_REGIONS,
};
