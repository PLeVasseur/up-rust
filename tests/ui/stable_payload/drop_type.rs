use up_rust::StablePayload;

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "example.BadDrop")]
struct BadDrop {
    value: u32,
}

impl Drop for BadDrop {
    fn drop(&mut self) {}
}

fn main() {}
