//! Traffic is impossible until runtime mode: setup cannot send a frame.
use up_rust::two_mode::*;

fn forward(setup: &TransportSetup, route: EgressRoute, frame: &CarriedFrame<'_>) {
    let _ = setup.send(route, frame);
}

fn main() {}
