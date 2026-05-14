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

//! Native uProtocol core service contracts.
//!
//! These modules replace the generated protobuf-backed core exports with native
//! Rust types that can be carried by any selected [`crate::WireFormat`].

pub mod udiscovery;
pub mod usubscription;
pub mod utwin;
