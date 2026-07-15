// SPDX-License-Identifier: Apache-2.0
//! # The UFrame data layer
//!
//! Everything about frames-as-data lives here, independent of any transport:
//!
//! - [`metadata`] — [`UFrameMetadata`](metadata::UFrameMetadata), the
//!   family-invariant semantic metadata model, plus
//!   [`PayloadEncoding`](metadata::PayloadEncoding) identity and the fallible
//!   projections to and from `UMessage` attributes.
//! - [`codec`] — the canonical metadata field-block profile: how metadata
//!   becomes bytes under a metadata profile.
//! - `envelope` — complete-frame `UPFE` serialization, available with
//!   `owned-frame-transport`, for transports that
//!   carry metadata and payload in one byte envelope. Selected-wire `UPWM`
//!   profile prefixes are owned by the wire adapter, not this module.
//! - [`abi`] — `UFrameMetadataAbiV1`, the fixed-layout `#[repr(C)]` profile
//!   for cross-language shared-memory boundaries. NOTE: its consumers are
//!   C/C++ peers; zero Rust references is its expected steady state.
//!
//! Spec: `up-spec/basics/uframe.adoc` and
//! `up-spec/up-l1/transport_families.adoc`.

pub mod view;
pub use view::{validate_frame_view_for_transport, UFrameView};
pub mod abi;
pub mod codec;
#[cfg(feature = "owned-frame-transport")]
pub mod envelope;
pub mod metadata;
