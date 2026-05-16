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

//! Native uProtocol core service identities and generated service payload bindings.
//!
//! Service DTOs are protobuf-defined payload contracts. Native frames carry those
//! DTO bytes when the relevant payload codec feature is enabled; they do not
//! replace service DTOs with partial native public-field structs.

pub mod udiscovery;
pub mod usubscription;
pub mod utwin;
