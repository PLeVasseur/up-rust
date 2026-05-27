#!/usr/bin/env bash
set -euo pipefail

rustup +nightly component add miri
cargo +nightly miri setup

run_miri() {
    local feature="$1"
    shift

    # The stable-container tests construct `UFrameMetadata::publish`, which
    # builds a UUID from `SystemTime::elapsed`; Miri isolation rejects the
    # underlying `clock_gettime` call, so disable isolation unless the caller
    # supplies explicit `MIRIFLAGS`.
    if [[ -n "${feature}" ]]; then
        MIRIFLAGS="${MIRIFLAGS:--Zmiri-disable-isolation}" \
            cargo +nightly miri test --features "${feature}" --test stable_container_miri "$@"
    else
        MIRIFLAGS="${MIRIFLAGS:--Zmiri-disable-isolation}" \
            cargo +nightly miri test --test stable_container_miri "$@"
    fi
}

run_miri "" "$@"
run_miri "unsafe-stable-payload-init" "$@"
run_miri "unsafe-uninit-payload-bytes" "$@"
run_miri "expert-unsafe-payloads" "$@"
