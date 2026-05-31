use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.DeclarationOrder")]
struct DeclarationOrder {
    sequence: u32,
    checksum: u32,
}

fn build<'a>(
    init: <DeclarationOrder as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<DeclarationOrder>, up_rust::UWireError> {
    init.sequence(1).checksum(2).finish()
}

fn main() {}
