//! Single-role constructors for tests that drive one native Door role at a time.
use crate::door::{native::NativeDoor, DoorState, SetDoor, CHANGED, SET};
use up_rust::{
    communication::application::{
        native::{Notifier, Receiver, RpcClient, RpcServer},
        Handler, RequestHandler,
    },
    NativeLoanCore, SetupError, UUri,
};

impl<C: NativeLoanCore> NativeDoor<C> {
    pub(crate) fn panel_address(&self) -> UUri {
        self.panel.address().get_source_uri()
    }
    pub(crate) fn changed_notifier(&self) -> Result<Notifier<C, DoorState>, SetupError> {
        CHANGED.native_notifier(&self.device, self.panel_address(), self.options.publish)
    }
    pub(crate) async fn set_client(&self) -> Result<RpcClient<C, SetDoor, DoorState>, SetupError> {
        SET.native_client(&self.panel, self.device.address(), self.options.rpc)
            .await
    }
    pub(crate) async fn set_server<H: RequestHandler<SetDoor, Reply = bool>>(
        &self,
        handler: H,
    ) -> Result<RpcServer<C, DoorState>, SetupError> {
        SET.native_server(
            &self.device,
            handler,
            |open, loan| loan.into_initializer().open(open).finish(),
            self.options.server,
        )
        .await
    }
    pub(crate) async fn changed_receiver<H: Handler<DoorState>>(
        &self,
        handler: H,
    ) -> Result<Receiver<C>, SetupError> {
        CHANGED
            .native_receiver(&self.panel, self.device.address(), handler)
            .await
    }
}
