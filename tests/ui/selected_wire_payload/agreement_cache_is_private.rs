use std::sync::Arc;

use up_rust::{NativeProfile, NativeProfileAgreement, NativeProfileMode, NativeProfileTable, NativeRepresentation, PayloadEncoding, StablePayload};

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "uprotocol.test.Door")]
struct Door {
    open: u32,
}

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "uprotocol.test.Seat")]
struct Seat {
    occupied: u32,
}

fn seed(agreement: &NativeProfileAgreement) {
    // Attempt to associate Door's TypeId with Seat's representation. The cache
    // can only be populated by crate code that resolves a type's own descriptor.
    let _ = agreement.resolve_type_identity::<Door, up_rust::UWireError>(|| {
        Ok::<NativeRepresentation, up_rust::UWireError>(Seat::native_representation())
    });
}

fn main() {
    let table = NativeProfileTable::new([
        (PayloadEncoding::private_use(0xF101), Door::native_representation()),
        (PayloadEncoding::private_use(0xF102), Seat::native_representation()),
    ])
    .unwrap();
    let profile = NativeProfile::new("ui", 1, NativeProfileMode::Table(table)).unwrap();
    let agreement = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
    seed(&agreement);
}
