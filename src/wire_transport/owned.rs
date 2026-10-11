/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/
//! Encoded owned carriage and listener composition.

use super::*;

#[cfg(feature = "owned-frame-transport")]
fn selected_wire_metadata_matches(
    metadata: &UFrameMetadata,
    source: &UUri,
    sink: Option<&UUri>,
) -> bool {
    source.matches(metadata.source())
        && match (sink, metadata.sink()) {
            (None, None) => true,
            (Some(filter), Some(actual)) => filter.matches(actual),
            _ => false,
        }
}

#[cfg(feature = "owned-frame-transport")]
pub(super) fn selected_wire_core_source_filter_for(source_filter: &UUri) -> UUri {
    // Preserve useful authority/entity scope even in a partial wildcard. The
    // binding chooses any necessary physical broadening; decoded metadata is
    // still filtered independently before public delivery.
    source_filter.clone()
}

#[cfg(feature = "owned-frame-transport")]
/// Prepared owned frame passed from the adapter to an owned core.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedOwnedFrame {
    metadata: UFrameMetadata,
    encoded_metadata: Vec<u8>,
    payload: Option<Bytes>,
}

#[cfg(feature = "owned-frame-transport")]
impl PreparedOwnedFrame {
    /// Encodes validated owned frame metadata for a selected wire.
    ///
    /// # Errors
    ///
    /// Returns an error if selected-wire metadata encoding fails.
    pub fn from_validated<W, C>(frame: UOwnedFrame, codec: &C) -> Result<Self, UStatus>
    where
        W: UWire,
        C: UWireMetadataCodecFor<W>,
    {
        let (metadata, payload) = frame.into_parts();
        let encoded_metadata =
            codec.encode_frame_metadata(UWireMetadataContext::from_wire::<W, C>(), &metadata)?;
        Ok(Self {
            metadata,
            encoded_metadata,
            payload,
        })
    }

    /// Returns decoded frame metadata.
    #[must_use]
    pub fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    /// Returns selected-wire encoded metadata bytes.
    #[must_use]
    pub fn encoded_metadata(&self) -> &[u8] {
        &self.encoded_metadata
    }

    /// Returns owned payload bytes, if present.
    #[must_use]
    pub fn payload(&self) -> Option<&Bytes> {
        self.payload.as_ref()
    }

    /// Consumes this frame and returns its parts.
    #[must_use]
    pub fn into_parts(self) -> (UFrameMetadata, Vec<u8>, Option<Bytes>) {
        (self.metadata, self.encoded_metadata, self.payload)
    }
}

#[cfg(feature = "owned-frame-transport")]
/// Encoded owned frame returned by an owned core.
#[derive(Clone, Debug, PartialEq)]
pub struct EncodedOwnedFrame {
    encoded_metadata: Vec<u8>,
    payload: Option<Bytes>,
}

#[cfg(feature = "owned-frame-transport")]
impl EncodedOwnedFrame {
    /// Creates an encoded owned frame from selected-wire metadata bytes and payload.
    #[must_use]
    pub fn new(encoded_metadata: impl Into<Vec<u8>>, payload: Option<Bytes>) -> Self {
        Self {
            encoded_metadata: encoded_metadata.into(),
            payload,
        }
    }

    /// Returns selected-wire encoded metadata bytes.
    #[must_use]
    pub fn encoded_metadata(&self) -> &[u8] {
        &self.encoded_metadata
    }

    /// Returns owned payload bytes, if present.
    #[must_use]
    pub fn payload(&self) -> Option<&Bytes> {
        self.payload.as_ref()
    }

    /// Decodes this raw frame into a public owned frame.
    ///
    /// # Errors
    ///
    /// Returns an error if selected-wire metadata decode or owned-frame validation fails.
    pub fn decode<W, C>(self, codec: &C) -> Result<UOwnedFrame, UStatus>
    where
        W: UWire,
        C: UWireMetadataCodecFor<W>,
    {
        let metadata = codec.decode_frame_metadata(
            UWireMetadataContext::from_wire::<W, C>(),
            &self.encoded_metadata,
        )?;
        UOwnedFrame::new(metadata, self.payload).map_err(invalid_metadata)
    }

