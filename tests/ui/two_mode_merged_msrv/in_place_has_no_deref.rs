//! An in-place frame is read through its checked `payload()`, never by deref.
use up_rust::two_mode::*;

fn deref(frame: Received<u32, InPlace>) -> u32 {
    *frame
}

fn main() {}
