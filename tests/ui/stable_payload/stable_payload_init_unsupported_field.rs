use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.UnsupportedField")]
struct UnsupportedField {
    value: *const u32,
}

fn main() {}
