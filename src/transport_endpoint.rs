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

use std::{
    fmt::{Debug, Formatter},
    marker::PhantomData,
    sync::Arc,
};

use async_trait::async_trait;

use crate::{
    utransport::{UZeroCopyListener, UZeroCopyTransport},
    validate_owned_frame_for_transport,
    zero_copy::{UTxBuffer, UZeroCopyPayloadCopyExt, UZeroCopyRxFrame},
    UCode, UOwnedFrame, UOwnedListener, UOwnedTransport, UStatus, UUri,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UOwnedFrameEndpointMode {
    /// The endpoint delegates owned-frame sends and listener registration to an
    /// [`UOwnedTransport`].
    Owned,
    /// The endpoint adapts an owned-frame routing API to a true
    /// [`UZeroCopyTransport`], copying at the adapter boundary.
    ZeroCopy,
}

/// Owned-frame routing facade over either owned or zero-copy transport capability.
///
/// This adapter is for routers and bridges that operate on [`UOwnedFrame`]. When
/// wrapping a zero-copy transport, it deliberately crosses the ownership
/// boundary: receives are copied from zero-copy leases into owned frames, and
/// sends are copied from owned frames into transmit loans.
#[derive(Clone)]
pub struct UOwnedFrameEndpoint {
    inner: Arc<dyn EndpointOps>,
}

impl Debug for UOwnedFrameEndpoint {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UOwnedFrameEndpoint")
            .field("mode", &self.mode())
            .finish()
    }
}

impl UOwnedFrameEndpoint {
    /// Creates an owned-frame endpoint facade around an owned-frame transport.
    ///
    /// Sends and listener registration are delegated to the supplied
    /// [`UOwnedTransport`] without changing frame ownership.
    pub fn from_owned(transport: Arc<dyn UOwnedTransport>) -> Self {
        Self {
            inner: Arc::new(OwnedEndpoint { transport }),
        }
    }

    /// Creates an owned-frame endpoint facade around a zero-copy transport.
    ///
    /// Owned sends are copied into a transmit loan, and zero-copy receives are
    /// copied into owned listener callbacks for generic routing code. This is an
    /// adapter boundary, not end-to-end zero-copy forwarding.
    ///
    /// Prefer [`Self::from_zero_copy_copying_adapter`] in new code when making
    /// the copy boundary explicit at call sites.
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use up_rust::{transport::UOwnedFrameEndpoint, zero_copy::UZeroCopyTransport};
    /// # fn wrap<T>(transport: Arc<T>) -> UOwnedFrameEndpoint
    /// # where
    /// #     T: UZeroCopyTransport + Send + Sync + 'static,
    /// # {
    /// let endpoint = UOwnedFrameEndpoint::from_zero_copy_copying_adapter(transport);
    /// # endpoint
    /// # }
    /// ```
    pub fn from_zero_copy<T>(transport: Arc<T>) -> Self
    where
        T: UZeroCopyTransport + Send + Sync + 'static,
    {
        Self::from_zero_copy_copying_adapter(transport)
    }

