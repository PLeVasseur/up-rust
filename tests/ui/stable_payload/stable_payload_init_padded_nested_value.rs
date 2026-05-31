use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.PaddedHeader")]
struct PaddedHeader {
    small: u8,
    large: u32,
}

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.PaddedNestedValue")]
struct PaddedNestedValue {
    header: PaddedHeader,
}

fn build<'a>(
    init: <PaddedNestedValue as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<PaddedNestedValue>, up_rust::UWireError> {
    init.header_value(PaddedHeader { small: 1, large: 2 })
        .finish()
}

fn main() {}
