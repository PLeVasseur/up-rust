use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.ArrayElement")]
struct ArrayElement {
    small: u8,
    large: u32,
}

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.ArrayNested")]
struct ArrayNested {
    elements: [ArrayElement; 2],
}

fn build<'a>(
    init: <ArrayNested as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<ArrayNested>, up_rust::UWireError> {
    init.elements(|index, element| element.large(index as u32).small(1).finish())?
        .finish()
}

fn main() {}
