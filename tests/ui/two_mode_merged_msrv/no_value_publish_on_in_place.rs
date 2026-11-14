//! An in-place handle has no value-based publish.
use up_rust::two_mode::*;

fn value_send(publisher: Publisher<u32, InPlace>) {
    let _ = publisher.publish(&1);
}

fn main() {}
