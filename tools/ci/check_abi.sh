#!/usr/bin/env bash
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
toolchain=${1:?Rust toolchain is required}
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"
output="${CARGO_TARGET_DIR:-target}/abi-check/$toolchain"
mkdir -p "$output"
cc --version
c++ --version
rustc "+$toolchain" --version
cc -std=c11 -Wall -Wextra -Werror -pedantic abi/uframe/c/check_layout.c -o "$output/c-layout"
"$output/c-layout"
c++ -std=c++17 -Wall -Wextra -Werror -pedantic abi/uframe/cpp/check_layout.cpp -o "$output/cpp-layout"
"$output/cpp-layout"
rustc "+$toolchain" --edition 2021 -D warnings --crate-type lib abi/uframe/rust/uframe_metadata_abi_v1.rs --out-dir "$output"
