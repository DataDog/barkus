#!/usr/bin/env bash
# Shared builder for Rust SUT fuzz harnesses. Each suts/<sut>/build.sh
# sources this file and calls `build_rust_sut`. Reads from the calling
# SUT's pin.txt:
#   rust_nightly  : pinned nightly toolchain (e.g. nightly-2026-04-15)
#
# Output: five binaries under fuzz/target/<host-triple>/release/:
#   barkus_strict, barkus_nearvalid, barkus_havoc, arbitrary, raw
# Each is a libFuzzer binary; invoke with -max_total_time=N for timed runs.

set -euo pipefail

build_rust_sut() {
    local sut_root="$1"
    source "${sut_root}/pin.txt"
    : "${rust_nightly:?pin.txt missing rust_nightly=}"

    cd "${sut_root}"
    # Cargo's libgit2 backend reads ~/.gitconfig; defeat any insteadOf
    # rule that would break unauthenticated fetches.
    export GIT_CONFIG_GLOBAL=/dev/null

    for target in barkus_strict barkus_nearvalid barkus_havoc arbitrary raw; do
        echo "==> cargo fuzz build ${target}"
        cargo "+${rust_nightly}" fuzz build --release "${target}"
    done

    echo "OK $(basename "${sut_root}") fuzz binaries built in fuzz/target/"
}
