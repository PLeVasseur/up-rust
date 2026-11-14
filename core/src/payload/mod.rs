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

//! Stable native layouts and the build-time native agreement.
//!
//! [`stable`] carries const data only: validity flag, name, revision, size,
//! alignment, the field table and byte validation. `up-rust` builds its
//! `NativeRepresentation` from that data through an extension trait.
//! [`loan`] is the checked typed borrow. [`agreement`] is the build-time
//! agreement table and the token computation.

pub mod agreement;
pub mod loan;
pub mod sha256;
pub mod stable;
