use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.ImplicitPadding")]
struct ImplicitPadding {
    small: u8,
    large: u32,
}

fn build<'a>(
    init: <ImplicitPadding as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<ImplicitPadding>, up_rust::UWireError> {
    init.large(2).small(1).finish()
}

fn main() {}
