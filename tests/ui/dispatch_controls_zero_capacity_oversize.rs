//! R4 negative: a one-byte future in a zero-capacity public `InlineFuture`.
//!
//! Intended diagnostic (code generation, not `cargo check`): E0080 const
//! evaluation failure whose message contains
//! `future exceeds inline dispatch capacity`, encountered while instantiating
//! `InlineFuture::<u8, 0>::new::<OneByte>` from `run_container::<0, OneByte>`.
//! Positive twin: `run_container::<0, _>(Zst)` (a zero-sized future).
#[path = "dispatch_controls/shapes.rs"]
mod shapes;

fn main() {
    assert_eq!(shapes::run_container::<0, _>(shapes::OneByte(7)), 7);
}
