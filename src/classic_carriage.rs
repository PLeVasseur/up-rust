//! One classic message boundary for the primary typed roles.
use crate::{
    OwnedAttachment, PayloadIdentity, UCode, UListener, UMessage, UOwnedFrame, UOwnedListener,
    UOwnedTransportImpl, UStatus, UTransport, UUri, UWireError,
};
use std::sync::Arc;

/// Payload-agnostic classic `.proto` carriage. Native-token projection always
/// refuses; this adapter is never a native loan capability. Scoped reception
/// requires the underlying binding's real synchronous-release attachment hook.
pub struct ClassicCarriage<T: UTransport + ?Sized> {
    transport: Arc<T>,
}
impl<T: UTransport + ?Sized> std::fmt::Debug for ClassicCarriage<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClassicCarriage").finish_non_exhaustive()
    }
}
impl<T: UTransport + ?Sized> ClassicCarriage<T> {
    /// Uses an existing classic binding; no serializer or negotiation is added.
    pub fn new(transport: Arc<T>) -> Self {
        Self { transport }
    }
}
fn project(message: UMessage) -> Result<UOwnedFrame, UStatus> {
    let metadata = crate::frame::metadata::try_project_umessage_to_frame_metadata(&message)
        .map_err(|e| UStatus::fail_with_code(UCode::InvalidArgument, e.to_string()))?;
    // Option<Bytes> retains absent versus present-empty and shares owned bytes.
    UOwnedFrame::new(metadata, message.payload())
        .map_err(|e| UStatus::fail_with_code(UCode::InvalidArgument, e.to_string()))
}
struct Listener {
    source: UUri,
    sink: Option<UUri>,
    inner: Arc<dyn UOwnedListener>,
}
#[async_trait::async_trait]
impl UListener for Listener {
    async fn on_receive(&self, message: UMessage) {
        if !self.source.matches(message.source())
            || match (&self.sink, message.sink()) {
                (None, None) => false,
                (Some(filter), Some(actual)) => !filter.matches(actual),
                _ => true,
            }
        {
            return;
        }
        if let Ok(frame) = project(message) {
            self.inner.on_receive_owned(frame).await;
        }
    }
}
#[async_trait::async_trait]
impl<T: UTransport + ?Sized> UOwnedTransportImpl for ClassicCarriage<T> {
    fn admit_payload_contract(
        &self,
        identity: PayloadIdentity,
        _family: Option<crate::wire::WireIdentity>,
    ) -> Result<(), UWireError> {
        if matches!(identity, PayloadIdentity::Fixed(_)) {
            Ok(())
        } else {
            Err(UWireError::invalid_payload(
                "classic carriage refuses native identity",
            ))
        }
    }
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        let (metadata, payload) = frame.into_parts();
        let message = crate::frame::metadata::try_project_frame_to_umessage(metadata, payload)
            .map_err(|e| UStatus::fail_with_code(UCode::InvalidArgument, e.to_string()))?;
        self.transport.send(message).await
    }
    async fn receive_validated_owned(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        loop {
            let frame = project(self.transport.receive(source, sink).await?)?;
            if source.matches(frame.metadata().source())
                && match (sink, frame.metadata().sink()) {
                    (None, None) => true,
                    (Some(filter), Some(actual)) => filter.matches(actual),
                    _ => false,
                }
            {
                return Ok(frame);
            }
        }
    }
    async fn attach_validated_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<OwnedAttachment, UStatus> {
        let wrapped = Arc::new(Listener {
            source: source.clone(),
            sink: sink.cloned(),
            inner: listener,
        });
        // The returned physical handle owns this exact wrapper. No per-message
        // listener map or pointer reconstruction; cancellation is binding-owned.
        self.transport.attach_listener(source, sink, wrapped).await
    }
}
