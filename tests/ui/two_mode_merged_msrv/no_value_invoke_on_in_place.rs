//! An in-place client has no value-based invoke.
use up_rust::two_mode::*;

fn value_call(client: RpcClient<u32, u32, InPlace>) {
    let _ = client.invoke(&1);
}

fn main() {}
