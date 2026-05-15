#![cfg(feature = "test-util")]

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

use up_rust::{
    communication::{
        MockNotifier, MockPublisher, MockRequestHandler, MockRpcClient, MockRpcServer,
        MockSubscriber,
    },
    test_util::{InMemoryOwnedTransport, InMemoryZeroCopyTransport, RecordingOwnedListener},
    MockLocalUriProvider, MockUOwnedListener, MockUOwnedTransport, MockUZeroCopyTransport,
};

#[test]
fn transport_and_communication_test_util_types_are_public() {
    let _ = MockLocalUriProvider::new();
    let _ = MockUOwnedListener::new();
    let _ = MockUOwnedTransport::new();
    let _ = MockUZeroCopyTransport::new();

    let _ = MockNotifier::new();
    let _ = MockPublisher::new();
    let _ = MockSubscriber::new();
    let _ = MockRequestHandler::new();
    let _ = MockRpcClient::new();
    let _ = MockRpcServer::new();

    let _ = RecordingOwnedListener::default();
    let _ = InMemoryOwnedTransport::default();
    let _ = InMemoryZeroCopyTransport::default();
}

#[cfg(feature = "protobuf-wire")]
#[test]
fn protobuf_service_test_util_types_are_public() {
    let _ = up_rust::usubscription::MockUSubscription::new();
    let _ = up_rust::communication::MockSubscriptionChangeHandler::new();
}
