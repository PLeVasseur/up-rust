//! POST-PATCH compile-pass control for `stable-payload-crate-path.patch`.
//!
//! Requires an SDK rlib (and `up_rust_macros` proc-macro) built WITH that
//! separate patch; without it this file fails exactly like
//! `renamed_derive_witness.rs`. Compiled as a library with
//! `--extern sdk=<patched up_rust rlib>` ONLY: no `up_rust` name and no alias.
#[repr(C)]
#[derive(Debug, sdk::StablePayload, sdk::StablePayloadInit)]
#[stable_payload(type_name = "external.Renamed", revision = 1, crate = "sdk")]
pub struct Renamed {
    pub value: u32,
}

/// The generated initializer is synchronous and writes in place.
pub fn initialize(
    loan: sdk::PayloadInit<'_, Renamed>,
) -> sdk::InitializedStablePayload<'_, Renamed> {
    loan.into_initializer().value(7).finish()
}
