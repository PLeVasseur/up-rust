use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.DuplicateNestedHeader")]
struct Header {
    sequence: u32,
}

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.DuplicateNested")]
struct DuplicateNested {
    header: Header,
}

fn build<'a>(
    init: <DuplicateNested as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<DuplicateNested>, up_rust::UWireError> {
    init.header(|header| header.sequence(1).finish())?
        .header(|header| header.sequence(2).finish())?
        .finish()
}

fn main() {}
