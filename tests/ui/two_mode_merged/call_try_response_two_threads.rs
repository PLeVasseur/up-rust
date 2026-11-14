//! A `Call` is one owner's: even holding it mutably, two threads cannot both poll it.
use up_rust::test_util::wires::DoorState;
use up_rust::two_mode::*;

fn race(client: RpcClient<DoorState, DoorState>, value: &DoorState) {
    let mut call = client.invoke(value).expect("invoke");
    std::thread::scope(|scope| {
        scope.spawn(|| call.try_response().map(|reply| reply.is_some()));
        scope.spawn(|| call.try_response().map(|reply| reply.is_some()));
    });
}

fn main() {}
