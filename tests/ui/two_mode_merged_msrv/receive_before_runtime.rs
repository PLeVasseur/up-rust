//! Traffic is impossible until runtime mode: a setup value cannot receive.
use up_rust::two_mode::*;

fn receive(setup: SubscriberSetup<u32>) {
    let _ = setup.try_recv();
}

fn main() {}
