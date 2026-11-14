//! A role must name its wire: with no default attached, `build` does not exist.
use up_rust::two_mode::*;

fn no_wire(setup: &TransportSetup, provider: &dyn LocalUriProvider) {
    let _ = Publisher::<u32>::on(setup, provider, 0x8001).build();
}

fn main() {}
