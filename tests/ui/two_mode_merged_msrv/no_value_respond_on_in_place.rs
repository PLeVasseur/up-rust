//! An in-place request has no value-based response.
use up_rust::two_mode::*;

fn value_answer(request: Request<u32, u32, InPlace>) {
    let _ = request.respond(&1);
}

fn main() {}
