//! The program that raced two threads in `try_response`: `&self` was enough.
use up_rust::test_util::wires::DoorState;
use up_rust::two_mode::*;

fn race(client: RpcClient<DoorState, DoorState>, value: &DoorState) {
    let call = client.invoke(value).expect("invoke");
    std::thread::scope(|scope| {
        scope.spawn(|| call.try_response().map(|reply| reply.is_some()));
        scope.spawn(|| call.try_response().map(|reply| reply.is_some()));
    });
}

fn main() {}
