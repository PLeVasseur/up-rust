//! An encoded client cannot construct a request in place.
use up_rust::test_util::wires::DoorState;
use up_rust::two_mode::*;

fn in_place(client: RpcClient<DoorState, DoorState>) {
    let _ = client.invoke_in_place(|init| init);
}

fn main() {}
