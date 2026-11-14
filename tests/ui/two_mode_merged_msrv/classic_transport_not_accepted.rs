//! The classic path allocates in SDK code and stays outside the two modes.
use up_rust::two_mode::*;
use up_rust::MockTransport;

fn main() {
    let memory = StartupMemory::from_source(&up_rust::startup::LEAKED_HEAP_REGIONS, 4096).unwrap();
    let _ = TransportSetup::handoff_in(&memory, MockTransport::default());
}
