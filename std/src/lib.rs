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

//! Standard-library startup conveniences for the dependency-free uProtocol core.
//!
//! This crate uses the process allocator during initialization. Its startup
//! regions remain allocated for the process lifetime; runtime handles do not
//! own or free them. The project's fixed-memory safety profile continues to
//! use `up_rust_core::memory::StartupRegion` or caller-supplied memory.
//!
//! Protobuf, Tokio and transport implementations are not dependencies here.
#![forbid(unsafe_code)]

pub mod startup;

/// The common SDK transport setup type.
pub use up_rust_core::transport::TransportSetup;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "nto", target_os = "qnx"))]
mod transport;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "nto", target_os = "qnx"))]
pub use transport::TransportSetupExt;
