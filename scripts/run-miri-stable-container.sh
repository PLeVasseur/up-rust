#!/usr/bin/env bash
set -euo pipefail

rustup +nightly component add miri
cargo +nightly miri setup

run_miri() {
    local feature="$1"
    shift

    # The tests build deterministic metadata fixtures, so the runner is
    # isolation-clean by default. Callers may still provide explicit MIRIFLAGS.
    if [[ -n "${feature}" ]]; then
        cargo +nightly miri test --features "${feature}" --test stable_container_miri "$@"
    else
        cargo +nightly miri test --test stable_container_miri "$@"
    fi
}

run_miri "" "$@"
run_miri "unsafe-stable-payload-init" "$@"
run_miri "unsafe-uninit-payload-bytes" "$@"
run_miri "expert-unsafe-payloads" "$@"
