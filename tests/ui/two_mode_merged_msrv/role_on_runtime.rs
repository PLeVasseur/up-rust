//! Setup is impossible in runtime mode: roles are built on a setup, never a runtime.
use up_rust::two_mode::*;
use up_rust::UUri;

fn add_route_in_runtime(runtime: &TransportRuntime, topic: UUri) {
    let _ = Subscriber::<u32>::on(runtime, topic);
}

fn main() {}
