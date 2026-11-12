//! A second domain: real integer schemas, declarative profiles and stored roles.
use protobuf::well_known_types::wrappers::UInt32Value;
use std::sync::{
    atomic::{AtomicU32, AtomicUsize, Ordering},
    Arc,
};
use up_rust::{
    communication::application::*, core::usubscription::USubscription, ProtobufPayload, UCode,
};

#[repr(C)]
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    derive(up_rust::StablePayload, up_rust::StablePayloadInit)
)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    stable_payload(type_name = "meter.Reading", revision = 1)
)]
pub(crate) struct Reading {
    pub(crate) value: u32,
}

#[repr(C)]
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    derive(up_rust::StablePayload, up_rust::StablePayloadInit)
)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    stable_payload(type_name = "meter.SetLevel", revision = 1)
)]
pub(crate) struct SetLevel {
    pub(crate) value: u32,
}

fn uint32(value: u32) -> UInt32Value {
    let mut wire = UInt32Value::new();
    wire.value = value;
    wire
}

up_rust::mapped_codec! {
    pub(crate) struct LevelProtobuf = "meter.level.protobuf.UInt32Value.v1" via ProtobufPayload, wire UInt32Value;
    Reading => |reading| uint32(reading.value), |wire| Reading { value: wire.value };
    SetLevel => |level| uint32(level.value), |wire| SetLevel { value: wire.value };
}
up_rust::json_codec!(pub(crate) struct LevelJson = ("example.meter.level.json-v1", 0xF102) for Reading, SetLevel);

#[derive(Clone, Copy, Debug)]
pub(crate) enum Profile {
    Protobuf,
    Mixed,
}
up_rust::schema_mapping!(Profile, Reading {
    Profile::Protobuf => (LevelProtobuf, LevelProtobuf),
    Profile::Mixed => (LevelJson, LevelJson),
});
up_rust::schema_mapping!(Profile, SetLevel {
    Profile::Protobuf => (LevelProtobuf, LevelProtobuf),
    Profile::Mixed => (LevelProtobuf, LevelProtobuf),
});
up_rust::declare_operations! {
    pub(crate) event READING: Reading = 0x8003;
    pub(crate) notification CHANGED: Reading = 0x8004;
    pub(crate) rpc SET: SetLevel => Reading = 0x43;
}

up_rust::role_set! {
    pub(crate) mod roles {
        publisher: Publisher<Reading>,
        notifier: Notifier<Reading>,
        subscription: Subscription,
        client: RpcClient<SetLevel, Reading>,
        receiver: Receiver,
        server: RpcServer,
    }
}
pub(crate) type MeterApp = roles::Roles;
pub(crate) type MeterSetup = roles::Setup;

/// The device and panel programs, with both sides' mappings admitted up front.
#[derive(Debug)]
pub(crate) struct Meter {
    pub(crate) device: BoundDeployment<Profile>,
    pub(crate) panel: BoundDeployment<Profile>,
    pub(crate) options: OperationOptions,
}
impl Meter {
    pub(crate) fn bind(
        device: Deployment,
        panel: Deployment,
        profile: Profile,
        options: OperationOptions,
    ) -> Result<Self, SerializedError> {
        Self::bind_profiles(device, panel, profile, profile, options)
    }
    pub(crate) fn bind_profiles(
        device: Deployment,
        panel: Deployment,
        device_profile: Profile,
        panel_profile: Profile,
        options: OperationOptions,
    ) -> Result<Self, SerializedError> {
        let meter = Self {
            device: device.bind(device_profile),
            panel: panel.bind(panel_profile),
            options,
        };
        READING.admit(&meter.device, &meter.panel)?;
        CHANGED.admit(&meter.device, &meter.panel)?;
        SET.admit(&meter.panel, &meter.device)?;
        Ok(meter)
    }
}

