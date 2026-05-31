use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.Unit")]
struct Unit;

fn build<'a>(
    init: <Unit as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<Unit>, up_rust::UWireError> {
    init.finish()
}

fn main() {}
