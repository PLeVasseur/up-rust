//! R4 negative: direct static implementation future keeps 8192 bytes live
//! across its suspension point, reached through public dyn `send_owned` via
//! `InlineOwnedTransport` (public capacity `SEND_FUTURE_BYTES` = 4096).
//!
//! Intended diagnostic (code generation, not `cargo check`): E0080 const
//! evaluation failure whose message contains
//! `future exceeds inline dispatch capacity`, encountered while instantiating
//! `InlineFuture::new` for the dispatch block of
//! `InlineOwnedTransport::<Direct<Fill<8192>>>::dispatch_owned`.
//! Positive twin: `run_direct::<Fill<{ DIRECT_FILL }>>()` in
//! `dispatch_controls_positive.rs`. No other error is expected.
#[path = "dispatch_controls/shapes.rs"]
mod shapes;

fn main() {
    shapes::run_direct::<shapes::Fill<8192>>()
        .expect("must not build: live state exceeds public inline capacity");
}
