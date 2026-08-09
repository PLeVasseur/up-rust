use up_rust::StablePayloadField;

struct Manual(u8);

unsafe impl StablePayloadField for Manual {
    fn validate_field_bytes(bytes: &[u8]) -> bool {
        bytes == [0]
    }
}

fn main() {}
