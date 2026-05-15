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

#[cfg(feature = "protobuf-wire")]
pub use crate::usubscription::{
    fetch_subscriptions_request, from_proto_uri, subscription_status, to_proto_uri,
    EventDeliveryConfig, FetchSubscribersRequest, FetchSubscribersResponse,
    FetchSubscriptionsRequest, FetchSubscriptionsResponse, NotificationsRequest,
    NotificationsResponse, PassiveMode, ProtoUUri, ResetRequest, ResetResponse, State,
    SubscribeAttributes, SubscriberInfo, Subscription, SubscriptionRequest, SubscriptionResponse,
    SubscriptionResponseExt, SubscriptionStatus, USubscription, UnsubscribeRequest,
    UnsubscribeResponse, Update, UpdateExt,
};

use crate::UUri;

/// The uEntity type identifier of the uSubscription service.
pub const USUBSCRIPTION_TYPE_ID: u32 = 0x0000_0000;
/// The latest major version of the uSubscription service contract represented here.
pub const USUBSCRIPTION_VERSION_MAJOR: u8 = 0x03;
/// Resource identifier of uSubscription's subscribe operation.
pub const RESOURCE_ID_SUBSCRIBE: u16 = 0x0001;
/// Resource identifier of uSubscription's unsubscribe operation.
pub const RESOURCE_ID_UNSUBSCRIBE: u16 = 0x0002;
/// Resource identifier of uSubscription's fetch-subscriptions operation.
pub const RESOURCE_ID_FETCH_SUBSCRIPTIONS: u16 = 0x0003;
/// Resource identifier of uSubscription's register-for-notifications operation.
pub const RESOURCE_ID_REGISTER_FOR_NOTIFICATIONS: u16 = 0x0006;
/// Resource identifier of uSubscription's unregister-for-notifications operation.
pub const RESOURCE_ID_UNREGISTER_FOR_NOTIFICATIONS: u16 = 0x0007;
/// Resource identifier of uSubscription's fetch-subscribers operation.
pub const RESOURCE_ID_FETCH_SUBSCRIBERS: u16 = 0x0008;
/// Resource identifier of uSubscription's reset operation.
pub const RESOURCE_ID_RESET: u16 = 0x0009;
/// Resource identifier of uSubscription's subscription-change topic.
pub const RESOURCE_ID_SUBSCRIPTION_CHANGE: u16 = 0x8000;

/// Gets a local uSubscription service URI for a resource.
pub fn usubscription_uri(resource_id: u16) -> UUri {
    UUri::try_from_parts(
        "",
        USUBSCRIPTION_TYPE_ID,
        USUBSCRIPTION_VERSION_MAJOR,
        resource_id,
    )
    .expect("native uSubscription constants must form a valid UUri")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usubscription_uri_uses_core_service_identity() {
        let uri = usubscription_uri(RESOURCE_ID_SUBSCRIPTION_CHANGE);

        assert_eq!(uri.authority_name(), "");
        assert_eq!(uri.ue_id(), USUBSCRIPTION_TYPE_ID);
        assert_eq!(
            uri.ue_version_major(),
            u32::from(USUBSCRIPTION_VERSION_MAJOR)
        );
        assert_eq!(
            uri.resource_id_raw(),
            u32::from(RESOURCE_ID_SUBSCRIPTION_CHANGE)
        );
    }
}
