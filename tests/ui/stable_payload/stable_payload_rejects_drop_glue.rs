use up_rust::{StableDescriptor, StablePayload, StablePayloadField};

struct Handle(u32);

impl Drop for Handle {
    fn drop(&mut self) {}
}

unsafe impl StablePayloadField for Handle {
    const FIELD_BITS_ALWAYS_VALID: bool = true;

    const DESCRIPTOR: &'static StableDescriptor = &StableDescriptor {
        name: "test.handle",
        revision: 0,
        size: 4,
        alignment: 4,
        fields: &[],
    };

    fn validate_field_bytes(bytes: &[u8]) -> bool {
        bytes.len() == 4
    }
}

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "test.DropGlue")]
struct DropGlue {
    handle: Handle,
}

fn main() {
    let _ = DropGlue::FIELD_BITS_ALWAYS_VALID;
}
