use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.MissingScalar")]
struct MissingScalar {
    sequence: u32,
    checksum: u32,
}

fn build<'a>(
    init: <MissingScalar as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<MissingScalar>, up_rust::UWireError> {
    init.sequence(1).finish()
}

fn main() {}
