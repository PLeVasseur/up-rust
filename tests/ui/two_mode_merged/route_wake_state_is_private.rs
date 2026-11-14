//! The waker word is private: writing it would make the async hook read garbage.
use std::sync::atomic::Ordering;
use up_rust::transport::wake::RouteWake;

fn main() {
    let cell = RouteWake::new();
    cell.state.store(4, Ordering::SeqCst);
}
