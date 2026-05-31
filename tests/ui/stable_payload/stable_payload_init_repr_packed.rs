use up_rust::{StablePayload, StablePayloadInit};

#[repr(C, packed)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.Packed")]
struct Packed {
    small: u8,
    large: u32,
}

fn main() {}
