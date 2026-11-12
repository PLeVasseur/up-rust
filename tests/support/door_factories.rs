//! Single-role constructors for tests that drive one role of the Door schema
//! at a time; applications construct roles with the schema operations directly.
use crate::door::{Door, DoorState, SetDoor, CHANGED, SET, STATE};
use std::sync::Arc;
use up_rust::{
    communication::application::{
        Handler, Notifier, Publisher, Receiver, RequestHandler, RpcClient, RpcServer,
        SerializedError, Subscription,
    },
    core::usubscription::USubscription,
    UUri,
};

impl Door {
    pub(crate) fn panel_address(&self) -> UUri {
        self.panel.address().get_source_uri()
    }
    pub(crate) fn state_publisher(&self) -> Result<Publisher<DoorState>, SerializedError> {
        STATE.publisher(&self.device, self.options.publish)
    }
    pub(crate) fn changed_notifier(&self) -> Result<Notifier<DoorState>, SerializedError> {
        CHANGED.notifier(&self.device, self.panel_address(), self.options.publish)
    }
    pub(crate) async fn set_client(
        &self,
    ) -> Result<RpcClient<SetDoor, DoorState>, SerializedError> {
        SET.client(&self.panel, self.device.address(), self.options.rpc)
            .await
    }
    pub(crate) async fn set_server<H: RequestHandler<SetDoor, Reply = bool>>(
        &self,
        handler: H,
    ) -> Result<RpcServer, SerializedError> {
        SET.server(
            &self.device,
            handler,
            |open| DoorState { open },
            self.options.server,
        )
        .await
    }
    pub(crate) async fn changed_receiver<H: Handler<DoorState>>(
        &self,
        handler: H,
    ) -> Result<Receiver, SerializedError> {
        CHANGED
            .receiver(&self.panel, self.device.address(), handler)
            .await
    }
    pub(crate) async fn state_subscription<H: Handler<DoorState>>(
        &self,
        service: Arc<dyn USubscription>,
        handler: H,
    ) -> Result<Subscription, SerializedError> {
        STATE
            .subscription(&self.panel, self.device.address(), service, handler)
            .await
    }
}
