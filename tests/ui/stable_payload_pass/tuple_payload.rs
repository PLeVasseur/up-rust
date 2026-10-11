// SPDX-License-Identifier: Apache-2.0
use std::mem::MaybeUninit;
use up_rust::{StablePayload, StablePayloadInit};

#[repr(C)]
#[derive(Debug, PartialEq, StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.Tuple")]
struct Tuple(u32, u32);

#[repr(align(8))]
struct Storage([MaybeUninit<u8>; 8]);

fn main() {
    let mut storage = Storage([MaybeUninit::uninit(); 8]);
    let value = Tuple::init(&mut storage.0).unwrap().field_0(7).field_1(9).finish();
    assert_eq!(*value, Tuple(7, 9));
    assert_eq!(Tuple::native_representation().size(), 8);
}
