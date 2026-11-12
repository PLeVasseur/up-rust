use up_rust::StablePayload;

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "example.Generic", inventory)]
struct Generic<T: 'static> {
    value: T,
}

fn main() {}
