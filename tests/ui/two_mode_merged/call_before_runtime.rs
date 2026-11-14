//! Traffic is impossible until runtime mode: an RPC client setup cannot call.
use up_rust::two_mode::*;

fn call(setup: RpcClientSetup<u32, u32>) {
    let _ = setup.invoke(&1);
}

fn main() {}
