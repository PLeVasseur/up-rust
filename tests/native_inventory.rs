// SPDX-License-Identifier: Apache-2.0
//! Linked concrete inventory, explicit generic registration and absent opt-in.
#![cfg(feature = "native-inventory")]

use up_rust::{native_inventory::registered_native_types, NativeTarget, StablePayload};

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "example.DoorState", inventory)]
struct DoorState {
    value: u32,
}

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "example.SeatState")]
struct SeatState {
    value: u32,
}

#[repr(transparent)]
#[derive(StablePayload)]
#[stable_payload(type_name = "example.GenericState")]
struct GenericState<T: 'static> {
    value: T,
}

up_rust::register_native_type!(GenericState<u64>);

#[test]
fn only_opted_in_concrete_types_are_exported_in_stable_order() {
    let inventory = registered_native_types().unwrap();
    assert_eq!(
        inventory
            .iter()
            .map(|value| value.name())
            .collect::<Vec<_>>(),
        ["example.DoorState", "example.GenericState"]
    );
    assert!(inventory
        .iter()
        .all(|value| value.target() == &NativeTarget::current()));
    assert!(!inventory
        .iter()
        .any(|value| value.name() == SeatState::TYPE_NAME));
}
