// SPDX-License-Identifier: Apache-2.0
// This generic const assertion is verified with code generation, not cargo check.
use up_rust::{UFrameFieldsMetadataCodec, StableContainerWire, UWireTransport};

struct Core;

fn build(core: Core) -> UWireTransport<Core, StableContainerWire, UFrameFieldsMetadataCodec> {
    UWireTransport::new(core, StableContainerWire, UFrameFieldsMetadataCodec)
}

fn main() {
    let _ = build(Core);
}
