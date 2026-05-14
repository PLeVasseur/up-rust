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

//! Demonstrates the native Communication Level RPC API without generated protobuf
//! transport envelopes.

use std::sync::Arc;

use up_rust::{
    communication::{
        CallOptions, InMemoryRpcClient, InMemoryRpcServer, RequestHandler, RpcClient, RpcServer,
        ServiceInvocationError, UPayload,
    },
    local_transport::LocalTransport,
    LocalUriProvider, StaticUriProvider, UAttributes,
};

struct EchoOperation;

#[async_trait::async_trait]
impl RequestHandler for EchoOperation {
    async fn handle_request(
        &self,
        _resource_id: u16,
        _attributes: &UAttributes,
        request_payload: Option<UPayload>,
    ) -> Result<Option<UPayload>, ServiceInvocationError> {
        let Some(request_payload) = request_payload else {
            return Err(ServiceInvocationError::InvalidArgument(
                "request has no payload".to_string(),
            ));
        };
        let name =
            String::from_utf8(request_payload.payload_bytes().to_vec()).map_err(|error| {
                ServiceInvocationError::InvalidArgument(format!(
                    "request payload is not UTF-8: {error}"
                ))
            })?;
        println!("service received request with payload: {name}");
        Ok(Some(UPayload::from_raw(format!("Hello, {name}!"))))
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    const METHOD_RESOURCE_ID: u16 = 0x00a0;

    let uri_provider = Arc::new(StaticUriProvider::new("my-vehicle", 0xa34b, 0x01));
    let transport = Arc::new(LocalTransport::default());

    let rpc_server = InMemoryRpcServer::new(transport.clone(), uri_provider.clone());
    let echo_op = Arc::new(EchoOperation);
    rpc_server
        .register_endpoint(None, METHOD_RESOURCE_ID, echo_op.clone())
        .await?;

    let rpc_client = InMemoryRpcClient::new(transport, uri_provider.clone());

    match rpc_client
        .invoke_method(
            uri_provider.get_resource_uri(METHOD_RESOURCE_ID),
            CallOptions::for_rpc_request(1_000, None, None, None),
            None,
        )
        .await
    {
        Err(ServiceInvocationError::InvalidArgument(message)) => {
            println!("service returned expected error: {message}");
        }
        _ => return Err("expected service to return an InvalidArgument error".into()),
    }

    let response = rpc_client
        .invoke_method(
            uri_provider.get_resource_uri(METHOD_RESOURCE_ID),
            CallOptions::for_rpc_request(1_000, None, None, None),
            Some(UPayload::from_raw("Peter")),
        )
        .await?
        .ok_or("expected service to return response payload")?;
    println!(
        "service returned message: {}",
        String::from_utf8_lossy(response.payload_bytes())
    );

    rpc_server
        .unregister_endpoint(None, METHOD_RESOURCE_ID, echo_op)
        .await?;
    Ok(())
}
