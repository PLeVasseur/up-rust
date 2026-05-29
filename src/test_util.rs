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

//! Test utilities for native-frame transports and listeners.
//!
//! The `test-util` feature also enables `mockall` mocks for public traits whose
//! signatures can be automocked without changing production ergonomics.

use std::{collections::VecDeque, sync::Arc};

use async_trait::async_trait;
use std::sync::Mutex;

use crate::{
    payload::{PayloadCodec, StableContainerPayload, StablePayload, UWireError},
    transport::{UOwnedTransportImpl, ValidatedOwnedFrame},
    zero_copy::{
        verify_loaned_rx_payload_layout, verify_tx_buffer_payload_layout,
        verify_uninit_tx_buffer_payload_layout, ULoanedContiguousZeroCopyRxFrame, UTxBuffer,
        UUninitTxBuffer, UVecRxLease, UVecTxBuffer, UVecUninitTxBuffer, UZeroCopyListener,
        UZeroCopyTransportImpl, UZeroCopyUninitTransportImpl, ValidatedTxLoanSpec,
    },
    PayloadEncoding, UCode, UOwnedFrame, UOwnedListener, UStatus, UUri,
};

/// Reusable zero-copy transport conformance checks for downstream transport tests.
pub mod zero_copy_conformance {
    use super::*;

    /// Verifies a TX loan exposes the exact visible payload length and alignment.
    pub fn verify_tx_payload_layout(
        buffer: &mut impl UTxBuffer,
        payload_len: usize,
        alignment: usize,
    ) -> Result<(), UWireError> {
        verify_tx_buffer_payload_layout(buffer, payload_len, alignment)
    }

    /// Verifies an uninitialized TX loan exposes the exact visible payload length and alignment.
    pub fn verify_uninit_tx_payload_layout(
        buffer: &mut impl UUninitTxBuffer,
        payload_len: usize,
        alignment: usize,
    ) -> Result<(), UWireError> {
        verify_uninit_tx_buffer_payload_layout(buffer, payload_len, alignment)
    }

    /// Verifies a loan-backed RX frame exposes the exact payload length and alignment.
    pub fn verify_loaned_rx_payload_layout_for(
        frame: &(impl ULoanedContiguousZeroCopyRxFrame + ?Sized),
        payload_len: usize,
        alignment: usize,
    ) -> Result<(), UWireError> {
        verify_loaned_rx_payload_layout(frame, payload_len, alignment)
    }

    /// Verifies and borrows a stable-container value from loan-backed RX storage.
    pub fn borrow_stable_payload<T>(
        frame: &(impl ULoanedContiguousZeroCopyRxFrame + ?Sized),
    ) -> Result<&T, UWireError>
    where
        T: StablePayload,
    {
        frame.borrow_stable_payload::<T>()
    }

    /// Builds stable-container metadata for a fixed-size stable payload type.
    pub fn stable_container_encoding_for<T: StablePayload>(
        type_name: &str,
        variant: &str,
        size: usize,
        align: usize,
    ) -> PayloadEncoding {
        PayloadEncoding::custom(
            StableContainerPayload::<T>::ENCODING_ID,
            format!(
                "application/vnd.uprotocol.stable-container;type=\"{type_name}\";variant={variant};size={size};align={align}"
            ),
        )
    }

    /// Verifies that the canonical stable-container metadata for `T` is accepted.
    pub fn verify_stable_container_encoding<T: StablePayload>() -> Result<(), UWireError> {
        StableContainerPayload::<T>::verify_encoding(Some(&StableContainerPayload::<T>::encoding()))
    }

    /// Verifies that a wrong stable type name is rejected for `T`.
    pub fn verify_stable_container_rejects_wrong_type_name<T: StablePayload>(
        wrong_type_name: &str,
    ) -> Result<(), UWireError> {
        let encoding = stable_container_encoding_for::<T>(
            wrong_type_name,
            "fixed",
            core::mem::size_of::<T>(),
            core::mem::align_of::<T>(),
        );
        expect_incompatible_stable_payload(StableContainerPayload::<T>::verify_encoding(Some(
            &encoding,
        )))
    }

