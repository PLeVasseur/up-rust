//! Traffic is impossible until runtime mode: a setup value cannot send.
use up_rust::two_mode::*;

fn publish(setup: PublisherSetup<u32>) {
    let _ = setup.publish(&1);
}

fn main() {}
