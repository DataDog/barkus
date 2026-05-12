#!/usr/bin/env bash
# Build the rust-cssparser fuzz binaries via cargo-fuzz on pinned nightly.
#
# Outputs five binaries in fuzz/target/<host-triple>/release/:
#   barkus_strict, barkus_nearvalid, barkus_havoc, arbitrary, raw
# Each is a libFuzzer binary; invoke with -max_total_time=N for timed runs.

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
source "${HERE}/pin.txt"
: "${rust_nightly:?pin.txt missing rust_nightly=}"

cd "${HERE}"

# Defeat any host-level git insteadOf rule that rewrites https → ssh
# (would break unauthenticated dependency fetches).
export GIT_CONFIG_GLOBAL=/dev/null

# cargo-fuzz build takes one target at a time; loop over our five.
for target in barkus_strict barkus_nearvalid barkus_havoc arbitrary raw; do
    echo "==> cargo fuzz build ${target}"
    cargo "+${rust_nightly}" fuzz build --release "${target}"
done

echo "OK rust-cssparser fuzz binaries built in fuzz/target/"
