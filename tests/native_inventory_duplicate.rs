// SPDX-License-Identifier: Apache-2.0
//! Duplicate linked registrations fail rather than depending on linker order.
#![cfg(feature = "native-inventory")]

#[repr(C)]
#[derive(up_rust::StablePayload)]
#[stable_payload(type_name = "example.Duplicate", inventory)]
struct Duplicate {
    value: u32,
}

up_rust::register_native_type!(Duplicate);

#[test]
fn duplicate_identity_is_rejected() {
    assert_eq!(
        up_rust::native_inventory::registered_native_types(),
        Err(up_rust::NativeProfileError::InvalidDescriptor(
            "duplicate native inventory name/revision"
        ))
    );
}
