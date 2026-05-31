use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.ArrayFill")]
struct ArrayFill {
    payload: [u8; 4096],
}

fn build<'a>(
    init: <ArrayFill as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<ArrayFill>, up_rust::UWireError> {
    init.payload_fill(0x5a).finish()
}

fn main() {}