    /// Creates an owned-frame copying adapter around a zero-copy transport.
    ///
    /// This is the explicit form of [`Self::from_zero_copy`]. Owned sends are
    /// copied into a transmit loan, and zero-copy receives are copied into owned
    /// listener callbacks for generic routing code. This adapter is useful for
    /// routers that operate on [`UOwnedFrame`], but it is not end-to-end
    /// zero-copy forwarding.
    pub fn from_zero_copy_copying_adapter<T>(transport: Arc<T>) -> Self
    where
        T: UZeroCopyTransport + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(ZeroCopyEndpoint { transport }),
        }
    }

    /// Returns whether this endpoint is backed by an owned or zero-copy
    /// transport capability.
    pub fn mode(&self) -> UOwnedFrameEndpointMode {
        self.inner.mode()
    }

    /// Sends an owned frame through the wrapped transport capability.
    ///
    /// For [`UOwnedFrameEndpointMode::Owned`], this delegates to
    /// [`UOwnedTransport::send_owned`]. For [`UOwnedFrameEndpointMode::ZeroCopy`],
    /// this reserves a transmit loan, copies the owned payload into that loan, and
    /// commits it with [`UZeroCopyTransport::send_zero_copy`].
    ///
    /// # Errors
    ///
    /// Returns transport validation errors, allocation errors from zero-copy
    /// reserve, or send errors from the wrapped transport.
    pub async fn send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        validate_owned_frame_for_transport(&frame)?;
        self.inner.send_owned(frame).await
    }

    /// Registers an owned-frame listener through the wrapped transport
    /// capability.
    ///
    /// For owned transports, the listener is registered directly. For zero-copy
    /// transports, the adapter registers a zero-copy listener internally and
    /// copies each received lease into an owned frame before invoking `listener`.
    /// The returned registration must be retained if the caller wants to
    /// unregister later.
    ///
    /// # Errors
    ///
    /// Returns validation or registration errors from the wrapped transport.
    pub async fn register_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<UOwnedFrameEndpointRegistration, UStatus> {
        self.inner
            .register_owned_listener(source_filter, sink_filter, listener)
            .await
    }
}

/// Registration handle returned by [`UOwnedFrameEndpoint::register_owned_listener`].
///
/// Dropping this value does not unregister automatically. Call [`Self::unregister`]
/// to remove the underlying owned or adapted zero-copy listener registration.
#[derive(Clone)]
pub struct UOwnedFrameEndpointRegistration {
    inner: Arc<dyn RegistrationOps>,
}

impl Debug for UOwnedFrameEndpointRegistration {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UOwnedFrameEndpointRegistration")
            .finish_non_exhaustive()
    }
}

impl UOwnedFrameEndpointRegistration {
    /// Unregisters the listener associated with this registration handle.
    ///
    /// The method is idempotent only if the wrapped transport's unregister
    /// operation is idempotent; most transport implementations report an error
    /// when unregistering a listener that is no longer registered.
    pub async fn unregister(&self) -> Result<(), UStatus> {
        self.inner.unregister().await
    }
}

#[async_trait]
trait EndpointOps: Send + Sync {
    fn mode(&self) -> UOwnedFrameEndpointMode;
    async fn send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus>;
    async fn register_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<UOwnedFrameEndpointRegistration, UStatus>;
}

#[async_trait]
trait RegistrationOps: Send + Sync {
    async fn unregister(&self) -> Result<(), UStatus>;
}

struct OwnedEndpoint {
    transport: Arc<dyn UOwnedTransport>,
}

#[async_trait]
impl EndpointOps for OwnedEndpoint {
    fn mode(&self) -> UOwnedFrameEndpointMode {
        UOwnedFrameEndpointMode::Owned
    }

    async fn send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.transport.send_owned(frame).await
    }

    async fn register_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<UOwnedFrameEndpointRegistration, UStatus> {
        self.transport
            .register_owned_listener(source_filter, sink_filter, listener.clone())
            .await?;
        Ok(UOwnedFrameEndpointRegistration {
            inner: Arc::new(OwnedRegistration {
                transport: self.transport.clone(),
                source_filter: source_filter.clone(),
                sink_filter: sink_filter.cloned(),
                listener,
            }),
        })
    }
}

struct OwnedRegistration {
    transport: Arc<dyn UOwnedTransport>,
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: Arc<dyn UOwnedListener>,
}

#[async_trait]
impl RegistrationOps for OwnedRegistration {
    async fn unregister(&self) -> Result<(), UStatus> {
        self.transport
            .unregister_owned_listener(
                &self.source_filter,
                self.sink_filter.as_ref(),
                self.listener.clone(),
            )
            .await
    }
}

struct ZeroCopyEndpoint<T> {
    transport: Arc<T>,
}

