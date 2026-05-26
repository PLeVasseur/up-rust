#!/usr/bin/env bash
set -euo pipefail

rustup +nightly component add miri
cargo +nightly miri setup

MIRIFLAGS="${MIRIFLAGS:--Zmiri-disable-isolation}" \
    cargo +nightly miri test --test stable_container_miri "$@"
