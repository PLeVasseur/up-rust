//! Runtime mode can only be entered: there is no way to construct the proof.
use up_rust::two_mode::TransportRuntime;

fn main() {
    let _ = TransportRuntime { core: todo!() };
}
