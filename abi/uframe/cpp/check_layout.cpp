// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
// Compile-time layout assertions and an executable smoke probe.
#include "uframe_metadata_abi_v1.hpp"

int main() {
    uprotocol::v2::UFrameMetadataAbiV1 m{};
    m.magic[0] = 'U';
    m.kind = static_cast<std::uint8_t>(uprotocol::v2::UFrameKind::Publish);
    (void)m;
    return 0;
}
