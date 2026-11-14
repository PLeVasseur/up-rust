use up_rust::inline_dispatch::InlineFuture;
fn main() {
    let future;
    {
        let local = String::from("borrowed");
        future = InlineFuture::<(), 128>::new(async {
            std::hint::black_box(&local);
        });
    }
    drop(future);
}
