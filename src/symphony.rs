/********************************************************************************
 * Copyright (c) 2025 Contributors to the Eclipse Foundation
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

//! Helpers for exposing an Eclipse Symphony Target Provider over the native
//! uProtocol Communication Level RPC API.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde_json::Value;
use symphony::models::{ComponentResultSpec, ComponentSpec, DeploymentSpec};
use tracing::{debug, error, trace, warn, Level};

use crate::{
    communication::{RequestHandler, RpcServer, ServiceInvocationError, UPayload},
    UAttributes, UEncoding,
};

pub const METHOD_GET_RESOURCE_ID: u16 = 0x0001;
pub const METHOD_UPDATE_RESOURCE_ID: u16 = 0x0002;
pub const METHOD_DELETE_RESOURCE_ID: u16 = 0x0003;

type DeploymentError = Box<dyn std::error::Error + Send + Sync>;

/// Registers RPC endpoints for managing a deployment target via Eclipse Symphony's uProtocol
/// Target Provider.
///
/// This registers three RPC endpoints that delegate to the provided [`DeploymentTarget`]:
/// `Get` (`0x0001`), `Update` (`0x0002`), and `Delete` (`0x0003`).
///
/// # Errors
///
/// Returns an error if any endpoint cannot be registered on the RPC server.
pub async fn register_target_provider_endpoints<R, T>(
    rpc_server: &R,
    deployment_target: Arc<T>,
) -> Result<(), Box<dyn std::error::Error>>
where
    R: RpcServer,
    T: DeploymentTarget + 'static,
{
    let get_op = Arc::new(GetOperation {
        target: deployment_target.clone(),
    });
    let apply_op = Arc::new(ApplyOperation {
        target: deployment_target,
    });
    rpc_server
        .register_endpoint(None, METHOD_GET_RESOURCE_ID, get_op)
        .await
        .inspect_err(|e| error!("failed to register Get operation on RPC Server: {e}"))?;
    rpc_server
        .register_endpoint(None, METHOD_UPDATE_RESOURCE_ID, apply_op.clone())
        .await
        .inspect_err(|e| error!("failed to register Update operation on RPC Server: {e}"))?;
    rpc_server
        .register_endpoint(None, METHOD_DELETE_RESOURCE_ID, apply_op)
        .await
        .inspect_err(|e| error!("failed to register Delete operation on RPC Server: {e}"))?;
    Ok(())
}

#[cfg_attr(any(test, feature = "test-util"), mockall::automock)]
#[async_trait]
pub trait DeploymentTarget: Send + Sync {
    /// Retrieves the current status of components within a deployment.
    async fn get(
        &self,
        components: Vec<ComponentSpec>,
        deployment_spec: DeploymentSpec,
    ) -> Result<Vec<ComponentSpec>, DeploymentError>;

    /// Updates the specified components within a deployment.
    async fn update(
        &self,
        components_to_update: Vec<ComponentSpec>,
        deployment_spec: DeploymentSpec,
    ) -> Result<HashMap<String, ComponentResultSpec>, DeploymentError>;

    /// Removes the specified components from a deployment.
    async fn delete(
        &self,
        components_to_delete: Vec<ComponentSpec>,
        deployment_spec: DeploymentSpec,
    ) -> Result<HashMap<String, ComponentResultSpec>, DeploymentError>;
}

fn json_encoding() -> UEncoding {
    UEncoding::new("json", "application/json", None::<String>)
}

fn extract_request_data(
    request_payload: Option<UPayload>,
) -> Result<Value, ServiceInvocationError> {
    let Some(req_payload) = request_payload.filter(|req_payload| {
        req_payload.encoding().format_id() == "json"
            || req_payload.encoding().content_type() == "application/json"
    }) else {
        return Err(ServiceInvocationError::InvalidArgument(
            "request has no JSON payload".to_string(),
        ));
    };

    serde_json::from_slice(req_payload.payload_bytes()).map_err(|err| {
        debug!("failed to deserialize request payload: {err:?}");
        ServiceInvocationError::InvalidArgument("request payload is not valid JSON".to_string())
    })
}

struct GetOperation<T: DeploymentTarget> {
    target: Arc<T>,
}

#[async_trait]
impl<T: DeploymentTarget> RequestHandler for GetOperation<T> {
    async fn handle_request(
        &self,
        _resource_id: u16,
        attributes: &UAttributes,
        request_payload: Option<UPayload>,
    ) -> Result<Option<UPayload>, ServiceInvocationError> {
        let source_uri = attributes.source().to_uri(true);
        if tracing::enabled!(Level::DEBUG) {
            debug!(source = source_uri, "processing GET request");
        }
        let request_data = extract_request_data(request_payload)?;
        if tracing::enabled!(Level::TRACE) {
            trace!(
                source = source_uri,
                "payload: {}",
                serde_json::to_string_pretty(&request_data).expect("failed to serialize Value")
            );
        }
        let deployment_spec: DeploymentSpec = request_data
            .get("deployment")
            .ok_or_else(|| {
                debug!(
                    source = source_uri,
                    "request does not contain DeploymentSpec"
                );
                ServiceInvocationError::InvalidArgument(
                    "request does not contain DeploymentSpec".to_string(),
                )
            })
            .and_then(|deployment| {
                serde_json::from_value(deployment.clone()).map_err(|err| {
                    debug!(
                        source = source_uri,
                        "request contains invalid DeploymentSpec: {err}"
                    );
                    ServiceInvocationError::InvalidArgument(
                        "request contains invalid DeploymentSpec".to_string(),
                    )
                })
            })?;
        let component_specs: Vec<ComponentSpec> = request_data
            .get("components")
            .ok_or_else(|| {
                debug!(
                    source = source_uri,
                    "request does not contain ComponentSpec array"
                );
                ServiceInvocationError::InvalidArgument(
                    "request does not contain ComponentSpec array".to_string(),
                )
            })
            .and_then(|components| {
                serde_json::from_value(components.clone()).map_err(|err| {
                    debug!(
                        source = source_uri,
                        "request contains invalid ComponentSpec array: {err}"
                    );
                    ServiceInvocationError::InvalidArgument(
                        "request contains invalid ComponentSpec array".to_string(),
                    )
                })
            })?;

        let result = self
            .target
            .get(component_specs, deployment_spec)
            .await
            .map_err(|err| {
                warn!(source = source_uri, "error getting component status: {err}");
                ServiceInvocationError::Internal("failed to get component status".to_string())
            })?;
        let serialized_response_data = serde_json::to_vec(&result).map_err(|err| {
            warn!(
                source = source_uri,
                "error serializing ComponentSpec: {err}"
            );
            ServiceInvocationError::Internal("failed to create response payload".to_string())
        })?;
        if tracing::enabled!(Level::TRACE) {
            trace!(
                source = source_uri,
                "returning response: {}",
                serde_json::to_string_pretty(&result).expect("failed to serialize Value")
            );
        }
        Ok(Some(UPayload::new(
            serialized_response_data,
            json_encoding(),
        )))
    }
}

struct ApplyOperation<T: DeploymentTarget> {
    target: Arc<T>,
}

#[async_trait]
impl<T: DeploymentTarget> RequestHandler for ApplyOperation<T> {
    async fn handle_request(
        &self,
        resource_id: u16,
        attributes: &UAttributes,
        request_payload: Option<UPayload>,
    ) -> Result<Option<UPayload>, ServiceInvocationError> {
        let source_uri = attributes.source().to_uri(true);
        let sink_uri = attributes
            .sink()
            .map(|sink| sink.to_uri(true))
            .unwrap_or_else(|| "<none>".to_string());
        if tracing::enabled!(Level::DEBUG) {
            debug!(source = source_uri, method = sink_uri, "processing request");
        }
        let request_data = extract_request_data(request_payload)?;
        if tracing::enabled!(Level::TRACE) {
            trace!(
                "payload: {}",
                serde_json::to_string_pretty(&request_data).expect("failed to serialize Value")
            );
        }

        let deployment_spec: DeploymentSpec = request_data
            .get("deployment")
            .ok_or_else(|| {
                debug!(
                    source = source_uri,
                    method = sink_uri,
                    "request does not contain DeploymentSpec"
                );
                ServiceInvocationError::InvalidArgument(
                    "request does not contain DeploymentSpec".to_string(),
                )
            })
            .and_then(|deployment| {
                serde_json::from_value(deployment.clone()).map_err(|err| {
                    debug!(
                        source = source_uri,
                        method = sink_uri,
                        "request contains invalid DeploymentSpec: {err}"
                    );
                    ServiceInvocationError::InvalidArgument(
                        "request contains invalid DeploymentSpec".to_string(),
                    )
                })
            })?;

        let affected_components: Vec<ComponentSpec> = request_data
            .get("components")
            .ok_or_else(|| {
                debug!(
                    source = source_uri,
                    method = sink_uri,
                    "request does not contain ComponentSpec array"
                );
                ServiceInvocationError::InvalidArgument(
                    "request does not contain ComponentSpec array".to_string(),
                )
            })
            .and_then(|components| {
                serde_json::from_value(components.clone()).map_err(|err| {
                    debug!(
                        source = source_uri,
                        method = sink_uri,
                        "request contains invalid ComponentSpec array: {err}"
                    );
                    ServiceInvocationError::InvalidArgument(
                        "request contains invalid ComponentSpec array".to_string(),
                    )
                })
            })?;

        let result = match resource_id {
            METHOD_UPDATE_RESOURCE_ID => self
                .target
                .update(affected_components, deployment_spec)
                .await
                .map_err(|err| {
                    warn!(
                        source = source_uri,
                        method = sink_uri,
                        "error updating components: {err}"
                    );
                    ServiceInvocationError::Internal("failed to update components".to_string())
                }),
            METHOD_DELETE_RESOURCE_ID => self
                .target
                .delete(affected_components, deployment_spec)
                .await
                .map_err(|err| {
                    warn!(
                        source = source_uri,
                        method = sink_uri,
                        "error deleting components: {err}"
                    );
                    ServiceInvocationError::Internal("failed to delete components".to_string())
                }),
            _ => Err(ServiceInvocationError::Unimplemented(
                "no such operation".to_string(),
            )),
        }?;

        let serialized_response_data = serde_json::to_vec(&result).map_err(|err| {
            warn!(
                source = source_uri,
                method = sink_uri,
                "error serializing HashMap: {err}"
            );
            ServiceInvocationError::Internal("failed to create response payload".to_string())
        })?;

        Ok(Some(UPayload::new(
            serialized_response_data,
            json_encoding(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{UMessageType, UUri, UUID};

    use super::*;

    struct TestDeploymentTarget;

    #[async_trait]
    impl DeploymentTarget for TestDeploymentTarget {
        async fn get(
            &self,
            _components: Vec<ComponentSpec>,
            _deployment_spec: DeploymentSpec,
        ) -> Result<Vec<ComponentSpec>, DeploymentError> {
            Ok(vec![])
        }

        async fn update(
            &self,
            _components_to_update: Vec<ComponentSpec>,
            _deployment_spec: DeploymentSpec,
        ) -> Result<HashMap<String, ComponentResultSpec>, DeploymentError> {
            Ok(HashMap::new())
        }

        async fn delete(
            &self,
            _components_to_delete: Vec<ComponentSpec>,
            _deployment_spec: DeploymentSpec,
        ) -> Result<HashMap<String, ComponentResultSpec>, DeploymentError> {
            Ok(HashMap::new())
        }
    }

    fn create_method_uri(resource_id: u16) -> UUri {
        UUri::try_from_parts("authority", 0x10aa2, 0x01, resource_id)
            .expect("failed to create method URI")
    }

    fn create_request_attributes(resource_id: u16) -> UAttributes {
        UAttributes::new(
            UUID::build(),
            UUri::try_from_parts("authority", 0x10aa1, 0x01, 0x0000)
                .expect("failed to create source URI"),
            Some(create_method_uri(resource_id)),
            UMessageType::Request,
        )
    }

    fn request_payload() -> UPayload {
        let request_data = json!({
            "deployment": DeploymentSpec::empty(),
            "components": []
        });
        UPayload::new(
            serde_json::to_vec(&request_data).expect("failed to create request payload"),
            json_encoding(),
        )
    }

    #[tokio::test]
    async fn endpoints_delegate_to_deployment_target() {
        let target = Arc::new(TestDeploymentTarget);
        let get_op = GetOperation {
            target: target.clone(),
        };
        let apply_op = ApplyOperation { target };

        assert!(get_op
            .handle_request(
                METHOD_GET_RESOURCE_ID,
                &create_request_attributes(METHOD_GET_RESOURCE_ID),
                Some(request_payload()),
            )
            .await
            .is_ok());
        assert!(apply_op
            .handle_request(
                METHOD_UPDATE_RESOURCE_ID,
                &create_request_attributes(METHOD_UPDATE_RESOURCE_ID),
                Some(request_payload()),
            )
            .await
            .is_ok());
        assert!(apply_op
            .handle_request(
                METHOD_DELETE_RESOURCE_ID,
                &create_request_attributes(METHOD_DELETE_RESOURCE_ID),
                Some(request_payload()),
            )
            .await
            .is_ok());
    }
}
