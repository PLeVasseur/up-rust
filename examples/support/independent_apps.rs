//! Each constructor has exactly one local deployment; peer arguments are addresses.
use super::schema::{self, Query, Reading};
use std::sync::Arc;
use up_rust::{
    communication::application::*, core::usubscription::USubscription, LocalUriProvider,
};

pub struct PublisherOnly {
    pub reading: Publisher<Reading>,
}
impl PublisherOnly {
    pub fn new(local: &schema::LocalDeployment) -> Result<Self, SerializedError> {
        Ok(Self {
            reading: schema::TEMPERATURE.publisher(local, PublishOptions::default())?,
        })
    }
}

pub struct MultiplePublishers {
    pub temperature: Publisher<Reading>,
    pub pressure: Publisher<Reading>,
}
impl MultiplePublishers {
    pub fn new(local: &schema::LocalDeployment) -> Result<Self, SerializedError> {
        Ok(Self {
            temperature: schema::TEMPERATURE.publisher(local, PublishOptions::default())?,
            pressure: schema::PRESSURE.publisher(local, PublishOptions::default())?,
        })
    }
}

pub struct SubscriberOnly {
    pub readings: Subscription,
}
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

pub struct ClientOnly {
    pub read: RpcClient<Query, Reading>,
}
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

pub struct ServerOnly {
    pub read: RpcServer,
}
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

pub struct NotifierOnly {
    pub alarm: Notifier<Reading>,
}
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

pub struct ReceiverOnly {
    pub alarm: Receiver,
}
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

pub struct MixedSubset {
    pub temperature: Publisher<Reading>,
    pub read: RpcClient<Query, Reading>,
}

pub fn mixed_subset(publisher: PublisherOnly, client: ClientOnly) -> MixedSubset {
    MixedSubset {
        temperature: publisher.reading,
        read: client.read,
    }
}
