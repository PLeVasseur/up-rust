use up_rust::{ByteBackedStablePayload, StablePayload};

#[repr(C)]
#[derive(StablePayload, ByteBackedStablePayload)]
#[stable_payload(type_name = "example.Padded")]
struct Padded {
    small: u8,
    large: u32,
}

fn main() {}
