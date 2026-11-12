//! Compile-fail control: a receive-only schema profile cannot build a publisher.
//!
//! Compile as a library (`--crate-type=lib --emit=metadata`) against the pinned
//! SDK rlib. Intended diagnostic: E0277, `ReceiveOnly: EncodeProfile<u32>` is
//! not satisfied (rustc may also name the blanket `MappingProfile<u32>` route).
//! Any unresolved import/path/crate or linker diagnostic is a wrong-reason
//! failure, not acceptance.
use up_rust::communication::application::{
    BoundDeployment, DecodeProfile, Decoder, Event, PublishOptions,
};

pub struct ReceiveOnly;

impl DecodeProfile<u32> for ReceiveOnly {
    fn decoder(&self) -> Result<Decoder<u32>, up_rust::UWireError> {
        Err(up_rust::UWireError::invalid_payload(
            "declaration-only fixture",
        ))
    }
}

pub fn cannot_publish(local: BoundDeployment<ReceiveOnly>) {
    let _ = Event::<u32>::new(0x8001).publisher(&local, PublishOptions::default());
}
