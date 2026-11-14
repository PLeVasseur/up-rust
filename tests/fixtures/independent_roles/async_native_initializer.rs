//! Compile-fail control: native TX initialization is synchronous and in place.
//!
//! Compile as a library (`--crate-type=lib --emit=metadata`) against an SDK
//! rlib built with `up-l2-zero-copy`. Intended diagnostic: E0308 mismatched
//! types: the initializer must return `InitializedStablePayload<'_, Payload>`
//! but returns an `async` block. Any unresolved import/path/crate or linker
//! diagnostic is a wrong-reason failure, not acceptance.
use up_rust::{communication::application::native::Publish, StablePayloadInit};

pub fn cannot_await<Payload: StablePayloadInit, Sender: Publish<Payload>>(sender: &Sender) {
    let _ = sender.publish_in_place(|loan| async move { loan });
}