    /// Verifies that a wrong advertised stable payload variant is rejected for `T`.
    pub fn verify_stable_container_rejects_wrong_variant<T: StablePayload>(
        wrong_variant: &str,
    ) -> Result<(), UWireError> {
        let encoding = stable_container_encoding_for::<T>(
            T::stable_type_name(),
            wrong_variant,
            core::mem::size_of::<T>(),
            core::mem::align_of::<T>(),
        );
        expect_incompatible_stable_payload(StableContainerPayload::<T>::verify_encoding(Some(
            &encoding,
        )))
    }

    /// Verifies that a wrong advertised stable payload size is rejected for `T`.
    pub fn verify_stable_container_rejects_wrong_size<T: StablePayload>() -> Result<(), UWireError>
    {
        let encoding = stable_container_encoding_for::<T>(
            T::stable_type_name(),
            "fixed",
            core::mem::size_of::<T>().saturating_add(1),
            core::mem::align_of::<T>(),
        );
        expect_incompatible_stable_payload(StableContainerPayload::<T>::verify_encoding(Some(
            &encoding,
        )))
    }

    /// Verifies that insufficient advertised stable payload alignment is rejected for `T`.
    pub fn verify_stable_container_rejects_insufficient_alignment<T: StablePayload>(
    ) -> Result<(), UWireError> {
        let encoding = stable_container_encoding_for::<T>(
            T::stable_type_name(),
            "fixed",
            core::mem::size_of::<T>(),
            core::mem::align_of::<T>().saturating_div(2).max(1),
        );
        expect_incompatible_stable_payload(StableContainerPayload::<T>::verify_encoding(Some(
            &encoding,
        )))
    }

    /// Verifies that the typed stable-container borrow rejects actual pointer misalignment.
    pub fn verify_stable_container_rejects_actual_misalignment<T: StablePayload>(
    ) -> Result<(), UWireError> {
        let alignment = core::mem::align_of::<T>();
        let size = core::mem::size_of::<T>();
        if alignment <= 1 || size == 0 {
            return Err(UWireError::invalid_payload(
                "stable-container misalignment check requires a non-zero payload with alignment > 1",
            ));
        }

        let storage = vec![0_u8; size + alignment];
        let base = storage.as_ptr() as usize;
        let Some(offset) = (1..alignment).find(|offset| !(base + offset).is_multiple_of(alignment))
        else {
            return Err(UWireError::invalid_payload(
                "failed to construct a misaligned stable-container payload view",
            ));
        };
        let payload = storage
            .get(offset..offset + size)
            .ok_or_else(|| UWireError::invalid_payload("misaligned payload view out of bounds"))?;

        match StableContainerPayload::<T>::borrow_checked_payload(payload) {
            Err(UWireError::InvalidPayloadAlignment { expected, .. }) if expected == alignment => {
                Ok(())
            }
            Ok(_) => Err(UWireError::invalid_payload(
                "stable-container borrow unexpectedly accepted misaligned payload bytes",
            )),
            Err(error) => Err(error),
        }
    }

    fn expect_incompatible_stable_payload(
        result: Result<(), UWireError>,
    ) -> Result<(), UWireError> {
        match result {
            Err(UWireError::IncompatibleStablePayload { .. }) => Ok(()),
            Ok(()) => Err(UWireError::invalid_payload(
                "stable-container compatibility check unexpectedly succeeded",
            )),
            Err(error) => Err(error),
        }
    }
}

/// Listener that records received owned frames for assertions.
#[derive(Default)]
pub struct RecordingOwnedListener {
    frames: Mutex<Vec<UOwnedFrame>>,
}

impl RecordingOwnedListener {
    /// Returns all frames received so far.
    pub fn frames(&self) -> Vec<UOwnedFrame> {
        self.frames.lock().expect("frames lock poisoned").clone()
    }

