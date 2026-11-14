//! Explicit legacy owned-carriage compatibility control. Publisher annotations
//! use LegacyPublisher; this is not a common bounded-core migration.
//! Schema and deployment setup for the same Door domain and five public roles.

#[path = "user_first_door/schema.rs"]
pub(crate) mod schema;
pub(crate) use schema::{
    registration, rpc_options, uris, DoorController, DoorObserver, DoorState, SetDoor,
};
use std::sync::{Arc, Mutex};
use up_rust::{
    communication::application::*, core::usubscription::USubscription, PayloadDecodeLimit, UCode,
    UOwnedTransport,
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Encoding {
    Protobuf,
    Json,
}
impl Encoding {
    pub(crate) fn named(name: &str) -> Result<Self, SerializedError> {
        match name {
            "protobuf" => Ok(Self::Protobuf),
            "json" => Ok(Self::Json),
            _ => Err(SerializedError::Config(
                "Door schema does not declare that encoding",
            )),
        }
    }
}

pub(crate) type DoorOptions = OperationOptions;

up_rust::schema_mapping!(Encoding, DoorState {
    Encoding::Protobuf => (schema::DoorProtobuf, schema::DoorProtobuf),
    Encoding::Json => (schema::DoorJson, schema::DoorJson),
});
up_rust::schema_mapping!(Encoding, SetDoor {
    Encoding::Protobuf => (schema::DoorProtobuf, schema::DoorProtobuf),
    Encoding::Json => (schema::DoorJson, schema::DoorJson),
});
up_rust::declare_operations! {
    pub(crate) event STATE: DoorState = 0x8001;
    pub(crate) notification CHANGED: DoorState = 0x8002;
    pub(crate) rpc SET: SetDoor => DoorState = 0x42;
}

up_rust::role_set! {
    /// The owners this Door program retains; see `start` for how they are acquired.
    pub(crate) mod roles {
        state: LegacyPublisher<DoorState>,
        changed: Notifier<DoorState>,
        subscription: Subscription,
        set: RpcClient<SetDoor, DoorState>,
        notifications: Receiver,
        server: RpcServer,
    }
}
pub(crate) type DoorApp = roles::Roles;

/// The device and panel programs, with both sides' mappings admitted up front.
/// A real deployment constructs one of these locally on each peer.
#[derive(Debug)]
pub(crate) struct Door {
    pub(crate) device: BoundDeployment<Encoding>,
    pub(crate) panel: BoundDeployment<Encoding>,
    pub(crate) options: DoorOptions,
}
impl Door {
    pub(crate) fn bind(
        device: Deployment,
        panel: Deployment,
        encoding: Encoding,
        options: DoorOptions,
    ) -> Result<Self, SerializedError> {
        let door = Self {
            device: device.bind(encoding),
            panel: panel.bind(encoding),
            options,
        };
        STATE.admit(&door.device, &door.panel)?;
        CHANGED.admit(&door.device, &door.panel)?;
        SET.admit(&door.panel, &door.device)?;
        Ok(door)
    }
}

pub(crate) fn deployment(
    transport: Arc<dyn UOwnedTransport>,
    encoding: Encoding,
) -> Result<Door, SerializedError> {
    let (device, panel) = uris();
    let limit = PayloadDecodeLimit::new(1024);
    Door::bind(
        Deployment::new(transport.clone(), device, limit, registration(), 32)?,
        Deployment::new(transport, panel, limit, registration(), 32)?,
        encoding,
        DoorOptions {
            publish: PublishOptions::default(),
            rpc: rpc_options(),
            server: ServerOptions { max_clients: 4 },
        },
    )
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    #[error("setup/send: {0}")]
    Setup(#[from] SerializedError),
    #[error("role set: {0}")]
    RoleSet(#[from] RoleSetError),
    #[error("subscription activation: {0}")]
    Activate(#[from] SubscriptionActivationError),
    #[error("resources closed with a terminal worker failure; see retained diagnostics")]
    CleanupFailed,
    #[error("RPC: {0}")]
    Call(#[from] CallError),
}

/// Acquires every Door role; `setup` keeps whatever was reached if this fails or
/// is cancelled, and refuses a second attempt.
pub(crate) async fn start(
    setup: &mut roles::Setup,
    door: &Door,
    service: Arc<dyn USubscription>,
    observed: Arc<Mutex<Vec<bool>>>,
) -> Result<(), AppError> {
    let slots = setup.begin()?;
    let panel = door.panel.address().get_source_uri();
    let device = door.device.address();
    slots
        .state
        .store(STATE.publisher(&door.device, door.options.publish)?);
    slots
        .changed
        .store(CHANGED.notifier(&door.device, panel.clone(), door.options.publish)?);
    slots
        .set
        .acquire(SET.client(&door.panel, device, door.options.rpc))
        .await?
        .set_token("door-control")?;
    slots
        .server
        .acquire(SET.server(
            &door.device,
            DoorController,
            |open| DoorState { open },
            door.options.server,
        ))
        .await?
        .prepare_client(&panel)?;
    slots
        .notifications
        .acquire(CHANGED.receiver(&door.panel, device, DoorObserver(observed.clone())))
        .await?;
    slots
        .subscription
        .acquire(STATE.subscription(&door.panel, device, service, DoorObserver(observed)))
        .await?;
    Ok(())
}

impl roles::Roles {
    pub(crate) async fn activate(&mut self) -> Result<(), SubscriptionActivationError> {
        self.subscription.activate().await
    }
    pub(crate) async fn exercise(&self) -> Result<Decoded<DoorState>, AppError> {
        self.state.publish(&DoorState { open: false }).await?;
        self.changed.notify(&DoorState { open: true }).await?;
        let pending = self.set.invoke(&SetDoor { open: true }).await?;
        Ok(pending.response().await?)
    }
}

/// Retains the first operation error beside the application while cleanup runs.
pub(crate) struct Recovery {
    pub(crate) app: DoorApp,
    pub(crate) operation: Option<AppError>,
}
impl Recovery {
    pub(crate) fn new(app: DoorApp) -> Self {
        Self {
            app,
            operation: None,
        }
    }
    pub(crate) async fn run(&mut self) {
        if self.operation.is_some() {
            return;
        }
        if let Err(error) = self.app.activate().await {
            self.operation = Some(error.into());
            return;
        }
        if let Err(error) = self.app.exercise().await {
            self.operation = Some(error);
        }
    }
}

pub(crate) fn recovery(error: &CallError) -> &'static str {
    match error {
        CallError::TooManyInFlight => "wait for capacity; do not replay an already submitted call",
        CallError::TimedOut => "completion is unknown; reconcile device state before retry",
        CallError::Closed => "create a new admitted client",
        CallError::Status(UCode::PermissionDenied) => "repair credentials before retry",
        CallError::Status(_) => "inspect the peer status",
        CallError::Submit(_) => {
            "inspect mapping or physical send failure; delivery may be uncertain"
        }
        CallError::Response(_) => "reject the peer declaration or malformed response",
    }
}

#[cfg(feature = "up-l2-zero-copy")]
#[path = "user_first_door/native.rs"]
pub(crate) mod native;