#[async_trait]
impl<T> EndpointOps for ZeroCopyEndpoint<T>
where
    T: UZeroCopyTransport + Send + Sync + 'static,
{
    fn mode(&self) -> UOwnedFrameEndpointMode {
        UOwnedFrameEndpointMode::ZeroCopy
    }

    async fn send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        let payload_len = frame.payload_bytes().len();
        let mut buffer = self
            .transport
            .reserve(frame.metadata().clone(), payload_len, 1)
            .await?;
        let reserved_len = buffer.payload_mut().len();
        if reserved_len != payload_len {
            return Err(UStatus::fail_with_code(
                UCode::INTERNAL,
                format!(
                    "zero-copy transport reserved payload buffer length {reserved_len}, expected {payload_len}"
                ),
            ));
        }
        buffer.payload_mut().copy_from_slice(frame.payload_bytes());
        self.transport.send_zero_copy(buffer).await
    }

    async fn register_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<UOwnedFrameEndpointRegistration, UStatus> {
        let zero_copy_listener: Arc<dyn UZeroCopyListener<T::Rx>> =
            Arc::new(ZeroCopyToOwnedListener::<T::Rx> {
                listener,
                _rx: PhantomData,
            });
        self.transport
            .register_zero_copy_listener(source_filter, sink_filter, zero_copy_listener.clone())
            .await?;
        Ok(UOwnedFrameEndpointRegistration {
            inner: Arc::new(ZeroCopyRegistration::<T> {
                transport: self.transport.clone(),
                source_filter: source_filter.clone(),
                sink_filter: sink_filter.cloned(),
                listener: zero_copy_listener,
            }),
        })
    }
}

struct ZeroCopyToOwnedListener<Rx> {
    listener: Arc<dyn UOwnedListener>,
    _rx: PhantomData<fn() -> Rx>,
}

#[async_trait]
impl<Rx> UZeroCopyListener<Rx> for ZeroCopyToOwnedListener<Rx>
where
    Rx: UZeroCopyRxFrame + Send + 'static,
{
    async fn on_receive_zero_copy(&self, frame: Rx) {
        let owned = if frame.metadata().encoding().is_some() {
            UOwnedFrame::new(frame.metadata().clone(), frame.payload_to_vec())
        } else {
            UOwnedFrame::without_payload(frame.metadata().clone())
        };
        self.listener.on_receive_owned(owned).await;
    }
}

struct ZeroCopyRegistration<T>
where
    T: UZeroCopyTransport + Send + Sync + 'static,
{
    transport: Arc<T>,
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: Arc<dyn UZeroCopyListener<T::Rx>>,
}

