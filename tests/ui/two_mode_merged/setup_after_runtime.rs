//! Setup is impossible after the transition: `enter_runtime` consumes the setup.
use up_rust::two_mode::*;

fn add_route_after_runtime(setup: TransportSetup, provider: &dyn LocalUriProvider) {
    let _runtime = setup.enter_runtime();
    let _ = Publisher::<u32>::on(&setup, provider, 0x8001);
}

fn main() {}
