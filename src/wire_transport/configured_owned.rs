//! Serialized selected-envelope context, admitted independently of payload type.
use super::owned::{EncodedOwnedFrame, PreparedOwnedFrame};
use super::*;
use crate::wire::{UWireMetadataCodec, WireIdentity};

/// An explicitly provisioned full wire/family/layout/version combination.
/// It assigns no registry value and performs no negotiation.
#[derive(Clone, Copy, Debug)]
pub struct OwnedWireDeclaration {
    /// Full-wire identity whose declared meaning includes this entire combination.
    pub wire: WireIdentity,
    /// Actual codec operation family, independent of payload encoding/schema ID.
    pub payload_family: WireIdentity,
    /// Codec's metadata layout identity.
    pub metadata_layout: WireIdentity,
    /// Exact format version.
    pub format_version: u16,
}
/// Matching independently provisioned local/peer declarations for serialized carriage.
#[derive(Clone, Copy, Debug)]
pub struct OwnedWireContext {
    context: UWireMetadataContext,
}
impl OwnedWireContext {
    /// Checks exact agreement and prevents redefining the SDK's existing presets.
    /// Private identities must first be admitted by deployment identity tooling;
    /// passing local as peer is only a fixture, never deployment verification.
    ///
    /// # Errors
    /// Refuses peer/context mismatch, invalid format and relabelled existing presets.
    pub fn agree<C: UWireMetadataCodec>(
        local: OwnedWireDeclaration,
        peer: &OwnedWireDeclaration,
    ) -> Result<Self, crate::UWireError> {
        if local.wire != peer.wire
            || local.payload_family != peer.payload_family
            || local.metadata_layout != peer.metadata_layout
            || local.format_version != peer.format_version
        {
            return Err(crate::UWireError::invalid_payload(
                "owned wire peer context mismatch",
            ));
        }
        if local.metadata_layout != C::METADATA_LAYOUT_ID
            || local.format_version != crate::wire::FORMAT_VERSION
        {
            return Err(crate::UWireError::invalid_payload(
                "owned wire layout/version mismatch",
            ));
        }
        for (wire, family) in [
            (
                crate::wire::UPROTOCOL_NATIVE_WIRE_ID,
                crate::wire::NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID,
            ),
            (
                crate::wire::PROTOBUF_WIRE_ID,
                crate::wire::PROTOBUF_PAYLOAD_FAMILY_ID,
            ),
            (
                crate::wire::XCDR_V2_WIRE_ID,
                crate::wire::XCDR_V2_PAYLOAD_FAMILY_ID,
            ),
        ] {
            if local.wire.literal_id() == wire.literal_id()
                && (local.wire != wire || local.payload_family != family)
            {
                return Err(crate::UWireError::invalid_payload(
                    "cannot redefine an existing selected-wire preset",
                ));
            }
        }
        if local.wire.literal_id() == "org.eclipse.uprotocol.wire.stable-container" {
            return Err(crate::UWireError::invalid_payload(
                "native stable-container context requires its native setup",
            ));
        }
        let context = UWireMetadataContext::new_exact(
            local.wire,
            local.payload_family,
            local.metadata_layout,
            local.format_version,
        );
        Ok(Self { context })
    }
    /// Exact admitted context for declaration tooling and diagnostics.
    pub fn metadata_context(self) -> UWireMetadataContext {
        self.context
    }
}

