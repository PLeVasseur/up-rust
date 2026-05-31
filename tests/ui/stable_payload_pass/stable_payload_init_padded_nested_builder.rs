use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.PaddedNestedHeader")]
struct PaddedNestedHeader {
    small: u8,
    large: u32,
}

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.PaddedNestedFrame")]
struct PaddedNestedFrame {
    header: PaddedNestedHeader,
    checksum: u32,
}

fn build<'a>(
    init: <PaddedNestedFrame as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<PaddedNestedFrame>, up_rust::UWireError> {
    init.header(|header| header.large(2).small(1).finish())?
        .checksum(3)
        .finish()
}

fn main() {}
