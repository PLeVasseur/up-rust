use up_rust::{NativeByteOrder, NativeRepresentation, StablePayload, StablePayloadField};

struct Handle(u32);

impl Drop for Handle {
    fn drop(&mut self) {}
}

unsafe impl StablePayloadField for Handle {
    const FIELD_BITS_ALWAYS_VALID: bool = true;

    fn field_representation() -> NativeRepresentation {
        NativeRepresentation::new("test.handle", 0, 4, 4, NativeByteOrder::native(), vec![]).unwrap()
    }

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
