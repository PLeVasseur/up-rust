use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.ArrayFromSlice")]
struct ArrayFromSlice {
    payload: [u8; 4],
}

fn build<'a>(
    init: <ArrayFromSlice as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<ArrayFromSlice>, up_rust::UWireError> {
    init.payload_from_slice(&[1, 2, 3, 4])?.finish()
}

fn main() {}
