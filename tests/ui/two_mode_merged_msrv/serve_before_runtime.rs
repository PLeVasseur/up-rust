//! Traffic is impossible until runtime mode: an RPC server setup cannot receive.
use up_rust::two_mode::*;

fn serve(setup: RpcServerSetup<u32, u32>) {
    let _ = setup.try_recv();
}

fn main() {}
