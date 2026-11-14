//! A native client cannot send before the transport enters runtime.
use up_rust::test_util::wires::DoorState;
use up_rust::two_mode::*;

fn early(setup: RpcClientSetup<DoorState, DoorState, InPlace>) {
    let _ = setup.invoke_in_place(|init| init);
}

fn main() {}
