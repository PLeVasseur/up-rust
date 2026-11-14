//! DEFECT CHARACTERIZATION — expected to fail today; NOT an acceptance gate.
//!
//! The `StablePayload`/`StablePayloadInit` derives emit hard-coded
//! `::up_rust::...` paths. Compiled as a library with `--extern sdk=<pinned
//! up_rust rlib>` ONLY (a renamed dependency), the derive cannot resolve the
//! SDK. Expected today: E0433, failed to resolve `up_rust`. The declarative SDK
//! macros do not have this defect (see `external_consumer_renamed.rs`); the
//! crate-root alias workaround is `external_consumer_derive_alias.rs`.
#[repr(C)]
#[derive(Debug, sdk::StablePayload, sdk::StablePayloadInit)]
#[stable_payload(type_name = "external.Renamed", revision = 1)]
pub struct Renamed {
    pub value: u32,
}
