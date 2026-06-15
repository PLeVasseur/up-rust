/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

use up_rust::{ProtobufWire, UWireLoan};

#[repr(C)]
#[derive(Default, up_rust::StablePayload, up_rust::ByteBackedStablePayload)]
#[stable_payload(type_name = "example.trybuild.UnsupportedProtobufStableLoan")]
struct UnsupportedProtobufStableLoan {
    bytes: [u8; 4],
}

fn needs_stable_loan<W, T>()
where
    W: UWireLoan<T>,
{
}

fn main() {
    needs_stable_loan::<ProtobufWire, UnsupportedProtobufStableLoan>();
}
