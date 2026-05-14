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
    Unspecified,
    Subscribed,
    SubscribePending,
    Unsubscribed,
    UnsubscribePending,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct SubscriptionStatus {
    pub state: State,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct EventDeliveryConfig {
    pub delivery_uri: Option<UUri>,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct SubscribeAttributes {
    pub delivery_config: Option<EventDeliveryConfig>,
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
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SubscriptionRequest {
    pub subscription: Option<Subscription>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SubscriptionResponse {
    pub subscription: Option<Subscription>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Update {
    pub topic: Option<UUri>,
    pub status: Option<SubscriptionStatus>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FetchSubscriptionsRequest;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FetchSubscriptionsResponse {
    pub subscriptions: Vec<Subscription>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UnsubscribeRequest {
    pub subscription: Option<Subscription>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UnsubscribeResponse;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NotificationsRequest {
    pub subscriber: Option<SubscriberInfo>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NotificationsResponse;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FetchSubscribersRequest {
    pub topic: Option<UUri>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FetchSubscribersResponse {
    pub subscribers: Vec<SubscriberInfo>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResetRequest;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResetResponse;

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
