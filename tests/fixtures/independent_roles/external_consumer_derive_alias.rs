//! Compile-pass control: the documented workaround for the renamed-dependency
//! stable-payload derive defect (`renamed_derive_witness.rs`).
//!
//! Compiled as a library with `--extern sdk=<pinned up_rust rlib>` ONLY. The
//! crate-root alias adds `up_rust` back to the extern prelude, so the derives'
//! hard-coded `::up_rust` paths resolve. This is a real migration cost for
//! renamed dependencies, not a solution to the defect.
extern crate sdk as up_rust;

#[repr(C)]
#[derive(Debug, sdk::StablePayload, sdk::StablePayloadInit)]
#[stable_payload(type_name = "external.Renamed", revision = 1)]
pub struct Renamed {
    pub value: u32,
}

/// The generated initializer is synchronous and writes in place.
pub fn initialize(
    loan: sdk::PayloadInit<'_, Renamed>,
) -> sdk::InitializedStablePayload<'_, Renamed> {
    loan.into_initializer().value(7).finish()
}
