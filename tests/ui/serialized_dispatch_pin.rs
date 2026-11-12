use up_rust::inline_dispatch::InlineFuture;
fn requires_unpin<T: Unpin>(_: T) {}
fn main() {
    requires_unpin(InlineFuture::<(), 128>::new(async {}));
}