#[async_trait]
impl<T> RegistrationOps for ZeroCopyRegistration<T>
where
    T: UZeroCopyTransport + Send + Sync + 'static,
{
    async fn unregister(&self) -> Result<(), UStatus> {
        self.transport
            .unregister_zero_copy_listener(
                &self.source_filter,
                self.sink_filter.as_ref(),
                self.listener.clone(),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use crate::{
        transport::{UOwnedFrameEndpoint, UOwnedFrameEndpointMode},
        zero_copy::{UTxBuffer, UVecTxBuffer, UZeroCopyListener, UZeroCopyTransport},
        UCode, UFrameMetadata, UOwnedFrame, UOwnedListener, UOwnedTransport, UStatus, UUri,
    };

    #[derive(Default)]
    struct MemoryOwnedTransport {
        sent: Mutex<Vec<UOwnedFrame>>,
        listener: Mutex<Option<Arc<dyn UOwnedListener>>>,
    }

    impl MemoryOwnedTransport {
        fn sent(&self) -> Vec<UOwnedFrame> {
            self.sent.lock().expect("sent lock poisoned").clone()
        }

        async fn inject(&self, frame: UOwnedFrame) {
            let listener = self
                .listener
                .lock()
                .expect("listener lock poisoned")
                .clone();
            if let Some(listener) = listener {
                listener.on_receive_owned(frame).await;
            }
        }
    }

    #[async_trait]
    impl UOwnedTransport for MemoryOwnedTransport {
        async fn send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
            self.sent.lock().expect("sent lock poisoned").push(frame);
            Ok(())
        }

        async fn register_owned_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            listener: Arc<dyn UOwnedListener>,
        ) -> Result<(), UStatus> {
            *self.listener.lock().expect("listener lock poisoned") = Some(listener);
            Ok(())
        }

        async fn unregister_owned_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            listener: Arc<dyn UOwnedListener>,
        ) -> Result<(), UStatus> {
            let mut registered = self.listener.lock().expect("listener lock poisoned");
            if registered
                .as_ref()
                .is_some_and(|existing| Arc::ptr_eq(existing, &listener))
            {
                *registered = None;
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryZeroCopyTransport {
        sent: Mutex<Vec<UOwnedFrame>>,
        listener: Mutex<Option<Arc<dyn UZeroCopyListener<UOwnedFrame>>>>,
    }

    impl MemoryZeroCopyTransport {
        fn sent(&self) -> Vec<UOwnedFrame> {
            self.sent.lock().expect("sent lock poisoned").clone()
        }

        async fn inject(&self, frame: UOwnedFrame) {
            let listener = self
                .listener
                .lock()
                .expect("listener lock poisoned")
                .clone();
            if let Some(listener) = listener {
                listener.on_receive_zero_copy(frame).await;
            }
        }
    }

    #[async_trait]
    impl UZeroCopyTransport for MemoryZeroCopyTransport {
        type Tx = UVecTxBuffer;
        type Rx = UOwnedFrame;

        async fn reserve(
            &self,
            metadata: UFrameMetadata,
            payload_len: usize,
            _alignment: usize,
        ) -> Result<Self::Tx, UStatus> {
            Ok(UVecTxBuffer::new(metadata, payload_len))
        }

        async fn send_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
            self.sent
                .lock()
                .expect("sent lock poisoned")
                .push(buffer.into_frame());
            Ok(())
        }

        async fn register_zero_copy_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
        ) -> Result<(), UStatus> {
            *self.listener.lock().expect("listener lock poisoned") = Some(listener);
            Ok(())
        }

        async fn unregister_zero_copy_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
        ) -> Result<(), UStatus> {
            let mut registered = self.listener.lock().expect("listener lock poisoned");
            if registered
                .as_ref()
                .is_some_and(|existing| Arc::ptr_eq(existing, &listener))
            {
                *registered = None;
            }
            Ok(())
        }
    }

    struct WrongLengthTxBuffer {
        metadata: UFrameMetadata,
        payload: Vec<u8>,
    }

    impl UTxBuffer for WrongLengthTxBuffer {
        fn metadata(&self) -> &UFrameMetadata {
            &self.metadata
        }

        fn payload(&self) -> &[u8] {
            &self.payload
        }

        fn payload_mut(&mut self) -> &mut [u8] {
            self.payload.as_mut_slice()
        }
    }

    struct WrongLengthZeroCopyTransport;

    #[async_trait]
    impl UZeroCopyTransport for WrongLengthZeroCopyTransport {
        type Tx = WrongLengthTxBuffer;
        type Rx = UOwnedFrame;

        async fn reserve(
            &self,
            metadata: UFrameMetadata,
            payload_len: usize,
            _alignment: usize,
        ) -> Result<Self::Tx, UStatus> {
            Ok(WrongLengthTxBuffer {
                metadata,
                payload: vec![0_u8; payload_len + 1],
            })
        }

        async fn send_zero_copy(&self, _buffer: Self::Tx) -> Result<(), UStatus> {
            Ok(())
        }

        async fn register_zero_copy_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            _listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
        ) -> Result<(), UStatus> {
            Ok(())
        }

        async fn unregister_zero_copy_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            _listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
        ) -> Result<(), UStatus> {
            Ok(())
        }
    }

    struct CaptureListener(Mutex<Vec<UOwnedFrame>>);

    #[async_trait]
    impl UOwnedListener for CaptureListener {
        async fn on_receive_owned(&self, frame: UOwnedFrame) {
            self.0.lock().expect("frames lock poisoned").push(frame);
        }
    }

    #[tokio::test]
    async fn endpoint_delegates_owned_transport_operations() {
        let transport = Arc::new(MemoryOwnedTransport::default());
        let endpoint = UOwnedFrameEndpoint::from_owned(transport.clone());
        let topic = UUri::try_from_parts("vehicle", 0x4210, 1, 0x9000).expect("valid topic");
        let frame = crate::UFrameBuilder::publish(topic.clone())
            .build_with_raw_payload(b"payload".as_slice())
            .unwrap();
        let listener = Arc::new(CaptureListener(Mutex::new(Vec::new())));

        endpoint
            .send_owned(frame.clone())
            .await
            .expect("send works");
        let registration = endpoint
            .register_owned_listener(&topic, None, listener.clone())
            .await
            .expect("register works");
        transport.inject(frame.clone()).await;
        registration.unregister().await.expect("unregister works");
        transport.inject(frame.clone()).await;

        assert_eq!(endpoint.mode(), UOwnedFrameEndpointMode::Owned);
        assert_eq!(transport.sent(), vec![frame.clone()]);
        assert_eq!(
            *listener.0.lock().expect("frames lock poisoned"),
            vec![frame]
        );
    }

    #[tokio::test]
    async fn endpoint_sends_owned_frame_through_zero_copy_transport() {
        let transport = Arc::new(MemoryZeroCopyTransport::default());
        let endpoint = UOwnedFrameEndpoint::from_zero_copy_copying_adapter(transport.clone());
        let topic = UUri::try_from_parts("vehicle", 0x4210, 1, 0x9000).expect("valid topic");
        let frame = crate::UFrameBuilder::publish(topic)
            .build_with_raw_payload(b"payload".as_slice())
            .unwrap();

        endpoint
            .send_owned(frame.clone())
            .await
            .expect("send works");

        assert_eq!(endpoint.mode(), UOwnedFrameEndpointMode::ZeroCopy);
        assert_eq!(transport.sent(), vec![frame]);
    }

    #[tokio::test]
    async fn endpoint_rejects_wrong_length_zero_copy_reservation() {
        let endpoint = UOwnedFrameEndpoint::from_zero_copy_copying_adapter(Arc::new(
            WrongLengthZeroCopyTransport,
        ));
        let topic = UUri::try_from_parts("vehicle", 0x4210, 1, 0x9000).expect("valid topic");
        let frame = crate::UFrameBuilder::publish(topic)
            .build_with_raw_payload(b"payload".as_slice())
            .unwrap();

        let err = endpoint
            .send_owned(frame)
            .await
            .expect_err("wrong-length reserve must fail");

        assert_eq!(err.get_code(), UCode::INTERNAL);
        assert!(err.get_message().contains("reserved payload buffer length"));
    }

    #[tokio::test]
    async fn endpoint_adapts_zero_copy_receive_to_owned_listener() {
        let transport = Arc::new(MemoryZeroCopyTransport::default());
        let endpoint = UOwnedFrameEndpoint::from_zero_copy_copying_adapter(transport.clone());
        let topic = UUri::try_from_parts("vehicle", 0x4210, 1, 0x9000).expect("valid topic");
        let frame = crate::UFrameBuilder::publish(topic.clone())
            .build_with_raw_payload(b"payload".as_slice())
            .unwrap();
        let listener = Arc::new(CaptureListener(Mutex::new(Vec::new())));

        let registration = endpoint
            .register_owned_listener(&topic, None, listener.clone())
            .await
            .expect("register works");
        transport.inject(frame.clone()).await;
        registration.unregister().await.expect("unregister works");
        transport.inject(frame.clone()).await;

        assert_eq!(
            *listener.0.lock().expect("frames lock poisoned"),
            vec![frame]
        );
    }
}
