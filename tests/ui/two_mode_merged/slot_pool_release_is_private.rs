//! A slot index is only returned by the guard that owns it.
use up_rust::transport::slots::SlotPool;

fn double_spend(pool: &'static SlotPool) {
    let first = pool.acquire().expect("a slot");
    pool.release(first.index());
}

fn main() {}
