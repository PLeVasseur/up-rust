use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.WrongArrayLength")]
struct WrongArrayLength {
    payload: [u8; 4],
}

fn build<'a>(
    init: <WrongArrayLength as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<WrongArrayLength>, up_rust::UWireError> {
    init.payload_from_array(&[1, 2, 3]).finish()
}

fn main() {}
