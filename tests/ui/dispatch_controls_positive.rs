//! R4 positive code-generation control for the `dispatch_controls_*` negatives.
//!
//! Build as an executable against the same pinned `up_rust` rlib as the
//! negatives, then run it. Every composition below is the admitted twin of one
//! negative file: same shared shapes module, same adapters and public dyn send,
//! different generic argument only. It must compile, link and exit 0.
#[path = "dispatch_controls/shapes.rs"]
mod shapes;

use shapes::{Align16, AlignedExact, Fill, Zst, ZstAlign16, CORE_FILL, DIRECT_FILL, EXACT_BYTES};

fn main() {
    // Twins: dispatch_controls_direct_oversize / _direct_overaligned.
    shapes::run_direct::<Fill<{ DIRECT_FILL }>>().expect("direct adapter admits large live state");
    shapes::run_direct::<Align16>().expect("direct adapter admits 16-byte aligned live state");
    // Twins: dispatch_controls_preset_core_oversize / _preset_core_overaligned.
    shapes::run_preset::<Fill<{ CORE_FILL }>>().expect("preset wire admits large core live state");
    shapes::run_preset::<Align16>().expect("preset wire admits 16-byte aligned core live state");
    // Twins: dispatch_controls_configured_core_oversize / _configured_core_overaligned.
    shapes::run_configured::<Fill<{ CORE_FILL }>>()
        .expect("provisioned-context wire admits large core live state");
    shapes::run_configured::<Align16>()
        .expect("provisioned-context wire admits 16-byte aligned core live state");
    // Twins: dispatch_controls_zero_capacity_oversize / _zst_overaligned.
    shapes::run_container::<0, _>(Zst);
    shapes::run_container::<0, _>(ZstAlign16);
    // Exactly full capacity at exactly the maximum supported alignment.
    assert!(shapes::run_container::<{ EXACT_BYTES }, _>(
        AlignedExact::default()
    ));
    println!("dispatch_controls_positive: all admitted compositions completed");
}