    /// Clears recorded frames.
    pub fn clear(&self) {
        self.frames.lock().expect("frames lock poisoned").clear();
    }
}

#[async_trait]
impl UOwnedListener for RecordingOwnedListener {
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        self.frames
            .lock()
            .expect("frames lock poisoned")
            .push(frame);
    }
}

#[derive(Clone)]
struct RegisteredOwnedListener {
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: Arc<dyn UOwnedListener>,
}

impl RegisteredOwnedListener {
    fn matches_frame(&self, frame: &UOwnedFrame) -> bool {
        if !self.source_filter.matches(frame.metadata().source()) {
            return false;
        }
        if let Some(sink_filter) = &self.sink_filter {
            frame
                .metadata()
                .sink()
                .is_some_and(|sink| sink_filter.matches(sink))
        } else {
            frame.metadata().sink().is_none()
        }
    }

    fn matches_registration(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: &Arc<dyn UOwnedListener>,
    ) -> bool {
        self.source_filter == *source_filter
            && self.sink_filter.as_ref() == sink_filter
            && Arc::ptr_eq(&self.listener, listener)
    }
}

/// In-memory owned transport useful for unit tests.
#[derive(Default)]
pub struct InMemoryOwnedTransport {
    listeners: Mutex<Vec<RegisteredOwnedListener>>,
    sent: Mutex<Vec<UOwnedFrame>>,
}

impl InMemoryOwnedTransport {
    /// Returns frames sent through [`UOwnedTransport::send_owned`].
    pub fn sent_frames(&self) -> Vec<UOwnedFrame> {
        self.sent.lock().expect("sent lock poisoned").clone()
    }

    /// Injects a frame into registered listeners without recording it as sent.
    pub async fn inject(&self, frame: UOwnedFrame) {
        self.dispatch(frame).await;
    }

    async fn dispatch(&self, frame: UOwnedFrame) {
        let listeners = self
            .listeners
            .lock()
            .expect("listeners lock poisoned")
            .clone();
        for registration in listeners {
            if registration.matches_frame(&frame) {
                registration.listener.on_receive_owned(frame.clone()).await;
            }
        }
    }
}

#[async_trait]
impl UOwnedTransportImpl for InMemoryOwnedTransport {
    async fn send_validated_owned(&self, frame: ValidatedOwnedFrame) -> Result<(), UStatus> {
        let frame = frame.into_inner();
        self.sent
            .lock()
            .expect("sent lock poisoned")
            .push(frame.clone());
        self.dispatch(frame).await;
        Ok(())
    }

    async fn register_validated_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        self.listeners
            .lock()
            .expect("listeners lock poisoned")
            .push(RegisteredOwnedListener {
                source_filter: source_filter.clone(),
                sink_filter: sink_filter.cloned(),
                listener,
            });
        Ok(())
    }

    async fn unregister_validated_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        let mut listeners = self.listeners.lock().expect("listeners lock poisoned");
        let Some(index) = listeners.iter().position(|existing| {
            existing.matches_registration(source_filter, sink_filter, &listener)
        }) else {
            return Err(UStatus::fail_with_code(
                UCode::NOT_FOUND,
                "no such owned listener registered for filters",
            ));
        };
        listeners.remove(index);
        Ok(())
    }
}

#[derive(Clone)]
struct RegisteredZeroCopyListener {
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: Arc<dyn UZeroCopyListener<UVecRxLease>>,
}

impl RegisteredZeroCopyListener {
    fn matches_frame(&self, frame: &UOwnedFrame) -> bool {
        if !self.source_filter.matches(frame.metadata().source()) {
            return false;
        }
        if let Some(sink_filter) = &self.sink_filter {
            frame
                .metadata()
                .sink()
                .is_some_and(|sink| sink_filter.matches(sink))
        } else {
            frame.metadata().sink().is_none()
        }
    }