#[derive(Default)]
pub(crate) struct Observation {
    pub(crate) count: AtomicUsize,
    pub(crate) value: AtomicU32,
}
#[derive(Default)]
pub(crate) struct Observer(pub(crate) Arc<Observation>);
impl Handler<Reading> for Observer {
    async fn on_receive<D: Delivery<Reading>>(&mut self, delivery: D) {
        // A native delivery re-checks on every borrow; a failed check is skipped.
        let Ok(retained) = delivery.payload() else {
            return;
        };
        tokio::task::yield_now().await;
        self.0.value.store(retained.value, Ordering::Release);
        self.0.count.fetch_add(1, Ordering::Release);
    }
}
pub(crate) struct Controller;
impl RequestHandler<SetLevel> for Controller {
    type Reply = u32;
    async fn handle<D: Delivery<SetLevel>>(&mut self, request: D) -> Result<u32, UCode> {
        Ok(request.payload().map_err(|_| UCode::InvalidArgument)?.value)
    }
}

/// Acquires every Meter role, one attempt; failure or cancellation keeps the
/// owners already acquired and leaves `setup` cleanup-only.
pub(crate) async fn start(
    setup: &mut roles::Setup,
    meter: &Meter,
    service: Arc<dyn USubscription>,
    events: Arc<Observation>,
    notifications: Arc<Observation>,
) -> Result<(), SerializedError> {
    let slots = setup.begin()?;
    let panel = meter.panel.address().get_source_uri();
    let device = meter.device.address();
    slots
        .publisher
        .store(READING.publisher(&meter.device, meter.options.publish)?);
    slots
        .notifier
        .store(CHANGED.notifier(&meter.device, panel.clone(), meter.options.publish)?);
    slots
        .client
        .acquire(SET.client(&meter.panel, device, meter.options.rpc))
        .await?;
    slots
        .server
        .acquire(SET.server(
            &meter.device,
            Controller,
            |value| Reading { value },
            meter.options.server,
        ))
        .await?
        .prepare_client(&panel)?;
    slots
        .receiver
        .acquire(CHANGED.receiver(&meter.panel, device, Observer(notifications)))
        .await?;
    slots
        .subscription
        .acquire(READING.subscription(&meter.panel, device, service, Observer(events)))
        .await?;
    Ok(())
}

#[cfg(feature = "up-l2-zero-copy")]
pub(crate) mod native {
    use super::*;
    use up_rust::{communication::application::native as binding, NativeLoanCore, SetupError};

    up_rust::role_set! {
        pub(crate) mod roles<C: NativeLoanCore> {
            publisher: binding::Publisher<C, Reading>,
            notifier: binding::Notifier<C, Reading>,
            subscription: binding::Subscription<C>,
            client: binding::RpcClient<C, SetLevel, Reading>,
            receiver: binding::Receiver<C>,
            server: binding::RpcServer<C, Reading>,
        }
    }
    pub(crate) type MeterApp<C> = roles::Roles<C>;
    pub(crate) type MeterSetup<C> = roles::Setup<C>;

    /// Acquires every native Meter role over two independently admitted deployments.
    pub(crate) async fn start<C: NativeLoanCore>(
        setup: &mut roles::Setup<C>,
        device: &binding::Deployment<C>,
        panel: &binding::Deployment<C>,
        options: OperationOptions,
        service: Arc<dyn USubscription>,
        events: Arc<Observation>,
        notifications: Arc<Observation>,
    ) -> Result<(), SetupError> {
        let slots = setup.begin()?;
        let panel_address = panel.address().get_source_uri();
        slots
            .publisher
            .store(READING.native_publisher(device, options.publish)?);
        slots.notifier.store(CHANGED.native_notifier(
            device,
            panel_address.clone(),
            options.publish,
        )?);
        slots
            .client
            .acquire(SET.native_client(panel, device.address(), options.rpc))
            .await?;
        slots
            .server
            .acquire(SET.native_server(
                device,
                Controller,
                |value, loan| loan.into_initializer().value(value).finish(),
                options.server,
            ))
            .await?
            .prepare_client(&panel_address)?;
        slots
            .receiver
            .acquire(CHANGED.native_receiver(panel, device.address(), Observer(notifications)))
            .await?;
        slots
            .subscription
            .acquire(READING.native_subscription(
                panel,
                device.address(),
                service,
                Observer(events),
            ))
            .await?;
        Ok(())
    }
}
