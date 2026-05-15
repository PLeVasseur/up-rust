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

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum State {
    #[default]
    Unsubscribed,
    SubscribePending,
    Subscribed,
    UnsubscribePending,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct SubscriptionStatus {
    pub state: State,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct EventDeliveryConfig {
    pub id: String,
    pub endpoint_type: String,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct SubscribeAttributes {
    pub sample_period_ms: Option<u32>,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct SubscriberInfo {
    pub uri: Option<UUri>,
}

impl SubscriberInfo {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Subscription {
    pub topic: Option<UUri>,
    pub subscriber: Option<SubscriberInfo>,
    pub status: Option<SubscriptionStatus>,
    pub attributes: Option<SubscribeAttributes>,
    pub config: Option<EventDeliveryConfig>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SubscriptionRequest {
    pub topic: Option<UUri>,
    pub attributes: Option<SubscribeAttributes>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SubscriptionResponse {
    pub status: Option<SubscriptionStatus>,
    pub config: Option<EventDeliveryConfig>,
    pub topic: Option<UUri>,
}

impl SubscriptionResponse {
    pub fn is_state(&self, state: State) -> bool {
        self.status
            .as_ref()
            .is_some_and(|status| status.state == state)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Update {
    pub topic: Option<UUri>,
    pub subscriber: Option<SubscriberInfo>,
    pub status: Option<SubscriptionStatus>,
    pub attributes: Option<SubscribeAttributes>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FetchSubscriptionsRequest {
    pub request: Option<FetchSubscriptionsRequestKind>,
    pub offset: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FetchSubscriptionsRequestKind {
    Topic(UUri),
    Subscriber(SubscriberInfo),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FetchSubscriptionsResponse {
    pub subscriptions: Vec<Subscription>,
    pub has_more_records: Option<bool>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UnsubscribeRequest {
    pub topic: Option<UUri>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UnsubscribeResponse;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NotificationsRequest {
    pub topic: Option<UUri>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NotificationsResponse;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FetchSubscribersRequest {
    pub topic: Option<UUri>,
    pub offset: Option<u32>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FetchSubscribersResponse {
    pub subscribers: Vec<SubscriberInfo>,
    pub has_more_records: Option<bool>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResetRequest;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResetResponse;

#[cfg_attr(any(test, feature = "test-util"), mockall::automock)]
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

#[cfg(feature = "protobuf-wire")]
mod protobuf_codec {
    use protobuf::{CodedOutputStream, EnumOrUnknown, Message};

    use super::*;
    use crate::{ProtobufWire, UDeserializer, USerializer, UUri, UWireError};

    use crate::up_core_api::usubscription as proto;

    type ProtoUri = crate::up_core_api::uri::UUri;

    fn to_proto_uri(uri: &UUri) -> ProtoUri {
        ProtoUri {
            authority_name: uri.authority_name.clone(),
            ue_id: uri.ue_id,
            ue_version_major: uri.ue_version_major,
            resource_id: uri.resource_id,
            ..Default::default()
        }
    }

    fn from_proto_uri(uri: &ProtoUri) -> UUri {
        UUri {
            authority_name: uri.authority_name.clone(),
            ue_id: uri.ue_id,
            ue_version_major: uri.ue_version_major,
            resource_id: uri.resource_id,
        }
    }

    fn to_proto_state(state: State) -> proto::subscription_status::State {
        match state {
            State::Unsubscribed => proto::subscription_status::State::UNSUBSCRIBED,
            State::SubscribePending => proto::subscription_status::State::SUBSCRIBE_PENDING,
            State::Subscribed => proto::subscription_status::State::SUBSCRIBED,
            State::UnsubscribePending => proto::subscription_status::State::UNSUBSCRIBE_PENDING,
        }
    }

    fn from_proto_state(state: proto::subscription_status::State) -> State {
        match state {
            proto::subscription_status::State::UNSUBSCRIBED => State::Unsubscribed,
            proto::subscription_status::State::SUBSCRIBE_PENDING => State::SubscribePending,
            proto::subscription_status::State::SUBSCRIBED => State::Subscribed,
            proto::subscription_status::State::UNSUBSCRIBE_PENDING => State::UnsubscribePending,
        }
    }

    fn to_proto_status(status: &SubscriptionStatus) -> proto::SubscriptionStatus {
        proto::SubscriptionStatus {
            state: EnumOrUnknown::from(to_proto_state(status.state)),
            message: status.message.clone(),
            ..Default::default()
        }
    }

    fn from_proto_status(status: &proto::SubscriptionStatus) -> SubscriptionStatus {
        SubscriptionStatus {
            state: status
                .state
                .enum_value()
                .map_or(State::Unsubscribed, from_proto_state),
            message: status.message.clone(),
        }
    }

    fn to_proto_config(config: &EventDeliveryConfig) -> proto::EventDeliveryConfig {
        proto::EventDeliveryConfig {
            id: config.id.clone(),
            type_: config.endpoint_type.clone(),
            ..Default::default()
        }
    }

    fn from_proto_config(config: &proto::EventDeliveryConfig) -> EventDeliveryConfig {
        EventDeliveryConfig {
            id: config.id.clone(),
            endpoint_type: config.type_.clone(),
        }
    }

    fn to_proto_attributes(attributes: &SubscribeAttributes) -> proto::SubscribeAttributes {
        proto::SubscribeAttributes {
            sample_period_ms: attributes.sample_period_ms,
            ..Default::default()
        }
    }

    fn from_proto_attributes(attributes: &proto::SubscribeAttributes) -> SubscribeAttributes {
        SubscribeAttributes {
            sample_period_ms: attributes.sample_period_ms,
        }
    }

    fn to_proto_subscriber(subscriber: &SubscriberInfo) -> proto::SubscriberInfo {
        proto::SubscriberInfo {
            uri: subscriber.uri.as_ref().map(to_proto_uri).into(),
            ..Default::default()
        }
    }

    fn from_proto_subscriber(subscriber: &proto::SubscriberInfo) -> SubscriberInfo {
        SubscriberInfo {
            uri: subscriber.uri.as_ref().map(from_proto_uri),
        }
    }

    fn to_proto_subscription(subscription: &Subscription) -> proto::Subscription {
        proto::Subscription {
            topic: subscription.topic.as_ref().map(to_proto_uri).into(),
            subscriber: subscription
                .subscriber
                .as_ref()
                .map(to_proto_subscriber)
                .into(),
            status: subscription.status.as_ref().map(to_proto_status).into(),
            attributes: subscription
                .attributes
                .as_ref()
                .map(to_proto_attributes)
                .into(),
            config: subscription.config.as_ref().map(to_proto_config).into(),
            ..Default::default()
        }
    }

    fn from_proto_subscription(subscription: &proto::Subscription) -> Subscription {
        Subscription {
            topic: subscription.topic.as_ref().map(from_proto_uri),
            subscriber: subscription.subscriber.as_ref().map(from_proto_subscriber),
            status: subscription.status.as_ref().map(from_proto_status),
            attributes: subscription.attributes.as_ref().map(from_proto_attributes),
            config: subscription.config.as_ref().map(from_proto_config),
        }
    }

    fn write_proto_message<M>(message: &M, dst: &mut [u8]) -> Result<usize, UWireError>
    where
        M: Message,
    {
        let expected = message.compute_size() as usize;
        let actual = dst.len();
        let out = dst
            .get_mut(..expected)
            .ok_or_else(|| UWireError::buffer_too_small(expected, actual))?;
        let mut output = CodedOutputStream::bytes(out);
        message
            .write_to(&mut output)
            .map_err(|error| UWireError::serialization_error(error.to_string()))?;
        output
            .flush()
            .map_err(|error| UWireError::serialization_error(error.to_string()))?;
        usize::try_from(output.total_bytes_written())
            .map_err(|error| UWireError::serialization_error(error.to_string()))
    }

    macro_rules! impl_proto_wire {
        ($native:ty, $proto:ty, $to_proto:expr, $from_proto:expr) => {
            impl USerializer<ProtobufWire> for $native {
                fn encoded_len(&self) -> usize {
                    let message: $proto = ($to_proto)(self);
                    message.compute_size() as usize
                }

                fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
                    let message: $proto = ($to_proto)(self);
                    write_proto_message(&message, dst)
                }
            }

            impl<'a> UDeserializer<'a, ProtobufWire> for $native {
                fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
                    let message: $proto = protobuf::Message::parse_from_bytes(src)
                        .map_err(|error| UWireError::invalid_payload(error.to_string()))?;
                    Ok(($from_proto)(&message))
                }
            }
        };
    }

    impl_proto_wire!(
        SubscriptionRequest,
        proto::SubscriptionRequest,
        to_proto_subscription_request,
        from_proto_subscription_request
    );
    impl_proto_wire!(
        SubscriptionResponse,
        proto::SubscriptionResponse,
        to_proto_subscription_response,
        from_proto_subscription_response
    );
    impl_proto_wire!(
        UnsubscribeRequest,
        proto::UnsubscribeRequest,
        to_proto_unsubscribe_request,
        from_proto_unsubscribe_request
    );
    impl_proto_wire!(
        UnsubscribeResponse,
        proto::UnsubscribeResponse,
        |_| proto::UnsubscribeResponse::default(),
        |_| UnsubscribeResponse
    );
    impl_proto_wire!(
        FetchSubscriptionsRequest,
        proto::FetchSubscriptionsRequest,
        to_proto_fetch_subscriptions_request,
        from_proto_fetch_subscriptions_request
    );
    impl_proto_wire!(
        FetchSubscriptionsResponse,
        proto::FetchSubscriptionsResponse,
        to_proto_fetch_subscriptions_response,
        from_proto_fetch_subscriptions_response
    );
    impl_proto_wire!(
        NotificationsRequest,
        proto::NotificationsRequest,
        to_proto_notifications_request,
        from_proto_notifications_request
    );
    impl_proto_wire!(
        NotificationsResponse,
        proto::NotificationsResponse,
        |_| proto::NotificationsResponse::default(),
        |_| NotificationsResponse
    );
    impl_proto_wire!(
        FetchSubscribersRequest,
        proto::FetchSubscribersRequest,
        to_proto_fetch_subscribers_request,
        from_proto_fetch_subscribers_request
    );
    impl_proto_wire!(
        FetchSubscribersResponse,
        proto::FetchSubscribersResponse,
        to_proto_fetch_subscribers_response,
        from_proto_fetch_subscribers_response
    );
    impl_proto_wire!(
        ResetRequest,
        proto::ResetRequest,
        |_| proto::ResetRequest::default(),
        |_| ResetRequest
    );
    impl_proto_wire!(
        ResetResponse,
        proto::ResetResponse,
        |_| proto::ResetResponse::default(),
        |_| ResetResponse
    );
    impl_proto_wire!(Update, proto::Update, to_proto_update, from_proto_update);

    fn to_proto_subscription_request(value: &SubscriptionRequest) -> proto::SubscriptionRequest {
        proto::SubscriptionRequest {
            topic: value.topic.as_ref().map(to_proto_uri).into(),
            attributes: value.attributes.as_ref().map(to_proto_attributes).into(),
            ..Default::default()
        }
    }

    fn from_proto_subscription_request(value: &proto::SubscriptionRequest) -> SubscriptionRequest {
        SubscriptionRequest {
            topic: value.topic.as_ref().map(from_proto_uri),
            attributes: value.attributes.as_ref().map(from_proto_attributes),
        }
    }

    fn to_proto_subscription_response(value: &SubscriptionResponse) -> proto::SubscriptionResponse {
        proto::SubscriptionResponse {
            status: value.status.as_ref().map(to_proto_status).into(),
            config: value.config.as_ref().map(to_proto_config).into(),
            topic: value.topic.as_ref().map(to_proto_uri).into(),
            ..Default::default()
        }
    }

    fn from_proto_subscription_response(
        value: &proto::SubscriptionResponse,
    ) -> SubscriptionResponse {
        SubscriptionResponse {
            status: value.status.as_ref().map(from_proto_status),
            config: value.config.as_ref().map(from_proto_config),
            topic: value.topic.as_ref().map(from_proto_uri),
        }
    }

    fn to_proto_unsubscribe_request(value: &UnsubscribeRequest) -> proto::UnsubscribeRequest {
        proto::UnsubscribeRequest {
            topic: value.topic.as_ref().map(to_proto_uri).into(),
            ..Default::default()
        }
    }

    fn from_proto_unsubscribe_request(value: &proto::UnsubscribeRequest) -> UnsubscribeRequest {
        UnsubscribeRequest {
            topic: value.topic.as_ref().map(from_proto_uri),
        }
    }

    fn to_proto_fetch_subscriptions_request(
        value: &FetchSubscriptionsRequest,
    ) -> proto::FetchSubscriptionsRequest {
        proto::FetchSubscriptionsRequest {
            request: value.request.as_ref().map(|request| match request {
                FetchSubscriptionsRequestKind::Topic(topic) => {
                    proto::fetch_subscriptions_request::Request::Topic(to_proto_uri(topic))
                }
                FetchSubscriptionsRequestKind::Subscriber(subscriber) => {
                    proto::fetch_subscriptions_request::Request::Subscriber(to_proto_subscriber(
                        subscriber,
                    ))
                }
            }),
            offset: value.offset,
            ..Default::default()
        }
    }

    fn from_proto_fetch_subscriptions_request(
        value: &proto::FetchSubscriptionsRequest,
    ) -> FetchSubscriptionsRequest {
        FetchSubscriptionsRequest {
            request: value.request.as_ref().map(|request| match request {
                proto::fetch_subscriptions_request::Request::Topic(topic) => {
                    FetchSubscriptionsRequestKind::Topic(from_proto_uri(topic))
                }
                proto::fetch_subscriptions_request::Request::Subscriber(subscriber) => {
                    FetchSubscriptionsRequestKind::Subscriber(from_proto_subscriber(subscriber))
                }
            }),
            offset: value.offset,
        }
    }

    fn to_proto_fetch_subscriptions_response(
        value: &FetchSubscriptionsResponse,
    ) -> proto::FetchSubscriptionsResponse {
        proto::FetchSubscriptionsResponse {
            subscriptions: value
                .subscriptions
                .iter()
                .map(to_proto_subscription)
                .collect(),
            has_more_records: value.has_more_records,
            ..Default::default()
        }
    }

    fn from_proto_fetch_subscriptions_response(
        value: &proto::FetchSubscriptionsResponse,
    ) -> FetchSubscriptionsResponse {
        FetchSubscriptionsResponse {
            subscriptions: value
                .subscriptions
                .iter()
                .map(from_proto_subscription)
                .collect(),
            has_more_records: value.has_more_records,
        }
    }

    fn to_proto_notifications_request(value: &NotificationsRequest) -> proto::NotificationsRequest {
        proto::NotificationsRequest {
            topic: value.topic.as_ref().map(to_proto_uri).into(),
            ..Default::default()
        }
    }

    fn from_proto_notifications_request(
        value: &proto::NotificationsRequest,
    ) -> NotificationsRequest {
        NotificationsRequest {
            topic: value.topic.as_ref().map(from_proto_uri),
        }
    }

    fn to_proto_fetch_subscribers_request(
        value: &FetchSubscribersRequest,
    ) -> proto::FetchSubscribersRequest {
        proto::FetchSubscribersRequest {
            topic: value.topic.as_ref().map(to_proto_uri).into(),
            offset: value.offset,
            ..Default::default()
        }
    }

    fn from_proto_fetch_subscribers_request(
        value: &proto::FetchSubscribersRequest,
    ) -> FetchSubscribersRequest {
        FetchSubscribersRequest {
            topic: value.topic.as_ref().map(from_proto_uri),
            offset: value.offset,
        }
    }

    fn to_proto_fetch_subscribers_response(
        value: &FetchSubscribersResponse,
    ) -> proto::FetchSubscribersResponse {
        proto::FetchSubscribersResponse {
            subscribers: value.subscribers.iter().map(to_proto_subscriber).collect(),
            has_more_records: value.has_more_records,
            ..Default::default()
        }
    }

    fn from_proto_fetch_subscribers_response(
        value: &proto::FetchSubscribersResponse,
    ) -> FetchSubscribersResponse {
        FetchSubscribersResponse {
            subscribers: value
                .subscribers
                .iter()
                .map(from_proto_subscriber)
                .collect(),
            has_more_records: value.has_more_records,
        }
    }

    fn to_proto_update(value: &Update) -> proto::Update {
        proto::Update {
            topic: value.topic.as_ref().map(to_proto_uri).into(),
            subscriber: value.subscriber.as_ref().map(to_proto_subscriber).into(),
            status: value.status.as_ref().map(to_proto_status).into(),
            attributes: value.attributes.as_ref().map(to_proto_attributes).into(),
            ..Default::default()
        }
    }

    fn from_proto_update(value: &proto::Update) -> Update {
        Update {
            topic: value.topic.as_ref().map(from_proto_uri),
            subscriber: value.subscriber.as_ref().map(from_proto_subscriber),
            status: value.status.as_ref().map(from_proto_status),
            attributes: value.attributes.as_ref().map(from_proto_attributes),
        }
    }
}

#[cfg(all(test, feature = "protobuf-wire"))]
mod tests {
    use super::*;
    use crate::{ProtobufWire, UDeserializer, USerializer, UUri};

    #[test]
    fn subscription_request_round_trips_as_protobuf_payload() {
        let request = SubscriptionRequest {
            topic: Some(UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap()),
            attributes: Some(SubscribeAttributes {
                sample_period_ms: Some(100),
            }),
        };

        let bytes = request.serialize_owned().unwrap();
        let decoded = SubscriptionRequest::deserialize_from(&bytes).unwrap();

        assert_eq!(decoded, request);
    }

    #[test]
    fn update_round_trips_as_protobuf_payload() {
        let update = Update {
            topic: Some(UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).unwrap()),
            status: Some(SubscriptionStatus {
                state: State::Subscribed,
                message: "ready".to_string(),
            }),
            ..Default::default()
        };

        let bytes = <Update as USerializer<ProtobufWire>>::serialize_owned(&update).unwrap();
        let decoded = <Update as UDeserializer<ProtobufWire>>::deserialize_from(&bytes).unwrap();

        assert_eq!(decoded, update);
    }
}
