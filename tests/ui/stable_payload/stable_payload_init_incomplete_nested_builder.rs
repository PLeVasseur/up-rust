use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.IncompleteHeader")]
struct Header {
    case_id: u32,
    sequence: u32,
}

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.IncompleteNested")]
struct IncompleteNested {
    header: Header,
}

fn build<'a>(
    init: <IncompleteNested as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<IncompleteNested>, up_rust::UWireError> {
    init.header(|header| header.case_id(1).finish())?.finish()
}

fn main() {}
