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

//! The safety-critical path of the uProtocol Rust SDK (R34, R48).
//!
//! `#![no_std]`, no `alloc`, no dependencies and no Cargo features, so nothing
//! else in a build can switch `std` on inside this crate. CI enforces this:
//! `tools/ci/check_core_gates.py` checks the manifest, a build against a sysroot
//! holding only `core`, a link-time panic check and the absence of
//! `core::task`, `core::future` and `core::pin`.
//!
//! `up-rust` re-exports this crate at the paths its users already know.

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(
    clippy::undocumented_unsafe_blocks,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unreachable,
    clippy::indexing_slicing
)]

pub mod communication;
pub mod frame;
pub mod memory;
pub mod payload;
pub mod platform;
pub mod signal;
pub mod transport;
pub mod uattributes;
pub mod ucode;
pub mod uri;
pub mod uuid;
pub mod validation_state;
pub mod wire;

#[doc(hidden)]
pub use wire::__zero_slot;

