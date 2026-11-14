//! An encoded request cannot construct its response in place.
use up_rust::test_util::wires::DoorState;
use up_rust::two_mode::*;

fn in_place(request: Request<DoorState, DoorState>) {
    let _ = request.respond_in_place(|init| init);
}

fn main() {}
