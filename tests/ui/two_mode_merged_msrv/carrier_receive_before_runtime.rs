//! Traffic is impossible until runtime mode: setup cannot take a frame.
use up_rust::two_mode::*;

fn take(setup: &TransportSetup, route: IngressRoute) {
    let _ = setup.receive(route, &mut |_frame: CarriedFrame<'_>| Flow::Continue);
}

fn main() {}
