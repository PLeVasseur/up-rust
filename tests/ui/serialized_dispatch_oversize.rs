use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use up_rust::inline_dispatch::InlineFuture;
struct TooLarge([u8; 65]);
impl Future for TooLarge {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        Poll::Ready(())
    }
}
fn main() {
    drop(InlineFuture::<(), 64>::new(TooLarge([0; 65])));
}
