use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.Header")]
struct Header {
    case_id: u32,
    sequence: u32,
}

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.NestedBuilder")]
struct NestedBuilder {
    header: Header,
    checksum: u32,
}

fn build<'a>(
    init: <NestedBuilder as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<NestedBuilder>, up_rust::UWireError> {
    init.checksum(3)
        .header(|header| header.sequence(2).case_id(1).finish())?
        .finish()
}

fn main() {}
