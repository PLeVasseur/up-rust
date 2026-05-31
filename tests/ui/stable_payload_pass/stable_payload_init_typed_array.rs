use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.TypedArray")]
struct TypedArray {
    values: [u32; 4],
}

fn build<'a>(
    init: <TypedArray as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<TypedArray>, up_rust::UWireError> {
    init.values_from_slice(&[1, 2, 3, 4])?.finish()
}

fn main() {}
