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

//! Core's single atomic access boundary: plain re-exports, no behavior change.
//!
//! All atomic accesses, loads and stores included, use these types. A future
//! backend must make every operation linearizable per word with the caller's
//! ordering, and must not wait on one core. Bypassing an emulation lock for a
//! load/store would invalidate its compare-and-swap. No such backend exists here.
//!
//! Surface: load, store, swap, compare_exchange, compare_exchange_weak,
//! fetch_add, fetch_sub, fetch_or, fetch_max, and fence. A future backend is
//! selected here at build time by an integrator cfg, never a Cargo feature or
//! a per-operation Platform trait call. No no-CAS support is claimed.
//!
//! AtomicU64 is temporarily retained until the separate 32-bit protocol port.
//! Facade routing alone does not establish 32-bit or no-64-bit-atomic support.
//! Preparation for req~ssr-atomic-width~1 at neutral spec 733b006; G3 is pending,
//! and that item does not yet carry SafetyGateUpRust. No OFT closure claimed.

// Keep the documented operation surface even when a target uses only part of it.
#[allow(unused_imports)]
pub(crate) use core::sync::atomic::{
    fence, AtomicBool, AtomicI32, AtomicPtr, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering,
};
