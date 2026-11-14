//! A `Ready` from one slot cannot be returned for another slot.
use up_rust::two_mode::{Ready, StartupSlot};

fn other<'a, 'b>(slot: StartupSlot<'a, u32>, other: StartupSlot<'b, u32>) -> Ready<'a, u32> {
    let _ = slot;
    other.write(1)
}

fn main() {}

// [utest->req~ssr-unsafe-wire-untrusted~1] method: compile-fail
