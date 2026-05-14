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

use std::sync::Arc;

use up_rust::{
    communication::{
        CallOptions, Publisher, SimplePublisher, SimpleSubscriber, Subscriber, UPayload,
    },
    local_transport::LocalTransport,
    LocalUriProvider, StaticUriProvider, UOwnedFrame, UOwnedListener,
};

struct ConsolePrinter;

#[async_trait::async_trait]
impl UOwnedListener for ConsolePrinter {
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        let payload = String::from_utf8_lossy(frame.payload_bytes());
        println!("received event: {payload}");
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    const TOPIC_RESOURCE_ID: u16 = 0x8001;

    let uri_provider = Arc::new(StaticUriProvider::new("my-vehicle", 0xa34b, 0x01));
    let transport = Arc::new(LocalTransport::default());
    let publisher = SimplePublisher::new(transport.clone(), uri_provider.clone());
    let subscriber = SimpleSubscriber::new(transport);
    let listener = Arc::new(ConsolePrinter);
    let topic = uri_provider.get_resource_uri(TOPIC_RESOURCE_ID);

    subscriber.subscribe(&topic, listener.clone()).await?;

    publisher
        .publish(
            TOPIC_RESOURCE_ID,
            CallOptions::for_publish(None, None, None),
            Some(UPayload::from_raw("Hello from native publish")),
        )
        .await?;

    subscriber.unsubscribe(&topic, listener).await?;
    Ok(())
}
