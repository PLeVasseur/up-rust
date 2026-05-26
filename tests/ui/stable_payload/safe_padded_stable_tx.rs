use up_rust::{payload::EncodePayload, StableContainerPayload, StablePayload};

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "example.PaddedTx")]
struct PaddedTx {
    small: u8,
    large: u32,
}

fn main() {
    let value = PaddedTx { small: 1, large: 2 };
    let _ = <StableContainerPayload<PaddedTx> as EncodePayload<PaddedTx>>::payload_layout(&value);
}
