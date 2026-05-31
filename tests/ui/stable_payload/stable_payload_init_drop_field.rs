use up_rust::{StablePayload, StablePayloadInit};

struct Droppy(u32);

impl Drop for Droppy {
    fn drop(&mut self) {}
}

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.fail.DropField")]
struct DropField {
    value: Droppy,
}

fn main() {}
