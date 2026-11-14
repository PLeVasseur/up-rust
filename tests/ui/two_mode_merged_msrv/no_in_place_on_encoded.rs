//! An encoded handle cannot construct a payload in place.
use up_rust::test_util::wires::DoorState;
use up_rust::two_mode::*;

fn in_place(publisher: Publisher<DoorState>) {
    let _ = publisher.publish_in_place(|init| init);
}

fn main() {}
