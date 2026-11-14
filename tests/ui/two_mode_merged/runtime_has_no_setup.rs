//! Runtime mode offers traffic only: no setup call exists on it.
use up_rust::test_util::wires::DoorWire;
use up_rust::two_mode::*;

fn no_setup(runtime: TransportRuntime, waiver: &'static RuntimeWaiver) {
    let _ = runtime.with_wire(DoorWire);
    let _ = runtime.with_waiver(waiver);
    runtime.signal_on(Box::leak(Box::new(Signal::new())));
}

fn main() {}
