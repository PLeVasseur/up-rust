//! Activation needs proof of runtime mode: a setup does not qualify.
use up_rust::two_mode::*;

fn activate(publisher: PublisherSetup<u32>, setup: &TransportSetup) {
    let _ = publisher.activate(setup);
}

fn main() {}
