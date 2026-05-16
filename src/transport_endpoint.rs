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
    UOwnedFrame, UOwnedListener, UOwnedTransport, UStatus, UTxBuffer, UUri, UZeroCopyListener,
    UZeroCopyRxFrame, UZeroCopyTransport,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UOwnedFrameEndpointMode {
    Owned,
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
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use up_rust::{UOwnedFrameEndpoint, UZeroCopyTransport};
    /// # fn wrap<T>(transport: Arc<T>) -> UOwnedFrameEndpoint
    /// # where
    /// #     T: UZeroCopyTransport + Send + Sync + 'static,
    /// # {
    /// let endpoint = UOwnedFrameEndpoint::from_zero_copy(transport);
    /// # endpoint
    /// # }
    /// ```
    pub fn from_zero_copy<T>(transport: Arc<T>) -> Self
    where
        T: UZeroCopyTransport + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(ZeroCopyEndpoint { transport }),
        }
    }

    pub fn mode(&self) -> UOwnedFrameEndpointMode {
        self.inner.mode()
    }

    pub async fn send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.inner.send_owned(frame).await
    }

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
        let payload_len = frame.payload().len();
        let mut buffer = self
            .transport
            .reserve(frame.metadata().clone(), payload_len, 1)
            .await?;
        buffer.payload_mut().copy_from_slice(frame.payload());
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
        self.listener
            .on_receive_owned(UOwnedFrame::new(
                frame.metadata().clone(),
                frame.payload().to_vec(),
            ))
            .await;
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
        UFrameMetadata, UOwnedFrame, UOwnedFrameEndpoint, UOwnedFrameEndpointMode, UOwnedListener,
        UOwnedTransport, UStatus, UUri, UVecTxBuffer, UZeroCopyListener, UZeroCopyTransport,
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
        let frame = UOwnedFrame::new(
            UFrameMetadata::publish(topic.clone()),
            b"payload".as_slice(),
        );
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
        let endpoint = UOwnedFrameEndpoint::from_zero_copy(transport.clone());
        let topic = UUri::try_from_parts("vehicle", 0x4210, 1, 0x9000).expect("valid topic");
        let frame = UOwnedFrame::new(UFrameMetadata::publish(topic), b"payload".as_slice());

        endpoint
            .send_owned(frame.clone())
            .await
            .expect("send works");

        assert_eq!(endpoint.mode(), UOwnedFrameEndpointMode::ZeroCopy);
        assert_eq!(transport.sent(), vec![frame]);
    }

    #[tokio::test]
    async fn endpoint_adapts_zero_copy_receive_to_owned_listener() {
        let transport = Arc::new(MemoryZeroCopyTransport::default());
        let endpoint = UOwnedFrameEndpoint::from_zero_copy(transport.clone());
        let topic = UUri::try_from_parts("vehicle", 0x4210, 1, 0x9000).expect("valid topic");
        let frame = UOwnedFrame::new(
            UFrameMetadata::publish(topic.clone()),
            b"payload".as_slice(),
        );
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
