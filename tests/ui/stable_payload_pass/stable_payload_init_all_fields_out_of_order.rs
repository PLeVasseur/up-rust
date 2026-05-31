use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.OutOfOrder")]
struct OutOfOrder {
    sequence: u32,
    checksum: u32,
    flags: u16,
}

fn build<'a>(
    init: <OutOfOrder as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<OutOfOrder>, up_rust::UWireError> {
    init.checksum(2).flags(3).sequence(1).finish()
}

fn main() {}
