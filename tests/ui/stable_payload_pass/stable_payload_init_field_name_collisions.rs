use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.Collisions")]
struct Collisions {
    payload: [u8; 4],
    payload_from_slice: u32,
    finish: u32,
}

fn build<'a>(
    init: <Collisions as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<Collisions>, up_rust::UWireError> {
    init.payload([1, 2, 3, 4])
        .payload_from_slice(5)
        .finish(6)
        .finish()
}

fn main() {}
