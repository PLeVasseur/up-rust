//! Shared programs' handlers for the independent-role proof targets.
//!
//! Handlers forward what an independently deployed program actually decoded
//! (value plus message context) through channels, so tests assert delivered
//! values and correlation rather than successful sends. `RecordingService` is
//! a local uSubscription double that records requested topics; it is not
//! remote-service consistency evidence.
use crate::schema;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, Notify};
use up_rust::{
    communication::{
        application::{Delivery, Handler, RequestHandler},
        SubscriptionStatus,
    },
    core::usubscription::{ResetReason, SubscriptionInfo, USubscription},
    PayloadEncoding, StaticUriProvider, UCode, UEntityId, UMessageType, UStatus, UUri, UUID,
};

/// Upper bound for one awaited delivery; a stalled control fails, never hangs.
pub(crate) const BOUND: Duration = Duration::from_secs(5);

/// Independently configured address. As a peer input it is routing data only:
/// never a peer deployment, endpoint, transport or opposite role.
pub(crate) fn address(authority: &str, service: u16) -> StaticUriProvider {
    StaticUriProvider::for_entity(authority, UEntityId::first_instance(service), 1)
        .expect("valid address")
}

/// What a receiving program decoded, including its delivery context.
#[derive(Debug)]
pub(crate) struct Seen {
    pub(crate) value: u32,
    pub(crate) kind: UMessageType,
    pub(crate) source: UUri,
    pub(crate) sink: Option<UUri>,
    pub(crate) encoding: Option<PayloadEncoding>,
    pub(crate) native_token: bool,
}

/// What a serving program decoded from one request.
#[derive(Debug)]
pub(crate) struct Request {
    pub(crate) slot: u32,
    pub(crate) source: UUri,
    pub(crate) sink: Option<UUri>,
    pub(crate) id: UUID,
    pub(crate) token: Option<String>,
}

fn observe<D: Delivery<schema::Reading>>(delivery: &D) -> Option<Seen> {
    let value = delivery.payload().ok()?.value;
    let context = delivery.context().ok()?;
    Some(Seen {
        value,
        kind: context.kind,
        source: context.source,
        sink: context.sink,
        encoding: context.payload_encoding,
        native_token: context.native_type_token.is_some(),
    })
}

/// Event/notification handler shared by serialized and native programs.
pub(crate) struct Record(pub(crate) mpsc::UnboundedSender<Seen>);
impl Handler<schema::Reading> for Record {
    async fn on_receive<D: Delivery<schema::Reading>>(&mut self, delivery: D) {
        if let Some(seen) = observe(&delivery) {
            let _ = self.0.send(seen);
        }
    }
}

/// Test gate: a handler reports entry, then retains its delivery until release.
#[derive(Default)]
pub(crate) struct Gate {
    pub(crate) entered: Notify,
    pub(crate) release: Notify,
}

/// One handler type for every repeated receiver; one instance may be gated.
pub(crate) struct Alarm {
    pub(crate) seen: mpsc::UnboundedSender<Seen>,
    pub(crate) hold: Option<Arc<Gate>>,
}
impl Handler<schema::Reading> for Alarm {
    async fn on_receive<D: Delivery<schema::Reading>>(&mut self, delivery: D) {
        if let Some(seen) = observe(&delivery) {
            let _ = self.seen.send(seen);
        }
        if let Some(gate) = &self.hold {
            gate.entered.notify_one();
            gate.release.notified().await;
            // The retained delivery is still borrowable after the held await.
            assert!(delivery.payload().is_ok(), "retained delivery stays valid");
        }
    }
}

/// Server business decision: scales the requested slot by a fixed factor.
pub(crate) struct Scale {
    pub(crate) factor: u32,
    pub(crate) seen: mpsc::UnboundedSender<Request>,
}
impl RequestHandler<schema::Query> for Scale {
    type Reply = u32;
    async fn handle<D: Delivery<schema::Query>>(&mut self, request: D) -> Result<u32, UCode> {
        let slot = request.payload().map_err(|_| UCode::InvalidArgument)?.slot;
        let context = request.context().map_err(|_| UCode::InvalidArgument)?;
        let _ = self.seen.send(Request {
            slot,
            source: context.source,
            sink: context.sink,
            id: context.id,
            token: context.token.map(str::to_owned),
        });
        Ok(slot * self.factor)
    }
}

/// Next decoded observation, bounded so a missing delivery fails the control.
pub(crate) async fn next<T>(channel: &mut mpsc::UnboundedReceiver<T>) -> T {
    tokio::time::timeout(BOUND, channel.recv())
        .await
        .expect("independent peer delivery stalled")
        .expect("handler channel open")
}

/// Yields to workers until a counter-based condition holds.
pub(crate) async fn eventually(condition: impl Fn() -> bool) {
    for _ in 0..10_000 {
        if condition() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("condition not reached within the yield budget");
}

/// Records exactly which topics subscribers asked the service to change.
#[derive(Default)]
pub(crate) struct RecordingService {
    subscribed: Mutex<Vec<UUri>>,
    unsubscribed: Mutex<Vec<UUri>>,
}
impl RecordingService {
    pub(crate) fn subscribed(&self) -> Vec<UUri> {
        self.subscribed.lock().unwrap().clone()
    }
    pub(crate) fn unsubscribed(&self) -> Vec<UUri> {
        self.unsubscribed.lock().unwrap().clone()
    }
}
#[async_trait::async_trait]
impl USubscription for RecordingService {
    async fn subscribe(
        &self,
        topic: &UUri,
        _: Option<u64>,
        _: Option<u32>,
    ) -> Result<SubscriptionStatus, UStatus> {
        self.subscribed.lock().unwrap().push(topic.clone());
        Ok(SubscriptionStatus::Subscribed)
    }
    async fn unsubscribe(&self, topic: &UUri) -> Result<(), UStatus> {
        self.unsubscribed.lock().unwrap().push(topic.clone());
        Ok(())
    }
    async fn fetch_subscribers(&self, _: &UUri) -> Result<Vec<UUri>, UStatus> {
        unreachable!("not used by the independent-role controls")
    }
    async fn fetch_subscriptions_by_topic(
        &self,
        _: &UUri,
    ) -> Result<Vec<SubscriptionInfo>, UStatus> {
        unreachable!("not used by the independent-role controls")
    }
    async fn fetch_subscriptions_by_subscriber(
        &self,
        _: &UUri,
    ) -> Result<Vec<SubscriptionInfo>, UStatus> {
        unreachable!("not used by the independent-role controls")
    }
    async fn register_for_notifications(&self, _: &UUri) -> Result<(), UStatus> {
        unreachable!("not used by the independent-role controls")
    }
    async fn unregister_for_notifications(&self, _: &UUri) -> Result<(), UStatus> {
        unreachable!("not used by the independent-role controls")
    }
    async fn reset(&self, _: ResetReason, _: Option<String>) -> Result<(), UStatus> {
        unreachable!("not used by the independent-role controls")
    }
}