    fn matches_registration(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: &Arc<dyn UZeroCopyListener<UVecRxLease>>,
    ) -> bool {
        self.source_filter == *source_filter
            && self.sink_filter.as_ref() == sink_filter
            && Arc::ptr_eq(&self.listener, listener)
    }
}

/// In-memory zero-copy transport useful for unit tests and examples.
///
/// This type uses [`UVecTxBuffer`] and [`UVecRxLease`] so tests can exercise the
/// zero-copy lease shape without requiring shared-memory middleware.
#[derive(Default)]
pub struct InMemoryZeroCopyTransport {
    listeners: Mutex<Vec<RegisteredZeroCopyListener>>,
    queue: Mutex<VecDeque<UOwnedFrame>>,
    sent: Mutex<Vec<UOwnedFrame>>,
}

impl InMemoryZeroCopyTransport {
    /// Returns frames sent through [`UZeroCopyTransport::send_zero_copy`].
    pub fn sent_frames(&self) -> Vec<UOwnedFrame> {
        self.sent.lock().expect("sent lock poisoned").clone()
    }

    /// Injects a frame into the receive queue and registered zero-copy listeners.
    pub async fn inject(&self, frame: UOwnedFrame) {
        self.enqueue_and_dispatch(frame).await;
    }

    async fn enqueue_and_dispatch(&self, frame: UOwnedFrame) {
        self.queue
            .lock()
            .expect("queue lock poisoned")
            .push_back(frame.clone());
        let listeners = self
            .listeners
            .lock()
            .expect("listeners lock poisoned")
            .clone();
        for registration in listeners {
            if registration.matches_frame(&frame) {
                registration
                    .listener
                    .on_receive_zero_copy(UVecRxLease::new(frame.clone()))
                    .await;
            }
        }
    }
}

#[async_trait]
impl UZeroCopyTransportImpl for InMemoryZeroCopyTransport {
    type Tx = UVecTxBuffer;
    type Rx = UVecRxLease;

    async fn loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus> {
        UVecTxBuffer::with_alignment(
            spec.metadata().clone(),
            spec.payload_len(),
            spec.payload_alignment(),
        )
        .map_err(UStatus::from)
    }

    async fn send_validated_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
        let frame = buffer.into_frame();
        self.sent
            .lock()
            .expect("sent lock poisoned")
            .push(frame.clone());
        self.enqueue_and_dispatch(frame).await;
        Ok(())
    }

    async fn receive_validated_zero_copy(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus> {
        self.queue
            .lock()
            .expect("queue lock poisoned")
            .pop_front()
            .map(UVecRxLease::new)
            .ok_or_else(|| UStatus::fail_with_code(UCode::NOT_FOUND, "no frame available"))
    }

    async fn register_validated_zero_copy_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        self.listeners
            .lock()
            .expect("listeners lock poisoned")
            .push(RegisteredZeroCopyListener {
                source_filter: source_filter.clone(),
                sink_filter: sink_filter.cloned(),
                listener,
            });
        Ok(())
    }

    async fn unregister_validated_zero_copy_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UZeroCopyListener<Self::Rx>>,
    ) -> Result<(), UStatus> {
        let mut listeners = self.listeners.lock().expect("listeners lock poisoned");
        let Some(index) = listeners.iter().position(|existing| {
            existing.matches_registration(source_filter, sink_filter, &listener)
        }) else {
            return Err(UStatus::fail_with_code(
                UCode::NOT_FOUND,
                "no such zero-copy listener registered for filters",
            ));
        };
        listeners.remove(index);
        Ok(())
    }
}

#[async_trait]
impl UZeroCopyUninitTransportImpl for InMemoryZeroCopyTransport {
    type UninitTx = UVecUninitTxBuffer;

    async fn loan_validated_uninit_tx(
        &self,
        spec: ValidatedTxLoanSpec,
    ) -> Result<Self::UninitTx, UStatus> {
        UVecUninitTxBuffer::with_alignment(
            spec.metadata().clone(),
            spec.payload_len(),
            spec.payload_alignment(),
        )
        .map_err(UStatus::from)
    }
}
