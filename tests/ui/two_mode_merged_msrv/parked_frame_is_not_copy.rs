//! A parked frame owns its slot. One lease, so the slot is released once.
use up_rust::two_mode::*;

fn twice(parked: ParkedFrame) {
    let first = parked;
    let second = parked;
    drop((first, second));
}

fn main() {}
