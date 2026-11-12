//! R4 negative: static core future keeps 3072 bytes live across its
//! suspension point, composed as the preset `UWireTransport` over
//! `InlineOwnedCore` and reached through public dyn `send_owned`.
//!
//! 3072 bytes definitely exceed the nested core capacity
//! (`CORE_FUTURE_BYTES` = 2048) while staying below the public 4096-byte
//! capacity; the public wire block holds only the fixed-size erased core
//! owner, so its own admission passes and only the nested core admission fails.
//!
//! Intended diagnostic (code generation, not `cargo check`): exactly one E0080
//! const evaluation failure whose message contains
//! `future exceeds inline dispatch capacity`, encountered while instantiating
//! `InlineFuture::new` from `InlineOwnedCore::<Core<Fill<3072>>>::dispatch_prepared_owned`.
//! Positive twin: `run_preset::<Fill<{ CORE_FILL }>>()`.
#[path = "dispatch_controls/shapes.rs"]
mod shapes;

fn main() {
    shapes::run_preset::<shapes::Fill<3072>>()
        .expect("must not build: core live state exceeds nested inline capacity");
}
