use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.BoolChar")]
struct BoolChar {
    flag: bool,
    marker: char,
}

fn build<'a>(
    init: <BoolChar as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<BoolChar>, up_rust::UWireError> {
    init.marker('z').flag(true).finish()
}

fn main() {}
