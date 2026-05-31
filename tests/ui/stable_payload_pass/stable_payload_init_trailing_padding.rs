use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.TrailingPadding")]
struct TrailingPadding {
    value: u32,
    tail: u8,
}

fn build<'a>(
    init: <TrailingPadding as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<TrailingPadding>, up_rust::UWireError> {
    init.tail(1).value(2).finish()
}

fn main() {}
