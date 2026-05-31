use up_rust::{StablePayload, StablePayloadInit};

#[repr(transparent)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.Newtype")]
struct Newtype(u32);

fn build<'a>(
    init: <Newtype as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<Newtype>, up_rust::UWireError> {
    init.field0(7).finish()
}

fn main() {}
