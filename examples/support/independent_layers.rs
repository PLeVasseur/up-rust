//! Four-audience examples use actual L1 contracts, not business-handler aliases.
use bytes::Bytes;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use up_rust::{
    EncodePayload, LocalUriProvider, OwnedAttachment, OwnedWireContext, OwnedWireDeclaration,
    OwnedWireTransport, PayloadIdentity, UFrameMetadata, UOwnedFrame, UOwnedListener,
    UOwnedTransport, UOwnedTransportCore, UOwnedTransportImpl, UStatus, UUri, UWireError,
    UWireMetadataCodec, WireIdentity,
};

#[derive(Debug, thiserror::Error)]
pub enum LayerError {
    #[error("payload or envelope declaration: {0}")]
    Payload(#[from] UWireError),
    #[error("carriage: {0}")]
    Transport(UStatus),
}

pub fn selected_wire<T: UOwnedTransportCore, C: UWireMetadataCodec>(
    core: T,
    metadata_codec: C,
    local: OwnedWireDeclaration,
    independently_provisioned_peer: &OwnedWireDeclaration,
) -> Result<OwnedWireTransport<T, C>, LayerError> {
    let context = OwnedWireContext::agree::<C>(local, independently_provisioned_peer)?;
    OwnedWireTransport::new(core, metadata_codec, context).map_err(LayerError::Transport)
}

pub struct CountedCarriage {
    pub inner: Arc<dyn UOwnedTransport>,
    pub completed_sends: AtomicUsize,
}

#[async_trait::async_trait]
impl UOwnedTransportImpl for CountedCarriage {
    fn admit_payload_contract(
        &self,
        identity: PayloadIdentity,
        family: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        self.inner.admit_payload_contract(identity, family)
    }
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.inner.send_owned(frame).await?;
        self.completed_sends.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    async fn attach_validated_owned_listener(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<OwnedAttachment, UStatus> {
        self.inner
            .attach_owned_listener(source, sink, listener)
            .await
    }
}

pub async fn direct_l1_publish(
    transport: &dyn UOwnedTransport,
    local: &dyn LocalUriProvider,
    value: &super::schema::Reading,
) -> Result<(), LayerError> {
    use up_rust::PayloadCodec;
    type Mapping = super::schema::ReadingProtobuf;
    let identity = Mapping::payload_identity(None)?;
    transport.admit_payload_contract(identity, Mapping::payload_family())?;
    let metadata =
        UFrameMetadata::publish(local.get_resource_uri(super::schema::TEMPERATURE.resource()))
            .build()
            .map_err(|error| UWireError::invalid_payload(error.to_string()))?;
    let payload = Mapping::encode_payload_owned(value)?;
    let frame = UOwnedFrame::with_payload(identity.apply_to(metadata)?, payload)
        .map_err(|error| UWireError::invalid_payload(error.to_string()))?;
    transport
        .send_owned(frame)
        .await
        .map_err(LayerError::Transport)
}

pub struct PublicationProtocol<P> {
    transport: Arc<dyn UOwnedTransport>,
    source: UUri,
    identity: PayloadIdentity,
    encode: fn(&P) -> Result<Bytes, UWireError>,
}

impl<P> PublicationProtocol<P> {
    pub fn for_schema<Mapping: EncodePayload<P>>(
        transport: Arc<dyn UOwnedTransport>,
        local: &dyn LocalUriProvider,
        event: &up_rust::communication::application::Event<P>,
    ) -> Result<Self, LayerError> {
        let identity = Mapping::payload_identity(None)?;
        if !matches!(identity, PayloadIdentity::Fixed(_)) {
            return Err(
                UWireError::invalid_payload("owned protocol cannot carry native payloads").into(),
            );
        }
        transport.admit_payload_contract(identity, Mapping::payload_family())?;
        let source = local.get_resource_uri(event.resource());
        identity.apply_to(
            UFrameMetadata::publish(source.clone())
                .build()
                .map_err(|error| UWireError::invalid_payload(error.to_string()))?,
        )?;
        Ok(Self {
            transport,
            source,
            identity,
            encode: Mapping::encode_payload_owned,
        })
    }

    pub async fn publish(&self, value: &P) -> Result<(), LayerError> {
        let metadata = UFrameMetadata::publish(self.source.clone())
            .build()
            .map_err(|error| UWireError::invalid_payload(error.to_string()))?;
        let frame =
            UOwnedFrame::with_payload(self.identity.apply_to(metadata)?, (self.encode)(value)?)
                .map_err(|error| UWireError::invalid_payload(error.to_string()))?;
        self.transport
            .send_owned(frame)
            .await
            .map_err(LayerError::Transport)
    }
}
