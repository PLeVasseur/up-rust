//! R4 negative: direct static implementation future keeps a 32-byte-aligned
//! value live across its suspension point, reached through public dyn
//! `send_owned` via `InlineOwnedTransport` (supported alignment 16).
//!
//! Intended diagnostic (code generation, not `cargo check`): E0080 const
//! evaluation failure whose message contains
//! `future exceeds inline dispatch alignment`; the size check passes first.
//! Encountered while instantiating `InlineFuture::new` for the dispatch block
//! of `InlineOwnedTransport::<Direct<Align32>>::dispatch_owned`.
//! Positive twin: `run_direct::<Align16>()`. No other error is expected.
#[path = "dispatch_controls/shapes.rs"]
mod shapes;

fn main() {
    shapes::run_direct::<shapes::Align32>()
        .expect("must not build: live state exceeds supported inline alignment");
}
