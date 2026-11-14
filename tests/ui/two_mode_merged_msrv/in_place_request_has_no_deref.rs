//! An in-place request is read through its checked borrow, not `Deref`.
use up_rust::two_mode::*;

fn read(request: Request<u32, u32, InPlace>) -> u32 {
    *request
}

fn main() {}
