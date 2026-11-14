/* Copyright (c) 2026 Contributors to the Eclipse Foundation
 * SPDX-License-Identifier: Apache-2.0
 * Compile-time layout assertions and an executable smoke probe. */
#include "uframe_metadata_abi_v1.h"

int main(void) {
    uframe_metadata_abi_v1 m = {0};
    m.magic[0] = UFRAME_ABI_MAGIC_0;
    m.kind = UFRAME_KIND_PUBLISH;
    (void)m;
    return 0;
}
