//! One mixed send/receive implementer written with plain `async fn` methods (no macro),
//! and one stored, non-generic L1 application handle.
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use up_rust::prelude::*;
use up_rust::{
    serialized_dispatch::{InlineOwnedTransport, StaticOwnedTransport},
    *,
};

struct Mailbox {
    frames: Mutex<VecDeque<UOwnedFrame>>,
    capacity: usize,
}
impl StaticOwnedTransport for Mailbox {
    fn admit_payload_contract(
        &self,
        identity: PayloadIdentity,
        _: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        if matches!(identity, PayloadIdentity::Fixed(_)) {
            Ok(())
        } else {
            Err(UWireError::invalid_payload("mailbox has no native storage"))
        }
    }
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        let mut frames = self.frames.lock().expect("mailbox lock poisoned");
        if frames.len() == self.capacity {
            return Err(UStatus::fail_with_code(
                UCode::ResourceExhausted,
                "mailbox full",
            ));
        }
        frames.push_back(frame);
        Ok(())
    }
    async fn receive_validated_owned(
        &self,
        _source: &UUri,
        _sink: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        self.frames
            .lock()
            .expect("mailbox lock poisoned")
            .pop_front()
            .ok_or_else(|| UStatus::fail_with_code(UCode::NotFound, "mailbox empty"))
    }
}
struct Application {
    transport: Arc<dyn UOwnedTransport>,
}
impl Application {
    async fn send(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.transport.send_owned(frame).await
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), UStatus> {
    let mailbox = Arc::new(InlineOwnedTransport::new(Mailbox {
        frames: Mutex::new(VecDeque::with_capacity(8)),
        capacity: 8,
    }));
    let application = Application {
        transport: mailbox.clone(),
    };
    let frame = UOwnedFrame::new(
        UFrameMetadata::publish(
            UUri::for_entity("mailbox", UEntityId::first_instance(1), 1, 0x8001).unwrap(),
        )
        .build()
        .unwrap(),
        None,
    )
    .unwrap();
    application.send(frame).await?;
    let received = mailbox
        .inner()
        .frames
        .lock()
        .expect("mailbox lock poisoned")
        .pop_front();
    assert!(received.is_some());
    Ok(())
}
