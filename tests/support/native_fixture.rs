//! Explicit test-only witness for SDK mechanics. It is heap-backed conformance,
//! never physical middleware acceptance, and is not exported by the SDK.
use std::sync::Arc;
use up_rust::*;
#[derive(Debug)]
pub(crate) struct FixtureCore(pub Arc<PoolCore>);
impl NativeLoanCore for FixtureCore {}
impl PreparedTransportCore for FixtureCore {
    type Route = <PoolCore as PreparedTransportCore>::Route;
    type Tx<'a>
        = <PoolCore as PreparedTransportCore>::Tx<'a>
    where
        Self: 'a;
    type Rx = <PoolCore as PreparedTransportCore>::Rx;
    type Subscription = <PoolCore as PreparedTransportCore>::Subscription;
    fn prepare_route(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        limits: RouteLimits,
    ) -> Result<Self::Route, SetupError> {
        self.0.prepare_route(source, sink, limits)
    }
    fn loan<'a>(
        &'a self,
        route: &'a Self::Route,
        layout: TxFrameLayout,
    ) -> Result<Self::Tx<'a>, LoanError> {
        self.0.loan(route, layout)
    }
    fn publish<'a>(
        &'a self,
        route: &'a Self::Route,
        frame: InitializedTx<Self::Tx<'a>>,
    ) -> Result<(), SendError> {
        self.0.publish(route, frame)
    }
    fn attach(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        inbox: Arc<dyn FrameSink<Self::Rx>>,
    ) -> impl std::future::Future<Output = Result<Self::Subscription, SetupError>> + Send {
        self.0.attach(source, sink, inbox)
    }
    fn detach(&self, subscription: &mut Self::Subscription) -> Result<(), SetupError> {
        self.0.detach(subscription)
    }
}
