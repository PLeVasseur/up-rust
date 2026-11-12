//! R4 negative: static core future keeps a 32-byte-aligned value live across
//! its suspension point, composed as the provisioned-context
//! `OwnedWireTransport` over `InlineOwnedCore` and reached through public dyn
//! `send_owned`. Only the nested core admission sees alignment 32.
//!
//! Intended diagnostic (code generation, not `cargo check`): exactly one E0080
//! const evaluation failure whose message contains
//! `future exceeds inline dispatch alignment`, encountered while instantiating
//! `InlineFuture::new` from `InlineOwnedCore::<Core<Align32>>::dispatch_prepared_owned`
//! inside `OwnedWireTransport::dispatch_owned`.
//! Positive twin: `run_configured::<Align16>()`.
#[path = "dispatch_controls/shapes.rs"]
mod shapes;

fn main() {
    shapes::run_configured::<shapes::Align32>()
        .expect("must not build: core live state exceeds supported inline alignment");
}
