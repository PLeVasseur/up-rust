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

//! uSubscription service contract bindings.
//!
//! The uSubscription service wire contract is defined by `up-core-api` Protocol
//! Buffers. When `protobuf-wire` is enabled this module re-exports the generated
//! DTOs directly so service payloads remain full-fidelity protobuf messages
//! inside native frames.

#[cfg(feature = "protobuf-wire")]
use async_trait::async_trait;

#[cfg(feature = "protobuf-wire")]
use crate::{UStatus, UUri};

#[cfg(feature = "protobuf-wire")]
pub use crate::up_core_api::usubscription::{
    fetch_subscriptions_request, subscription_status, EventDeliveryConfig, FetchSubscribersRequest,
    FetchSubscribersResponse, FetchSubscriptionsRequest, FetchSubscriptionsResponse,
    NotificationsRequest, NotificationsResponse, PassiveMode, ResetRequest, ResetResponse,
    SubscribeAttributes, SubscriberInfo, Subscription, SubscriptionRequest, SubscriptionResponse,
    SubscriptionStatus, UnsubscribeRequest, UnsubscribeResponse, Update,
};

#[cfg(feature = "protobuf-wire")]
pub use subscription_status::State;

#[cfg(feature = "protobuf-wire")]
pub type ProtoUUri = crate::up_core_api::uri::UUri;

#[cfg(feature = "protobuf-wire")]
pub fn to_proto_uri(uri: &UUri) -> ProtoUUri {
    ProtoUUri {
        authority_name: uri.authority_name().to_string(),
        ue_id: uri.ue_id(),
        ue_version_major: uri.ue_version_major(),
        resource_id: uri.resource_id_raw(),
        ..Default::default()
    }
}

#[cfg(feature = "protobuf-wire")]
pub fn from_proto_uri(uri: &ProtoUUri) -> UUri {
    UUri::from_parts_unchecked(
        uri.authority_name.clone(),
        uri.ue_id,
        uri.ue_version_major,
        uri.resource_id,
    )
}

#[cfg(feature = "protobuf-wire")]
pub fn subscription_status(state: State, message: impl Into<String>) -> SubscriptionStatus {
    SubscriptionStatus {
        state: protobuf::EnumOrUnknown::from(state),
        message: message.into(),
        ..Default::default()
    }
}

#[cfg(feature = "protobuf-wire")]
pub trait SubscriptionResponseExt {
    fn is_state(&self, state: State) -> bool;
}

#[cfg(feature = "protobuf-wire")]
impl SubscriptionResponseExt for SubscriptionResponse {
    fn is_state(&self, state: State) -> bool {
        self.status.as_ref().is_some_and(|status| {
            status
                .state
                .enum_value()
                .is_ok_and(|actual| actual == state)
        })
    }
}

#[cfg(feature = "protobuf-wire")]
pub trait UpdateExt {
    fn native_topic(&self) -> Option<UUri>;
}

#[cfg(feature = "protobuf-wire")]
impl UpdateExt for Update {
    fn native_topic(&self) -> Option<UUri> {
        self.topic.as_ref().map(from_proto_uri)
    }
}

#[cfg_attr(any(test, feature = "test-util"), mockall::automock)]
#[cfg(feature = "protobuf-wire")]
#[async_trait]
pub trait USubscription: Send + Sync {
    async fn subscribe(
        &self,
        subscription_request: SubscriptionRequest,
    ) -> Result<SubscriptionResponse, UStatus>;

    async fn fetch_subscriptions(
        &self,
        fetch_subscriptions_request: FetchSubscriptionsRequest,
    ) -> Result<FetchSubscriptionsResponse, UStatus>;

    async fn unsubscribe(&self, unsubscribe_request: UnsubscribeRequest) -> Result<(), UStatus>;

    async fn register_for_notifications(
        &self,
        notifications_register_request: NotificationsRequest,
    ) -> Result<(), UStatus>;

    async fn unregister_for_notifications(
        &self,
        notifications_unregister_request: NotificationsRequest,
    ) -> Result<(), UStatus>;

    async fn fetch_subscribers(
        &self,
        fetch_subscribers_request: FetchSubscribersRequest,
    ) -> Result<FetchSubscribersResponse, UStatus>;

    async fn reset(&self, reset_request: ResetRequest) -> Result<ResetResponse, UStatus>;
}

#[cfg(all(test, feature = "protobuf-wire"))]
mod tests {
    use super::*;
    use crate::{
        wire::{UDeserializer, USerializer},
        ProtobufWire, UUri,
    };

    #[test]
    fn subscription_request_round_trips_as_protobuf_payload() {
        let request = SubscriptionRequest {
            topic: Some(to_proto_uri(
                &UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap(),
            ))
            .into(),
            attributes: Some(SubscribeAttributes {
                sample_period_ms: Some(100),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        };

        let bytes = request.serialize_owned().unwrap();
        let decoded = SubscriptionRequest::deserialize_from(&bytes).unwrap();

        assert_eq!(decoded, request);
    }

    #[test]
    fn update_round_trips_as_protobuf_payload() {
        let update = Update {
            topic: Some(to_proto_uri(
                &UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap(),
            ))
            .into(),
            status: Some(subscription_status(State::SUBSCRIBED, "ready")).into(),
            ..Default::default()
        };

        let bytes = <Update as USerializer<ProtobufWire>>::serialize_owned(&update).unwrap();
        let decoded = <Update as UDeserializer<ProtobufWire>>::deserialize_from(&bytes).unwrap();

        assert_eq!(decoded, update);
    }
}
