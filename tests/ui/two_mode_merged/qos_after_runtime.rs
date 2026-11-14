//! QoS is chosen during initialization: a built or activated role has no `.qos`.
use up_rust::two_mode::*;

fn after_setup(setup: PublisherSetup<u32>) {
    let _ = setup.qos(Qos::DEFAULT);
}

fn after_runtime(publisher: Publisher<u32>) {
    let _ = publisher.qos(Qos::DEFAULT);
}

fn main() {}
