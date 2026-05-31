use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.RawBool")]
struct RawBool {
    flag: bool,
}

fn build<'a>(
    init: <RawBool as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<RawBool>, up_rust::UWireError> {
    init.flag_from_slice(&[1])?.finish()
}

fn main() {}
