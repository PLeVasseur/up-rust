// SPDX-License-Identifier: Apache-2.0

//! Compile-time gate for a crate that depends only on `up-rust-core` and
//! `up-rust-macros`. Everything here is `no_std` and heap-free.

#![no_std]

use up_rust_core::memory::{StartupError, StartupMemory, StartupRegion};
use up_rust_core::signal::Signal;
use up_rust_core::uattributes::{PayloadEncoding, UPriority};
use up_rust_core::uri::UUri;
use up_rust_core::uuid::UUID;

static REGION: StartupRegion<4096> = StartupRegion::new();
static WAKE: Signal = Signal::new();

/// Parses a URI and returns its resource id, with no heap.
pub fn parse_resource(text: &str) -> Option<u16> {
    text.parse::<UUri>().ok().map(|uri| uri.resource_id())
}

/// Parses a UUID and returns its timestamp.
pub fn parse_uuid_time(text: &str) -> Option<u64> {
    text.parse::<UUID>().ok().map(|uuid| uuid.time())
}

/// Places a value in static startup memory.
pub fn place_value(value: u32) -> Result<u32, StartupError> {
    let memory: StartupMemory = REGION.claim()?;
    let slot = memory.place(value)?;
    let result = *slot;
    let _report = memory.seal();
    Ok(result)
}

/// Touches the priority and encoding value types.
pub fn identifiers() -> (u8, u32) {
    (UPriority::CS4.wire_code(), PayloadEncoding::TEXT.id())
}

/// Raises the shared signal through the native platform.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn notify() {
    WAKE.notify();
}

/// The derives work on the core-only path (R5), with `crate = "up_rust_core"`.
#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    up_rust_macros::StablePayload,
    up_rust_macros::StablePayloadInit,
    up_rust_macros::ZeroSlot,
)]
#[stable_payload(type_name = "test.CorePath", crate = "up_rust_core")]
#[zero_slot(crate = "up_rust_core")]
pub struct Payload {
    /// A value.
    pub value: u32,
}
