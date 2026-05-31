use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.MissingNestedHeader")]
struct Header {
    sequence: u32,
}

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.MissingNested")]
struct MissingNested {
    header: Header,
    checksum: u32,
}

fn build<'a>(
    init: <MissingNested as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<MissingNested>, up_rust::UWireError> {
    init.checksum(1).finish()
}

fn main() {}
