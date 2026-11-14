use std::{
    cell::Cell,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use up_rust::inline_dispatch::InlineFuture;
struct Fits {
    _state: [u8; 64],
}
impl Future for Fits {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        Poll::Ready(())
    }
}
fn requires_send<T: Send>(_: &T) {}
fn main() {
    drop(InlineFuture::<(), 64>::new(Fits { _state: [0; 64] }));
    let local = String::from("borrowed");
    let owned_cell = Cell::new(0);
    let future = InlineFuture::<(), 128>::new(async move {
        owned_cell.set(1);
        drop(owned_cell);
    });
    requires_send(&future);
    drop(future);
    drop(InlineFuture::<(), 128>::new(async {
        std::hint::black_box(&local);
    }));
}
