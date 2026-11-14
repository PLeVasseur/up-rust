// SPDX-License-Identifier: Apache-2.0
use std::sync::Arc;
use up_rust::{
    NativeProfile, NativeProfileAgreement, StableContainerWire, UFrameFieldsMetadataCodec,
    UHasWire, UWireTransport,
};

fn main() {
    let profile = NativeProfile::new(
        "constructor",
        1,
        up_rust::PayloadEncoding::private_use(0xF100),
        [],
    )
    .unwrap();
    let agreement = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
    let transport = UWireTransport::with_native_agreement(
        (),
        StableContainerWire,
        UFrameFieldsMetadataCodec,
        agreement.clone(),
    );
    assert!(transport
        .native_agreement()
        .unwrap()
        .is_same_generation(&agreement));
}
