use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.PublicType")]
pub struct PublicType {
    pub value: u32,
}

pub fn build<'a>(
    init: <PublicType as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<PublicType>, up_rust::UWireError> {
    init.value(1).finish()
}

fn main() {}
