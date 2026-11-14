use up_rust::inline_dispatch::InlineFuture;
fn main() {
    let shared = std::rc::Rc::new(1);
    drop(InlineFuture::<(), 128>::new(async move {
        drop(shared);
    }));
}
