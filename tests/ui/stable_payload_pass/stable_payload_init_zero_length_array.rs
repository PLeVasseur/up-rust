use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.ZeroLengthArray")]
struct ZeroLengthArray {
    payload: [u8; 0],
}

fn build<'a>(
    init: <ZeroLengthArray as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<ZeroLengthArray>, up_rust::UWireError> {
    init.payload_from_slice(&[])?.finish()
}

fn main() {}
