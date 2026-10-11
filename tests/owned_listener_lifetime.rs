/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

//! Owned-listener lifetime is owned by the transport instance, not by the process.
//!
//! A registration is scoped to its transport instance. The caller and that
//! instance own their listener references; drop and unregister release the
//! transport's references without affecting a separate instance.
#![cfg(feature = "owned-frame-transport")]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use up_rust::{UOwnedFrame, UOwnedListener, UOwnedTransport, UOwnedTransportImpl, UStatus, UUri};

#[derive(Default)]
struct RetainingTransport {
    listeners: Mutex<Vec<Arc<dyn UOwnedListener>>>,
}

#[async_trait]
impl UOwnedTransportImpl for RetainingTransport {
    async fn send_validated_owned(&self, _frame: UOwnedFrame) -> Result<(), UStatus> {
        Ok(())
    }

    async fn register_validated_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        self.listeners
            .lock()
            .expect("listener lock poisoned")
            .push(listener);
        Ok(())
    }

    async fn unregister_validated_owned_listener(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
        listener: Arc<dyn UOwnedListener>,
    ) -> Result<(), UStatus> {
        self.listeners
            .lock()
            .expect("listener lock poisoned")
            .retain(|held| !Arc::ptr_eq(held, &listener));
        Ok(())
    }
}

struct NoopListener;

#[async_trait]
impl UOwnedListener for NoopListener {
    async fn on_receive_owned(&self, _frame: UOwnedFrame) {}
}

fn topic() -> UUri {
    UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).expect("valid topic")
}

#[tokio::test]
async fn dropping_the_transport_releases_its_listeners() {
    let listener: Arc<dyn UOwnedListener> = Arc::new(NoopListener);
    let transport = RetainingTransport::default();
    transport
        .register_owned_listener(&topic(), None, Arc::clone(&listener))
        .await
        .expect("registration succeeds");
    assert_eq!(
        Arc::strong_count(&listener),
        2,
        "only the caller and the transport may hold the listener"
    );
    drop(transport);
    assert_eq!(
        Arc::strong_count(&listener),
        1,
        "a dropped transport must not keep its listeners alive"
    );
}

#[tokio::test]
async fn unregistering_passes_the_callers_listener_through() {
    let listener: Arc<dyn UOwnedListener> = Arc::new(NoopListener);
    let transport = RetainingTransport::default();
    transport
        .register_owned_listener(&topic(), None, Arc::clone(&listener))
        .await
        .expect("registration succeeds");
    transport
        .unregister_owned_listener(&topic(), None, Arc::clone(&listener))
        .await
        .expect("unregistration succeeds");
    assert_eq!(Arc::strong_count(&listener), 1);
    assert!(transport
        .listeners
        .lock()
        .expect("listener lock poisoned")
        .is_empty());
}

#[tokio::test]
async fn a_second_transport_does_not_observe_the_first_transports_listeners() {
    let listener: Arc<dyn UOwnedListener> = Arc::new(NoopListener);
    let first = RetainingTransport::default();
    first
        .register_owned_listener(&topic(), None, Arc::clone(&listener))
        .await
        .expect("registration succeeds");
    drop(first);
    let second = RetainingTransport::default();
    second
        .unregister_owned_listener(&topic(), None, Arc::clone(&listener))
        .await
        .expect("unregistration of an unknown listener is the implementation's call");
    assert_eq!(Arc::strong_count(&listener), 1);
}
