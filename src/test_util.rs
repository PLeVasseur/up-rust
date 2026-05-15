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
    UCode, UFrameMetadata, UOwnedFrame, UOwnedListener, UOwnedTransport, UStatus, UUri,
    UVecTxBuffer, UZeroCopyListener, UZeroCopyTransport,
};

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
impl UOwnedTransport for InMemoryOwnedTransport {
    async fn send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.sent
            .lock()
            .expect("sent lock poisoned")
            .push(frame.clone());
        self.dispatch(frame).await;
        Ok(())
    }

    async fn register_owned_listener(
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

    async fn unregister_owned_listener(
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
    listener: Arc<dyn UZeroCopyListener<UOwnedFrame>>,
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
        listener: &Arc<dyn UZeroCopyListener<UOwnedFrame>>,
    ) -> bool {
        self.source_filter == *source_filter
            && self.sink_filter.as_ref() == sink_filter
            && Arc::ptr_eq(&self.listener, listener)
    }
}

/// In-memory zero-copy transport useful for unit tests and examples.
///
/// This type uses [`UVecTxBuffer`] and [`UOwnedFrame`] so tests can exercise the
/// zero-copy trait shape without requiring shared-memory middleware.
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
                    .on_receive_zero_copy(frame.clone())
                    .await;
            }
        }
    }
}

#[async_trait]
impl UZeroCopyTransport for InMemoryZeroCopyTransport {
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
        let frame = buffer.into_frame();
        self.sent
            .lock()
            .expect("sent lock poisoned")
            .push(frame.clone());
        self.enqueue_and_dispatch(frame).await;
        Ok(())
    }

    async fn receive_zero_copy(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus> {
        self.queue
            .lock()
            .expect("queue lock poisoned")
            .pop_front()
            .ok_or_else(|| UStatus::fail_with_code(UCode::NOT_FOUND, "no frame available"))
    }

    async fn register_zero_copy_listener(
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

    async fn unregister_zero_copy_listener(
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
