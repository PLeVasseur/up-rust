// SPDX-License-Identifier: Apache-2.0
//! Defining-crate fixtures for linked inventory and feature-toggle behavior.

/// A fixed-size application identity distinct from an equally sized seat state.
#[repr(C)]
#[derive(up_rust::StablePayload)]
#[stable_payload(type_name = "example.DoorState", inventory)]
pub struct DoorState {
    /// Application-defined state value.
    pub value: u32,
}

/// An independently named equally sized application identity.
#[cfg(feature = "seat")]
#[repr(C)]
#[derive(up_rust::StablePayload)]
#[stable_payload(type_name = "example.SeatState", inventory)]
pub struct SeatState {
    /// Application-defined state value.
    pub value: u32,
}

/// A feature-selected identity that sorts before existing type names.
#[cfg(feature = "air")]
#[repr(C)]
#[derive(up_rust::StablePayload)]
#[stable_payload(type_name = "example.AirState", inventory)]
pub struct AirState {
    /// Application-defined state value.
    pub value: u32,
}
