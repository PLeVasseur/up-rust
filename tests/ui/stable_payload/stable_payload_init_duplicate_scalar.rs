use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.DuplicateScalar")]
struct DuplicateScalar {
    checksum: u32,
}

fn build<'a>(
    init: <DuplicateScalar as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<DuplicateScalar>, up_rust::UWireError> {
    init.checksum(1).checksum(2).finish()
}

fn main() {}
