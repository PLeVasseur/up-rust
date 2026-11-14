//! Serialized-dispatch qualification controls R2, R3 and R4.
//!
//! Source controls for `serialized-dispatch-review-1/REVIEW.md`; they are not
//! middleware, zero-copy, allocation-acceptance or soundness certification.
//!
//! * `r2`: an implementation future retains a real `&mut` borrow into its own
//!   async-local state across `Pending`, and a guard destructor dereferences
//!   it. It is driven only through public dyn `send_owned`: direct
//!   `InlineOwnedTransport`, the preset selected wire and the
//!   provisioned-context selected wire, each over `InlineOwnedCore`.
//! * `r3`: repoll refusal after `Ready` and after a caught poll panic with
//!   the pinned owner retained outside `catch_unwind`, plus panicking
//!   destructors whose counters are checked outside the catch.
//! * `r4`: executable layout positives at the public dyn boundary and for the
//!   `InlineFuture` container. The compile-time negatives are standalone
//!   `tests/ui/dispatch_controls_*.rs` files sharing `shapes` with this target.
//!
//! Runtime cases poll manually with a no-op waker, use no runtime and perform
//! no I/O, so all of them run under Miri. Miri gates use
//! `-Zmiri-disable-isolation`: the provisioned-context adapter's setup probe
//! (`OwnedWireTransport::new`) calls `UUID::build()`, which reads the realtime
//! clock. Frames built by these controls carry a fixed id instead.

#[path = "support/dispatch_controls_common.rs"]
mod common;
#[path = "support/dispatch_controls_r2.rs"]
mod r2;
#[path = "support/dispatch_controls_r3.rs"]
mod r3;
#[path = "support/dispatch_controls_r4.rs"]
mod r4;
#[path = "ui/dispatch_controls/shapes.rs"]
mod shapes;
