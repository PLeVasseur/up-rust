use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use up_rust::inline_dispatch::InlineFuture;
#[repr(align(32))]
struct TooAligned;
impl Future for TooAligned {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        Poll::Ready(())
    }
}
fn main() {
    drop(InlineFuture::<(), 64>::new(TooAligned));
}
