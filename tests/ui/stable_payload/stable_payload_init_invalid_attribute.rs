use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.InvalidAttr")]
#[stable_payload_init(rename = "bad")]
struct InvalidAttr {
    value: u32,
}

fn main() {}
