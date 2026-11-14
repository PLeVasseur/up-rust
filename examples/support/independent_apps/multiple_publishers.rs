//! The existing temperature/pressure service, independent of deployment target.
use super::super::schema::{Format, Reading, PRESSURE_RESOURCE, TEMPERATURE_RESOURCE};
use up_rust_application::application::{
    PublishError, Publisher, PublisherDeployment, PublisherOptions, PublisherSetup,
};
use up_rust_core::transport::{StartupError, TransportRuntime};

pub struct MultiplePublishers {
    pub temperature: Publisher<Reading>,
    pub pressure: Publisher<Reading>,
}

/// This aggregate only contains SDK setup handles; it implements no engine.
pub struct MultiplePublishersSetup {
    temperature: PublisherSetup<Reading>,
    pressure: PublisherSetup<Reading>,
}
impl MultiplePublishers {
    pub fn prepare(local: &PublisherDeployment<'_, Format>) -> Result<MultiplePublishersSetup, StartupError> {
        Ok(MultiplePublishersSetup {
            temperature: local.publisher(TEMPERATURE_RESOURCE, PublisherOptions::default())?,
            pressure: local.publisher(PRESSURE_RESOURCE, PublisherOptions::default())?,
        })
    }

    /// Publish each temperature in order, then pressure, stopping on the first error.
    /// Each publication completes synchronously when polled, so the whole slice
    /// may run in one poll. Scheduling must account for that work and any admitted
    /// binding wait. Dropping before polling sends nothing; already sent values
    /// are not rolled back. There is no new fixed publisher or batch-size cap.
    pub async fn publish_readings(
        &self,
        temperatures: &[u32],
        pressure: u32,
    ) -> Result<(), PublishError> {
        for &value in temperatures {
            self.temperature.publish(&Reading { value }).await?;
        }
        self.pressure.publish(&Reading { value: pressure }).await?;
        Ok(())
    }
}
impl MultiplePublishersSetup {
    pub fn activate(self, runtime: &TransportRuntime) -> Result<MultiplePublishers, StartupError> {
        Ok(MultiplePublishers {
            temperature: self.temperature.activate(runtime)?,
            pressure: self.pressure.activate(runtime)?,
        })
    }
}
