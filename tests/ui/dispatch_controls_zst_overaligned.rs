//! R4 negative: a zero-sized but 32-byte-aligned future in a zero-capacity
//! public `InlineFuture`. Size admission (0 <= 0) passes; alignment is still
//! checked for zero-sized futures.
//!
//! Intended diagnostic (code generation, not `cargo check`): E0080 const
//! evaluation failure whose message contains
//! `future exceeds inline dispatch alignment`, encountered while instantiating
//! `InlineFuture::<(), 0>::new::<ZstAlign32>` from `run_container::<0, ZstAlign32>`.
//! Positive twin: `run_container::<0, _>(ZstAlign16)`.
#[path = "dispatch_controls/shapes.rs"]
mod shapes;

fn main() {
    shapes::run_container::<0, _>(shapes::ZstAlign32);
}
