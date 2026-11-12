// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
use up_rust::{UFrameFieldsMetadataCodec, UWire, UWireTransport, WireIdentity};

struct NeedsAgreement;

impl UWire for NeedsAgreement {
    const WIRE_ID: WireIdentity = WireIdentity::new("test.native", 0x8123);
    const PAYLOAD_FAMILY_ID: WireIdentity = WireIdentity::new("test.native-family", 0x8124);
    const FORMAT_VERSION: u16 = 1;
    const REQUIRES_NATIVE_AGREEMENT: bool = true;
}

fn main() {
    let _ = UWireTransport::new((), NeedsAgreement, UFrameFieldsMetadataCodec);
}