/// One configured encoded-owned adapter shared by all five application roles.
/// Old `UWireTransport<W,C>` remains an exact preset; this adapter admits a new
/// explicit full-wire combination without pretending it is an old W preset.
pub struct OwnedWireTransport<T, C> {
    core: T,
    codec: C,
    context: OwnedWireContext,
}
impl<T, C> std::fmt::Debug for OwnedWireTransport<T, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedWireTransport")
            .field("context", &self.context)
            .finish_non_exhaustive()
    }
}
impl<T, C: UWireMetadataCodec> OwnedWireTransport<T, C> {
    /// Validates codec framing against the admitted context before role setup.
    ///
    /// # Errors
    /// Refuses invalid identity-reference combinations or unsupported byte format.
    pub fn new(core: T, codec: C, context: OwnedWireContext) -> Result<Self, UStatus> {
        if context.context.metadata_layout_id != C::METADATA_LAYOUT_ID {
            return Err(UStatus::fail_with_code(
                UCode::InvalidArgument,
                "admitted layout differs from codec",
            ));
        }
        let probe = crate::UFrameMetadata::publish(
            crate::UUri::try_from_wire_parts("context-check", 0x0001_0001, 1, 0x8001)
                .map_err(|e| UStatus::fail_with_code(UCode::InvalidArgument, e.to_string()))?,
        )
        .build()
        .map_err(|e| UStatus::fail_with_code(UCode::InvalidArgument, e.to_string()))?;
        let bytes = codec.encode_frame_metadata(context.context, &probe)?;
        let decoded = codec.decode_frame_metadata(context.context, &bytes)?;
        if decoded != probe {
            return Err(UStatus::fail_with_code(
                UCode::InvalidArgument,
                "context codec did not round trip semantic metadata",
            ));
        }
        Ok(Self {
            core,
            codec,
            context,
        })
    }
}
struct Listener<C> {
    codec: C,
    context: UWireMetadataContext,
    source: UUri,
    sink: Option<UUri>,
    inner: Arc<dyn UOwnedListener>,
}
#[async_trait]
impl<C: UWireMetadataCodec + Send + Sync> UEncodedOwnedListener for Listener<C> {
    async fn on_receive_encoded_owned(&self, raw: EncodedOwnedFrame) {
        if let Ok(frame) = raw.decode_with_context(&self.codec, self.context) {
            let m = frame.metadata();
            if self.source.matches(m.source())
                && match (&self.sink, m.sink()) {
                    (None, None) => true,
                    (Some(f), Some(v)) => f.matches(v),
                    _ => false,
                }
            {
                self.inner.on_receive_owned(frame).await;
            }
        }
    }
}
#[async_trait]
impl<T: UOwnedTransportCore, C: UWireMetadataCodec + Clone + Send + Sync + 'static>
    UOwnedTransportImpl for OwnedWireTransport<T, C>
{
    fn admit_payload_contract(
        &self,
        identity: crate::PayloadIdentity,
        family: Option<WireIdentity>,
    ) -> Result<(), crate::UWireError> {
        if matches!(identity, crate::PayloadIdentity::Fixed(_))
            && family == Some(self.context.context.payload_family_id)
        {
            Ok(())
        } else {
            Err(crate::UWireError::invalid_payload(
                "codec does not match admitted owned wire family",
            ))
        }
    }
    fn dispatch_owned(&self, frame: UOwnedFrame) -> crate::inline_dispatch::SendFuture<'_> {
        crate::inline_dispatch::SendFuture::new(async move {
            self.core
                .dispatch_prepared_owned(PreparedOwnedFrame::from_context(
                    frame,
                    &self.codec,
                    self.context.context,
                )?)
                .await
        })
    }

    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.core
            .send_prepared_owned(PreparedOwnedFrame::from_context(
                frame,
                &self.codec,
                self.context.context,
            )?)
            .await
    }
    async fn receive_validated_owned(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        loop {
            let frame = self
                .core
                .receive_encoded_owned(source, sink)
                .await?
                .decode_with_context(&self.codec, self.context.context)?;
            if source.matches(frame.metadata().source())
                && match (sink, frame.metadata().sink()) {
                    (None, None) => true,
                    (Some(f), Some(v)) => f.matches(v),
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
        inner: Arc<dyn UOwnedListener>,
    ) -> Result<crate::OwnedAttachment, UStatus> {
        self.core
            .attach_encoded_owned_listener(
                source,
                sink,
                Arc::new(Listener {
                    codec: self.codec.clone(),
                    context: self.context.context,
                    source: source.clone(),
                    sink: sink.cloned(),
                    inner,
                }),
            )
            .await
    }
}
