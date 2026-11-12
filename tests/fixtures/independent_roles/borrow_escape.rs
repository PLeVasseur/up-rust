//! Compile-fail control: a checked delivery borrow cannot outlive its owner.
//!
//! Compile as a library (`--crate-type=lib --emit=metadata`) against the pinned
//! SDK rlib. Intended diagnostic: E0515, cannot return a value referencing the
//! function parameter `owner`. Any unresolved import/path/crate or linker
//! diagnostic is a wrong-reason failure, not acceptance.
use up_rust::communication::application::Delivery;

pub fn cannot_escape<Owner: Delivery<u32>>(owner: Owner) -> &'static u32 {
    owner.payload().unwrap()
}
