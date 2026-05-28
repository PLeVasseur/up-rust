use up_rust::{payload::StableContainerPayload, UFrameMetadata, UOwnedFrame, UUri};

#[repr(C)]
#[derive(up_rust::StablePayload)]
#[stable_payload(type_name = "example.OwnedStableBorrow")]
struct OwnedStableBorrow {
    value: u32,
}

fn main() {
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(UUri::try_from("//vehicle/4210/1/9000").unwrap())
            .with_encoding(StableContainerPayload::<OwnedStableBorrow>::encoding()),
        vec![0_u8; core::mem::size_of::<OwnedStableBorrow>()],
    );

    let _ = frame
        .borrow_payload_as::<StableContainerPayload<OwnedStableBorrow>, OwnedStableBorrow>()
        .unwrap();
}
