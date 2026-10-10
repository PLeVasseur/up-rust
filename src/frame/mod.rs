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
//! # The UFrame data layer
//!
//! Everything about frames-as-data lives here, independent of any transport:
//!
//! - [`metadata`](crate::frame::metadata): [`UFrameMetadata`](crate::UFrameMetadata), the
//!   family-invariant semantic metadata model and the fallible projections to
//!   and from `UMessage` attributes. Payload identity is
//!   [`PayloadEncoding`](crate::PayloadEncoding), defined with the attributes
//!   and shared by both models.
//! - [`codec`](crate::frame::codec): the variable-length metadata field block.
//! - [`abi`](crate::frame::abi): the opt-in fixed-layout `#[repr(C)]` metadata
//!   representation for explicitly agreed cross-language boundaries.
//!
//! These provisional representation formats are defined by their codec and
//! the cross-language artifacts in `abi/uframe`. Their UMessage projection
//! follows the uProtocol attribute and payload-presence contracts.

pub mod view;
pub use view::{validate_frame_view_for_transport, UFrameView};
pub mod abi;
pub mod codec;
#[cfg(feature = "zero-copy-transport")]
pub mod header;
pub mod metadata;