    /// Consumes this frame and returns its parts.
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, Option<Bytes>) {
        (self.encoded_metadata, self.payload)
    }
}

#[cfg(feature = "owned-frame-transport")]
/// Listener used by cores to deliver raw encoded owned frames.
#[async_trait]
pub trait UEncodedOwnedListener: Send + Sync {
    /// Handles one raw encoded owned frame.
    async fn on_receive_encoded_owned(&self, frame: EncodedOwnedFrame);
}

/// *Role: implemented by transports carrying already-encoded owned frames; the wire adapter composes above it — see the [trait map](crate::guide::trait_map).*
///
#[cfg(feature = "owned-frame-transport")]
/// Encoded physical owned-frame mechanics implemented by product transports.
///
/// Implementing this core buys selected-wire metadata encoding/decoding,
/// identity rejection, semantic frame validation, and the public owned-frame
/// transport API from [`UWireTransport`]. The required send receives metadata
/// already encoded for the selected profile. Pull receive and listener hooks
/// default to unsupported.
#[async_trait]
pub trait UOwnedTransportCore: Send + Sync {
    /// Sends an owned frame with already encoded metadata bytes.
    async fn send_prepared_owned(&self, frame: PreparedOwnedFrame) -> Result<(), UStatus>;

    /// Receives one matching raw encoded owned frame from cores that support pull receive.
    async fn receive_encoded_owned(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<EncodedOwnedFrame, UStatus> {
        Err(unimplemented())
    }

    /// Registers a raw encoded owned listener after public filter validation.
    async fn register_encoded_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UEncodedOwnedListener>,
    ) -> Result<(), UStatus> {
        Err(unimplemented())
    }

    /// Unregisters a raw encoded owned listener after public filter validation.
    async fn unregister_encoded_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        _listener: Arc<dyn UEncodedOwnedListener>,
    ) -> Result<(), UStatus> {
        Err(unimplemented())
    }
}

#[cfg(feature = "owned-frame-transport")]
#[async_trait]
impl<TCore, W, C> UOwnedTransportImpl for UWireTransport<TCore, W, C>
where
    TCore: UOwnedTransportCore,
    W: UWire + Send + Sync + 'static,
    C: UWireMetadataCodecFor<W> + Clone + Send + Sync + 'static,
{
    async fn send_validated_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.core
            .send_prepared_owned(PreparedOwnedFrame::from_validated::<W, C>(
                frame,
                &self.metadata_codec,
            )?)
            .await
    }

    async fn receive_validated_owned(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
    ) -> Result<UOwnedFrame, UStatus> {
        let core_source_filter = selected_wire_core_source_filter_for(source_filter);
        loop {
            let frame = self
                .core
                .receive_encoded_owned(&core_source_filter, sink_filter)
                .await?
                .decode::<W, C>(&self.metadata_codec)?;
            if owned_frame_matches(&frame, source_filter, sink_filter) {
                return Ok(frame);
            }
        }
    }

    async fn register_validated_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        let key = listener_key(
            source_filter,
            sink_filter,
            owned_listener_pointer(&listener),
        );
        let (listener, inserted) =
            self.registered_owned_listener(&key, source_filter, sink_filter, listener);
        let core_source_filter = selected_wire_core_source_filter_for(source_filter);
        let result = self
            .core
            .register_encoded_owned_listener(&core_source_filter, sink_filter, listener)
            .await;
        if result.is_err() && inserted {
            self.owned_listeners
                .lock()
                .expect("wire owned listener registry lock poisoned")
                .remove(&key);
        }
        result
    }

    async fn unregister_validated_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        let key = listener_key(
            source_filter,
            sink_filter,
            owned_listener_pointer(&listener),
        );
        let listener = self.owned_listener_for_unregister(&key, listener);
        let core_source_filter = selected_wire_core_source_filter_for(source_filter);
        let result = self
            .core
            .unregister_encoded_owned_listener(&core_source_filter, sink_filter, listener)
            .await;
        if result.is_ok() {
            self.owned_listeners
                .lock()
                .expect("wire owned listener registry lock poisoned")
                .remove(&key);
        }
        result
    }
}

