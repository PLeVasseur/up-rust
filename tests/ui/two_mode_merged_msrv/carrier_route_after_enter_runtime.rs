//! Setup is impossible after the transition: `enter_runtime` consumes the setup.
use up_rust::two_mode::*;

fn late_route(setup: TransportSetup, route: &CarrierRoute<'_>) {
    let _runtime = setup.enter_runtime();
    let _ = setup.carrier_ingress(route);
}

fn main() {}
