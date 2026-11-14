use up_rust::StablePayload;

enum Mode {
    Off,
    On,
}

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "test.EnumField")]
struct EnumField {
    mode: Mode,
    pad: [u8; 3],
}

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "test.ReferenceField")]
struct ReferenceField {
    text: &'static str,
}

fn main() {
    let _ = Mode::Off;
    let _ = Mode::On;
}
