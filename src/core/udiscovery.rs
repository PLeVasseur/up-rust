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

use async_trait::async_trait;

use crate::{UStatus, UUri};

/// The uEntity type identifier of the uDiscovery service.
pub const UDISCOVERY_TYPE_ID: u32 = 0x0000_0001;
/// The latest major version of the uDiscovery service contract represented here.
pub const UDISCOVERY_VERSION_MAJOR: u8 = 0x03;
/// Resource identifier of uDiscovery's find-services operation.
pub const RESOURCE_ID_FIND_SERVICES: u16 = 0x0001;
/// Resource identifier of uDiscovery's get-service-topics operation.
pub const RESOURCE_ID_GET_SERVICE_TOPICS: u16 = 0x0002;

/// Gets a local uDiscovery service URI for a resource.
pub fn udiscovery_uri(resource_id: u16) -> UUri {
    UUri::try_from_parts(
        "",
        UDISCOVERY_TYPE_ID,
        UDISCOVERY_VERSION_MAJOR,
        resource_id,
    )
    .expect("native uDiscovery constants must form a valid UUri")
}

/// Request for finding services matching a URI pattern.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FindServicesRequest {
    pub uri_pattern: UUri,
    pub recursive: bool,
}

/// Response containing service URIs matching a discovery request.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FindServicesResponse {
    pub services: Vec<UUri>,
}

/// Request for topic information matching a URI pattern.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetServiceTopicsRequest {
    pub topic_pattern: UUri,
    pub recursive: bool,
}

/// Native topic information published by a service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceTopicInfo {
    pub topic: UUri,
    pub publisher: Option<UUri>,
}

/// Response containing service topic information.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GetServiceTopicsResponse {
    pub topics: Vec<ServiceTopicInfo>,
}

/// Native uDiscovery client contract.
#[async_trait]
pub trait UDiscovery: Send + Sync {
    async fn find_services(
        &self,
        request: FindServicesRequest,
    ) -> Result<FindServicesResponse, UStatus>;

    async fn get_service_topics(
        &self,
        request: GetServiceTopicsRequest,
    ) -> Result<GetServiceTopicsResponse, UStatus>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udiscovery_uri_uses_core_service_identity() {
        let uri = udiscovery_uri(RESOURCE_ID_FIND_SERVICES);

        assert_eq!(uri.authority_name(), "");
        assert_eq!(uri.ue_id(), UDISCOVERY_TYPE_ID);
        assert_eq!(uri.ue_version_major(), u32::from(UDISCOVERY_VERSION_MAJOR));
        assert_eq!(uri.resource_id_raw(), u32::from(RESOURCE_ID_FIND_SERVICES));
    }
}
