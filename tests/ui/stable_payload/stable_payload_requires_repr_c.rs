use up_rust::StablePayload;

#[derive(StablePayload)]
#[stable_payload(type_name = "test.NoRepr")]
struct NoRepr {
    word: u32,
}

fn main() {}
