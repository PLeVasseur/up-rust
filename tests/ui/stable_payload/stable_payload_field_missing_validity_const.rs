use up_rust::{StableDescriptor, StablePayloadField};

struct Manual(u8);

unsafe impl StablePayloadField for Manual {
    const DESCRIPTOR: &'static StableDescriptor = &StableDescriptor {
        name: "manual",
        revision: 0,
        size: 1,
        alignment: 1,
        fields: &[],
    };

    fn validate_field_bytes(bytes: &[u8]) -> bool {
        bytes == [0]
    }
}

fn main() {}
