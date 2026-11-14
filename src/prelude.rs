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

//! The classic names of types that live in `up-rust-core`.
//!
//! `up-rust-core` is heap-free and has only fixed-error names. The classic
//! names and error types (`UUID::build()`, `UUri::try_from_parts`, `to_uri`,
//! `verify_no_wildcards`, `UCode::try_from_i32` and so on) are extension traits
//! here, so existing code keeps its spelling. Import them with:
//!
//! ```rust
//! use up_rust::prelude::*;
//! ```

#[cfg(feature = "zero-copy-transport")]
pub use crate::payload::stable::{StablePayloadExt, StablePayloadFieldExt};
pub use crate::two_mode::TransportSetupExt;
pub use crate::uattributes::{PayloadEncodingExt, UMessageTypeExt, UPriorityExt};
#[cfg(feature = "up-core-api")]
pub use crate::uri::UUriProtobufExt;
pub use crate::uri::{ExactUUriExt, UUriExt};
pub use crate::ustatus::UCodeExt;
pub use crate::uuid::UuidExt;
#[cfg(feature = "up-core-api")]
pub use crate::uuid::UuidProtobufExt;