#[cfg(feature = "owned-frame-transport")]
impl<TCore, W, C> UWireTransport<TCore, W, C>
where
    TCore: UOwnedTransportCore,
    W: UWire + Send + Sync + 'static,
    C: UWireMetadataCodecFor<W> + Clone + Send + Sync + 'static,
{
    fn registered_owned_listener(
        &self,
        key: &WireListenerKey,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> (Arc<dyn UEncodedOwnedListener>, bool) {
        let mut registry = self
            .owned_listeners
            .lock()
            .expect("wire owned listener registry lock poisoned");
        if let Some(existing) = registry.get(key) {
            if let Ok(existing) = existing.clone().downcast::<WireOwnedListener<W, C>>() {
                return (existing, false);
            }
        }

        let wrapped = Arc::new(WireOwnedListener::<W, C> {
            source_filter: source_filter.clone(),
            sink_filter: sink_filter.cloned(),
            listener,
            metadata_codec: self.metadata_codec.clone(),
            _wire: PhantomData,
        });
        registry.insert(key.clone(), wrapped.clone());
        (wrapped, true)
    }

    fn owned_listener_for_unregister(
        &self,
        key: &WireListenerKey,
        fallback: Arc<dyn UOwnedListener>,
    ) -> Arc<dyn UEncodedOwnedListener> {
        self.owned_listeners
            .lock()
            .expect("wire owned listener registry lock poisoned")
            .get(key)
            .and_then(|listener| listener.clone().downcast::<WireOwnedListener<W, C>>().ok())
            .unwrap_or_else(|| {
                Arc::new(WireOwnedListener::<W, C> {
                    source_filter: key.source_filter.clone(),
                    sink_filter: key.sink_filter.clone(),
                    listener: fallback,
                    metadata_codec: self.metadata_codec.clone(),
                    _wire: PhantomData,
                })
            })
    }
}

#[cfg(feature = "owned-frame-transport")]
struct WireOwnedListener<W, C>
where
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: Arc<dyn UOwnedListener>,
    metadata_codec: C,
    _wire: PhantomData<W>,
}

#[cfg(feature = "owned-frame-transport")]
#[async_trait]
impl<W, C> UEncodedOwnedListener for WireOwnedListener<W, C>
where
    W: UWire + Send + Sync + 'static,
    C: UWireMetadataCodecFor<W> + Send + Sync + 'static,
{
    async fn on_receive_encoded_owned(&self, frame: EncodedOwnedFrame) {
        match frame.decode::<W, C>(&self.metadata_codec) {
            Ok(frame)
                if owned_frame_matches(&frame, &self.source_filter, self.sink_filter.as_ref()) =>
            {
                self.listener.on_receive_owned(frame).await;
            }
            Ok(_) => {}
            Err(error) => warn!(%error, "dropping invalid selected-wire owned frame"),
        }
    }
}

#[cfg(feature = "owned-frame-transport")]
fn owned_frame_matches(
    frame: &UOwnedFrame,
    source_filter: &UUri,
    sink_filter: Option<&UUri>,
) -> bool {
    selected_wire_metadata_matches(frame.metadata(), source_filter, sink_filter)
}

#[cfg(feature = "owned-frame-transport")]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct WireListenerKey {
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: usize,
}

#[cfg(feature = "owned-frame-transport")]
fn listener_key(
    source_filter: &UUri,
    sink_filter: Option<&UUri>,
    listener: usize,
) -> WireListenerKey {
    WireListenerKey {
        source_filter: source_filter.clone(),
        sink_filter: sink_filter.cloned(),
        listener,
    }
}

#[cfg(feature = "owned-frame-transport")]
fn owned_listener_pointer(listener: &Arc<dyn UOwnedListener>) -> usize {
    let ptr = Arc::as_ptr(listener);
    let thin_ptr = ptr as *const ();
    thin_ptr as usize
}

#[cfg(feature = "owned-frame-transport")]
fn unimplemented() -> UStatus {
    UStatus::fail_with_code(UCode::Unimplemented, "not implemented")
}

#[cfg(feature = "owned-frame-transport")]
fn invalid_metadata(error: crate::UFrameMetadataError) -> UStatus {
    UStatus::fail_with_code(UCode::InvalidArgument, error.to_string())
}
