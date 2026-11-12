//! The same Door operations and handlers over a native core, with one role set.
use super::{DoorController, DoorObserver, DoorOptions, DoorState, SetDoor, CHANGED, SET, STATE};
use std::sync::{Arc, Mutex};
use up_rust::{
    communication::{
        application::{native, RoleSetError, SubscriptionActivationError},
        zero_copy::StableTransport,
    },
    core::usubscription::USubscription,
    AdmissionError, NativeCallError, NativeLoanCore, RxLease, SendError, SetupError, TokenError,
};

up_rust::role_set! {
    pub(crate) mod roles<C: NativeLoanCore> {
        state: native::Publisher<C, DoorState>,
        changed: native::Notifier<C, DoorState>,
        subscription: native::Subscription<C>,
        set: native::RpcClient<C, SetDoor, DoorState>,
        notifications: native::Receiver<C>,
        server: native::RpcServer<C, DoorState>,
    }
}
pub(crate) type NativeDoorApp<C> = roles::Roles<C>;
pub(crate) type NativeDoorSetup<C> = roles::Setup<C>;

/// Both native peers of the demonstration; one real agreement, no owned fallback.
pub(crate) struct NativeDoor<C: NativeLoanCore> {
    pub(crate) device: native::Deployment<C>,
    pub(crate) panel: native::Deployment<C>,
    pub(crate) options: DoorOptions,
}
impl<C: NativeLoanCore> NativeDoor<C> {
    pub(crate) fn bind(
        transport: Arc<StableTransport<C>>,
        config: native::NativeConfig,
        options: DoorOptions,
    ) -> Result<Self, SetupError> {
        let (device, panel) = super::uris();
        Ok(Self {
            device: native::Deployment::new(transport.clone(), config, device)?,
            panel: native::Deployment::new(transport, config, panel)?,
            options,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum NativeAppError {
    #[error("native setup: {0}")]
    Setup(#[from] SetupError),
    #[error("role set: {0}")]
    RoleSet(#[from] RoleSetError),
    #[error("native loan/send: {0}")]
    Send(#[from] SendError),
    #[error("native call: {0}")]
    Call(#[from] NativeCallError),
    #[error("native token: {0}")]
    Token(#[from] TokenError),
    #[error("native borrow: {0}")]
    Admission(#[from] AdmissionError),
}

/// Acquires every native Door role; `setup` keeps whatever was reached if this
/// fails or is cancelled, and refuses a second attempt.
pub(crate) async fn start<C: NativeLoanCore>(
    setup: &mut roles::Setup<C>,
    door: &NativeDoor<C>,
    service: Arc<dyn USubscription>,
    observed: Arc<Mutex<Vec<bool>>>,
) -> Result<(), NativeAppError> {
    let slots = setup.begin()?;
    let panel = door.panel.address().get_source_uri();
    let device = door.device.address();
    slots
        .state
        .store(STATE.native_publisher(&door.device, door.options.publish)?);
    slots.changed.store(CHANGED.native_notifier(
        &door.device,
        panel.clone(),
        door.options.publish,
    )?);
    slots
        .set
        .acquire(SET.native_client(&door.panel, device, door.options.rpc))
        .await?
        .set_token("door-control")?;
    slots
        .server
        .acquire(SET.native_server(
            &door.device,
            DoorController,
            |open, loan| loan.into_initializer().open(open).finish(),
            door.options.server,
        ))
        .await?
        .prepare_client(&panel)?;
    slots
        .notifications
        .acquire(CHANGED.native_receiver(&door.panel, device, DoorObserver(observed.clone())))
        .await?;
    slots
        .subscription
        .acquire(STATE.native_subscription(&door.panel, device, service, DoorObserver(observed)))
        .await?;
    Ok(())
}

impl<C: NativeLoanCore> roles::Roles<C> {
    pub(crate) async fn activate(&mut self) -> Result<(), SubscriptionActivationError> {
        self.subscription.activate().await
    }
    pub(crate) async fn exercise(&mut self) -> Result<RxLease<DoorState, C::Rx>, NativeAppError> {
        self.state
            .publish_in_place(|loan| loan.into_initializer().open(false).finish())?;
        self.changed
            .notify_in_place(|loan| loan.into_initializer().open(true).finish())?;
        let pending = self
            .set
            .invoke_in_place(|loan| loan.into_initializer().open(true).finish())?;
        Ok(pending.response().await?)
    }
}
