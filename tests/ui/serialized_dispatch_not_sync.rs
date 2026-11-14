use up_rust::inline_dispatch::InlineFuture;
fn requires_sync<T: Sync>(_: T) {}
fn main() {
    requires_sync(InlineFuture::<(), 128>::new(async {}));
}
