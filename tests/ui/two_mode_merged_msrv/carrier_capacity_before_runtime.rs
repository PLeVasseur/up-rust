//! Traffic is impossible until runtime mode: setup has no capacity to check.
use up_rust::two_mode::*;

fn check(setup: &TransportSetup, route: EgressRoute) {
    let _ = setup.has_capacity(route);
}

fn main() {}
