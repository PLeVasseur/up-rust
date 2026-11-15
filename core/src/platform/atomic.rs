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

//! Native-width atomic access boundary. Host generators use independent native
//! 64-bit words where available. Other generator profiles, peer and RPC records
//! use the private scoped metadata operations, not an emulated wide atomic.
//! Loads, stores and read-modify-writes on the remaining narrow words use these
//! types. The explicit experimental model is selected by platform::exclusion.

// Keep the documented operation surface even when a target uses only part of it.
#[allow(unused_imports)]
pub(crate) use core::sync::atomic::{
    compiler_fence, fence, AtomicBool, AtomicI32, AtomicPtr, AtomicU32, AtomicU8, AtomicUsize, Ordering,
};

#[cfg(all(up_rust_native32_model = "host", target_has_atomic = "64"))]
pub(crate) use core::sync::atomic::AtomicU64;
