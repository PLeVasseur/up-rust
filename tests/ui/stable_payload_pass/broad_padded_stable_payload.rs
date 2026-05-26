use up_rust::{
    payload::{BorrowPayload, StableContainerPayload},
    StablePayload,
};

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "example.BroadPadded")]
struct BroadPadded {
    small: u8,
    large: u32,
}

#[repr(C, align(4))]
struct AlignedBytes([u8; core::mem::size_of::<BroadPadded>()]);

fn main() {
    let mut storage = AlignedBytes([0; core::mem::size_of::<BroadPadded>()]);
    storage.0[0] = 1;
    storage.0[4..8].copy_from_slice(&2_u32.to_ne_bytes());

    let _ = <StableContainerPayload<BroadPadded> as BorrowPayload<BroadPadded>>::borrow_payload(
        storage.0.as_slice(),
    )
    .unwrap();
}
