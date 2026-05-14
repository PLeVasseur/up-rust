/********************************************************************************
 * Copyright (c) 2024 Contributors to the Eclipse Foundation
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

//! Local owned-frame transport for examples and tests.

use std::{collections::HashSet, sync::Arc};

use tokio::sync::RwLock;

use crate::{
    ComparableOwnedListener, UCode, UOwnedFrame, UOwnedListener, UOwnedTransport, UStatus, UUri,
};

#[derive(Eq, PartialEq, Hash)]
struct RegisteredOwnedListener {
    source_filter: UUri,
    sink_filter: Option<UUri>,
    listener: ComparableOwnedListener,
}

impl RegisteredOwnedListener {
    fn matches(&self, source: &UUri, sink: Option<&UUri>) -> bool {
        if !self.source_filter.matches(source) {
            return false;
        }

        if let Some(pattern) = &self.sink_filter {
            sink.is_some_and(|candidate_sink| pattern.matches(candidate_sink))
        } else {
            sink.is_none()
        }
    }

    fn matches_frame(&self, frame: &UOwnedFrame) -> bool {
        self.matches(frame.header().source(), frame.header().sink())
    }

    async fn on_receive(&self, frame: UOwnedFrame) {
        self.listener.on_receive_owned(frame).await
    }
}

/// A local owned-frame transport for exchanging frames within a process.
#[derive(Default)]
pub struct LocalTransport {
    owned_listeners: RwLock<HashSet<RegisteredOwnedListener>>,
}

impl LocalTransport {
    async fn dispatch_owned(&self, frame: UOwnedFrame) {
        let listeners = self.owned_listeners.read().await;
        for listener in listeners.iter() {
            if listener.matches_frame(&frame) {
                listener.on_receive(frame.clone()).await;
            }
        }
    }
}

#[async_trait::async_trait]
impl UOwnedTransport for LocalTransport {
    async fn send_owned(&self, frame: UOwnedFrame) -> Result<(), UStatus> {
        self.dispatch_owned(frame).await;
        Ok(())
    }

    async fn register_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        let registered_listener = RegisteredOwnedListener {
            source_filter: source_filter.to_owned(),
            sink_filter: sink_filter.map(ToOwned::to_owned),
            listener: ComparableOwnedListener::new(listener),
        };
        let mut listeners = self.owned_listeners.write().await;
        if listeners.contains(&registered_listener) {
            Err(UStatus::fail_with_code(
                UCode::ALREADY_EXISTS,
                "owned listener already registered for filters",
            ))
        } else {
            listeners.insert(registered_listener);
            Ok(())
        }
    }

    async fn unregister_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        let registered_listener = RegisteredOwnedListener {
            source_filter: source_filter.to_owned(),
            sink_filter: sink_filter.map(ToOwned::to_owned),
            listener: ComparableOwnedListener::new(listener),
        };
        let mut listeners = self.owned_listeners.write().await;
        if listeners.remove(&registered_listener) {
            Ok(())
        } else {
            Err(UStatus::fail_with_code(
                UCode::NOT_FOUND,
                "no such owned listener registered for filters",
            ))
        }
    }
}
