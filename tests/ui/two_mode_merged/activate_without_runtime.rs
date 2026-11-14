//! Activation cannot be done without naming the running transport.
use up_rust::two_mode::*;

fn activate(publisher: PublisherSetup<u32>) {
    let _ = publisher.activate();
}

fn main() {}
