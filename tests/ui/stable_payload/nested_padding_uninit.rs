use up_rust::{ByteBackedStablePayload, StablePayload};

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "example.PaddedField")]
struct PaddedField {
    small: u8,
    large: u32,
}

#[repr(C)]
#[derive(StablePayload, ByteBackedStablePayload)]
#[stable_payload(type_name = "example.Outer")]
struct Outer {
    field: PaddedField,
}

fn main() {}
