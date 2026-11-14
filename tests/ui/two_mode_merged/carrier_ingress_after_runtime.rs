//! Setup is impossible in runtime mode: a running transport declares no routes.
use up_rust::two_mode::*;

fn declare(runtime: TransportRuntime, route: &CarrierRoute<'_>) {
    let _ = runtime.carrier_ingress(route);
}

fn main() {}
