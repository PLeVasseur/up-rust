//! Each constructor has exactly one local deployment; peer arguments are addresses.
#[cfg(feature = "typed-communication")]
use super::schema::{self, Query, Reading};
#[cfg(feature = "typed-communication")]
use std::sync::Arc;
#[cfg(feature = "typed-communication")]
use up_rust::{
    communication::application::*, core::usubscription::USubscription, LocalUriProvider,
};

#[cfg(feature = "typed-communication")]
pub struct PublisherOnly {
    pub reading: LegacyPublisher<Reading>,
}
#[cfg(feature = "typed-communication")]
impl PublisherOnly {
    pub fn new(local: &schema::LocalDeployment) -> Result<Self, SerializedError> {
        Ok(Self {
            reading: schema::TEMPERATURE.publisher(local, PublishOptions::default())?,
        })
    }
}

#[path = "independent_apps/multiple_publishers.rs"]
mod multiple_publishers;
pub use multiple_publishers::{MultiplePublishers, MultiplePublishersSetup};

#[cfg(feature = "typed-communication")]
pub struct SubscriberOnly {
    pub readings: Subscription,
}
#[cfg(feature = "typed-communication")]
impl SubscriberOnly {
    pub async fn prepare<H: Handler<Reading>>(
        local: &schema::LocalDeployment,
        publisher_address: &dyn LocalUriProvider,
        service: Arc<dyn USubscription>,
        handler: H,
    ) -> Result<Self, SerializedError> {
        Ok(Self {
            readings: schema::TEMPERATURE
                .subscription(local, publisher_address, service, handler)
                .await?,
        })
    }
}

#[cfg(feature = "typed-communication")]
pub struct ClientOnly {
    pub read: RpcClient<Query, Reading>,
}
#[cfg(feature = "typed-communication")]
impl ClientOnly {
    pub async fn prepare(
        local: &schema::LocalDeployment,
        server_address: &dyn LocalUriProvider,
    ) -> Result<Self, SerializedError> {
        Ok(Self {
            read: schema::READ
                .client(local, server_address, OperationOptions::default().rpc)
                .await?,
        })
    }
}

#[cfg(feature = "typed-communication")]
pub struct ServerOnly {
    pub read: RpcServer,
}
#[cfg(feature = "typed-communication")]
impl ServerOnly {
    pub async fn prepare<H: RequestHandler<Query, Reply = u32>>(
        local: &schema::LocalDeployment,
        handler: H,
    ) -> Result<Self, SerializedError> {
        Ok(Self {
            read: schema::READ
                .server(
                    local,
                    handler,
                    |value| Reading { value },
                    OperationOptions::default().server,
                )
                .await?,
        })
    }
}

#[cfg(feature = "typed-communication")]
pub struct NotifierOnly {
    pub alarm: Notifier<Reading>,
}
#[cfg(feature = "typed-communication")]
impl NotifierOnly {
    pub fn new(
        local: &schema::LocalDeployment,
        destination: up_rust::UUri,
    ) -> Result<Self, SerializedError> {
        Ok(Self {
            alarm: schema::ALARM.notifier(local, destination, PublishOptions::default())?,
        })
    }
}

#[cfg(feature = "typed-communication")]
pub struct ReceiverOnly {
    pub alarm: Receiver,
}
#[cfg(feature = "typed-communication")]
impl ReceiverOnly {
    pub async fn prepare<H: Handler<Reading>>(
        local: &schema::LocalDeployment,
        source: &dyn LocalUriProvider,
        handler: H,
    ) -> Result<Self, SerializedError> {
        Ok(Self {
            alarm: schema::ALARM.receiver(local, source, handler).await?,
        })
    }
}

#[cfg(feature = "typed-communication")]
pub struct MixedSubset {
    pub temperature: LegacyPublisher<Reading>,
    pub read: RpcClient<Query, Reading>,
}

#[cfg(feature = "typed-communication")]
pub fn mixed_subset(publisher: PublisherOnly, client: ClientOnly) -> MixedSubset {
    MixedSubset {
        temperature: publisher.reading,
        read: client.read,
    }
}
