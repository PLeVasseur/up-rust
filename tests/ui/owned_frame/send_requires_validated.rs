use up_rust::{UFrameMetadata, UOwnedFrame, UOwnedTransport, UOwnedTransportImpl, UStatus, UUri};

struct Transport;

#[async_trait::async_trait]
impl UOwnedTransportImpl for Transport {
    async fn send_validated_owned(&self, _frame: UOwnedFrame) -> Result<(), UStatus> {
        Ok(())
    }
}

async fn send_unvalidated(transport: &Transport) -> Result<(), UStatus> {
    let topic = UUri::try_from_parts("vehicle", 0x0001_4210, 1, 0x9000).unwrap();
    let metadata = UFrameMetadata::publish(topic).build().unwrap();
    let unvalidated = UOwnedFrame::new_unchecked(metadata, None);
    transport.send_owned(unvalidated).await
}

fn main() {
    let _ = send_unvalidated;
}
use up_rust::prelude::*;
