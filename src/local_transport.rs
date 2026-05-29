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
    transport::{ComparableOwnedListener, UOwnedTransportImpl, ValidatedOwnedFrame},
    verify_filter_criteria, UCode, UOwnedFrame, UOwnedListener, UStatus, UUri,
};

#[derive(Clone, Eq, PartialEq, Hash)]
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
        self.matches(frame.metadata().source(), frame.metadata().sink())
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
        let listeners = {
            let listeners = self.owned_listeners.read().await;
            listeners
                .iter()
                .filter(|listener| listener.matches_frame(&frame))
                .cloned()
                .collect::<Vec<_>>()
        };
        for listener in listeners {
            listener.on_receive(frame.clone()).await;
        }
    }
}

#[async_trait::async_trait]
impl UOwnedTransportImpl for LocalTransport {
    async fn send_validated_owned(&self, frame: ValidatedOwnedFrame) -> Result<(), UStatus> {
        let frame = frame.into_inner();
        self.dispatch_owned(frame).await;
        Ok(())
    }

    async fn register_validated_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        verify_filter_criteria(source_filter, sink_filter)?;
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

    async fn unregister_validated_owned_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        verify_filter_criteria(source_filter, sink_filter)?;
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{local_transport::LocalTransport, UCode, UOwnedListener, UOwnedTransport, UUri};

    struct NoopListener;

    #[async_trait::async_trait]
    impl UOwnedListener for NoopListener {
        async fn on_receive_owned(&self, _frame: crate::UOwnedFrame) {}
    }

    #[tokio::test]
    async fn receive_owned_defaults_to_unimplemented() {
        let transport = Arc::new(LocalTransport::default());
        let topic = UUri::try_from_parts("vehicle", 0x4210, 1, 0x9000).expect("valid topic");

        assert!(transport
            .receive_owned(&topic, None)
            .await
            .is_err_and(|status| status.get_code() == UCode::UNIMPLEMENTED));
    }

    #[tokio::test]
    async fn register_owned_listener_rejects_invalid_filters() {
        let transport = LocalTransport::default();
        let rpc_method = UUri::try_from_parts("vehicle", 0x4210, 1, 0x0001).expect("valid URI");
        let listener = Arc::new(NoopListener);

        let status = transport
            .register_owned_listener(&rpc_method, None, listener)
            .await
            .expect_err("invalid filter must be rejected");

        assert_eq!(status.get_code(), UCode::INVALID_ARGUMENT);
    }

    #[tokio::test]
    async fn unregister_owned_listener_rejects_invalid_filters_before_lookup() {
        let transport = LocalTransport::default();
        let rpc_method = UUri::try_from_parts("vehicle", 0x4210, 1, 0x0001).expect("valid URI");
        let listener = Arc::new(NoopListener);

        let status = transport
            .unregister_owned_listener(&rpc_method, None, listener)
            .await
            .expect_err("invalid filter must be rejected");

        assert_eq!(status.get_code(), UCode::INVALID_ARGUMENT);
    }
}
