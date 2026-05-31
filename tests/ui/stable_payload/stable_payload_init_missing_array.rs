use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.MissingArray")]
struct MissingArray {
    sequence: u32,
    payload: [u8; 4],
}

fn build<'a>(
    init: <MissingArray as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<MissingArray>, up_rust::UWireError> {
    init.sequence(1).finish()
}

fn main() {}
