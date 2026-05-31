use up_rust::{ByteBackedStablePayload, StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, ByteBackedStablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.ValueHeader")]
struct ValueHeader {
    case_id: u32,
    sequence: u32,
}

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.NestedValue")]
struct NestedValue {
    header: ValueHeader,
    checksum: u32,
}

fn build<'a>(
    init: <NestedValue as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<NestedValue>, up_rust::UWireError> {
    init.header_value(ValueHeader {
        case_id: 1,
        sequence: 2,
    })
    .checksum(3)
    .finish()
}

fn main() {}
