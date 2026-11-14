//! Concrete stored names; PoolCore is explicitly a test binding, not middleware proof.
use super::schema::{self, Query, Reading};
use up_rust::{communication::application::native, NativeCallError, NativeLoanCore, SendError};

up_rust::native_roles! {
    pub mod selected {
        core: crate::native_fixture::FixtureCore;
        publisher Temperature, temperature: schema::TEMPERATURE => Reading;
        publisher Pressure, pressure: schema::PRESSURE => Reading;
        subscription Readings, readings: schema::TEMPERATURE => Reading;
        client ReadClient, read_client, ReadResponse: schema::READ => Query, Reading;
        server ReadServer, read_server: schema::READ => Query, Reading;
        reply u32 => |value, loan| loan.into_initializer().value(value).finish();
        notifier AlarmSender, alarm_sender: schema::ALARM => Reading;
        receiver AlarmReceiver, alarm_receiver: schema::ALARM => Reading;
    }
}

pub struct PublisherOnly {
    pub reading: selected::Temperature,
}
impl PublisherOnly {
    pub fn new(local: &selected::Deployment) -> Result<Self, up_rust::SetupError> {
        Ok(Self {
            reading: selected::temperature(local)?,
        })
    }
    pub fn publish(&self, value: u32) -> Result<(), SendError> {
        self.reading
            .publish_in_place(|loan| loan.into_initializer().value(value).finish())
    }
}

pub struct SubscriberOnly {
    pub readings: selected::Readings,
}
pub struct ClientOnly {
    pub read: selected::ReadClient,
    pub retained: Option<selected::ReadResponse>,
}
impl ClientOnly {
    pub async fn prepare(
        local: &selected::Deployment,
        server: &dyn up_rust::LocalUriProvider,
    ) -> Result<Self, up_rust::SetupError> {
        Ok(Self {
            read: selected::read_client(local, server).await?,
            retained: None,
        })
    }
    pub async fn request(&mut self, slot: u32) -> Result<(), NativeCallError> {
        let pending = self
            .read
            .invoke_in_place(|loan| loan.into_initializer().slot(slot).finish())?;
        self.retained = Some(pending.response().await?);
        Ok(())
    }
    pub fn latest(&self) -> Result<Option<&Reading>, up_rust::AdmissionError> {
        self.retained
            .as_ref()
            .map(|delivery| delivery.payload())
            .transpose()
    }
}
pub struct ServerOnly {
    pub read: selected::ReadServer,
}
impl ServerOnly {
    pub async fn prepare<
        H: up_rust::communication::application::RequestHandler<Query, Reply = u32>,
    >(
        local: &selected::Deployment,
        handler: H,
    ) -> Result<Self, up_rust::SetupError> {
        Ok(Self {
            read: selected::read_server(local, handler).await?,
        })
    }
}
impl SubscriberOnly {
    pub async fn prepare<H: up_rust::communication::application::Handler<Reading>>(
        local: &selected::Deployment,
        source: &dyn up_rust::LocalUriProvider,
        service: std::sync::Arc<dyn up_rust::core::usubscription::USubscription>,
        handler: H,
    ) -> Result<Self, up_rust::SetupError> {
        Ok(Self {
            readings: selected::readings(local, source, service, handler).await?,
        })
    }
}
pub struct MultiplePublishers {
    pub temperature: selected::Temperature,
    pub pressure: selected::Pressure,
}
impl MultiplePublishers {
    pub fn new(local: &selected::Deployment) -> Result<Self, up_rust::SetupError> {
        Ok(Self {
            temperature: selected::temperature(local)?,
            pressure: selected::pressure(local)?,
        })
    }
}

pub struct ReusablePublisher<Core: NativeLoanCore> {
    pub reading: native::Publisher<Core, Reading>,
}
impl<Core: NativeLoanCore> ReusablePublisher<Core> {
    pub fn new(local: &native::Deployment<Core>) -> Result<Self, up_rust::SetupError> {
        Ok(Self {
            reading: schema::TEMPERATURE.native_publisher(local, Default::default())?,
        })
    }
    pub fn publish(&self, value: u32) -> Result<(), SendError> {
        self.reading
            .publish_in_place(|loan| loan.into_initializer().value(value).finish())
    }
}
