//! `enter_runtime_all` starts from setup values; a running transport is not one.
use up_rust::two_mode::*;

fn twice(running: Vec<TransportRuntime>) {
    let _: Result<Vec<TransportRuntime>, StartupError> = enter_runtime_all(running);
}

fn main() {}
