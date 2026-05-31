use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.DuplicateArray")]
struct DuplicateArray {
    payload: [u8; 4],
}

fn build<'a>(
    init: <DuplicateArray as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<DuplicateArray>, up_rust::UWireError> {
    init.payload_fill(1).payload_fill(2).finish()
}

fn main() {}
