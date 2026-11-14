//! An in-place response is read through its checked borrow, not `Deref`.
use up_rust::two_mode::*;

fn read(reply: Reply<'_, u32, InPlace>) -> u32 {
    *reply
}

fn main() {}
