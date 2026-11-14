//! The async layer's access to the waker word is `unsafe`.
use std::sync::atomic::Ordering;
use up_rust::transport::wake::RouteWake;

fn main() {
    let cell = RouteWake::new();
    cell.state().store(4, Ordering::SeqCst);
}
