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

//! L2 roles in runtime mode.

use crate::uri::UUri;

pub mod two_mode;

/// Supplies this uEntity's URIs without touching the heap.
///
/// A heap-free counterpart to `up_rust::LocalUriProvider`. Implement it on your
/// deployment's identity; `up-rust` implements it for its `StaticUriProvider`.
pub trait LocalUriProvider {
    /// A URI that represents a given resource of this uEntity.
    fn get_resource_uri(&self, resource_id: u16) -> UUri;
    /// The URI this uEntity expects RPC responses and notifications to be sent to.
    fn get_source_uri(&self) -> UUri;
}
